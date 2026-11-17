//! Piles and pile caps (see [`cc_core::pile`]): their bodies, plans and
//! exact quantities. A round pile is a cylinder of smooth facets, a square
//! one a prism; a cap is a prism of its outline, a round cap a cylinder and
//! a ring cap an annulus, their curved faces smooth. Volumes and areas are
//! exact (circles as circles, not chords).

use crate::{Face, Solid};
use cc_core::arc::{ARC_STEP, segments};
use cc_core::pile::{CapOutline, PileSection};
use cc_core::{ElementId, Project};
use glam::{DVec2, DVec3};
use std::f64::consts::TAU;

/// Number of chords of a full circle.
pub fn circle_segments() -> usize {
    segments(TAU, ARC_STEP)
}

/// A pile in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct PileGeom {
    pub element: ElementId,
    /// Axis in plan.
    pub position: DVec2,
    /// Rotation of a square section, degrees.
    pub rotation_deg: f64,
    pub section: PileSection,
    /// Elevations of the toe and the head (cut-off).
    pub z_range: (f64, f64),
    /// The pile cap it belongs to.
    pub cap: Option<ElementId>,
}

impl PileGeom {
    /// Length from the toe to the head.
    pub fn length(&self) -> f64 {
        self.z_range.1 - self.z_range.0
    }

    /// Exact volume.
    pub fn volume(&self) -> f64 {
        self.section.area() * self.length()
    }

    /// Plan outline, counter-clockwise (a circle as chords).
    pub fn outline(&self) -> Vec<DVec2> {
        match self.section {
            PileSection::Round { diameter } => circle(self.position, diameter / 2.0, circle_segments()),
            PileSection::Square { side } => {
                let (u, h) = (DVec2::from_angle(self.rotation_deg.to_radians()), side / 2.0);
                [(-h, -h), (h, -h), (h, h), (-h, h)].map(|(a, b)| self.position + u * a + u.perp() * b).to_vec()
            }
        }
    }

    /// Add the body to `s`.
    pub fn add_to(&self, s: &mut Solid) {
        let (z0, z1) = self.z_range;
        match self.section {
            PileSection::Round { diameter } => add_cylinder(s, self.position, diameter / 2.0, z0, z1),
            PileSection::Square { .. } => s.add_prism(&self.outline(), |_| z0, |_| z1),
        }
    }
}

/// The pile `e` in the world (`None` when its type is no pile type or it has no length).
pub fn pile_geom(project: &Project, e: &cc_core::Element, p: &cc_core::pile::Pile) -> Option<PileGeom> {
    let cc_core::TypeData::Pile { section, .. } = project.element_type(e.type_id)?.data else { return None };
    let level = project.level_elevation(e.level);
    let z_range = (level + p.toe, level + p.cut_off);
    let ok = section.is_valid() && p.is_valid() && z_range.1 - z_range.0 > 1e-6;
    ok.then_some(PileGeom {
        element: e.id,
        position: p.position,
        rotation_deg: p.rotation_deg,
        section,
        z_range,
        cap: p.cap.map(|c| c.cap),
    })
}

/// A pile cap's frame and plan, for its geometry and reinforcement.
#[derive(Debug, Clone, PartialEq)]
pub struct CapGeom {
    /// Origin of the cap's frame in plan.
    pub position: DVec2,
    /// Direction of the frame's `u` axis, degrees counter-clockwise (`v` to its left).
    pub rotation_deg: f64,
    /// The plan in the cap's frame.
    pub outline: CapOutline,
    /// The piles' axes in the cap's frame, in layout order.
    pub piles: Vec<DVec2>,
}

impl CapGeom {
    /// A point of the cap's frame in plan.
    pub fn world(&self, local: DVec2) -> DVec2 {
        self.position + DVec2::from_angle(self.rotation_deg.to_radians()).rotate(local)
    }

    /// The outer boundary and the holes in plan, counter-clockwise (circles as chords).
    pub fn loops(&self) -> Vec<Vec<DVec2>> {
        self.outline
            .loops(circle_segments())
            .into_iter()
            .map(|l| l.into_iter().map(|p| self.world(p)).collect())
            .collect()
    }

    /// Add the body between `z0` and `z1` to `s`: curved faces as smooth facets.
    pub fn add_to(&self, s: &mut Solid, z0: f64, z1: f64) {
        match self.outline {
            CapOutline::Polygon(_) => {
                if let Some(outer) = self.loops().first() {
                    s.add_prism(outer, |_| z0, |_| z1);
                }
            }
            CapOutline::Disk { radius } => add_cylinder(s, self.position, radius, z0, z1),
            CapOutline::Ring { inner, outer } => add_ring(s, self.position, inner, outer, z0, z1),
        }
    }
}

