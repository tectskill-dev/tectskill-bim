//! Snap points that come from two pieces of geometry, or from the point a
//! segment is drawn from (issue #159): where lines and arcs cross, the foot
//! of the perpendicular from the anchor, and the points where a line from
//! the anchor touches an arc or circle. Lines are segments, arcs are
//! `cc_core::arc::Arc`s (a whole circle sweeps a full turn).

use cc_core::arc::Arc;
use glam::DVec2;
use std::f64::consts::TAU;

/// Most features near the pointer taken into the pairwise crossings.
const MAX_NEAR: usize = 64;
/// How far past its ends a point still counts as on a segment or arc (meters).
const ON_TOL: f64 = 1e-7;

/// Whether `p` lies on the arc's span (it is assumed on its circle).
fn on_arc(arc: &Arc, p: DVec2) -> bool {
    if arc.sweep.abs() >= TAU - 1e-9 {
        return true;
    }
    let (u, _) = arc.locate(p);
    (-ON_TOL..=arc.length() + ON_TOL).contains(&u)
}

/// Whether `p`, on the line through `a` and `b`, lies between them.
fn on_segment(a: DVec2, b: DVec2, p: DVec2) -> bool {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-18 {
        return false;
    }
    let t = (p - a).dot(d) / len2;
    let tol = ON_TOL / len2.sqrt();
    (-tol..=1.0 + tol).contains(&t)
}

fn distance_to_segment(a: DVec2, b: DVec2, p: DVec2) -> f64 {
    let d = b - a;
    let len2 = d.length_squared();
    let t = if len2 < 1e-18 { 0.0 } else { ((p - a).dot(d) / len2).clamp(0.0, 1.0) };
    p.distance(a + d * t)
}

fn distance_to_arc(arc: &Arc, p: DVec2) -> f64 {
    let radial = (p.distance(arc.center) - arc.radius).abs();
    let on_circle = arc.center + (p - arc.center).normalize_or_zero() * arc.radius;
    if on_arc(arc, on_circle) { radial } else { p.distance(arc.point(0.0)).min(p.distance(arc.point(arc.length()))) }
}

