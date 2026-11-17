//! Coordinate systems and curve tessellation for flattening drawings.

use glam::{DAffine3, DMat2, DVec2, DVec3};
use std::f64::consts::TAU;

/// Most segments one curve (ellipse, spline) is tessellated into.
pub const MAX_CURVE_SEGMENTS: usize = 4096;

/// The object coordinate system of an extrusion direction (the DXF
/// "arbitrary axis algorithm"): its X and Y axes in world coordinates.
pub fn ocs_axes(normal: DVec3) -> (DVec3, DVec3, DVec3) {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let limit = 1.0 / 64.0;
    let ax = if n.x.abs() < limit && n.y.abs() < limit { DVec3::Y.cross(n) } else { DVec3::Z.cross(n) };
    let ax = ax.try_normalize().unwrap_or(DVec3::X);
    let ay = n.cross(ax).try_normalize().unwrap_or(DVec3::Y);
    (ax, ay, n)
}

/// The transform from an OCS (of `normal`) to world coordinates.
pub fn ocs(normal: DVec3) -> DAffine3 {
    let (ax, ay, n) = ocs_axes(normal);
    DAffine3::from_cols(ax, ay, n, DVec3::ZERO)
}

/// A 2D affine map `p ↦ m·p + t` (the plan projection of a 3D transform of a plane).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2 {
    pub m: DMat2,
    pub t: DVec2,
}

impl Affine2 {
    /// Points `(x, y)` of the plane `z = elevation` of the frame `t`, projected on the world XY plane.
    pub fn of_plane(t: &DAffine3, elevation: f64) -> Self {
        let x = t.matrix3.x_axis;
        let y = t.matrix3.y_axis;
        let origin = t.transform_point3(DVec3::new(0.0, 0.0, elevation));
        Self { m: DMat2::from_cols(DVec2::new(x.x, x.y), DVec2::new(y.x, y.y)), t: DVec2::new(origin.x, origin.y) }
    }

    pub fn apply(&self, p: DVec2) -> DVec2 {
        self.m * p + self.t
    }

    pub fn vector(&self, v: DVec2) -> DVec2 {
        self.m * v
    }

    pub fn det(&self) -> f64 {
        self.m.determinant()
    }

    /// The uniform scale when the map is a similarity (rotation, uniform
    /// scale, maybe a mirror), else `None`.
    pub fn similarity(&self) -> Option<f64> {
        let a = self.m.x_axis;
        let b = self.m.y_axis;
        let (la, lb) = (a.length(), b.length());
        if !(la.is_finite() && lb.is_finite()) || la < 1e-15 {
            return None;
        }
        let tol = 1e-6 * la.max(lb);
        ((la - lb).abs() <= tol && a.dot(b).abs() <= tol * la).then_some(la)
    }
}

/// Number of segments for `sweep` radians, about 5° each, within bounds.
pub fn arc_segments(sweep: f64) -> usize {
    ((sweep.abs() / (5f64.to_radians())).ceil() as usize).clamp(2, 720)
}

/// Points of a circular arc.
pub fn arc_points(center: DVec2, radius: f64, start: f64, sweep: f64) -> Vec<DVec2> {
    let n = arc_segments(sweep);
    (0..=n).map(|i| center + DVec2::from_angle(start + sweep * i as f64 / n as f64) * radius).collect()
}

/// The arc of a polyline segment from `a` to `b` with `bulge`: center,
/// radius, start angle and sweep; `None` for a straight segment.
pub fn bulge_arc(a: DVec2, b: DVec2, bulge: f64) -> Option<(DVec2, f64, f64, f64)> {
    if !bulge.is_finite() || bulge.abs() < 1e-9 {
        return None;
    }
    let chord = a.distance(b);
    if chord < 1e-12 {
        return None;
    }
    let sweep = 4.0 * bulge.atan();
    let n = (b - a) / chord;
    let center = (a + b) * 0.5 + n.perp() * (chord / (2.0 * (sweep * 0.5).tan()));
    let radius = center.distance(a);
    let d = a - center;
    (center.is_finite() && radius.is_finite()).then_some((center, radius, d.y.atan2(d.x), sweep))
}

/// Points of an elliptical arc (world coordinates) from parameter `t0` to
/// `t1`, counter-clockwise about `normal`.
pub fn ellipse_points(center: DVec3, major: DVec3, ratio: f64, normal: DVec3, t0: f64, t1: f64) -> Vec<DVec3> {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let minor = n.cross(major) * ratio;
    let mut sweep = t1 - t0;
    if !sweep.is_finite() {
        return Vec::new();
    }
    sweep = sweep.rem_euclid(TAU);
    if sweep < 1e-9 {
        sweep = TAU;
    }
    let segs = arc_segments(sweep).min(MAX_CURVE_SEGMENTS);
    (0..=segs)
        .map(|i| {
            let t = t0 + sweep * i as f64 / segs as f64;
            center + major * t.cos() + minor * t.sin()
        })
        .collect()
}

