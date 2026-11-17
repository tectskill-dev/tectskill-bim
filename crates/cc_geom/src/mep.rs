//! Pipes, conduits and cable trays: each pipe or conduit segment is a
//! cylinder of its type's diameter along its center line, each cable tray an
//! open channel (or a ladder) of its type's section around it, colored by
//! its system. Valves sit on their pipes as a body, a stem and a handwheel
//! or lever.

use crate::Solid;
use cc_core::{Element, ElementId, ElementKind, Project, TrayStyle, ValveKind};
use glam::DVec3;

/// Sides of the polygon approximating a segment's section.
const SIDES: usize = 12;
/// Most rungs drawn on one ladder tray (longer runs get them farther apart).
const MAX_RUNGS: usize = 400;

/// The section of a cable tray run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraySection {
    pub width: f64,
    pub height: f64,
    pub style: TrayStyle,
}

/// A pipe, conduit or cable tray run in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentGeom {
    pub element: ElementId,
    /// Center line ends.
    pub a: DVec3,
    pub b: DVec3,
    /// Outside diameter (cable trays: their width).
    pub diameter: f64,
    /// Cable trays: their section.
    pub tray: Option<TraySection>,
}

impl SegmentGeom {
    pub fn length(&self) -> f64 {
        self.a.distance(self.b)
    }
}

/// A valve in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValveGeom {
    pub element: ElementId,
    /// The pipe it sits on.
    pub host: ElementId,
    pub kind: ValveKind,
    /// Its center, on the pipe's center line.
    pub point: DVec3,
    /// Unit direction of the pipe (start to end).
    pub dir: DVec3,
    /// The pipe's diameter.
    pub diameter: f64,
}

/// The run of a pipe, conduit or cable tray element (`None` for other
/// elements or a degenerate run).
pub fn segment_geom(project: &Project, e: &Element) -> Option<SegmentGeom> {
    let s = e.as_segment()?;
    let ty = project.element_type(e.type_id)?;
    let (diameter, tray) = match &e.kind {
        ElementKind::CableTray(_) => {
            let (width, height, style) = ty.tray_section()?;
            (width, Some(TraySection { width, height, style }))
        }
        _ => (ty.diameter()?, None),
    };
    let (a, b) = s.points(project.level_elevation(e.level));
    (a.distance(b) > 1e-6 && a.is_finite() && b.is_finite() && diameter.is_finite() && diameter > 0.0)
        .then_some(SegmentGeom { element: e.id, a, b, diameter, tray })
}

/// The valve element `e` placed on its pipe (`None` for other elements, or
/// when its pipe has no run).
pub fn valve_geom(project: &Project, e: &Element) -> Option<ValveGeom> {
    let ElementKind::Valve(v) = &e.kind else { return None };
    let kind = project.element_type(e.type_id)?.valve_kind()?;
    let (host, run) = project.valve_host(v)?;
    let pipe = segment_geom(project, host)?;
    let dir = (pipe.b - pipe.a).try_normalize()?;
    let point = v.point(run, project.level_elevation(host.level));
    point.is_finite().then_some(ValveGeom { element: e.id, host: host.id, kind, point, dir, diameter: pipe.diameter })
}

/// The faces of a closed frustum from `a` (radius `ra`) to `b` (radius `rb`).
pub fn frustum(a: DVec3, b: DVec3, ra: f64, rb: f64) -> Vec<Vec<DVec3>> {
    let Some(axis) = (b - a).try_normalize() else { return Vec::new() };
    let u = axis.any_orthonormal_vector();
    let v = axis.cross(u);
    let ring = |c: DVec3, r: f64| -> Vec<DVec3> {
        (0..SIDES)
            .map(|i| {
                let t = i as f64 / SIDES as f64 * std::f64::consts::TAU;
                c + (u * t.cos() + v * t.sin()) * r
            })
            .collect()
    };
    let (bottom, top) = (ring(a, ra), ring(b, rb));
    let mut faces = Vec::with_capacity(SIDES + 2);
    // Caps wound to face outwards: the bottom against the axis, the top along it.
    faces.push(bottom.iter().rev().copied().collect());
    faces.push(top.clone());
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        faces.push(vec![bottom[i], bottom[j], top[j], top[i]]);
    }
    faces
}

/// The faces of a closed cylinder of `radius` from `a` to `b`.
pub fn cylinder(a: DVec3, b: DVec3, radius: f64) -> Vec<Vec<DVec3>> {
    frustum(a, b, radius, radius)
}