/// Where the segments `a`-`b` and `c`-`d` cross.
fn segment_segment(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<DVec2> {
    let (r, s) = (b - a, d - c);
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let p = a + r * t;
    (on_segment(a, b, p) && on_segment(c, d, p)).then_some(p)
}

/// Where the line through `a` and `b` meets the circle (`center`, `radius`).
fn line_circle(a: DVec2, b: DVec2, center: DVec2, radius: f64) -> Vec<DVec2> {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-18 || radius <= 0.0 {
        return Vec::new();
    }
    let t0 = (center - a).dot(d) / len2;
    let foot = a + d * t0;
    let h2 = radius * radius - foot.distance_squared(center);
    if h2 < -1e-12 {
        return Vec::new();
    }
    let h = h2.max(0.0).sqrt() / len2.sqrt();
    if h < 1e-12 {
        return vec![foot];
    }
    vec![foot - d * h, foot + d * h]
}

/// Where two circles meet.
fn circle_circle(c1: DVec2, r1: f64, c2: DVec2, r2: f64) -> Vec<DVec2> {
    let dist = c1.distance(c2);
    if dist < 1e-12 || dist > r1 + r2 + 1e-12 || dist < (r1 - r2).abs() - 1e-12 {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + dist * dist) / (2.0 * dist);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let u = (c2 - c1) / dist;
    let mid = c1 + u * a;
    if h < 1e-12 {
        return vec![mid];
    }
    vec![mid + u.perp() * h, mid - u.perp() * h]
}

/// The crossing of two of the `lines` and `arcs` nearest `raw`, within `r`.
pub fn crossing(lines: &[(DVec2, DVec2)], arcs: &[Arc], raw: DVec2, r: f64) -> Option<DVec2> {
    let near_lines: Vec<(DVec2, DVec2)> =
        lines.iter().copied().filter(|(a, b)| distance_to_segment(*a, *b, raw) <= r).take(MAX_NEAR).collect();
    let near_arcs: Vec<Arc> =
        arcs.iter().copied().filter(|arc| distance_to_arc(arc, raw) <= r).take(MAX_NEAR).collect();
    let mut points = Vec::new();
    for (i, (a, b)) in near_lines.iter().enumerate() {
        for (c, d) in near_lines.iter().skip(i + 1) {
            points.extend(segment_segment(*a, *b, *c, *d));
        }
        for arc in &near_arcs {
            let hits = line_circle(*a, *b, arc.center, arc.radius);
            points.extend(hits.into_iter().filter(|p| on_segment(*a, *b, *p) && on_arc(arc, *p)));
        }
    }
    for (i, p) in near_arcs.iter().enumerate() {
        for q in near_arcs.iter().skip(i + 1) {
            let hits = circle_circle(p.center, p.radius, q.center, q.radius);
            points.extend(hits.into_iter().filter(|x| on_arc(p, *x) && on_arc(q, *x)));
        }
    }
    nearest(points, raw, r)
}

/// The foot of the perpendicular from `anchor` to one of the `lines` (on
/// the segment) or `arcs` (towards or away from the center, on the arc)
/// nearest `raw`, within `r`.
pub fn perpendicular(anchor: DVec2, lines: &[(DVec2, DVec2)], arcs: &[Arc], raw: DVec2, r: f64) -> Option<DVec2> {
    let mut points = Vec::new();
    for (a, b) in lines {
        let d = *b - *a;
        let len2 = d.length_squared();
        if len2 < 1e-18 {
            continue;
        }
        let foot = *a + d * ((anchor - *a).dot(d) / len2);
        if on_segment(*a, *b, foot) {
            points.push(foot);
        }
    }
    for arc in arcs {
        let dir = (anchor - arc.center).normalize_or_zero();
        if dir == DVec2::ZERO {
            continue;
        }
        for p in [arc.center + dir * arc.radius, arc.center - dir * arc.radius] {
            if on_arc(arc, p) {
                points.push(p);
            }
        }
    }
    points.retain(|p| p.distance(anchor) > 1e-9);
    nearest(points, raw, r)
}

/// A point where a line from `anchor` touches one of the `arcs`, nearest
/// `raw`, within `r` (none from inside a circle).
pub fn tangent(anchor: DVec2, arcs: &[Arc], raw: DVec2, r: f64) -> Option<DVec2> {
    let mut points = Vec::new();
    for arc in arcs {
        let v = anchor - arc.center;
        let dist = v.length();
        if dist <= arc.radius + 1e-9 {
            continue;
        }
        let base = v.y.atan2(v.x);
        let spread = (arc.radius / dist).acos();
        for angle in [base + spread, base - spread] {
            let p = arc.center + DVec2::from_angle(angle) * arc.radius;
            if on_arc(arc, p) {
                points.push(p);
            }
        }
    }
    nearest(points, raw, r)
}

fn nearest(points: Vec<DVec2>, raw: DVec2, r: f64) -> Option<DVec2> {
    points
        .into_iter()
        .filter(|p| p.is_finite())
        .map(|p| (p, p.distance(raw)))
        .filter(|(_, d)| *d <= r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<DVec2>, b: DVec2) -> bool {
        a.is_some_and(|a| a.distance(b) < 1e-9)
    }

    fn circle(center: DVec2, radius: f64) -> Arc {
        Arc { center, radius, start_angle: 0.0, sweep: TAU }
    }

    #[test]
    fn lines_and_arcs_cross_where_they_meet() {
        let x = (DVec2::new(-5.0, 0.0), DVec2::new(5.0, 0.0));
        let y = (DVec2::new(1.0, -5.0), DVec2::new(1.0, 5.0));
        // Two lines.
        assert!(close(crossing(&[x, y], &[], DVec2::new(1.1, 0.1), 0.5), DVec2::new(1.0, 0.0)));
        // Segments that would meet only when extended do not cross.
        let short = (DVec2::new(1.0, 1.0), DVec2::new(1.0, 5.0));
        assert_eq!(crossing(&[x, short], &[], DVec2::new(1.0, 0.0), 0.5), None);
        // A line and a circle: x = ±3 on the X axis.
        let c = circle(DVec2::ZERO, 3.0);
        assert!(close(crossing(&[x], &[c], DVec2::new(2.9, 0.2), 0.5), DVec2::new(3.0, 0.0)));
        // Two circles.
        let d = circle(DVec2::new(4.0, 0.0), 3.0);
        let top = DVec2::new(2.0, 5f64.sqrt());
        assert!(close(crossing(&[], &[c, d], top + DVec2::splat(0.1), 0.5), top));
        // Half an arc (the upper half) does not cross where its circle would.
        let upper = Arc { center: DVec2::ZERO, radius: 3.0, start_angle: 0.0, sweep: std::f64::consts::PI };
        assert_eq!(crossing(&[y], &[upper], DVec2::new(1.0, -8f64.sqrt()), 0.3), None);
        assert!(close(crossing(&[y], &[upper], DVec2::new(1.0, 2.9), 0.3), DVec2::new(1.0, 8f64.sqrt())));
        // Too far from the pointer.
        assert_eq!(crossing(&[x, y], &[], DVec2::new(3.0, 3.0), 0.5), None);
    }

    #[test]
    fn the_perpendicular_from_the_anchor_lands_on_the_line_or_arc() {
        let line = (DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0));
        let anchor = DVec2::new(4.0, 6.0);
        assert!(close(perpendicular(anchor, &[line], &[], DVec2::new(4.2, 0.3), 0.5), DVec2::new(4.0, 0.0)));
        // Off the segment: no foot.
        assert_eq!(perpendicular(DVec2::new(12.0, 6.0), &[line], &[], DVec2::new(12.0, 0.0), 0.5), None);
        // On a circle, towards the center.
        let c = circle(DVec2::ZERO, 2.0);
        let p = perpendicular(DVec2::new(5.0, 0.0), &[], &[c], DVec2::new(2.1, 0.1), 0.5);
        assert!(close(p, DVec2::new(2.0, 0.0)));
    }

    #[test]
    fn tangents_from_the_anchor_touch_the_circle() {
        let c = circle(DVec2::ZERO, 1.0);
        let anchor = DVec2::new(2.0, 0.0);
        // The tangent points of a unit circle seen from (2, 0) are at 60 degrees.
        let t = DVec2::new(0.5, 3f64.sqrt() / 2.0);
        let p = tangent(anchor, &[c], t + DVec2::splat(0.05), 0.3);
        assert!(close(p, t));
        assert!((p.unwrap() - anchor).dot(p.unwrap()).abs() < 1e-9, "the radius is square to the line");
        // From inside: none.
        assert_eq!(tangent(DVec2::new(0.2, 0.0), &[c], t, 0.3), None);
    }
}
