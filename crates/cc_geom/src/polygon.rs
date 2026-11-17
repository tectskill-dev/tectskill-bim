//! 2D polygon helpers.

use glam::DVec2;

/// Signed area (positive for counter-clockwise).
pub fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() * 0.5
}

pub fn area(poly: &[DVec2]) -> f64 {
    signed_area(poly).abs()
}

pub fn perimeter(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].distance(poly[(i + 1) % n])).sum()
}

/// Most points [`convex_hull`] takes; the rest are left out.
pub const MAX_HULL_POINTS: usize = 100_000;

/// The convex hull of `points`, counter-clockwise, without collinear points
/// (Andrew's monotone chain). Non-finite points are skipped; fewer than three
/// distinct points give them back as they are.
pub fn convex_hull(points: &[DVec2]) -> Vec<DVec2> {
    let mut pts: Vec<DVec2> = points.iter().copied().filter(|p| p.is_finite()).take(MAX_HULL_POINTS).collect();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<DVec2> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &DVec2>> =
            if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in ordered {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        // The last point of each chain starts the other one.
        hull.pop();
    }
    hull
}

/// Copy of `poly` wound counter-clockwise.
pub fn ccw(poly: &[DVec2]) -> Vec<DVec2> {
    let mut p = poly.to_vec();
    if signed_area(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// Offset a simple polygon outward by `d` using mitered corners.
pub fn offset(poly: &[DVec2], d: f64) -> Vec<DVec2> {
    let p = ccw(poly);
    let n = p.len();
    if n < 3 || d == 0.0 {
        return p;
    }
    (0..n)
        .map(|i| {
            let prev = p[(i + n - 1) % n];
            let cur = p[i];
            let next = p[(i + 1) % n];
            // Outward normals of a CCW polygon point to the right of each edge.
            let n1 = -(cur - prev).normalize_or_zero().perp();
            let n2 = -(next - cur).normalize_or_zero().perp();
            let bisector = (n1 + n2).normalize_or_zero();
            let cos = bisector.dot(n1).max(0.2);
            cur + bisector * (d / cos)
        })
        .collect()
}

pub fn contains(poly: &[DVec2], pt: DVec2) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > pt.y) != (b.y > pt.y) && pt.x < (b.x - a.x) * (pt.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Triangulate a simple polygon; returns index triples (none when a point
/// is not finite: earcutr panics on such input).
pub fn triangulate(poly: &[DVec2]) -> Vec<[usize; 3]> {
    if poly.iter().any(|p| !p.is_finite()) {
        return Vec::new();
    }
    match poly.len() {
        0..=2 => vec![],
        3 => vec![[0, 1, 2]],
        _ => {
            let flat: Vec<f64> = poly.iter().flat_map(|p| [p.x, p.y]).collect();
            let indices = earcutr::earcut(&flat, &[], 2).unwrap_or_default();
            indices.as_chunks::<3>().0.iter().map(|t| [t[0], t[1], t[2]]).collect()
        }
    }
}

/// Triangulate a polygon with holes; triangles keep the winding of `outer`.
pub fn triangulate_with_holes(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<[DVec2; 3]> {
    let mut pts: Vec<DVec2> = outer.to_vec();
    let mut starts = Vec::new();
    for h in holes.iter().filter(|h| h.len() >= 3) {
        starts.push(pts.len());
        pts.extend_from_slice(h);
    }
    if outer.len() < 3 || pts.iter().any(|p| !p.is_finite()) {
        return Vec::new();
    }
    let flat: Vec<f64> = pts.iter().flat_map(|p| [p.x, p.y]).collect();
    let indices = earcutr::earcut(&flat, &starts, 2).unwrap_or_default();
    let ccw = signed_area(outer) >= 0.0;
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| {
            let [a, b, c] = [pts.get(t[0])?, pts.get(t[1])?, pts.get(t[2])?];
            let up = (*b - *a).perp_dot(*c - *a) >= 0.0;
            Some(if up == ccw { [*a, *b, *c] } else { [*a, *c, *b] })
        })
        .collect()
}

/// Most segment tests [`keyhole`] makes before it gives up.
const KEYHOLE_WORK: usize = 4_000_000;

/// One ring covering `outer` less `holes` (simple, inside it, apart): the
/// outline counter-clockwise with each hole, clockwise, joined to it by a
/// bridge walked there and back. A single polygon with holes, for painters
/// and formats that fill one ring (nonzero and even-odd fills both leave
/// the holes out). `None` when a bridge could not be found within the work
/// limit or the input is degenerate; callers then fall back to
/// [`triangulate_with_holes`].
pub fn keyhole(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Option<Vec<DVec2>> {
    if outer.len() < 3 || outer.iter().chain(holes.iter().flatten()).any(|p| !p.is_finite()) {
        return None;
    }
    let mut ring = ccw(outer);
    let mut pending: Vec<Vec<DVec2>> = holes
        .iter()
        .filter(|h| h.len() >= 3)
        .map(|h| {
            let mut h = ccw(h);
            h.reverse();
            h
        })
        .collect();
    // Rightmost holes first, so later bridges see earlier ones as ring.
    let max_x = |h: &Vec<DVec2>| h.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    pending.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
    let mut work = 0usize;
    while !pending.is_empty() {
        let hole = pending.remove(0);
        let hi = (0..hole.len()).max_by(|&a, &b| hole[a].x.total_cmp(&hole[b].x))?;
        let m = hole[hi];
        let mut order: Vec<usize> = (0..ring.len()).collect();
        order.sort_by(|&a, &b| ring[a].distance_squared(m).total_cmp(&ring[b].distance_squared(m)));
        let mut found = None;
        for ri in order {
            let v = ring[ri];
            let n = ring.len();
            let (prev, next) = (ring[(ri + n - 1) % n], ring[(ri + 1) % n]);
            if v.distance_squared(m) < 1e-24 || !in_wedge(prev, v, next, m - v) {
                continue;
            }
            work = work.saturating_add(ring.len() + hole.len() + pending.iter().map(Vec::len).sum::<usize>());
            if work > KEYHOLE_WORK {
                return None;
            }
            let blocked = |poly: &[DVec2]| {
                let k = poly.len();
                (0..k).any(|i| {
                    let (a, b) = (poly[i], poly[(i + 1) % k]);
                    crosses(m, v, a, b)
                })
            };
            if !blocked(&ring) && !blocked(&hole) && !pending.iter().any(|h| blocked(h)) {
                found = Some(ri);
                break;
            }
        }
        let ri = found?;
        let v = ring[ri];
        let mut joined = Vec::with_capacity(ring.len() + hole.len() + 2);
        joined.extend_from_slice(&ring[..=ri]);
        joined.extend((0..hole.len()).map(|k| hole[(hi + k) % hole.len()]));
        joined.push(m);
        joined.push(v);
        joined.extend_from_slice(&ring[ri + 1..]);
        ring = joined;
    }
    Some(ring)
}

/// Whether direction `d` from vertex `v` of a counter-clockwise ring
/// (between `prev` and `next`) points into the ring.
fn in_wedge(prev: DVec2, v: DVec2, next: DVec2, d: DVec2) -> bool {
    let (a, b) = (next - v, prev - v);
    if a.perp_dot(b) > 0.0 {
        a.perp_dot(d) > 0.0 && d.perp_dot(b) > 0.0
    } else {
        a.perp_dot(d) > 0.0 || d.perp_dot(b) > 0.0
    }
}

/// Whether segments `p`-`q` and `a`-`b` meet anywhere but at a shared end.
fn crosses(p: DVec2, q: DVec2, a: DVec2, b: DVec2) -> bool {
    let same = |x: DVec2, y: DVec2| x.distance_squared(y) < 1e-24;
    if same(p, a) || same(p, b) || same(q, a) || same(q, b) {
        // Sharing an end: they still overlap when collinear and pointing the same way.
        let (d, e) = (q - p, b - a);
        return d.perp_dot(e).abs() <= 1e-12 * d.length() * e.length()
            && d.perp_dot(a - p).abs() <= 1e-12 * d.length().max(1e-300) * (a - p).length()
            && overlap_collinear(p, q, a, b);
    }
    let o = |x: DVec2, y: DVec2, z: DVec2| (y - x).perp_dot(z - x);
    let (d1, d2, d3, d4) = (o(p, q, a), o(p, q, b), o(a, b, p), o(a, b, q));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    // Touching: an end of one on the other.
    let on = |x: DVec2, y: DVec2, z: DVec2| {
        o(x, y, z).abs() <= 1e-12 * (y - x).length() * (z - x).length().max(1e-300) && (z - x).dot(z - y) <= 0.0
    };
    on(p, q, a) || on(p, q, b) || on(a, b, p) || on(a, b, q)
}

/// Whether collinear segments `p`-`q` and `a`-`b` share more than a point.
fn overlap_collinear(p: DVec2, q: DVec2, a: DVec2, b: DVec2) -> bool {
    let d = q - p;
    let len2 = d.length_squared();
    if len2 <= 0.0 {
        return false;
    }
    let (ta, tb) = ((a - p).dot(d) / len2, (b - p).dot(d) / len2);
    let (lo, hi) = (ta.min(tb), ta.max(tb));
    hi.min(1.0) - lo.max(0.0) > 1e-9
}

/// Parts of the infinite line `origin + t*dir` inside `poly`, as `(t0, t1)` pairs.
pub fn clip_line(poly: &[DVec2], origin: DVec2, dir: DVec2) -> Vec<(f64, f64)> {
    let n = poly.len();
    let mut ts = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let e = b - a;
        let denom = dir.perp_dot(e);
        if denom.abs() < 1e-12 {
            continue;
        }
        let w = a - origin;
        let s = -dir.perp_dot(w) / denom; // parameter on the edge
        // Half-open interval avoids counting shared vertices twice.
        if (0.0..1.0).contains(&s) {
            ts.push(w.perp_dot(e) / denom);
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
}

/// Closest point on segment `a`-`b` to `p` and the segment parameter.
pub fn closest_on_segment(a: DVec2, b: DVec2, p: DVec2) -> (DVec2, f64) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t, t)
}

/// Intersection of lines `p1 + t*d1` and `p2 + s*d2`, returning `t`.
pub fn line_intersect(p1: DVec2, d1: DVec2, p2: DVec2, d2: DVec2) -> Option<f64> {
    let denom = d1.perp_dot(d2);
    if denom.abs() < 1e-9 {
        return None;
    }
    Some((p2 - p1).perp_dot(d2) / denom)
}

/// Whether segment `a`–`b` passes through the rectangle `lo`..`hi`
/// (Liang–Barsky clipping; touching counts).
pub fn segment_meets_rect(a: DVec2, b: DVec2, lo: DVec2, hi: DVec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-d.x, a.x - lo.x), (d.x, hi.x - a.x), (-d.y, a.y - lo.y), (d.y, hi.y - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<DVec2> {
        vec![DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0), DVec2::new(4.0, 4.0), DVec2::new(0.0, 4.0)]
    }

    /// Regression: earcutr panics on non-finite coordinates, and the 3D view
    /// triangulates every face each time the model changes (issue #51).
    #[test]
    fn triangulating_non_finite_points_gives_nothing() {
        let n = f64::NAN;
        let mut bad = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, n),
            DVec2::new(1.0, 1.0),
            DVec2::new(0.0, 1.0),
            DVec2::new(0.5, 2.0),
        ];
        assert!(triangulate(&bad).is_empty());
        bad[1] = DVec2::new(f64::INFINITY, 0.0);
        assert!(triangulate(&bad).is_empty());
        assert!(triangulate(&[DVec2::new(n, n); 6]).is_empty());
    }

    #[test]
    fn keyhole_rings_keep_the_area_and_leave_holes_out() {
        let hole = |x: f64, y: f64| {
            vec![DVec2::new(x, y), DVec2::new(x + 1.0, y), DVec2::new(x + 1.0, y + 1.0), DVec2::new(x, y + 1.0)]
        };
        let holes = vec![hole(0.5, 0.5), hole(2.5, 2.5), hole(2.5, 0.5)];
        let ring = keyhole(&square(), &holes).unwrap();
        assert!((signed_area(&ring) - 13.0).abs() < 1e-9, "{}", signed_area(&ring));
        assert!(contains(&ring, DVec2::new(2.0, 2.0)));
        for p in [DVec2::new(1.0, 1.0), DVec2::new(3.0, 3.0), DVec2::new(3.0, 1.0)] {
            assert!(!contains(&ring, p));
        }
        let tris: f64 = triangulate(&ring).iter().map(|t| area(&[ring[t[0]], ring[t[1]], ring[t[2]]])).sum();
        assert!((tris - 13.0).abs() < 1e-6, "{tris}");
        assert!(keyhole(&square(), &[vec![DVec2::new(f64::NAN, 0.0); 3]]).is_none());
    }

    #[test]
    fn area_and_offset() {
        assert_eq!(area(&square()), 16.0);
        let o = offset(&square(), 1.0);
        assert!((area(&o) - 36.0).abs() < 1e-9);
    }

    #[test]
    fn clip_line_through_square() {
        let parts = clip_line(&square(), DVec2::new(-1.0, 2.0), DVec2::X);
        assert_eq!(parts.len(), 1);
        assert!((parts[0].0 - 1.0).abs() < 1e-9 && (parts[0].1 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn triangulates_around_holes() {
        let sq = |c: f64, h: f64| {
            vec![DVec2::new(c - h, c - h), DVec2::new(c + h, c - h), DVec2::new(c + h, c + h), DVec2::new(c - h, c + h)]
        };
        let tris = triangulate_with_holes(&sq(0.0, 2.0), &[sq(0.0, 1.0).into_iter().rev().collect()]);
        let total: f64 = tris.iter().map(|t| signed_area(t)).sum();
        assert!((total - 12.0).abs() < 1e-9, "{total}");
    }

    #[test]
    fn triangulates_l_shape() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        assert_eq!(triangulate(&l).len(), 4);
    }

    #[test]
    fn convex_hulls_box_concave_outlines() {
        // An I: its hull is its box, counter-clockwise, corners only.
        let (w, d, t, f) = (0.05, 0.1, 0.003, 0.008);
        let i = [
            (-w, -d),
            (w, -d),
            (w, -d + f),
            (t, -d + f),
            (t, d - f),
            (w, d - f),
            (w, d),
            (-w, d),
            (-w, d - f),
            (-t, d - f),
            (-t, -d + f),
            (-w, -d + f),
        ]
        .map(|(x, y)| DVec2::new(x, y));
        let hull = convex_hull(&i);
        assert_eq!(hull.len(), 4, "{hull:?}");
        assert!((signed_area(&hull) - 4.0 * w * d).abs() < 1e-12);
        // Bad input: non-finite points are skipped, nothing gives nothing.
        assert!(convex_hull(&[]).is_empty());
        let nan = [DVec2::NAN, DVec2::ZERO, DVec2::X, DVec2::Y, DVec2::INFINITY];
        assert_eq!(convex_hull(&nan).len(), 3);
        assert_eq!(convex_hull(&[DVec2::ZERO, DVec2::X, DVec2::X * 2.0]).len(), 2, "collinear");
    }
}
