//! Structural framing joins: a beam end inside a structural column stops at
//! the column's face, and beam ends meet other beams the way walls do (a
//! mitered L corner, a butt T against the side of a beam, a collinear pair
//! running through a junction of three or more).

use crate::polygon;
use crate::wall::{EndJoin, WallLine, resolve_join};
use cc_core::ElementId;
use cc_core::SectionOutline;
use cc_core::arc::Arc;
use glam::{DVec2, DVec3};

/// How close to a column outline a beam end must lie to frame into it.
const COLUMN_TOL: f64 = 0.01;

/// What a beam end is framed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamEnd {
    /// Free: the end is square at its location point.
    Free,
    /// Cut back to the face of a structural column.
    Column(ElementId),
    /// Mitered or butted against other beams.
    Beam,
}

/// Resolved beam geometry: frame, section and the ends after joins.
#[derive(Debug, Clone, PartialEq)]
pub struct BeamGeom {
    pub element: ElementId,
    pub origin: DVec2,
    pub dir: DVec2,
    /// Left normal (+v).
    pub normal: DVec2,
    /// Length between the location points.
    pub length: f64,
    pub half: f64,
    /// Bottom and top elevations at the start location point (`u = 0`).
    pub z_range: (f64, f64),
    /// Rise per meter along the axis (sloped beams).
    pub slope: f64,
    /// The section, square to the plan axis: `x` across the beam (`v`,
    /// centered on its box), `y` up from the bottom of the section.
    pub section: SectionOutline,
    /// Whether the section has arcs (their facets are drawn smooth).
    pub curved: bool,
    /// Axial extents `(left, right)` of the start/end faces after joins.
    pub start_ext: (f64, f64),
    pub end_ext: (f64, f64),
    pub start: BeamEnd,
    pub end: BeamEnd,
    /// The centerline arc of an arc beam or arc strip footing (`dir` and
    /// `normal` are then its frame at the start, `u` runs along the arc and
    /// `v` to its left). [`Self::footprint`] gives only the four corners:
    /// [`Self::outline`] follows the arc.
    pub arc: Option<Arc>,
}

impl BeamGeom {
    /// World plan position of beam-local `(u, v)`.
    pub fn world(&self, u: f64, v: f64) -> DVec2 {
        match &self.arc {
            Some(a) => a.at(u, v),
            None => self.origin + self.dir * u + self.normal * v,
        }
    }

    /// Unit direction of the axis at `u` (the tangent of an arc beam).
    pub fn dir_at(&self, u: f64) -> DVec2 {
        self.arc.as_ref().map_or(self.dir, |a| a.tangent(u))
    }

    /// Position of the plan point `p` along the axis.
    pub fn locate(&self, p: DVec2) -> f64 {
        match &self.arc {
            Some(a) => a.locate(p).0,
            None => (p - self.origin).dot(self.dir),
        }
    }

    /// The axis between the location points: its two ends, or the chords of an arc.
    pub fn axis(&self) -> Vec<DVec2> {
        match &self.arc {
            Some(a) => a.points(0.0, self.length, 0.0, cc_core::arc::ARC_STEP),
            None => vec![self.origin, self.world(self.length, 0.0)],
        }
    }

    /// The footprint corners in beam-local `(u, v)`, counter-clockwise.
    pub fn local_footprint(&self) -> [DVec2; 4] {
        let h = self.half;
        [
            DVec2::new(self.start_ext.1, -h),
            DVec2::new(self.end_ext.1, -h),
            DVec2::new(self.end_ext.0, h),
            DVec2::new(self.start_ext.0, h),
        ]
    }

    /// Plan footprint of the body, counter-clockwise.
    pub fn footprint(&self) -> [DVec2; 4] {
        self.local_footprint().map(|p| self.world(p.x, p.y))
    }

    /// Plan outline of the body, counter-clockwise: the footprint, with the
    /// faces of an arc beam as short chords.
    pub fn outline(&self) -> Vec<DVec2> {
        let n = self.stations();
        if n <= 1 {
            return self.footprint().to_vec();
        }
        let side = |v: f64| (0..=n).map(move |i| self.world(self.station_u(i as f64 / n as f64, v), v));
        let mut pts: Vec<DVec2> = side(-self.half).collect();
        pts.extend(side(self.half).collect::<Vec<_>>().into_iter().rev());
        pts
    }

    /// The line `v` across the body from its start face to its end face
    /// (chords along an arc): an edge of its section in plan.
    pub fn line(&self, v: f64) -> Vec<DVec2> {
        let n = self.stations();
        (0..=n).map(|i| self.world(self.station_u(i as f64 / n as f64, v), v)).collect()
    }

    /// Number of pieces the body is swept in: 1 for a straight beam, the
    /// chords of an arc.
    fn stations(&self) -> usize {
        match &self.arc {
            Some(a) => {
                let du = (self.end_ext.0 - self.start_ext.0).abs().max((self.end_ext.1 - self.start_ext.1).abs());
                cc_core::arc::segments(du / a.radius.max(1e-9), cc_core::arc::ARC_STEP)
            }
            None => 1,
        }
    }