/// A frame across `axis`: `side` to the left of a horizontal run and `up`
/// above it (`side × up = axis`); vertical runs take the X axis as side.
pub fn section_frame(axis: DVec3) -> (DVec3, DVec3) {
    let side = DVec3::Z.cross(axis).try_normalize().unwrap_or_else(|| DVec3::Y.cross(axis).normalize_or(DVec3::X));
    (side, axis.cross(side))
}

/// The faces of a box from `a` to `b` whose section spans `s0..s1` along
/// `side` and `u0..u1` along `up` (a frame from [`section_frame`]).
fn bar(a: DVec3, b: DVec3, (side, up): (DVec3, DVec3), [s0, s1, u0, u1]: [f64; 4]) -> Vec<Vec<DVec3>> {
    let corners = |c: DVec3| -> Vec<DVec3> {
        [(s0, u0), (s1, u0), (s1, u1), (s0, u1)].iter().map(|(s, u)| c + side * *s + up * *u).collect()
    };
    let (bottom, top) = (corners(a), corners(b));
    let mut faces = Vec::with_capacity(6);
    faces.push(bottom.iter().rev().copied().collect());
    faces.push(top.clone());
    for i in 0..4 {
        let j = (i + 1) % 4;
        faces.push(vec![bottom[i], bottom[j], top[j], top[i]]);
    }
    faces
}

/// Thickness of a tray's sheet for a section `height` tall.
pub fn tray_sheet(height: f64) -> f64 {
    (height * 0.06).clamp(0.0015, 0.006)
}

/// The section of a tray as an open channel, counter-clockwise in
/// (`side`, `up`) coordinates of [`section_frame`] around its center line.
pub fn tray_profile(t: TraySection) -> Vec<glam::DVec2> {
    use glam::DVec2 as P;
    let (w, h, k) = (t.width * 0.5, t.height * 0.5, tray_sheet(t.height));
    vec![
        P::new(-w, h),
        P::new(-w, -h),
        P::new(w, -h),
        P::new(w, h),
        P::new(w - k, h),
        P::new(w - k, -h + k),
        P::new(-w + k, -h + k),
        P::new(-w + k, h),
    ]
}

/// The faces of a cable tray of section `t` from `a` to `b` (center line in
/// the middle of the section): two side rails and a bottom, or rungs for a
/// ladder.
pub fn tray(a: DVec3, b: DVec3, t: TraySection) -> Vec<Vec<DVec3>> {
    let Some(axis) = (b - a).try_normalize() else { return Vec::new() };
    let f = section_frame(axis);
    let (w, h, k) = (t.width * 0.5, t.height * 0.5, tray_sheet(t.height));
    let mut faces = bar(a, b, f, [w - k, w, -h, h]);
    faces.extend(bar(a, b, f, [-w, -w + k, -h, h]));
    match t.style {
        TrayStyle::SolidBottom | TrayStyle::Perforated => faces.extend(bar(a, b, f, [-w + k, w - k, -h, -h + k])),
        TrayStyle::Ladder => {
            let length = a.distance(b);
            let rung = 0.03_f64.min(length * 0.5);
            let count = ((length / cc_core::mep::TRAY_RUNG_SPACING).floor() as usize).clamp(1, MAX_RUNGS);
            let step = length / count as f64;
            for i in 0..count {
                let c = a + axis * (step * (i as f64 + 0.5));
                faces.extend(bar(c - axis * rung * 0.5, c + axis * rung * 0.5, f, [-w + k, w - k, -h, -h + 2.0 * k]));
            }
        }
    }
    faces
}

/// The faces of a valve: a body narrowing to its middle along the pipe and,
/// except for check valves, a stem with a handwheel (or a lever on ball valves).
pub fn valve(v: &ValveGeom) -> Vec<Vec<DVec3>> {
    let d = v.diameter;
    let half = d * 1.2;
    let (a, b) = (v.point - v.dir * half, v.point + v.dir * half);
    let mut faces = frustum(a, v.point, d * 0.8, d * 0.45);
    faces.extend(frustum(v.point, b, d * 0.45, d * 0.8));
    let (side, up) = section_frame(v.dir);
    match v.kind {
        ValveKind::Globe => faces.extend(cylinder(v.point - up * d * 0.6, v.point + up * d * 0.6, d * 0.6)),
        ValveKind::Ball => faces.extend(bar(
            v.point - v.dir * d * 0.5,
            v.point + v.dir * d * 0.5,
            (side, up),
            [-d * 0.55, d * 0.55, -d * 0.55, d * 0.55],
        )),
        ValveKind::Gate | ValveKind::Check => {}
    }
    if v.kind.has_handle() {
        let top = v.point + up * d * 1.4;
        faces.extend(cylinder(v.point, top, d * 0.12));
        match v.kind {
            // A lever along the pipe.
            ValveKind::Ball => {
                faces.extend(bar(top, top + v.dir * d * 2.0, (side, up), [-d * 0.1, d * 0.1, 0.0, d * 0.1]))
            }
            _ => faces.extend(cylinder(top, top + up * d * 0.15, d * 0.75)),
        }
    }
    faces
}