/// Points along a NURBS curve of `degree` with `knots` and `control` points
/// (weights optional), by de Boor's algorithm; `None` when the data is
/// inconsistent.
pub fn spline_points(degree: usize, knots: &[f64], control: &[DVec3], weights: &[f64]) -> Option<Vec<DVec3>> {
    let n = control.len();
    let p = degree;
    if p == 0 || p > 11 || n <= p || knots.len() != n + p + 1 {
        return None;
    }
    if knots.iter().any(|k| !k.is_finite()) || knots.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    let w = |i: usize| weights.get(i).copied().filter(|w| w.is_finite() && *w > 0.0).unwrap_or(1.0);
    // Homogeneous control points.
    let hom: Vec<(DVec3, f64)> = control.iter().enumerate().map(|(i, c)| (*c * w(i), w(i))).collect();
    let (u0, u1) = (*knots.get(p)?, *knots.get(n)?);
    if u1 - u0 <= 1e-12 {
        return None;
    }
    let segs = (n * 8).clamp(8, MAX_CURVE_SEGMENTS);
    let mut out = Vec::with_capacity(segs + 1);
    // The knot span: knots[k] <= u < knots[k + 1], within p..n-1; `u` only
    // grows, so the search goes on from the last span (linear in all).
    let mut k = p;
    for s in 0..=segs {
        let u = (u0 + (u1 - u0) * s as f64 / segs as f64).min(u1);
        while k + 1 < n && knots.get(k + 1).is_some_and(|kn| *kn <= u) {
            k += 1;
        }
        let mut d: Vec<(DVec3, f64)> =
            (0..=p).map(|j| hom.get(j + k - p).copied().unwrap_or((DVec3::ZERO, 1.0))).collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = j + k - p;
                let (ki, kj) = (knots.get(i).copied().unwrap_or(0.0), knots.get(i + p + 1 - r).copied().unwrap_or(0.0));
                let denom = kj - ki;
                let alpha = if denom.abs() < 1e-15 { 0.0 } else { (u - ki) / denom };
                let (a, b) = (d[j - 1], d[j]);
                d[j] = (a.0 * (1.0 - alpha) + b.0 * alpha, a.1 * (1.0 - alpha) + b.1 * alpha);
            }
        }
        let (pt, wt) = d[p];
        let q = if wt.abs() > 1e-15 { pt / wt } else { pt };
        if q.is_finite() {
            out.push(q);
        }
    }
    (out.len() >= 2).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_axis_of_a_downward_normal_mirrors_x() {
        let (ax, ay, _) = ocs_axes(DVec3::new(0.0, 0.0, -1.0));
        assert!((ax - DVec3::new(-1.0, 0.0, 0.0)).length() < 1e-12);
        assert!((ay - DVec3::Y).length() < 1e-12);
        let (ax, ay, _) = ocs_axes(DVec3::Z);
        assert!((ax - DVec3::X).length() < 1e-12 && (ay - DVec3::Y).length() < 1e-12);
    }

    #[test]
    fn bulge_of_one_is_a_half_circle() {
        let (c, r, start, sweep) = bulge_arc(DVec2::ZERO, DVec2::new(2.0, 0.0), 1.0).unwrap();
        assert!((c - DVec2::new(1.0, 0.0)).length() < 1e-12);
        assert!((r - 1.0).abs() < 1e-12);
        assert!((start.abs() - std::f64::consts::PI).abs() < 1e-12);
        assert!((sweep - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn quadratic_bezier_spline() {
        // Clamped quadratic B-spline = Bézier: the middle point is at
        // 0.25·P0 + 0.5·P1 + 0.25·P2.
        let ctrl = [DVec3::ZERO, DVec3::new(1.0, 2.0, 0.0), DVec3::new(2.0, 0.0, 0.0)];
        let pts = spline_points(2, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], &ctrl, &[]).unwrap();
        assert!((pts[0] - ctrl[0]).length() < 1e-12);
        assert!((pts[pts.len() - 1] - ctrl[2]).length() < 1e-12);
        let mid = pts[pts.len() / 2];
        assert!((mid - DVec3::new(1.0, 1.0, 0.0)).length() < 1e-12, "{mid}");
        assert!(spline_points(2, &[0.0, 1.0], &ctrl, &[]).is_none());
    }

    #[test]
    fn similarity_detection() {
        let rot = Affine2 { m: DMat2::from_angle(0.3) * 2.0, t: DVec2::ZERO };
        assert!((rot.similarity().unwrap() - 2.0).abs() < 1e-12);
        let stretch = Affine2 { m: DMat2::from_diagonal(DVec2::new(1.0, 2.0)), t: DVec2::ZERO };
        assert!(stretch.similarity().is_none());
    }
}