    /// Where the fraction `t` of the way from the start face to the end face lies on the line `v`.
    fn station_u(&self, t: f64, v: f64) -> f64 {
        let (a, b) = (self.face_u(false, v), self.face_u(true, v));
        a + (b - a) * t
    }

    /// Length of the body along its (sloped) axis, the mean of its two sides.
    pub fn cut_length(&self) -> f64 {
        ((self.end_ext.0 - self.start_ext.0) + (self.end_ext.1 - self.start_ext.1)) * 0.5 * self.stretch()
    }

    /// Length along the slope per meter in plan.
    pub fn stretch(&self) -> f64 {
        self.slope.hypot(1.0)
    }

    /// Bottom and top elevations at `u` along the axis.
    pub fn z_at(&self, u: f64) -> (f64, f64) {
        (self.z_range.0 + self.slope * u, self.z_range.1 + self.slope * u)
    }

    /// Lowest and highest elevations of the body.
    pub fn z_bounds(&self) -> (f64, f64) {
        let lo = self.start_ext.0.min(self.start_ext.1);
        let hi = self.end_ext.0.max(self.end_ext.1);
        let (a, b) = (self.z_at(lo), self.z_at(hi));
        (a.0.min(b.0), a.1.max(b.1))
    }

    /// Where the start (`at_end` false) or end face crosses the line `v`.
    fn face_u(&self, at_end: bool, v: f64) -> f64 {
        let (left, right) = if at_end { self.end_ext } else { self.start_ext };
        let t = if self.half > 0.0 { (v + self.half) / (2.0 * self.half) } else { 0.5 };
        right + (left - right) * t
    }

    /// The rings of the section: the outline counter-clockwise, then each
    /// hole of three points or more clockwise. Empty when the outline has
    /// fewer than three points.
    fn rings(&self) -> Vec<Vec<DVec2>> {
        let outer = polygon::ccw(&self.section.outer);
        if outer.len() < 3 {
            return Vec::new();
        }
        let mut rings = vec![outer];
        rings.extend(self.section.holes.iter().filter(|h| h.len() >= 3).map(|h| {
            let mut h = polygon::ccw(h);
            h.reverse();
            h
        }));
        rings
    }

    /// The cap triangles of the section (counter-clockwise in `(v, y)`).
    fn cap_triangles(&self, rings: &[Vec<DVec2>]) -> Vec<[DVec2; 3]> {
        match rings.split_first() {
            Some((outer, holes)) => polygon::triangulate_with_holes(outer, holes),
            None => Vec::new(),
        }
    }

    /// World point of section point `p` on the start (`at_end` false) or
    /// end face, following the slope.
    fn point(&self, at_end: bool, p: DVec2) -> DVec3 {
        self.point_at(if at_end { 1.0 } else { 0.0 }, p)
    }

    /// World point of section point `p` the fraction `t` of the way from the
    /// start face to the end face.
    fn point_at(&self, t: f64, p: DVec2) -> DVec3 {
        let u = self.station_u(t, p.x);
        self.world(u, p.x).extend(self.z_range.0 + self.slope * u + p.y * self.stretch())
    }

    /// Add the body to `s`: the section swept between the start and end
    /// faces, following the slope. Faces are the cap triangles (start, then
    /// end, for each triangle), then one side per edge of each ring (per
    /// chord along an arc beam); sides on the arcs of a curved section, and
    /// the curved sides of an arc beam, are smooth. Nothing when the
    /// section makes no cap triangles.
    pub fn add_to(&self, s: &mut crate::Solid) {
        let rings = self.rings();
        let caps = self.cap_triangles(&rings);
        // A section that makes no cap (degenerate) adds nothing: sides alone
        // would be an open shell.
        if caps.is_empty() {
            return;
        }
        for t in caps {
            s.add_polygons([t.iter().rev().map(|p| self.point(false, *p)).collect::<Vec<_>>()]);
            s.add_polygons([t.iter().map(|p| self.point(true, *p)).collect::<Vec<_>>()]);
        }
        let stations = self.stations();
        let square = self.is_square();
        for ring in &rings {
            let smooth = if self.curved { crate::section::smooth_sides(ring) } else { vec![false; ring.len()] };
            let n = ring.len();
            for (i, flag) in smooth.into_iter().enumerate() {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                // Along an arc only the horizontal sides stay flat.
                let level = (a.y - b.y).abs() < 1e-9;
                let flag = flag || (self.arc.is_some() && !level);
                for k in 0..stations {
                    let (t0, t1) = (k as f64 / stations as f64, (k + 1) as f64 / stations as f64);
                    let quad = [self.point_at(t0, a), self.point_at(t0, b), self.point_at(t1, b), self.point_at(t1, a)];
                    let before = s.faces.len();
                    // Between radial sections a side is a flat trapezoid;
                    // between mitered ends a sloped one is not, and is split.
                    if self.arc.is_none() || square || level || (a.x - b.x).abs() < 1e-9 {
                        s.add_polygons([quad.to_vec()]);
                    } else {
                        s.add_polygons([vec![quad[0], quad[1], quad[2]], vec![quad[0], quad[2], quad[3]]]);
                    }
                    for face in s.faces.iter_mut().skip(before) {
                        face.smooth = flag;
                    }
                }
            }
        }
    }

