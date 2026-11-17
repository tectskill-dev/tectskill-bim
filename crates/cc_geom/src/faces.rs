//! The faces of a column's section, a given distance inside it: where its
//! longitudinal bars stand and its stirrups run. A face is a straight side,
//! a run of the chords of a curve, or (for an annular sector) a true arc;
//! faces meet at corners, in counter-clockwise order.

use crate::polygon;
use cc_core::FaceBars;
use cc_core::arc::{ARC_STEP, Arc};
use cc_core::profiles_project::SectorPlacement;
use glam::DVec2;

/// The outline turns by at least this much at a corner between two faces.
const CORNER_TURN_DEG: f64 = 30.0;

/// Sides of one curve are at most this many times as long as each other.
const CURVE_LENGTH_RATIO: f64 = 2.0;

/// One face of a section.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionFace {
    /// Points along the face from its first corner to its last (at least 2).
    pub points: Vec<DVec2>,
    /// The circle the face lies on when it is a true arc (its points are on it).
    pub arc: Option<Arc>,
}

impl SectionFace {
    fn line(a: DVec2, b: DVec2) -> Self {
        Self { points: vec![a, b], arc: None }
    }

    fn of_arc(arc: Arc) -> Self {
        Self { points: arc.points(0.0, arc.length(), 0.0, ARC_STEP), arc: Some(arc) }
    }

    /// Whether the face is curved (an arc, or the chords of a curve).
    pub fn is_curved(&self) -> bool {
        self.arc.is_some() || self.points.len() > 2
    }

    /// Whether the face runs all around a section without corners (a circle).
    pub fn is_closed(&self) -> bool {
        self.points.len() > 3 && self.points.first().zip(self.points.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9)
    }

    /// Length along the face: the arc's, else its chords'.
    pub fn length(&self) -> f64 {
        match &self.arc {
            Some(a) => a.length(),
            None => self.points.windows(2).map(|w| w[0].distance(w[1])).sum(),
        }
    }

    /// The point the fraction `t` of the way along the face, and the unit
    /// normal into the section there (to the left of the way round).
    pub fn at(&self, t: f64) -> (DVec2, DVec2) {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        if let Some(a) = &self.arc {
            let u = a.length() * t;
            return (a.point(u), a.tangent(u).perp());
        }
        let mut left = self.length() * t;
        let mut last = (self.points.first().copied().unwrap_or(DVec2::ZERO), DVec2::Y);
        for w in self.points.windows(2) {
            let len = w[0].distance(w[1]);
            let dir = (w[1] - w[0]).normalize_or_zero();
            if left <= len && len > 0.0 {
                return (w[0] + dir * left, dir.perp());
            }
            left -= len;
            last = (w[1], dir.perp());
        }
        last
    }
}

