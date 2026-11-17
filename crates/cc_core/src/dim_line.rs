//! The dimension line of linear and aligned dimensions: lining it up with
//! the lines of other dimensions, and chains of dimensions that share one
//! line (continuous dimensioning).
//!
//! The line of a dimension with direction `d` (unit) is the set of points
//! `x` with `x · n = a · n + offset`, where `n = d.perp()` (the left of `d`).

use crate::{Dimension, DimensionKind, ViewId};
use glam::DVec2;

/// Largest sine of the angle between two dimension lines that still counts
/// as parallel (about 0.06°).
pub const PARALLEL_SIN: f64 = 1e-3;

/// The drawn extent of a linear or aligned dimension's line: where the
/// witness lines of `a` and `b` meet it. `None` for other kinds or a
/// degenerate direction.
pub fn extent(dim: &Dimension) -> Option<(DVec2, DVec2)> {
    let d = dim.direction()?;
    let pa = dim.a + d.perp() * dim.offset;
    let pb = pa + d * (dim.b - dim.a).dot(d);
    (pa.is_finite() && pb.is_finite()).then_some((pa, pb))
}

/// The offset (from `dim.a`, left of its direction positive) that puts the
/// line of `dim` through `p`.
pub fn offset_through(dim: &Dimension, p: DVec2) -> Option<f64> {
    let d = dim.direction()?;
    let offset = (p - dim.a).dot(d.perp());
    offset.is_finite().then_some(offset)
}

/// The line of `dim` lined up with another dimension's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineAlignment {
    /// The offset of `dim` that puts its line on the other line.
    pub offset: f64,
    /// The end of the other dimension's line nearest to `dim` (where a guide starts).
    pub reference: DVec2,
    /// The point of the aligned line of `dim` nearest to `reference` (where the guide ends).
    pub point: DVec2,
}

/// Snap the line of `dim` (a linear or aligned dimension being placed, at
/// its current offset) onto the line of a parallel dimension of `others`
/// when it lies within `tol` (model units) of it. The nearest line wins;
/// `None` when no parallel line is close enough.
pub fn align<'a>(dim: &Dimension, others: impl IntoIterator<Item = &'a Dimension>, tol: f64) -> Option<LineAlignment> {
    if !(tol.is_finite() && tol > 0.0) || !dim.offset.is_finite() {
        return None;
    }
    let d = dim.direction()?;
    let mut best: Option<(LineAlignment, f64)> = None;
    for other in others {
        let Some(od) = other.direction() else { continue };
        if d.perp_dot(od).abs() > PARALLEL_SIN {
            continue;
        }
        let Some((oa, ob)) = extent(other) else { continue };
        let Some(offset) = offset_through(dim, oa) else { continue };
        let gap = (offset - dim.offset).abs();
        if gap > tol || best.is_some_and(|(_, g)| gap >= g) {
            continue;
        }
        let Some((pa, pb)) = extent(&Dimension { offset, ..dim.clone() }) else { continue };
        let near = |p: DVec2| closest_on_segment(pa, pb, p);
        let reference = if near(oa).distance(oa) <= near(ob).distance(ob) { oa } else { ob };
        best = Some((LineAlignment { offset, reference, point: near(reference) }, gap));
    }
    best.map(|(a, _)| a)
}

fn closest_on_segment(a: DVec2, b: DVec2, p: DVec2) -> DVec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 1e-18 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    a + ab * t
}

/// The dimensions of a chain through `points` measured along `dir`, all on
/// the line at `offset` from the first point (left of `dir` positive): one
/// dimension from each point to the next. With `aligned`, a segment along
/// `dir` (or against it) is an aligned dimension, so it keeps following the
/// direction of what it measures; any other segment (and every segment
/// without `aligned`) is a linear dimension along `dir`. `None` when `dir`
/// is degenerate, a value is not finite, or a segment measures nothing
/// along `dir` (shorter than 1e-6).
pub fn chain(view: ViewId, points: &[DVec2], dir: DVec2, offset: f64, aligned: bool) -> Option<Vec<Dimension>> {
    let dir = dir.try_normalize()?;
    let first = *points.first()?;
    if !offset.is_finite() || points.len() < 2 || points.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let on_line = first + dir.perp() * offset;
    points
        .windows(2)
        .map(|w| {
            let (a, b) = (*w.first()?, *w.get(1)?);
            if (b - a).dot(dir).abs() <= 1e-6 {
                return None;
            }
            let along = (b - a).try_normalize().is_some_and(|u| u.perp_dot(dir).abs() <= PARALLEL_SIN);
            let kind = if aligned && along { DimensionKind::Aligned } else { DimensionKind::Linear { dir } };
            let dim = Dimension { view, a, b, offset: 0.0, kind, refs: Default::default() };
            let offset = offset_through(&dim, on_line)?;
            Some(Dimension { offset, ..dim })
        })
        .collect()
}

