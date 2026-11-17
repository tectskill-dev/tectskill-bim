//! Hatch patterns: the line work of a material [`Pattern`] clipped to a
//! polygon. Lines are anchored to the drawing origin so neighbouring
//! regions line up; `spacing` is the pattern's base spacing in drawing
//! units (callers derive it from paper millimeters and the view scale).

use cc_core::Pattern;
use cc_core::fill_pattern::{FillPattern, FillPatternKind, FillPatterns};
use cc_geom::polygon;
use glam::DVec2;

/// Most lines one region may produce (keeps huge regions at fine spacing cheap).
const MAX_LINES: usize = 4000;

/// Segments of `pattern` inside `poly`. `Solid` and `None` give no lines
/// (solid patterns are filled by the caller).
pub fn hatch(pattern: Pattern, poly: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    if poly.len() < 3 || !(spacing.is_finite() && spacing > 1e-9) {
        return Vec::new();
    }
    let (lo, hi) = bounds(poly);
    let mut out = Vec::new();
    let parallel = |out: &mut Vec<(DVec2, DVec2)>, angle_deg: f64, step: f64| {
        let dir = DVec2::from_angle(angle_deg.to_radians());
        let n = dir.perp();
        // Offsets of lines crossing the bounding box, on the global grid.
        let corners = [lo, hi, DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)];
        let (o0, o1) =
            corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), c| (a.min(c.dot(n)), b.max(c.dot(n))));
        let first = (o0 / step).ceil() as i64;
        let last = (o1 / step).floor() as i64;
        for k in first..=last {
            if out.len() >= MAX_LINES {
                return;
            }
            let origin = n * (k as f64 * step);
            for (t0, t1) in polygon::clip_line(poly, origin, dir) {
                out.push((origin + dir * t0, origin + dir * t1));
            }
        }
    };
    match pattern {
        Pattern::None | Pattern::Solid | Pattern::Custom(_) => {}
        Pattern::Diagonal => parallel(&mut out, 45.0, spacing),
        Pattern::DiagonalBack => parallel(&mut out, 135.0, spacing),
        Pattern::Crosshatch => {
            parallel(&mut out, 45.0, spacing);
            parallel(&mut out, 135.0, spacing);
        }
        Pattern::Horizontal => parallel(&mut out, 0.0, spacing),
        Pattern::Vertical => parallel(&mut out, 90.0, spacing),
        Pattern::Grid => {
            parallel(&mut out, 0.0, spacing);
            parallel(&mut out, 90.0, spacing);
        }
        Pattern::Brick => {
            // Courses of height `spacing`, bricks twice as long, joints staggered.
            let course = spacing;
            parallel(&mut out, 0.0, course);
            let first = (lo.y / course).floor() as i64;
            let last = (hi.y / course).ceil() as i64;
            for row in first..last {
                let (y0, y1) = (row as f64 * course, (row + 1) as f64 * course);
                let shift = if row.rem_euclid(2) == 0 { 0.0 } else { course };
                let mut k = ((lo.x - shift) / (2.0 * course)).floor() as i64;
                while (k as f64) * 2.0 * course + shift <= hi.x && out.len() < MAX_LINES {
                    let x = k as f64 * 2.0 * course + shift;
                    for (t0, t1) in polygon::clip_line(poly, DVec2::new(x, 0.0), DVec2::Y) {
                        let (a, b) = (t0.max(y0), t1.min(y1));
                        if b > a {
                            out.push((DVec2::new(x, a), DVec2::new(x, b)));
                        }
                    }
                    k += 1;
                }
            }
        }
        Pattern::Sand => dots(&mut out, poly, lo, hi, spacing * 0.5, false),
        Pattern::Earth => weave(&mut out, poly, lo, hi, spacing * 2.0),
        Pattern::Concrete => dots(&mut out, poly, lo, hi, spacing, true),
        Pattern::Insulation => {
            // Zig-zag running along X, one wave per `spacing`.
            let amp = spacing * 0.5;
            let rows_first = (lo.y / spacing).floor() as i64;
            let rows_last = (hi.y / spacing).ceil() as i64;
            for row in rows_first..rows_last {
                let y = row as f64 * spacing;
                let mut k = (lo.x / spacing).floor() as i64;
                while (k as f64) * spacing <= hi.x && out.len() < MAX_LINES {
                    let x = k as f64 * spacing;
                    let a = DVec2::new(x, y);
                    let b = DVec2::new(x + spacing * 0.5, y + amp * 2.0);
                    let c = DVec2::new(x + spacing, y);
                    for (p, q) in [(a, b), (b, c)] {
                        clip_segment(&mut out, poly, p, q);
                    }
                    k += 1;
                }
            }
        }
    }
    out
}