/// The faces of an annular sector `inset` inside it, exact: the outer arc,
/// the left radial side, the inner arc and the right radial side (a
/// circular sector, or one whose radial sides meet before the inner arc,
/// has no inner arc: its radial sides meet at a point). Empty when nothing
/// is left inside the inset.
pub fn sector_faces(s: &SectorPlacement, inset: f64) -> Vec<SectionFace> {
    let (half, outer) = (s.sweep * 0.5, s.r_out - inset);
    let sound = inset.is_finite() && inset >= 0.0 && outer > 1e-9;
    if !(sound && half > 0.0 && half <= std::f64::consts::FRAC_PI_2 + 1e-9) {
        return Vec::new();
    }
    // The angle either side of the axis a circle of `radius` keeps clear of the radial sides.
    let reach = |radius: f64| (radius > inset).then(|| half - (inset / radius).asin()).filter(|a| *a > 1e-9);
    let Some(wide) = reach(outer) else { return Vec::new() };
    let axis = s.axis.y.atan2(s.axis.x);
    let at = |radius: f64, angle: f64| s.center + DVec2::from_angle(axis + angle) * radius;
    let arc =
        |radius: f64, from: f64, to: f64| Arc { center: s.center, radius, start_angle: axis + from, sweep: to - from };
    let inner = s.r_in + inset;
    // The inset has eaten the ring.
    if s.r_in > 0.0 && inner >= outer - 1e-9 {
        return Vec::new();
    }
    let narrow = if s.r_in > 0.0 { reach(inner) } else { None };
    let mut faces = vec![SectionFace::of_arc(arc(outer, -wide, wide))];
    match narrow {
        Some(narrow) => {
            faces.push(SectionFace::line(at(outer, wide), at(inner, narrow)));
            faces.push(SectionFace::of_arc(arc(inner, narrow, -narrow)));
            faces.push(SectionFace::line(at(inner, -narrow), at(outer, -wide)));
        }
        None => {
            // Where the two radial sides, moved in, cross on the axis.
            let apex = s.center + s.axis * (inset / half.sin()).max(0.0);
            if apex.distance(s.center) >= outer * wide.cos() {
                return Vec::new();
            }
            faces.push(SectionFace::line(at(outer, wide), apex));
            faces.push(SectionFace::line(apex, at(outer, -wide)));
        }
    }
    faces
}

/// The faces of a section's `outline` moved `inset` inside it: its
/// corners are where it turns sharply, or where the lengths of its sides
/// jump (a straight side beside the chords of a fillet); the sides between
/// two corners are one face. A circle is one closed face. Empty when the
/// outline does not survive the inset.
pub fn outline_faces(outline: &[DVec2], inset: f64) -> Vec<SectionFace> {
    let outer = polygon::ccw(outline);
    let n = outer.len();
    if n < 3 || !inset.is_finite() || inset < 0.0 {
        return Vec::new();
    }
    let moved = polygon::offset(&outer, -inset);
    // Every side must keep its direction: one that flips has been eaten by the inset.
    let side = |p: &[DVec2], i: usize| p[(i + 1) % n] - p[i];
    if moved.len() != n || (0..n).any(|i| side(&moved, i).dot(side(&outer, i)) <= 1e-12) {
        return Vec::new();
    }
    let dir = |i: usize| side(&outer, i).normalize_or_zero();
    let len = |i: usize| side(&outer, i).length();
    // Vertex `i` is a corner between the sides before and after it.
    let corner = |i: usize| {
        let (a, b) = ((i + n - 1) % n, i);
        let ratio = len(a).max(len(b)) / len(a).min(len(b)).max(1e-12);
        dir(a).dot(dir(b)) < CORNER_TURN_DEG.to_radians().cos() || ratio > CURVE_LENGTH_RATIO
    };
    let corners: Vec<usize> = (0..n).filter(|&i| corner(i)).collect();
    if corners.is_empty() {
        let mut points = moved.clone();
        points.push(moved[0]);
        return vec![SectionFace { points, arc: None }];
    }
    (0..corners.len())
        .map(|k| {
            let (from, to) = (corners[k], corners[(k + 1) % corners.len()]);
            let steps = if to > from { to - from } else { to + n - from };
            let steps = if corners.len() == 1 { n } else { steps };
            let points = (0..=steps).map(|j| moved[(from + j) % n]).collect();
            SectionFace { points, arc: None }
        })
        .collect()
}

/// A longitudinal bar in the section: where it stands, the normal into the
/// section there, and whether it stands at a corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBar {
    pub at: DVec2,
    pub inward: DVec2,
    pub corner: bool,
}