/// The running (ordinate) dimensions from the origin `points[0]` to each
/// other point, measured along `dir`, all on the line at `offset` from the
/// origin (left of `dir` positive). `None` when `dir` is degenerate, a value
/// is not finite, or a point measures nothing from the origin along `dir`
/// (closer than 1e-6).
pub fn ordinate(view: ViewId, points: &[DVec2], dir: DVec2, offset: f64) -> Option<Vec<Dimension>> {
    let dir = dir.try_normalize()?;
    let (origin, rest) = points.split_first()?;
    if !offset.is_finite() || rest.is_empty() || points.iter().any(|p| !p.is_finite()) {
        return None;
    }
    rest.iter()
        .map(|b| {
            ((*b - *origin).dot(dir).abs() > 1e-6).then(|| Dimension {
                view,
                a: *origin,
                b: *b,
                offset,
                kind: DimensionKind::Ordinate { dir },
                refs: Default::default(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim(a: (f64, f64), b: (f64, f64), offset: f64, kind: DimensionKind) -> Dimension {
        Dimension { view: ViewId(1), a: a.into(), b: b.into(), offset, kind, refs: Default::default() }
    }

    /// The constant `c` of the line `x · n = c` for the direction `dir`.
    fn line_constant(d: &Dimension, dir: DVec2) -> f64 {
        let (pa, _) = extent(d).unwrap();
        pa.dot(dir.perp())
    }

    #[test]
    fn extent_of_aligned_and_linear_dimensions() {
        let (pa, pb) = extent(&dim((0.0, 0.0), (4.0, 0.0), 1.0, DimensionKind::Aligned)).unwrap();
        assert_eq!((pa, pb), (DVec2::new(0.0, 1.0), DVec2::new(4.0, 1.0)));
        let (pa, pb) = extent(&dim((0.0, 0.0), (3.0, 4.0), -2.0, DimensionKind::Linear { dir: DVec2::X })).unwrap();
        assert_eq!((pa, pb), (DVec2::new(0.0, -2.0), DVec2::new(3.0, -2.0)));
        assert!(extent(&dim((0.0, 0.0), (3.0, 4.0), 1.0, DimensionKind::Radial)).is_none());
    }

    #[test]
    fn a_line_snaps_onto_a_parallel_dimension_line() {
        // An existing horizontal dimension line at y = 2.
        let existing = dim((10.0, 0.0), (14.0, 0.0), 2.0, DimensionKind::Aligned);
        // A new one from (0, 0.5) to (5, 0.5) with its line at y = 2.08.
        let placing = dim((0.0, 0.5), (5.0, 0.5), 1.58, DimensionKind::Linear { dir: DVec2::X });
        let a = align(&placing, [&existing], 0.1).unwrap();
        assert!((a.offset - 1.5).abs() < 1e-12, "{a:?}");
        // The guide runs from the near end of the other line to this one.
        assert_eq!(a.reference, DVec2::new(10.0, 2.0));
        assert!(a.point.distance(DVec2::new(5.0, 2.0)) < 1e-12);
        // Too far: no snap.
        assert!(align(&placing, [&existing], 0.05).is_none());
    }

    #[test]
    fn opposite_directions_and_the_nearest_line() {
        // Measured right to left: its left is down, the line at y = 2 is offset -2.
        let existing = dim((14.0, 0.0), (10.0, 0.0), -2.0, DimensionKind::Aligned);
        let closer = dim((0.0, 0.0), (4.0, 0.0), 3.0, DimensionKind::Aligned);
        let placing = dim((0.0, 0.0), (5.0, 0.0), 2.9, DimensionKind::Aligned);
        let a = align(&placing, [&existing, &closer], 0.5).unwrap();
        assert!((a.offset - 3.0).abs() < 1e-12, "{a:?}");
        let a = align(&placing, [&existing], 0.95).unwrap();
        assert!((a.offset - 2.0).abs() < 1e-12, "{a:?}");
    }

    #[test]
    fn crossing_lines_and_bad_input_do_not_snap() {
        let vertical = dim((0.0, 0.0), (0.0, 4.0), 1.0, DimensionKind::Aligned);
        let slanted = dim((0.0, 0.0), (4.0, 0.1), 1.0, DimensionKind::Aligned);
        let placing = dim((0.0, 0.0), (5.0, 0.0), 1.0, DimensionKind::Aligned);
        assert!(align(&placing, [&vertical, &slanted], 1.0).is_none());
        let same = dim((0.0, 0.0), (5.0, 0.0), 1.0, DimensionKind::Aligned);
        assert!(align(&placing, [&same], f64::NAN).is_none());
        assert!(align(&placing, [&same], 0.0).is_none());
        let nan = Dimension { offset: f64::NAN, ..placing.clone() };
        assert!(align(&nan, [&same], 1.0).is_none());
        let angle =
            dim((0.0, 0.0), (5.0, 0.0), 1.0, DimensionKind::Angular { vertex: DVec2::ZERO, rays: [None, None] });
        assert!(align(&angle, [&same], 1.0).is_none());
    }

    #[test]
    fn a_chain_shares_one_line() {
        let pts = [DVec2::new(0.0, 0.0), DVec2::new(3.0, 0.5), DVec2::new(5.0, -1.0), DVec2::new(9.0, 0.0)];
        let dims = chain(ViewId(1), &pts, DVec2::X, 2.0, false).unwrap();
        assert_eq!(dims.len(), 3);
        for (d, w) in dims.iter().zip(pts.windows(2)) {
            assert_eq!((d.a, d.b), (w[0], w[1]));
            assert_eq!(d.kind, DimensionKind::Linear { dir: DVec2::X });
            assert!((line_constant(d, DVec2::X) - 2.0).abs() < 1e-12);
        }
        let total: f64 = dims.iter().map(Dimension::value).sum();
        assert!((total - 9.0).abs() < 1e-12);
    }

    #[test]
    fn an_aligned_chain_keeps_collinear_segments_aligned() {
        // Along a 45° line, then a point off it.
        let dir = DVec2::new(1.0, 1.0).normalize();
        let pts = [DVec2::new(0.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(3.0, 3.0), DVec2::new(5.0, 4.0)];
        let dims = chain(ViewId(1), &pts, dir, -0.5, true).unwrap();
        assert_eq!(dims[0].kind, DimensionKind::Aligned);
        assert_eq!(dims[1].kind, DimensionKind::Aligned);
        assert!(matches!(dims[2].kind, DimensionKind::Linear { dir: d } if d.distance(dir) < 1e-12));
        for d in &dims {
            assert!((line_constant(d, dir) + 0.5).abs() < 1e-12);
        }
        // A segment measured backwards stays aligned, with its offset flipped.
        let back = chain(ViewId(1), &[DVec2::new(4.0, 0.0), DVec2::new(1.0, 0.0)], DVec2::X, 1.0, true).unwrap();
        assert_eq!(back[0].kind, DimensionKind::Aligned);
        assert!((back[0].offset + 1.0).abs() < 1e-12);
        assert_eq!(extent(&back[0]).unwrap().0, DVec2::new(4.0, 1.0));
    }

    #[test]
    fn degenerate_chains_are_refused() {
        let id = ViewId(1);
        assert!(chain(id, &[DVec2::ZERO], DVec2::X, 1.0, false).is_none());
        assert!(chain(id, &[DVec2::ZERO, DVec2::X], DVec2::ZERO, 1.0, false).is_none());
        assert!(chain(id, &[DVec2::ZERO, DVec2::new(0.0, 3.0)], DVec2::X, 1.0, false).is_none());
        assert!(chain(id, &[DVec2::ZERO, DVec2::X], DVec2::X, f64::INFINITY, false).is_none());
        assert!(chain(id, &[DVec2::ZERO, DVec2::new(f64::NAN, 0.0)], DVec2::X, 1.0, false).is_none());
    }

    #[test]
    fn ordinate_sets_share_their_origin_and_line() {
        let points = [DVec2::new(1.0, 1.0), DVec2::new(1.6, 1.2), DVec2::new(3.4, 0.9)];
        let dims = ordinate(ViewId(1), &points, DVec2::new(2.0, 0.0), 0.5).unwrap();
        assert_eq!(dims.len(), 2);
        for (d, value) in dims.iter().zip([0.6, 2.4]) {
            assert_eq!(d.a, points[0]);
            assert!((d.value() - value).abs() < 1e-12);
            assert_eq!(d.kind, DimensionKind::Ordinate { dir: DVec2::X });
            // Every line at y = 1.5.
            let (pa, pb) = extent(d).unwrap();
            assert!((pa.y - 1.5).abs() < 1e-12 && (pb.y - 1.5).abs() < 1e-12);
        }
        assert!(ordinate(ViewId(1), &points[..1], DVec2::X, 0.5).is_none());
        assert!(ordinate(ViewId(1), &points, DVec2::ZERO, 0.5).is_none());
        assert!(ordinate(ViewId(1), &[points[0], DVec2::new(1.0, 5.0)], DVec2::X, 0.5).is_none());
        assert!(ordinate(ViewId(1), &points, DVec2::X, f64::NAN).is_none());
    }
}
