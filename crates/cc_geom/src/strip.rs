//! Arc strip footings: the body of a strip (a ground beam) that follows an
//! arc, a band between concentric arcs either side of its centerline, its
//! ends cut where it meets other strips. Plans draw it as short chords of
//! the arcs; its area and volume are exact.

use crate::{Face, Solid};
use cc_core::arc::{ARC_STEP, Arc, segments};
use glam::{DVec2, DVec3};

/// The plan of an arc strip: `half` either side of the centerline `arc`,
/// in strip-local `(u, v)` (along the arc from its start, and to its left);
/// the left face spans `left` and the right face `right` along the arc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripArc {
    pub arc: Arc,
    pub half: f64,
    pub left: (f64, f64),
    pub right: (f64, f64),
}

impl StripArc {
    /// World plan position of strip-local `(u, v)`.
    pub fn world(&self, u: f64, v: f64) -> DVec2 {
        self.arc.at(u, v)
    }

    /// Number of chords along each face.
    fn segments(&self) -> usize {
        let du = (self.left.1 - self.left.0).abs().max((self.right.1 - self.right.0).abs());
        segments(du / self.arc.radius.max(1e-9), ARC_STEP)
    }

    /// The left and right faces as matching runs of points from the start to the end.
    fn faces(&self) -> (Vec<DVec2>, Vec<DVec2>) {
        let n = self.segments();
        let at = |range: (f64, f64), i: usize| range.0 + (range.1 - range.0) * i as f64 / n as f64;
        let left = (0..=n).map(|i| self.world(at(self.left, i), self.half)).collect();
        let right = (0..=n).map(|i| self.world(at(self.right, i), -self.half)).collect();
        (left, right)
    }

    /// Plan outline, counter-clockwise: the arcs as short chords.
    pub fn outline(&self) -> Vec<DVec2> {
        let (left, mut pts) = self.faces();
        pts.extend(left.into_iter().rev());
        pts
    }

    /// Exact plan area (the arcs as arcs).
    pub fn area(&self) -> f64 {
        crate::wall::arc_band_area(&self.arc, self.half, self.left, self.right)
    }

    /// Add the body between `z0` and `z1` to `s`: the curved faces as smooth facets.
    pub(crate) fn add_to(&self, s: &mut Solid, z0: f64, z1: f64) {
        let (left, right) = self.faces();
        let lift = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        let flat = |v: DVec2| DVec3::new(v.x, v.y, 0.0);
        let side = |a: DVec2, b: DVec2, n: DVec3| {
            let mut f = Face::new(vec![lift(a, z0), lift(b, z0), lift(b, z1), lift(a, z1)], n);
            f.smooth = true;
            f
        };
        let n = left.len().min(right.len()).saturating_sub(1);
        for i in 0..n {
            let (r0, r1, l1, l0) = (right[i], right[i + 1], left[i + 1], left[i]);
            let poly = [r0, r1, l1, l0];
            s.faces.push(Face::new(poly.iter().map(|p| lift(*p, z1)).collect(), DVec3::Z));
            s.faces.push(Face::new(poly.iter().rev().map(|p| lift(*p, z0)).collect(), -DVec3::Z));
            // Outward: away from the centerline on each side.
            let mid = |a: DVec2, b: DVec2| (a + b) * 0.5;
            let out = |p: DVec2| {
                let d = p - self.arc.center;
                d.try_normalize().unwrap_or(DVec2::X)
            };
            let (rm, lm) = (mid(r0, r1), mid(l0, l1));
            let (rn, ln) = (out(rm), out(lm));
            // The right face is the outer one of a counter-clockwise arc.
            let sign = self.arc.sign();
            s.faces.push(side(r0, r1, flat(rn * sign)));
            s.faces.push(side(l1, l0, flat(-ln * sign)));
        }
        // End caps, square to the line between their corners, facing out of the strip.
        let cap_normal = |l: DVec2, r: DVec2, outward: DVec2| {
            let n = (l - r).perp().try_normalize().unwrap_or(outward);
            flat(if n.dot(outward) < 0.0 { -n } else { n })
        };
        if let (Some(&l), Some(&r)) = (left.first(), right.first()) {
            let n = cap_normal(l, r, -self.arc.tangent((self.left.0 + self.right.0) * 0.5));
            s.faces.push(Face::new(vec![lift(l, z0), lift(r, z0), lift(r, z1), lift(l, z1)], n));
        }
        if let (Some(&l), Some(&r)) = (left.last(), right.last()) {
            let n = cap_normal(l, r, self.arc.tangent((self.left.1 + self.right.1) * 0.5));
            s.faces.push(Face::new(vec![lift(r, z0), lift(l, z0), lift(l, z1), lift(r, z1)], n));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn a_quarter_ring_has_the_annulus_area_and_volume() {
        // Centerline radius 4 from (4, 0) to (0, 4), 0.4 wide.
        let arc = Arc::from_bulge(DVec2::new(4.0, 0.0), DVec2::new(0.0, 4.0), (FRAC_PI_2 / 4.0).tan()).unwrap();
        let len = arc.length();
        let strip = StripArc { arc, half: 0.2, left: (0.0, len), right: (0.0, len) };
        let exact = FRAC_PI_2 / 2.0 * (4.2f64.powi(2) - 3.8f64.powi(2));
        assert!((strip.area() - exact).abs() < 1e-9, "{}", strip.area());
        // The chords lie inside the arcs, a little short of the exact area.
        let chords = crate::polygon::area(&strip.outline());
        assert!(chords < exact && exact - chords < 1e-2, "{chords}");
        let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Foundation, [0; 3]);
        strip.add_to(&mut s, -0.4, 0.0);
        assert!(s.faces.iter().filter(|f| f.smooth).count() >= 2);
        // A closed body: its mesh volume is the chorded prism.
        assert!((s.volume() - chords * 0.4).abs() < 1e-9, "{}", s.volume());
        // The outer face of a counter-clockwise arc faces away from the center.
        for f in s.faces.iter().filter(|f| f.smooth) {
            let c: DVec3 = f.verts.iter().copied().sum::<DVec3>() / f.verts.len() as f64;
            let r = DVec2::new(c.x, c.y).length();
            let outward = DVec2::new(c.x, c.y).dot(DVec2::new(f.normal.x, f.normal.y)) > 0.0;
            assert_eq!(outward, r > 4.0, "{c:?} {:?}", f.normal);
        }
    }
}