/// The bars on `faces`: one at every corner, and between the corners of
/// each face what its entry of `spec` says; without a `spec`, `count` bars,
/// the corners first and each of the others on the face with the widest
/// gap. A closed face (a circle) takes its bars evenly around.
pub fn face_bars(faces: &[SectionFace], spec: &[FaceBars], count: usize) -> Vec<FaceBar> {
    if let [face] = faces
        && face.is_closed()
    {
        let n = spec.first().map_or(count, |s| s.between(face.length()) + if s.count > 0 { 2 } else { 1 });
        let n = n.min(FaceBars::MAX_COUNT as usize);
        return (0..n)
            .map(|i| {
                let (at, inward) = face.at(i as f64 / n as f64);
                FaceBar { at, inward, corner: false }
            })
            .collect();
    }
    let lengths: Vec<f64> = faces.iter().map(SectionFace::length).collect();
    let between: Vec<usize> = if spec.is_empty() {
        let mut extra = vec![0usize; faces.len()];
        for _ in faces.len()..count.min(faces.len() * FaceBars::MAX_COUNT as usize) {
            let gap = |i: usize| lengths[i] / (extra[i] + 1) as f64;
            // The first of the widest, so the result does not depend on float noise order.
            let mut best = 0;
            for i in 1..faces.len() {
                if gap(i) > gap(best) + 1e-12 {
                    best = i;
                }
            }
            extra[best] += 1;
        }
        extra
    } else {
        (0..faces.len()).map(|i| spec.get(i).map_or(0, |s| s.between(lengths[i]))).collect()
    };
    // Fewer bars than corners: the first corners only.
    let corners = if spec.is_empty() { count.min(faces.len()) } else { faces.len() };
    let mut bars = Vec::new();
    for (i, face) in faces.iter().enumerate() {
        if i < corners {
            let (at, inward) = face.at(0.0);
            bars.push(FaceBar { at, inward, corner: true });
        }
        for k in 1..=between[i] {
            let (at, inward) = face.at(k as f64 / (between[i] + 1) as f64);
            bars.push(FaceBar { at, inward, corner: false });
        }
    }
    bars
}

/// The closed loop around `faces` (a stirrup's center line) and its true
/// length, arcs as arcs.
/// The largest rectangles `(min, max)` inside an outline whose sides all
/// run along its own X or Y axis: two in an L or a T, three in a U, one in a
/// rectangle. Together they cover it, overlapping where its parts meet.
/// Empty for any other outline, or one with too many corners.
pub fn rectangles(outline: &[DVec2]) -> Vec<(DVec2, DVec2)> {
    const TOL: f64 = 1e-6;
    const MAX_LINES: usize = 16;
    let n = outline.len();
    let square = (0..n).all(|i| {
        let e = outline[(i + 1) % n] - outline[i];
        e.x.abs() < TOL || e.y.abs() < TOL
    });
    if n < 4 || !square || outline.iter().any(|p| !p.is_finite()) {
        return Vec::new();
    }
    let lines = |pick: fn(&DVec2) -> f64| {
        let mut v: Vec<f64> = outline.iter().map(pick).collect();
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < TOL);
        v
    };
    let (xs, ys) = (lines(|p| p.x), lines(|p| p.y));
    if xs.len() < 2 || ys.len() < 2 || xs.len() > MAX_LINES || ys.len() > MAX_LINES {
        return Vec::new();
    }
    // The cells of the grid of those lines that lie inside the outline.
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    let inside: Vec<Vec<bool>> = (0..nx)
        .map(|i| {
            (0..ny)
                .map(|j| polygon::contains(outline, DVec2::new((xs[i] + xs[i + 1]) * 0.5, (ys[j] + ys[j + 1]) * 0.5)))
                .collect()
        })
        .collect();
    let full = |i0: usize, i1: usize, j0: usize, j1: usize| (i0..i1).all(|i| (j0..j1).all(|j| inside[i][j]));
    let mut out = Vec::new();
    for i0 in 0..nx {
        for i1 in i0 + 1..=nx {
            for j0 in 0..ny {
                for j1 in j0 + 1..=ny {
                    if !full(i0, i1, j0, j1) {
                        continue;
                    }
                    // The largest: it cannot grow a row or a column of cells any way.
                    let grows = (i0 > 0 && full(i0 - 1, i0, j0, j1))
                        || (i1 < nx && full(i1, i1 + 1, j0, j1))
                        || (j0 > 0 && full(i0, i1, j0 - 1, j0))
                        || (j1 < ny && full(i0, i1, j1, j1 + 1));
                    if !grows {
                        out.push((DVec2::new(xs[i0], ys[j0]), DVec2::new(xs[i1], ys[j1])));
                    }
                }
            }
        }
    }
    out
}