/// Segments of `pattern` inside `poly` and outside each of `holes`.
pub fn hatch_with_holes(pattern: Pattern, poly: &[DVec2], holes: &[Vec<DVec2>], spacing: f64) -> Vec<(DVec2, DVec2)> {
    cut_holes(hatch(pattern, poly, spacing), holes)
}

/// Most segments one region may produce with a custom pattern.
pub const MAX_CUSTOM_SEGMENTS: usize = 20_000;
/// Most dash steps walked for one region (gaps included).
const MAX_STEPS: usize = 400_000;
/// Families whose lines would print closer than this are left out
/// (they would print as a gray blot), paper millimeters.
pub const MIN_PAPER_SPACING: f64 = 0.1;
/// Length of a dot (a zero dash), paper millimeters.
const DOT_MM: f64 = 0.15;

/// Drawing units per paper millimeter for a hatch `spacing` (see [`crate::cut::spacing`]).
pub fn paper_unit(spacing: f64) -> f64 {
    spacing / crate::cut::PATTERN_MM
}

/// Drawing units per millimeter of `fp`'s definition, with its scale: paper
/// millimeters for drafting patterns, real ones (drawings in meters) for
/// model patterns.
pub fn custom_unit(fp: &FillPattern, spacing: f64) -> f64 {
    let per_mm = match fp.kind {
        FillPatternKind::Drafting => paper_unit(spacing),
        FillPatternKind::Model => 0.001,
    };
    per_mm * fp.scale
}

/// Segments of `pattern` (built-in, or one of `patterns`) inside `poly`;
/// a custom pattern that is gone gives none.
pub fn hatch_in(patterns: &FillPatterns, pattern: Pattern, poly: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    match pattern {
        Pattern::Custom(id) => match patterns.get(&id) {
            Some(fp) => hatch_custom(fp, poly, custom_unit(fp, spacing), paper_unit(spacing)),
            None => Vec::new(),
        },
        p => hatch(p, poly, spacing),
    }
}

/// [`hatch_in`] outside each of `holes`.
pub fn hatch_with_holes_in(
    patterns: &FillPatterns,
    pattern: Pattern,
    poly: &[DVec2],
    holes: &[Vec<DVec2>],
    spacing: f64,
) -> Vec<(DVec2, DVec2)> {
    cut_holes(hatch_in(patterns, pattern, poly, spacing), holes)
}