/// The cap `e` in the world (`None` when it is no pile cap).
pub fn cap_geom(project: &Project, e: &cc_core::Element) -> Option<CapGeom> {
    let c = project.pile_cap(e)?;
    Some(CapGeom {
        position: c.position,
        rotation_deg: c.rotation_deg,
        outline: c.cap.outline(),
        piles: c.cap.positions(),
    })
}

/// The circle of `radius` around `center` as `n` points, counter-clockwise from +X.
fn circle(center: DVec2, radius: f64, n: usize) -> Vec<DVec2> {
    (0..n).map(|i| center + DVec2::from_angle(i as f64 / n as f64 * TAU) * radius).collect()
}

fn lift(p: DVec2, z: f64) -> DVec3 {
    DVec3::new(p.x, p.y, z)
}

/// A side facet between `a` and `b` (in plan) from `z0` to `z1`, facing `out`.
fn side(a: DVec2, b: DVec2, z0: f64, z1: f64, out: DVec2) -> Face {
    let mut f = Face::new(vec![lift(a, z0), lift(b, z0), lift(b, z1), lift(a, z1)], out.extend(0.0));
    f.smooth = true;
    f
}

/// A vertical cylinder: flat top and bottom, its side as smooth facets.
pub(crate) fn add_cylinder(s: &mut Solid, center: DVec2, radius: f64, z0: f64, z1: f64) {
    if !(radius > 1e-9 && z1 - z0 > 1e-9 && center.is_finite()) {
        return;
    }
    let ring = circle(center, radius, circle_segments());
    let n = ring.len();
    s.faces.push(Face::new(ring.iter().map(|p| lift(*p, z1)).collect(), DVec3::Z));
    s.faces.push(Face::new(ring.iter().rev().map(|p| lift(*p, z0)).collect(), -DVec3::Z));
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let out = ((a + b) * 0.5 - center).normalize_or_zero();
        s.faces.push(side(a, b, z0, z1, out));
    }
}

/// A vertical annulus: flat top and bottom (one quad per chord), its inner
/// and outer sides as smooth facets.
pub(crate) fn add_ring(s: &mut Solid, center: DVec2, inner: f64, outer: f64, z0: f64, z1: f64) {
    if !(inner > 1e-9 && outer - inner > 1e-9 && z1 - z0 > 1e-9 && center.is_finite()) {
        return;
    }
    let n = circle_segments();
    let (big, small) = (circle(center, outer, n), circle(center, inner, n));
    for i in 0..n {
        let j = (i + 1) % n;
        let quad = [small[i], big[i], big[j], small[j]];
        s.faces.push(Face::new(quad.iter().map(|p| lift(*p, z1)).collect(), DVec3::Z));
        s.faces.push(Face::new(quad.iter().rev().map(|p| lift(*p, z0)).collect(), -DVec3::Z));
        let out = ((big[i] + big[j]) * 0.5 - center).normalize_or_zero();
        s.faces.push(side(big[i], big[j], z0, z1, out));
        s.faces.push(side(small[j], small[i], z0, z1, -out));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Category;

    fn closed(s: &Solid) -> bool {
        // Every edge is shared by exactly two faces, in opposite directions.
        let key = |p: DVec3| ((p.x * 1e9).round() as i64, (p.y * 1e9).round() as i64, (p.z * 1e9).round() as i64);
        let mut edges = std::collections::HashMap::new();
        for f in &s.faces {
            let n = f.verts.len();
            for i in 0..n {
                *edges.entry((key(f.verts[i]), key(f.verts[(i + 1) % n]))).or_insert(0) += 1;
            }
        }
        edges.iter().all(|((a, b), k)| *k == 1 && edges.get(&(*b, *a)) == Some(&1))
    }

    #[test]
    fn cylinders_and_rings_are_closed_and_smooth() {
        let mut s = Solid::new(ElementId(1), Category::Pile, [0; 3]);
        add_cylinder(&mut s, DVec2::new(1.0, 2.0), 0.2, -10.0, 0.0);
        assert!(closed(&s));
        assert!(s.faces.iter().filter(|f| f.smooth).count() == circle_segments());
        // The chorded volume falls a little short of the exact one.
        let exact = std::f64::consts::PI * 0.04 * 10.0;
        assert!(s.volume() < exact && exact - s.volume() < 1e-3 * exact);
        let mut r = Solid::new(ElementId(1), Category::Foundation, [0; 3]);
        add_ring(&mut r, DVec2::ZERO, 1.6, 2.4, 0.0, 1.0);
        assert!(closed(&r));
        let exact = std::f64::consts::PI * (2.4f64.powi(2) - 1.6f64.powi(2));
        assert!((r.volume() - exact).abs() < 2e-3 * exact);
    }
}