pub fn face_loop(faces: &[SectionFace]) -> (Vec<DVec2>, f64) {
    let mut points: Vec<DVec2> = Vec::new();
    for face in faces {
        for p in &face.points {
            if points.last().is_none_or(|q| q.distance(*p) > 1e-9) {
                points.push(*p);
            }
        }
    }
    if points.len() > 1 && points.first().zip(points.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9) {
        points.pop();
    }
    (points, faces.iter().map(SectionFace::length).sum())
}

/// Where the ray from `from` along the unit `dir` first meets `faces`
/// again, at least `clear` away, and the normal into the section there.
fn first_hit(faces: &[SectionFace], from: DVec2, dir: DVec2, clear: f64) -> Option<(DVec2, DVec2)> {
    faces
        .iter()
        .flat_map(|f| f.points.windows(2))
        .filter_map(|w| {
            let t = polygon::line_intersect(from, dir, w[0], w[1] - w[0])?;
            let hit = from + dir * t;
            let (q, _) = polygon::closest_on_segment(w[0], w[1], hit);
            (t > clear && q.distance(hit) < 1e-9).then_some((t, hit, (w[1] - w[0]).normalize_or_zero().perp()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, hit, normal)| (hit, normal))
}

/// Where a cross-tie from `from` (on a face whose normal into the section
/// is `inward`) meets the face opposite, at least `clear` away: straight
/// across, or square to the bisector of that face and a straight face that
/// leans towards it (the radial sides of a sector); whichever leaves and
/// arrives nearest to square, and within 45° of both faces' normals.
pub fn across(faces: &[SectionFace], from: DVec2, inward: DVec2, clear: f64) -> Option<DVec2> {
    let inward = inward.try_normalize()?;
    let leaning = faces.iter().filter(|f| !f.is_curved()).filter_map(|f| (inward - f.at(0.5).1).try_normalize());
    std::iter::once(inward)
        .chain(leaning)
        .filter_map(|dir| {
            let (hit, normal) = first_hit(faces, from, dir, clear)?;
            let square = dir.dot(inward).min(-dir.dot(normal));
            (square > std::f64::consts::FRAC_1_SQRT_2).then_some((square, hit))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, hit)| hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_of_square_corners_is_made_of_overlapping_rectangles() {
        let p = |x: f64, y: f64| DVec2::new(x, y);
        // A T: the flange, and the web through it.
        let t = [p(0.0, 0.4), p(0.6, 0.4), p(0.6, 0.6), p(0.0, 0.6)];
        let web = [p(0.4, 0.4), p(0.4, 0.0), p(0.2, 0.0), p(0.2, 0.4)];
        let outline = [t[0], web[3], web[2], web[1], web[0], t[1], t[2], t[3]];
        let mut r = rectangles(&outline);
        r.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        assert_eq!(r, vec![(p(0.0, 0.4), p(0.6, 0.6)), (p(0.2, 0.0), p(0.4, 0.6))]);
        // A rectangle is itself, wound either way.
        let mut square = vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 2.0), p(0.0, 2.0)];
        assert_eq!(rectangles(&square), vec![(p(0.0, 0.0), p(1.0, 2.0))]);
        square.reverse();
        assert_eq!(rectangles(&square).len(), 1);
        // A slanted side, too few points or a bad number: none.
        assert!(rectangles(&[p(0.0, 0.0), p(1.0, 0.0), p(1.2, 1.0), p(0.0, 1.0)]).is_empty());
        assert!(rectangles(&[p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)]).is_empty());
        assert!(rectangles(&[p(0.0, 0.0), p(f64::NAN, 0.0), p(1.0, 1.0), p(0.0, 1.0)]).is_empty());
    }
    use std::f64::consts::FRAC_PI_2;

    fn sector(r_in: f64, r_out: f64, sweep_deg: f64) -> SectorPlacement {
        SectorPlacement { center: DVec2::ZERO, axis: DVec2::Y, r_in, r_out, sweep: sweep_deg.to_radians() }
    }

    #[test]
    fn a_sector_has_two_arcs_and_two_radial_faces() {
        let s = sector(2.0, 2.2, 30.0);
        let faces = sector_faces(&s, 0.04);
        assert_eq!(faces.len(), 4);
        assert_eq!(faces.iter().map(SectionFace::is_curved).collect::<Vec<_>>(), [true, false, true, false]);
        // The arcs lie 4 cm inside the curved faces, and the radial faces 4 cm inside the radial sides.
        let (outer, inner) = (faces[0].arc.unwrap(), faces[2].arc.unwrap());
        assert!((outer.radius - 2.16).abs() < 1e-12 && (inner.radius - 2.04).abs() < 1e-12);
        let edge = DVec2::from_angle(FRAC_PI_2 + 15f64.to_radians());
        for p in &faces[1].points {
            assert!((p.perp_dot(edge).abs() - 0.04).abs() < 1e-9, "{p}");
        }
        // Counter-clockwise: each face ends where the next starts, and the section is on the left.
        for (a, b) in faces.iter().zip(faces.iter().cycle().skip(1)) {
            assert!(a.points.last().unwrap().distance(b.points[0]) < 1e-9);
        }
        let (mid, inward) = faces[0].at(0.5);
        assert!(mid.distance(DVec2::new(0.0, 2.16)) < 1e-9 && inward.distance(-DVec2::Y) < 1e-9);
        // The loop's length: the two arcs and the two sides.
        let (points, length) = face_loop(&faces);
        let (wide, narrow) = (outer.sweep.abs(), inner.sweep.abs());
        assert!((length - (2.16 * wide + 2.04 * narrow + 2.0 * faces[1].length())).abs() < 1e-9);
        assert!(points.len() > 8 && polygon::signed_area(&points) > 0.0);
        // Nothing left: an inset as deep as the ring is thick.
        assert!(sector_faces(&s, 0.11).is_empty());
        // A circular sector's radial faces meet at a point on the axis.
        let pie = sector_faces(&sector(0.0, 0.5, 90.0), 0.03);
        assert_eq!(pie.len(), 3);
        let apex = *pie[1].points.last().unwrap();
        assert!(apex.x.abs() < 1e-9 && (apex.y - 0.03 / 45f64.to_radians().sin()).abs() < 1e-9);
    }

    #[test]
    fn bars_by_face_share_the_corners() {
        // The issue's example: 7 on each curved face, 3 on each radial face.
        let faces = sector_faces(&sector(2.0, 2.2, 30.0), 0.04);
        let seven = FaceBars { count: 7, spacing: 0.0 };
        let three = FaceBars { count: 3, spacing: 0.0 };
        let bars = face_bars(&faces, &[seven, three, seven, three], 0);
        assert_eq!(bars.len(), 7 + 7 + 3 + 3 - 4);
        assert_eq!(bars.iter().filter(|b| b.corner).count(), 4);
        // On a curved face the bars are at even angles on its circle.
        let outer: Vec<f64> = bars[..6].iter().map(|b| b.at.y.atan2(b.at.x)).collect();
        assert!(bars[..6].iter().all(|b| (b.at.length() - 2.16).abs() < 1e-9));
        let step = outer[1] - outer[0];
        assert!(outer.windows(2).all(|w| (w[1] - w[0] - step).abs() < 1e-9) && step > 0.0);
        // By spacing: as many as keep them 5 cm apart.
        let spaced = FaceBars { count: 0, spacing: 0.05 };
        let by_spacing = face_bars(&faces, &[spaced; 4], 0);
        let on_outer = (faces[0].length() / 0.05 - 1e-9).ceil() as usize - 1;
        assert!(by_spacing.len() > bars.len() && by_spacing[1..=on_outer].iter().all(|b| !b.corner));
        // Without a list, a count is spread: the corners, then the widest gaps.
        let spread = face_bars(&faces, &[], 8);
        assert_eq!(spread.len(), 8);
        assert_eq!(spread.iter().filter(|b| (b.at.length() - 2.16).abs() < 1e-9 && !b.corner).count(), 2);
        assert_eq!(face_bars(&faces, &[], 3).len(), 3);
    }

    #[test]
    fn an_l_section_has_six_faces_and_a_circle_one() {
        let v = DVec2::new;
        let l = [v(0.0, 0.0), v(0.6, 0.0), v(0.6, 0.2), v(0.2, 0.2), v(0.2, 0.6), v(0.0, 0.6)];
        let faces = outline_faces(&l, 0.03);
        assert_eq!(faces.len(), 6);
        assert!(faces.iter().all(|f| !f.is_curved()));
        let (points, length) = face_loop(&faces);
        assert_eq!(points.len(), 6);
        // Each side 3 cm in: the perimeter less 8 insets at the 5 convex corners plus 2 at the re-entrant one.
        assert!((length - (2.4 - 8.0 * 0.03)).abs() < 1e-9, "{length}");
        assert!(points.iter().any(|p| p.distance(v(0.17, 0.17)) < 1e-9), "{points:?}");
        // An inset as deep as half a leg leaves nothing.
        assert!(outline_faces(&l, 0.11).is_empty());
        // A tie from the middle of the long bottom face crosses the leg.
        let (from, inward) = faces[0].at(0.5);
        let hit = across(&faces, from, inward, 0.01).unwrap();
        assert!((hit.y - 0.17).abs() < 1e-9 && (hit.x - from.x).abs() < 1e-9);
        // Between the radial sides of a sector a tie runs square to the axis, not into the outer arc.
        let ring = sector_faces(&sector(2.0, 2.4, 30.0), 0.04);
        let (from, inward) = ring[1].at(0.5);
        let hit = across(&ring, from, inward, 0.01).unwrap();
        assert!((hit.y - from.y).abs() < 1e-9 && (hit.x + from.x).abs() < 1e-9, "{from} {hit}");
        // And between its arcs along the radius.
        let (from, inward) = ring[0].at(0.25);
        let hit = across(&ring, from, inward, 0.01).unwrap();
        assert!((hit.length() - 2.04).abs() < 1e-3 && hit.perp_dot(from).abs() < 1e-9, "on a chord of the inner arc");
        // In a thin ring the straight line between the radial sides would
        // pass inside the inner arc, out of the cage: no tie.
        let thin = sector_faces(&sector(2.0, 2.2, 30.0), 0.04);
        let (from, inward) = thin[1].at(0.5);
        assert!(across(&thin, from, inward, 0.01).is_none());
        // A circle of 32 chords is one closed face.
        let circle: Vec<DVec2> =
            (0..32).map(|i| DVec2::from_angle(i as f64 / 32.0 * std::f64::consts::TAU) * 0.2).collect();
        let faces = outline_faces(&circle, 0.03);
        assert!(faces.len() == 1 && faces[0].is_closed());
        assert_eq!(face_bars(&faces, &[], 6).len(), 6);
        // A rectangle with filleted corners: the sides and the fillets are separate faces.
        let rounded = cc_core::ProfileShape::RectTube { h: 0.4, b: 0.3, t: 0.01, ro: 0.05 }.outline().unwrap();
        assert_eq!(outline_faces(&rounded.outer, 0.0).len(), 8);
    }
}