impl crate::ModelGeometry {
    pub(crate) fn build_mep(&mut self, project: &Project) {
        for e in project.elements.values() {
            let Some(g) = segment_geom(project, e) else { continue };
            let mut solid = Solid::new(e.id, e.category(), project.system_color(e));
            match g.tray {
                Some(t) => solid.add_polygons(tray(g.a, g.b, t)),
                None => solid.add_polygons(cylinder(g.a, g.b, g.diameter * 0.5)),
            }
            self.solids.push(solid);
            self.segments.insert(e.id, g);
        }
        for e in project.elements.values() {
            let Some(v) = valve_geom(project, e) else { continue };
            let mut solid = Solid::new(e.id, e.category(), project.system_color(e));
            solid.add_polygons(valve(&v));
            self.solids.push(solid);
            self.valves.insert(e.id, v);
        }
        self.mep = crate::mep_net::MepNetwork::build(project, &self.segments, &self.valves);
        for f in &self.mep.fittings {
            let Some(e) = f.runs.first().and_then(|id| project.element(*id)) else { continue };
            let mut solid = Solid::new(e.id, e.category(), project.system_color(e));
            if let Some(t) = self.segments.get(&e.id).and_then(|g| g.tray) {
                // Trays: a bottom plate as wide as the widest branch, along the first one.
                let Some(&(dir, w)) = f.branches.first() else { continue };
                let w = f.diameter().max(w) * 0.5;
                let k = tray_sheet(t.height);
                let h = t.height * 0.5;
                let (a, b) = (f.point - dir * w, f.point + dir * w);
                solid.add_polygons(bar(a, b, section_frame(dir), [-w, w, -h, -h + k]));
            } else {
                // Pipes and conduits: a socket a little wider than the pipe on each branch, part of the first run.
                for &(dir, d) in &f.branches {
                    solid.add_polygons(cylinder(f.point, f.point + dir * (d * 0.8), d * 0.6));
                }
            }
            self.solids.push(solid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cylinder_is_closed_with_outward_faces() {
        let (a, b) = (DVec3::ZERO, DVec3::new(2.0, 0.0, 1.0));
        let mut s = Solid::new(ElementId(1), cc_core::Category::Pipe, [0, 0, 0]);
        s.add_polygons(cylinder(a, b, 0.05));
        assert_eq!(s.faces.len(), SIDES + 2);
        // The polygon's area times the length.
        let n = SIDES as f64;
        let area = 0.5 * n * 0.05 * 0.05 * (std::f64::consts::TAU / n).sin();
        assert!((s.volume() - area * a.distance(b)).abs() < 1e-9, "{}", s.volume());
    }

    #[test]
    fn solid_bottom_tray_is_a_channel_of_its_section() {
        let t = TraySection { width: 0.3, height: 0.1, style: TrayStyle::SolidBottom };
        for b in [DVec3::new(4.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 3.0), DVec3::new(3.0, 4.0, 1.0)] {
            let mut s = Solid::new(ElementId(1), cc_core::Category::CableTray, [0, 0, 0]);
            s.add_polygons(tray(DVec3::ZERO, b, t));
            let k = tray_sheet(t.height);
            // Two sides and the bottom between them, along the whole run.
            let area = 2.0 * k * t.height + (t.width - 2.0 * k) * k;
            assert!((s.volume() - area * b.length()).abs() < 1e-9, "{} vs {}", s.volume(), area * b.length());
        }
        // The open top: the highest point of a horizontal run is the top of its sides.
        let faces = tray(DVec3::ZERO, DVec3::X, t);
        let top = faces.iter().flatten().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((top - 0.05).abs() < 1e-12);
    }

    #[test]
    fn ladder_rungs_are_bounded() {
        let t = TraySection { width: 0.3, height: 0.1, style: TrayStyle::Ladder };
        // Two rails, a rung every 0.3 m: 10 on 3 m, at most MAX_RUNGS on a long run.
        assert_eq!(tray(DVec3::ZERO, DVec3::new(3.0, 0.0, 0.0), t).len(), 6 * (2 + 10));
        assert_eq!(tray(DVec3::ZERO, DVec3::new(1e6, 0.0, 0.0), t).len(), 6 * (2 + MAX_RUNGS));
    }
}