    /// The start cap in world space: the outline, then the holes (as
    /// [`Self::add_to`] winds them).
    pub fn section_at_start(&self) -> Vec<Vec<DVec3>> {
        self.rings().iter().map(|r| r.iter().map(|p| self.point(false, *p)).collect()).collect()
    }

    /// Whether both end faces are square to the axis.
    pub fn is_square(&self) -> bool {
        (self.start_ext.0 - self.start_ext.1).abs() < 1e-9 && (self.end_ext.0 - self.end_ext.1).abs() < 1e-9
    }

    /// Volume of the body: each cap triangle swept between the end faces,
    /// exact since the faces' offsets are linear across the beam (and, along
    /// an arc, the path of each point grows linearly with its radius: the
    /// arcs as arcs, not as the chords of the solid).
    pub fn volume(&self) -> f64 {
        let k = self.stretch();
        // Length swept by the point `v` across the beam.
        let run = |v: f64| {
            let du = self.face_u(true, v) - self.face_u(false, v);
            match &self.arc {
                Some(a) => du * a.radius_at(v) / a.radius.max(1e-9),
                None => du,
            }
        };
        self.cap_triangles(&self.rings())
            .iter()
            .map(|t| {
                // Exact for a quadratic: the mean over the edge midpoints.
                let mean = (0..3).map(|i| run((t[i].x + t[(i + 1) % 3].x) * 0.5)).sum::<f64>() / 3.0;
                polygon::area(t) * mean * k
            })
            .sum()
    }
}

/// A `width` x `height` box section: centered across, up from 0.
pub(crate) fn box_section(width: f64, height: f64) -> SectionOutline {
    let w = width * 0.5;
    let outer = vec![DVec2::new(-w, 0.0), DVec2::new(w, 0.0), DVec2::new(w, height), DVec2::new(-w, height)];
    SectionOutline { outer, holes: Vec::new() }
}

/// A type's section as a beam sweeps it: centered across on its box, its
/// bottom at 0 (`x` across, `y` up). Returns the section and its half width.
pub(crate) fn beam_section(section: &SectionOutline) -> (SectionOutline, f64) {
    let (lo, hi) = section.bbox();
    let shift = DVec2::new(-(lo.x + hi.x) * 0.5, -lo.y);
    let at = |ring: &[DVec2]| ring.iter().map(|p| *p + shift).collect::<Vec<_>>();
    let placed = SectionOutline { outer: at(&section.outer), holes: section.holes.iter().map(|h| at(h)).collect() };
    let half = (hi.x - lo.x) * 0.5;
    (placed, if half.is_finite() { half.max(0.0) } else { 0.0 })
}

/// A beam as seen by the join resolution.
#[derive(Debug, Clone)]
pub(crate) struct BeamInput {
    pub element: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub half: f64,
    /// Bottom and top elevations at the start.
    pub z_range: (f64, f64),
    pub slope: f64,
    pub section: SectionOutline,
    pub curved: bool,
    pub join_start: bool,
    pub join_end: bool,
    /// Bulge of the centerline: 0 when straight; arc beams and arc strip
    /// footings bend (see [`cc_core::arc`]).
    pub bulge: f64,
}

impl BeamInput {
    /// Lowest and highest elevations along the whole beam.
    fn envelope(&self) -> (f64, f64) {
        let rise = self.slope * self.start.distance(self.end);
        (self.z_range.0 + rise.min(0.0), self.z_range.1 + rise.max(0.0))
    }
}

/// A structural column as seen by the join resolution.
#[derive(Debug, Clone)]
pub(crate) struct ColumnInput {
    pub element: ElementId,
    /// Plan outline: the convex hull of its section, counter-clockwise.
    pub outline: Vec<DVec2>,
    pub z_range: (f64, f64),
}

fn overlap(a: (f64, f64), b: (f64, f64)) -> bool {
    a.0 < b.1 - 1e-6 && b.0 < a.1 - 1e-6
}