/// The line families of `fp` inside `poly`, its lengths at `unit` drawing
/// units per millimeter (scale included), `paper` drawing units per paper
/// millimeter (too dense families are left out; dots are drawn as short
/// strokes). Lines are anchored to the drawing origin, so neighbouring
/// regions line up.
pub fn hatch_custom(fp: &FillPattern, poly: &[DVec2], unit: f64, paper: f64) -> Vec<(DVec2, DVec2)> {
    let mut out = Vec::new();
    if poly.len() < 3 || !(unit.is_finite() && unit > 0.0 && paper.is_finite() && paper > 0.0) {
        return out;
    }
    let (lo, hi) = bounds(poly);
    let corners = [lo, hi, DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)];
    let mut steps = 0usize;
    for family in fp.lines.iter().filter(|l| l.problem().is_none()) {
        let dir = DVec2::from_angle(family.angle.to_radians());
        let n = dir.perp();
        let origin = DVec2::new(family.origin[0], family.origin[1]) * unit;
        let (shift, gap) = (family.offset[0] * unit, family.offset[1] * unit);
        if gap.abs() < MIN_PAPER_SPACING * paper {
            continue;
        }
        let (o0, o1) =
            corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), c| (a.min(c.dot(n)), b.max(c.dot(n))));
        let base = origin.dot(n);
        let (ka, kb) = ((o0 - base) / gap, (o1 - base) / gap);
        let (first, last) = (ka.min(kb).ceil(), ka.max(kb).floor());
        if !(first.is_finite() && last.is_finite()) || last - first > 1e7 {
            continue;
        }
        let dashes: Vec<f64> = family.dashes.iter().map(|d| d * unit).collect();
        let period: f64 = dashes.iter().map(|d| d.abs()).sum();
        let dot = DOT_MM * paper;
        let mut k = first as i64;
        while k <= last as i64 {
            if out.len() >= MAX_CUSTOM_SEGMENTS || steps >= MAX_STEPS {
                return out;
            }
            let start = origin + dir * (k as f64 * shift) + n * (k as f64 * gap);
            k += 1;
            for (t0, t1) in polygon::clip_line(poly, start, dir) {
                if dashes.is_empty() || period <= 0.0 {
                    out.push((start + dir * t0, start + dir * t1));
                    steps += 1;
                    continue;
                }
                // The dashes run from the line's start point: begin at the repeat before t0.
                let mut pos = (t0 / period).floor() * period;
                let mut i = 0;
                while pos < t1 && steps < MAX_STEPS && out.len() < MAX_CUSTOM_SEGMENTS {
                    let d = dashes.get(i).copied().unwrap_or(0.0);
                    steps += 1;
                    if d > 0.0 {
                        let (a, b) = (pos.max(t0), (pos + d).min(t1));
                        if b > a {
                            out.push((start + dir * a, start + dir * b));
                        }
                    } else if d == 0.0 && pos >= t0 && pos < t1 {
                        out.push((start + dir * pos, start + dir * (pos + dot).min(t1)));
                    }
                    pos += d.abs();
                    i = (i + 1) % dashes.len();
                }
            }
        }
    }
    out
}

/// The parts of `lines` outside each of `holes`.
fn cut_holes(mut lines: Vec<(DVec2, DVec2)>, holes: &[Vec<DVec2>]) -> Vec<(DVec2, DVec2)> {
    for hole in holes.iter().filter(|h| h.len() >= 3) {
        lines = lines.into_iter().flat_map(|(a, b)| outside(hole, a, b)).collect();
    }
    lines
}

/// The parts of segment `a`-`b` outside `poly`.
fn outside(poly: &[DVec2], a: DVec2, b: DVec2) -> Vec<(DVec2, DVec2)> {
    let d = b - a;
    let len = d.length();
    if len < 1e-12 {
        return Vec::new();
    }
    let dir = d / len;
    let mut out = Vec::new();
    let mut from = 0.0;
    for (t0, t1) in polygon::clip_line(poly, a, dir) {
        let (t0, t1) = (t0.clamp(0.0, len), t1.clamp(0.0, len));
        if t0 > from {
            out.push((a + dir * from, a + dir * t0));
        }
        from = from.max(t1);
    }
    if len > from {
        out.push((a + dir * from, b));
    }
    out
}

fn bounds(poly: &[DVec2]) -> (DVec2, DVec2) {
    poly.iter()
        .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

/// The parts of segment `p`-`q` inside `poly`.
fn clip_segment(out: &mut Vec<(DVec2, DVec2)>, poly: &[DVec2], p: DVec2, q: DVec2) {
    let d = q - p;
    let len = d.length();
    if len < 1e-12 {
        return;
    }
    let dir = d / len;
    for (t0, t1) in polygon::clip_line(poly, p, dir) {
        let (a, b) = (t0.max(0.0), t1.min(len));
        if b > a {
            out.push((p + dir * a, p + dir * b));
        }
    }
}

/// Earth: a basket weave turned 45°. Square blocks of `cell` along the
/// diagonals, each with three parallel strokes stopping short of its sides,
/// along one diagonal in a block and the other in the next.
fn weave(out: &mut Vec<(DVec2, DVec2)>, poly: &[DVec2], lo: DVec2, hi: DVec2, cell: f64) {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let (u_axis, v_axis) = (DVec2::new(s, s), DVec2::new(-s, s));
    // The box's extent in the turned frame.
    let corners = [lo, hi, DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)];
    let range = |axis: DVec2| {
        corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), c| (a.min(c.dot(axis)), b.max(c.dot(axis))))
    };
    let ((u0, u1), (v0, v1)) = (range(u_axis), range(v_axis));
    let (i0, i1) = ((u0 / cell).floor() as i64, (u1 / cell).ceil() as i64);
    let (j0, j1) = ((v0 / cell).floor() as i64, (v1 / cell).ceil() as i64);
    let gap = cell * 0.12;
    for i in i0..i1 {
        for j in j0..j1 {
            if out.len() >= MAX_LINES {
                return;
            }
            let (ua, va) = (i as f64 * cell, j as f64 * cell);
            let at = |u: f64, v: f64| u_axis * u + v_axis * v;
            for k in 1..=3 {
                let t = k as f64 * cell / 4.0;
                let (p, q) = if (i + j).rem_euclid(2) == 0 {
                    (at(ua + gap, va + t), at(ua + cell - gap, va + t))
                } else {
                    (at(ua + t, va + gap), at(ua + t, va + cell - gap))
                };
                clip_segment(out, poly, p, q);
            }
        }
    }
}