/// Distance from `p` to the boundary of `poly`.
fn distance_to_outline(poly: &[DVec2], p: DVec2) -> f64 {
    (0..poly.len())
        .map(|i| {
            let (q, _) = polygon::closest_on_segment(poly[i], poly[(i + 1) % poly.len()], p);
            q.distance(p)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Where the line through `q` along `d` leaves `outline` (the largest exit).
fn exit(outline: &[DVec2], q: DVec2, d: DVec2) -> Option<f64> {
    polygon::clip_line(outline, q, d).into_iter().map(|(_, t1)| t1).reduce(f64::max)
}

/// How far along `arc` from its start (or back from its end) the circle
/// `v` to its left leaves `outline` for the last time, searched over a
/// stretch as long as the outline is wide; `None` when it never enters it.
fn arc_exit(outline: &[DVec2], arc: &Arc, v: f64, from_start: bool) -> Option<f64> {
    const STEPS: usize = 64;
    let lo = outline.iter().fold(DVec2::INFINITY, |m, p| m.min(*p));
    let hi = outline.iter().fold(DVec2::NEG_INFINITY, |m, p| m.max(*p));
    let reach = (hi - lo).length().min(arc.length());
    if !reach.is_finite() || reach <= 0.0 {
        return None;
    }
    let inside = |d: f64| {
        let u = if from_start { d } else { arc.length() - d };
        polygon::contains(outline, arc.at(u, v))
    };
    let step = reach / STEPS as f64;
    let last = (0..=STEPS).rev().find(|&i| inside(step * i as f64))?;
    if last == STEPS {
        return Some(reach);
    }
    let (mut a, mut b) = (step * last as f64, step * (last + 1) as f64);
    for _ in 0..40 {
        let m = (a + b) * 0.5;
        if inside(m) {
            a = m;
        } else {
            b = m;
        }
    }
    Some((a + b) * 0.5)
}

/// The end of `beam` at `p` (ray `da` into the beam) framed into a structural
/// column whose outline holds it: each side of the beam stops where it leaves
/// the column.
fn column_join(
    beam: &BeamInput,
    columns: &[ColumnInput],
    p: DVec2,
    da: DVec2,
    at_start: bool,
) -> Option<(ElementId, EndJoin)> {
    let arc = Arc::from_bulge(beam.start, beam.end, beam.bulge).filter(|a| a.radius > beam.half + 1e-6);
    let length = arc.map_or_else(|| beam.start.distance(beam.end), |a| a.length());
    let na = da.perp();
    columns
        .iter()
        .filter(|c| c.outline.len() >= 3 && overlap(c.z_range, beam.envelope()))
        .filter(|c| polygon::contains(&c.outline, p) || distance_to_outline(&c.outline, p) < COLUMN_TOL)
        .find_map(|c| {
            let (left, right) = match &arc {
                // Along an arc each side leaves the column on its own circle.
                Some(a) => {
                    // Ray-left is the beam's left at the start, its right at the end.
                    let turn = if at_start { 1.0 } else { -1.0 };
                    let side = |off: f64| arc_exit(&c.outline, a, off * turn, at_start);
                    let center = side(0.0)?;
                    (side(beam.half).unwrap_or(center), side(-beam.half).unwrap_or(center))
                }
                None => {
                    let center = exit(&c.outline, p, da)?;
                    let side = |off: f64| exit(&c.outline, p + na * off, da).unwrap_or(center);
                    (side(beam.half), side(-beam.half))
                }
            };
            // A beam that never leaves the column keeps its whole length.
            (left.max(right) < length - 1e-3)
                .then_some((c.element, EndJoin { ray_left: left.max(0.0), ray_right: right.max(0.0), host: None }))
        })
}

/// Resolve the joins of every beam: column faces first, then beam to beam
/// among beams whose depths overlap.
pub(crate) fn resolve_beams(beams: &[BeamInput], columns: &[ColumnInput]) -> Vec<BeamGeom> {
    let ends = |b: &BeamInput| {
        let dir = (b.end - b.start).try_normalize().unwrap_or(DVec2::X);
        let (d0, d1) = match Arc::from_bulge(b.start, b.end, b.bulge) {
            Some(a) => (a.tangent(0.0), a.tangent(a.length())),
            None => (dir, dir),
        };
        [(true, b.join_start, b.start, d0), (false, b.join_end, b.end, -d1)]
    };
    // Ends framed into columns; they take no part in beam joins.
    let framed: Vec<[Option<(ElementId, EndJoin)>; 2]> = beams
        .iter()
        .map(|b| {
            ends(b)
                .map(|(at_start, allowed, p, da)| allowed.then(|| column_join(b, columns, p, da, at_start)).flatten())
        })
        .collect();
    let lines: Vec<WallLine> = beams
        .iter()
        .zip(&framed)
        .map(|(b, f)| WallLine {
            start: b.start,
            end: b.end,
            half: b.half,
            join_start: b.join_start && f[0].is_none(),
            join_end: b.join_end && f[1].is_none(),
            bulge: b.bulge,
            drawn: [b.start, b.end],
        })
        .collect();
    beams
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // The beams this one can meet: those at a common depth.
            let near: Vec<usize> =
                (0..beams.len()).filter(|&j| j == i || overlap(beams[j].envelope(), b.envelope())).collect();
            // Usually every beam is at a common depth: then the lines are used as they are.
            let copied: Vec<WallLine>;
            let (local, me): (&[WallLine], usize) = if near.len() == lines.len() {
                (&lines, i)
            } else {
                copied = near.iter().map(|&j| lines[j]).collect();
                (&copied, near.iter().position(|&j| j == i).unwrap_or(0))
            };
            let resolve = |k: usize, at_start: bool| match framed[i][k] {
                Some((column, join)) => (BeamEnd::Column(column), Some(join)),
                None => match resolve_join(local, me, at_start) {
                    Some(join) => (BeamEnd::Beam, Some(join)),
                    None => (BeamEnd::Free, None),
                },
            };
            let (start, start_join) = resolve(0, true);
            let (end, end_join) = resolve(1, false);
            beam_geom(b, &lines[i], (start, start_join), (end, end_join))
        })
        .collect()
}

fn beam_geom(
    b: &BeamInput,
    line: &WallLine,
    start: (BeamEnd, Option<EndJoin>),
    end: (BeamEnd, Option<EndJoin>),
) -> BeamGeom {
    // An arc too tight for the body's width stays straight, as walls do.
    let arc = line.arc();
    let dir = arc.map_or_else(|| (b.end - b.start).try_normalize().unwrap_or(DVec2::X), |a| a.tangent(0.0));
    let length = arc.map_or_else(|| b.start.distance(b.end), |a| a.length());
    let mut g = BeamGeom {
        element: b.element,
        origin: b.start,
        dir,
        normal: dir.perp(),
        length,
        half: b.half,
        z_range: b.z_range,
        slope: b.slope,
        section: b.section.clone(),
        curved: b.curved,
        start_ext: (0.0, 0.0),
        end_ext: (length, length),
        start: BeamEnd::Free,
        end: BeamEnd::Free,
        arc,
    };
    // At the start the ray runs along +u and ray-left is the beam's left;
    // at the end the ray runs along -u and ray-left is the beam's right.
    let start_ext = start.1.map_or((0.0, 0.0), |j| (j.ray_left, j.ray_right));
    let end_ext = end.1.map_or((length, length), |j| (length - j.ray_right, length - j.ray_left));
    // Joins that would turn the body inside out leave it whole.
    if end_ext.0 - start_ext.0 > 1e-4 && end_ext.1 - start_ext.1 > 1e-4 {
        (g.start_ext, g.end_ext, g.start, g.end) = (start_ext, end_ext, start.0, end.0);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beam(id: u64, a: (f64, f64), b: (f64, f64)) -> BeamInput {
        BeamInput {
            element: ElementId(id),
            start: DVec2::new(a.0, a.1),
            end: DVec2::new(b.0, b.1),
            half: 0.1,
            z_range: (2.6, 3.0),
            slope: 0.0,
            section: box_section(0.2, 0.4),
            curved: false,
            join_start: true,
            join_end: true,
            bulge: 0.0,
        }
    }

    /// A 0.4 m square column centered at `c`, 0 to 3 m.
    fn column(id: u64, c: (f64, f64)) -> ColumnInput {
        let c = DVec2::new(c.0, c.1);
        let outline = [(-0.2, -0.2), (0.2, -0.2), (0.2, 0.2), (-0.2, 0.2)].map(|(x, y)| c + DVec2::new(x, y)).to_vec();
        ColumnInput { element: ElementId(id), outline, z_range: (0.0, 3.0) }
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_degenerate_section_adds_no_faces() {
        let mut b = beam(1, (0.0, 0.0), (5.0, 0.0));
        // Every point on one line: no area, no cap triangles.
        b.section.outer = vec![DVec2::new(-0.1, 0.0), DVec2::new(0.0, 0.0), DVec2::new(0.1, 0.0)];
        let g = &resolve_beams(&[b], &[])[0];
        let mut s = crate::Solid::new(ElementId(1), cc_core::Category::Beam, [0; 3]);
        g.add_to(&mut s);
        assert!(s.faces.is_empty(), "{} faces", s.faces.len());
    }

    #[test]
    fn beam_between_columns_stops_at_their_faces() {
        let g =
            &resolve_beams(&[beam(1, (0.0, 0.0), (5.0, 0.0))], &[column(10, (0.0, 0.0)), column(11, (5.0, 0.0))])[0];
        assert!(close(g.start_ext, (0.2, 0.2)) && close(g.end_ext, (4.8, 4.8)), "{g:?}");
        assert_eq!((g.start, g.end), (BeamEnd::Column(ElementId(10)), BeamEnd::Column(ElementId(11))));
        assert!((g.cut_length() - 4.6).abs() < 1e-9);
        assert!((g.volume() - 4.6 * 0.2 * 0.4).abs() < 1e-9);
    }

    #[test]
    fn a_rotated_column_cuts_the_beam_along_its_face() {
        let mut c = column(10, (0.0, 0.0));
        let a = 10f64.to_radians();
        let rot = DVec2::from_angle(a);
        c.outline = c.outline.iter().map(|p| rot.rotate(*p)).collect();
        let g = &resolve_beams(&[beam(1, (0.0, 0.0), (5.0, 0.0))], &[c])[0];
        // The face x·cos a + y·sin a = 0.2 crosses y = ±0.1 at different x.
        let x = |y: f64| (0.2 - y * a.sin()) / a.cos();
        assert!(close(g.start_ext, (x(0.1), x(-0.1))), "{:?}", g.start_ext);
    }

    #[test]
    fn columns_frame_beams_only_where_they_overlap_in_height() {
        let mut c = column(10, (0.0, 0.0));
        c.z_range = (0.0, 2.6); // The beam sits on top of the column.
        let g = &resolve_beams(&[beam(1, (0.0, 0.0), (5.0, 0.0))], &[c])[0];
        assert_eq!(g.start, BeamEnd::Free);
        assert!(close(g.start_ext, (0.0, 0.0)));
    }

    #[test]
    fn beams_meeting_at_a_corner_are_mitered() {
        let beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        let g = resolve_beams(&beams, &[]);
        assert_eq!(g[0].end, BeamEnd::Beam);
        assert!(close(g[0].end_ext, (3.9, 4.1)), "{:?}", g[0].end_ext);
        assert!(close(g[1].start_ext, (0.1, -0.1)), "{:?}", g[1].start_ext);
        // The two bodies tile the corner: 4.0 + 3.0 m of axis at 0.2 m wide.
        let area: f64 = g.iter().map(|b| polygon::area(&b.footprint())).sum();
        assert!((area - 7.0 * 0.2).abs() < 1e-9, "{area}");
    }

    #[test]
    fn a_beam_ending_on_another_stops_at_its_side() {
        let beams = [beam(1, (0.0, 0.0), (6.0, 0.0)), beam(2, (3.0, 0.0), (3.0, 4.0))];
        let g = resolve_beams(&beams, &[]);
        assert!(close(g[1].start_ext, (0.1, 0.1)), "{:?}", g[1].start_ext);
        assert_eq!((g[0].start, g[0].end), (BeamEnd::Free, BeamEnd::Free));
    }

    #[test]
    fn beams_at_different_depths_do_not_join() {
        let mut beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        beams[1].z_range = (1.0, 1.4);
        let g = resolve_beams(&beams, &[]);
        assert_eq!((g[0].end, g[1].start), (BeamEnd::Free, BeamEnd::Free));
    }

    #[test]
    fn beams_meeting_at_a_column_frame_into_it_instead_of_each_other() {
        let beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        let g = resolve_beams(&beams, &[column(10, (4.0, 0.0))]);
        assert!(close(g[0].end_ext, (3.8, 3.8)) && close(g[1].start_ext, (0.2, 0.2)), "{g:?}");
    }

    #[test]
    fn disallowed_ends_stay_square() {
        let mut beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        beams[0].join_start = false;
        beams[0].join_end = false;
        let g = resolve_beams(&beams, &[column(10, (0.0, 0.0))]);
        assert!(close(g[0].start_ext, (0.0, 0.0)) && close(g[0].end_ext, (4.0, 4.0)));
        // The other beam no longer miters against it.
        assert_eq!(g[1].start, BeamEnd::Free);
    }

    #[test]
    fn an_arc_strip_meets_a_straight_one_at_shared_corners() {
        // A straight strip along +X ending where a quarter circle (center at the origin) starts.
        let mut arc = beam(2, (4.0, 0.0), (0.0, 4.0));
        arc.bulge = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
        let g = resolve_beams(&[beam(1, (0.0, 0.0), (4.0, 0.0)), arc], &[]);
        assert!(g[0].arc.is_none());
        let a = g[1].arc.unwrap();
        assert!((a.radius - 4.0).abs() < 1e-9 && (g[1].length - a.length()).abs() < 1e-12);
        assert_eq!((g[0].end, g[1].start), (BeamEnd::Beam, BeamEnd::Beam));
        // The straight strip's end corners lie on the arc strip's faces at its start.
        let inner = DVec2::new((3.9f64 * 3.9 - 0.01).sqrt(), 0.1);
        let outer = DVec2::new((4.1f64 * 4.1 - 0.01).sqrt(), -0.1);
        let fa = g[0].footprint();
        assert!(fa.iter().any(|p| p.distance(inner) < 1e-9) && fa.iter().any(|p| p.distance(outer) < 1e-9), "{fa:?}");
        let (s, e) = (g[1].start_ext, g[1].end_ext);
        assert!(g[1].world(s.0, 0.1).distance(inner) < 1e-9, "{:?}", g[1].world(s.0, 0.1));
        assert!(g[1].world(s.1, -0.1).distance(outer) < 1e-9, "{:?}", g[1].world(s.1, -0.1));
        // The free end stays square.
        assert!(close(e, (a.length(), a.length())));
    }

    /// A level beam from `a` to `b` with square ends and `section` (as a
    /// type section: centered), shifted up so its bottom is at 0.
    fn beam_with_section(section: SectionOutline, a: DVec2, b: DVec2) -> BeamGeom {
        let input = section_input(section, a, b);
        resolve_beams(&[BeamInput { join_start: false, join_end: false, ..input }], &[]).remove(0)
    }

    fn section_input(section: SectionOutline, a: DVec2, b: DVec2) -> BeamInput {
        let (section, half) = beam_section(&section);
        let height = section.bbox().1.y;
        BeamInput {
            element: ElementId(1),
            start: a,
            end: b,
            half,
            z_range: (1.0, 1.0 + height),
            slope: 0.0,
            section,
            curved: true,
            join_start: true,
            join_end: true,
            bulge: 0.0,
        }
    }

    /// Every directed edge of `s` is matched by the reverse edge of another face.
    fn is_closed(s: &crate::Solid) -> bool {
        let key = |p: glam::DVec3| ((p.x * 1e7).round() as i64, (p.y * 1e7).round() as i64, (p.z * 1e7).round() as i64);
        let mut edges: std::collections::HashMap<_, i32> = std::collections::HashMap::new();
        for f in &s.faces {
            let n = f.verts.len();
            for i in 0..n {
                let (a, b) = (key(f.verts[i]), key(f.verts[(i + 1) % n]));
                *edges.entry((a, b)).or_default() += 1;
                *edges.entry((b, a)).or_default() -= 1;
            }
        }
        !s.faces.is_empty() && edges.values().all(|c| *c == 0)
    }

    fn solid() -> crate::Solid {
        crate::Solid::new(ElementId(1), cc_core::Category::Beam, [0; 3])
    }

    fn w_section() -> SectionOutline {
        cc_core::ProfileShape::I { d: 0.3, bf: 0.15, tw: 0.0071, tf: 0.0107, r: 0.0 }.outline().unwrap()
    }

    #[test]
    fn general_section_volume_is_area_times_length() {
        let i = w_section();
        let area = i.properties(0.0).area;
        let g = beam_with_section(i, DVec2::ZERO, DVec2::new(5.0, 0.0));
        assert!((g.volume() - area * 5.0).abs() < 1e-9, "{} vs {}", g.volume(), area * 5.0);
        let mut s = solid();
        g.add_to(&mut s);
        assert!((s.volume() - area * 5.0).abs() / (area * 5.0) < 1e-6, "{}", s.volume());
        assert!(is_closed(&s));
        assert!(s.faces.iter().all(|f| !f.smooth), "a plain I has no arcs");
        // The bottom flange rests on the beam's bottom elevation.
        let (lo, hi) = s.bounds().unwrap();
        assert!((lo.z - 1.0).abs() < 1e-12 && (hi.z - 1.3).abs() < 1e-12, "{lo} {hi}");
        // The start cap in world space: the outline at x = 0.
        let caps = g.section_at_start();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].len(), 12);
        assert!(caps[0].iter().all(|p| p.x.abs() < 1e-12));
    }

    #[test]
    fn round_tube_beam_is_watertight() {
        let t = cc_core::ProfileShape::RoundTube { d: 0.1, t: 0.005 }.outline().unwrap();
        let area = t.properties(0.0).area;
        let g = beam_with_section(t, DVec2::ZERO, DVec2::new(3.0, 4.0));
        let mut s = solid();
        g.add_to(&mut s);
        assert!(is_closed(&s), "every edge shared by two faces");
        assert!((s.volume() - area * 5.0).abs() / (area * 5.0) < 1e-6);
        assert!(s.faces.iter().any(|f| f.smooth), "the tube's facets are smooth");
        assert_eq!(g.section_at_start().len(), 2, "outside and hole");
    }

    #[test]
    fn sloped_profile_beams_rise_with_their_axis() {
        let i = w_section();
        let area = i.properties(0.0).area;
        let mut input = section_input(i, DVec2::ZERO, DVec2::new(4.0, 0.0));
        input.slope = 0.75;
        let k = 1.25;
        input.z_range = (1.0, 1.0 + 0.3 * k);
        let g = resolve_beams(&[input], &[]).remove(0);
        let mut s = solid();
        g.add_to(&mut s);
        assert!(is_closed(&s));
        // Square to the plan axis, the vertical depth stretched by k.
        assert!((g.volume() - area * 4.0 * k).abs() < 1e-9);
        assert!((s.volume() - g.volume()).abs() < 1e-9);
        let (_, hi) = s.bounds().unwrap();
        assert!((hi.z - (1.0 + 0.3 * k + 3.0)).abs() < 1e-12, "{hi}");
    }

    #[test]
    fn mitered_profile_beams_tile_the_corner() {
        let i = w_section();
        let area = i.properties(0.0).area;
        let beams = [
            section_input(i.clone(), DVec2::ZERO, DVec2::new(4.0, 0.0)),
            section_input(i, DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ];
        let g = resolve_beams(&beams, &[]);
        assert!(!g[0].is_square() && !g[1].is_square());
        for b in &g {
            let mut s = solid();
            b.add_to(&mut s);
            assert!(is_closed(&s));
            assert!((s.volume() - b.volume()).abs() < 1e-9, "{} vs {}", s.volume(), b.volume());
        }
        // A symmetric I mitered on its axis keeps exactly area x axis length.
        let total: f64 = g.iter().map(|b| b.volume()).sum();
        assert!((total - area * 7.0).abs() < 1e-9, "{total}");
    }

    /// A half circle of radius 3 around the origin, from +X to -X.
    fn half_ring(section: SectionOutline) -> BeamInput {
        BeamInput { bulge: 1.0, ..section_input(section, DVec2::new(3.0, 0.0), DVec2::new(-3.0, 0.0)) }
    }

    #[test]
    fn an_arc_beam_of_a_profile_is_a_closed_body_of_the_exact_volume() {
        let i = w_section();
        let area = i.properties(0.0).area;
        let g = resolve_beams(&[half_ring(i)], &[]).remove(0);
        let arc = g.arc.expect("an arc");
        assert!((arc.radius - 3.0).abs() < 1e-9 && (g.length - 3.0 * std::f64::consts::PI).abs() < 1e-9);
        // A symmetric section turned about the center: its area times the axis (Pappus).
        let exact = area * 3.0 * std::f64::consts::PI;
        assert!((g.volume() - exact).abs() < 1e-9, "{} vs {exact}", g.volume());
        let mut s = solid();
        g.add_to(&mut s);
        assert!(is_closed(&s));
        assert!(s.volume() < exact && exact - s.volume() < exact * 1e-3, "{}", s.volume());
        // The web's and the flange edges' faces are curved; the flanges' flat ones are not.
        assert!(s.faces.iter().any(|f| f.smooth) && s.faces.iter().any(|f| !f.smooth));
        let (lo, hi) = s.bounds().unwrap();
        assert!((lo.z - 1.0).abs() < 1e-12 && (hi.z - 1.3).abs() < 1e-12, "{lo} {hi}");
        // The plan outline follows both faces of the half ring.
        let outline = g.outline();
        assert!(outline.len() > 100);
        assert!(outline.iter().all(|p| ((p.length() - 3.0).abs() - 0.075).abs() < 1e-9), "on the two faces");
        assert!(polygon::area(&outline) > 0.0, "counter-clockwise");
        // The tangent turns with the arc.
        assert!(g.dir_at(0.0).distance(DVec2::Y) < 1e-9 && g.dir_at(g.length).distance(-DVec2::Y) < 1e-9);
        assert!((g.locate(DVec2::new(0.0, 3.0)) - g.length / 2.0).abs() < 1e-9);
    }

    #[test]
    fn an_arc_beam_stops_at_the_faces_of_the_columns_at_its_ends() {
        // A quarter circle of radius 4 between columns on the axes.
        let mut b = beam(1, (4.0, 0.0), (0.0, 4.0));
        b.bulge = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
        let g = &resolve_beams(&[b], &[column(10, (4.0, 0.0)), column(11, (0.0, 4.0))])[0];
        assert_eq!((g.start, g.end), (BeamEnd::Column(ElementId(10)), BeamEnd::Column(ElementId(11))));
        // Each side leaves the first column through its face y = 0.2, on its own circle.
        for (v, u) in [(0.1, g.start_ext.0), (-0.1, g.start_ext.1)] {
            let p = g.world(u, v);
            assert!((p.y - 0.2).abs() < 1e-9 && (p.length() - (4.0 - v)).abs() < 1e-9, "{p}");
        }
        // And the second through x = 0.2.
        for (v, u) in [(0.1, g.end_ext.0), (-0.1, g.end_ext.1)] {
            let p = g.world(u, v);
            assert!((p.x - 0.2).abs() < 1e-9 && (p.length() - (4.0 - v)).abs() < 1e-9, "{p}");
        }
        // The body between them: exact, and its mitered solid closed.
        let ring = |r: f64| r * ((0.2f64 / r).asin());
        let swept = |r: f64| r * std::f64::consts::FRAC_PI_2 - 2.0 * ring(r);
        let mut s = solid();
        g.add_to(&mut s);
        assert!(is_closed(&s));
        assert!(
            g.volume() > 0.4 * 0.2 * swept(4.1).min(swept(3.9)) && g.volume() < 0.4 * 0.2 * swept(4.1).max(swept(3.9))
        );
        assert!((s.volume() - g.volume()).abs() < g.volume() * 1e-3);
    }

    #[test]
    fn a_beam_inside_a_column_keeps_its_length() {
        let g = &resolve_beams(&[beam(1, (-0.1, 0.0), (0.1, 0.0))], &[column(10, (0.0, 0.0))])[0];
        assert!(close(g.start_ext, (0.0, 0.0)) && close(g.end_ext, (0.2, 0.2)), "{g:?}");
    }
}