/// Deterministic jitter in [0, 1) for a grid cell.
fn jitter(i: i64, j: i64, salt: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ salt;
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Scattered dots (tiny segments) and, for concrete, small triangles.
fn dots(out: &mut Vec<(DVec2, DVec2)>, poly: &[DVec2], lo: DVec2, hi: DVec2, step: f64, triangles: bool) {
    let dot = step * 0.06;
    let (i0, i1) = ((lo.x / step).floor() as i64, (hi.x / step).ceil() as i64);
    let (j0, j1) = ((lo.y / step).floor() as i64, (hi.y / step).ceil() as i64);
    for i in i0..i1 {
        for j in j0..j1 {
            if out.len() >= MAX_LINES {
                return;
            }
            let p = DVec2::new((i as f64 + jitter(i, j, 1)) * step, (j as f64 + jitter(i, j, 2)) * step);
            if !polygon::contains(poly, p) {
                continue;
            }
            if triangles && jitter(i, j, 3) < 0.25 {
                let r = step * 0.22;
                let a = jitter(i, j, 4) * std::f64::consts::TAU;
                let pts: Vec<DVec2> =
                    (0..3).map(|k| p + DVec2::from_angle(a + k as f64 * std::f64::consts::TAU / 3.0) * r).collect();
                for k in 0..3 {
                    clip_segment(out, poly, pts[k], pts[(k + 1) % 3]);
                }
            } else {
                out.push((p, p + DVec2::new(dot, dot * 0.3)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Vec<DVec2> {
        vec![DVec2::ZERO, DVec2::new(s, 0.0), DVec2::new(s, s), DVec2::new(0.0, s)]
    }

    #[test]
    fn lines_stay_inside_and_follow_the_spacing() {
        let poly = square(1.0);
        let h = hatch(Pattern::Horizontal, &poly, 0.1);
        // y = 0.1 .. 0.9 (the edges at 0 and 1 are on the boundary).
        assert!((9..=11).contains(&h.len()), "{}", h.len());
        for (a, b) in hatch(Pattern::Crosshatch, &poly, 0.1).iter().chain(&h) {
            for p in [a, b] {
                assert!(p.x > -1e-9 && p.x < 1.0 + 1e-9 && p.y > -1e-9 && p.y < 1.0 + 1e-9, "{p}");
            }
        }
        for pattern in Pattern::ALL {
            let lines = hatch(pattern, &poly, 0.1);
            let empty = matches!(pattern, Pattern::None | Pattern::Solid);
            assert_eq!(lines.is_empty(), empty, "{pattern:?}");
            assert!(lines.iter().all(|(a, b)| a.is_finite() && b.is_finite()));
        }
    }

    fn custom(kind: FillPatternKind, lines: Vec<cc_core::PatternLine>) -> FillPattern {
        FillPattern {
            id: cc_core::FillPatternId(7),
            name: "Mine".into(),
            description: String::new(),
            kind,
            scale: 1.0,
            lines,
        }
    }

    #[test]
    fn custom_patterns_follow_their_families_and_kind() {
        let poly = square(1.0);
        // Model: 100 mm apart in the world, whatever the scale.
        let model = custom(FillPatternKind::Model, vec![cc_core::PatternLine::solid(0.0, 100.0)]);
        let mut patterns = FillPatterns::new();
        patterns.insert(model.id, model.clone());
        for scale in [20, 100] {
            let lines = hatch_in(&patterns, Pattern::Custom(model.id), &poly, crate::cut::spacing(scale));
            assert!((9..=11).contains(&lines.len()), "1:{scale}: {}", lines.len());
        }
        // Drafting: 2 mm on paper, so 0.1 m apart at 1:50.
        let drafting = custom(FillPatternKind::Drafting, vec![cc_core::PatternLine::solid(90.0, 2.0)]);
        patterns.insert(drafting.id, drafting);
        let lines = hatch_in(&patterns, Pattern::Custom(drafting_id()), &poly, crate::cut::spacing(50));
        assert!((9..=11).contains(&lines.len()), "{}", lines.len());
        assert!(lines.iter().all(|(a, b)| (a.x - b.x).abs() < 1e-9));
        // Dashes: half drawn, so about half the length of continuous lines.
        let dashed = custom(
            FillPatternKind::Model,
            vec![cc_core::PatternLine { dashes: vec![50.0, -50.0], ..cc_core::PatternLine::solid(0.0, 100.0) }],
        );
        let total = |l: &[(DVec2, DVec2)]| l.iter().map(|(a, b)| a.distance(*b)).sum::<f64>();
        let full = hatch_custom(&model, &poly, 0.001, 0.05);
        let half = hatch_custom(&dashed, &poly, 0.001, 0.05);
        assert!((total(&half) / total(&full) - 0.5).abs() < 0.05, "{} {}", total(&half), total(&full));
        for (a, b) in &half {
            for p in [a, b] {
                assert!(p.x > -1e-9 && p.x < 1.0 + 1e-9 && p.y > -1e-9 && p.y < 1.0 + 1e-9, "{p}");
            }
        }
        // A pattern that is gone draws nothing.
        assert!(hatch_in(&patterns, Pattern::Custom(cc_core::FillPatternId(99)), &poly, 0.1).is_empty());
    }

    fn drafting_id() -> cc_core::FillPatternId {
        cc_core::FillPatternId(7)
    }

    #[test]
    fn custom_patterns_are_bounded() {
        // Too dense on paper: left out.
        let fine = custom(FillPatternKind::Model, vec![cc_core::PatternLine::solid(0.0, 1.0)]);
        assert!(
            hatch_custom(&fine, &square(1.0), 0.001, crate::hatch::paper_unit(crate::cut::spacing(100))).is_empty()
        );
        // Huge regions at fine dashes stop at the cap.
        let dots = custom(
            FillPatternKind::Model,
            vec![cc_core::PatternLine { dashes: vec![0.0, -0.5], ..cc_core::PatternLine::solid(30.0, 1.0) }],
        );
        let lines = hatch_custom(&dots, &square(100.0), 0.001, 1e-5);
        assert!(!lines.is_empty() && lines.len() <= MAX_CUSTOM_SEGMENTS);
        assert!(lines.iter().all(|(a, b)| a.is_finite() && b.is_finite()));
        assert!(hatch_custom(&dots, &square(1.0), f64::NAN, 1.0).is_empty());
    }

    #[test]
    fn hostile_input_is_bounded() {
        let holed = hatch_with_holes(
            Pattern::Vertical,
            &square(1.0),
            &[vec![DVec2::new(0.25, 0.25), DVec2::new(0.75, 0.25), DVec2::new(0.75, 0.75), DVec2::new(0.25, 0.75)]],
            0.1,
        );
        let inside = |p: DVec2| p.x > 0.26 && p.x < 0.74 && p.y > 0.26 && p.y < 0.74;
        assert!(!holed.is_empty() && holed.iter().all(|(a, b)| !inside((*a + *b) * 0.5)));
        assert!(hatch(Pattern::Grid, &square(1.0), 0.0).is_empty());
        assert!(hatch(Pattern::Grid, &square(1.0), f64::NAN).is_empty());
        assert!(hatch(Pattern::Grid, &square(1e6), 1e-3).len() <= MAX_LINES * 2);
        assert!(hatch(Pattern::Sand, &square(1e6), 1e-3).len() <= MAX_LINES);
    }
}
