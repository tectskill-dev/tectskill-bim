//! Lookups over a drawing's primitives: the primitives of each element and
//! a grid of the pickable ones, so picking under the pointer does not walk
//! every primitive of a large drawing (an imported DXF holds 100 000).

use crate::drawing::Prim;
use cc_core::ElementId;
use glam::DVec2;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Drawings with fewer primitives are searched in full.
const MIN_PRIMS: usize = 64;
/// Most cells of the grid.
const MAX_CELLS: usize = 1 << 20;
/// Most cells on one side of the grid.
const MAX_SIDE: usize = 1024;
/// A primitive whose box covers more cells is kept apart and always tested.
const MAX_CELLS_PER_PRIM: usize = 64;

/// Built once for a drawing whose primitives no longer change: the cached
/// drawings of views are shared and read only. Each lookup checks that the
/// primitives are still the ones it was built from (their count and
/// storage); when they are not, callers fall back to searching them all.
#[derive(Default)]
pub struct DrawingIndex {
    elements: OnceLock<Stamped<HashMap<ElementId, Vec<u32>>>>,
    grid: OnceLock<Stamped<Option<Grid>>>,
}

impl Clone for DrawingIndex {
    /// A copy is built again for the copied primitives.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for DrawingIndex {
    /// Derived data: drawings compare by their primitives.
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for DrawingIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DrawingIndex")
    }
}

/// A lookup with the primitives it was built from.
struct Stamped<T> {
    len: usize,
    ptr: usize,
    value: T,
}

fn stamp(prims: &[Prim]) -> (usize, usize) {
    (prims.len(), prims.as_ptr() as usize)
}

impl<T> Stamped<T> {
    fn of(&self, prims: &[Prim]) -> Option<&T> {
        ((self.len, self.ptr) == stamp(prims)).then_some(&self.value)
    }
}

impl DrawingIndex {
    /// The primitives of each element, in drawing order.
    pub(crate) fn elements(&self, prims: &[Prim]) -> Option<&HashMap<ElementId, Vec<u32>>> {
        if prims.len() > u32::MAX as usize {
            return None;
        }
        let built = self.elements.get_or_init(|| {
            let mut map: HashMap<ElementId, Vec<u32>> = HashMap::new();
            for (i, prim) in prims.iter().enumerate() {
                if let Some(id) = prim.element() {
                    map.entry(id).or_default().push(i as u32);
                }
            }
            let (len, ptr) = stamp(prims);
            Stamped { len, ptr, value: map }
        });
        built.of(prims)
    }

    /// The pickable primitives (lines, fills and pictures of an element)
    /// that may lie within `tol` of `p`, sorted in drawing order; `None`
    /// when they should all be searched.
    pub(crate) fn candidates(&self, prims: &[Prim], p: DVec2, tol: f64) -> Option<Vec<u32>> {
        if prims.len() < MIN_PRIMS || prims.len() > u32::MAX as usize || !p.is_finite() || !tol.is_finite() {
            return None;
        }
        let built = self.grid.get_or_init(|| {
            let (len, ptr) = stamp(prims);
            Stamped { len, ptr, value: Grid::build(prims) }
        });
        built.of(prims)?.as_ref()?.near(p, tol)
    }
}

/// Pickable primitives by the cells of a regular grid over their extents.
struct Grid {
    min: DVec2,
    cell: DVec2,
    nx: usize,
    ny: usize,
    /// Where each cell's primitives start in `items` (one more than the cells).
    starts: Vec<u32>,
    items: Vec<u32>,
    /// Primitives covering many cells, or not finite: always tested.
    big: Vec<u32>,
}

/// The box of a pickable primitive, if it is one.
fn pick_box(prim: &Prim) -> Option<(DVec2, DVec2)> {
    let fold = |pts: &mut dyn Iterator<Item = DVec2>| {
        let first = pts.next()?;
        Some(pts.fold((first, first), |(lo, hi), q| (lo.min(q), hi.max(q))))
    };
    match prim {
        Prim::Line { a, b, element: Some(_), .. } => Some((a.min(*b), a.max(*b))),
        Prim::Fill { pts, element: Some(_), .. } => fold(&mut pts.iter().copied()),
        Prim::Image { origin, u, v, element: Some(_), .. } => {
            fold(&mut Prim::image_corners(*origin, *u, *v).into_iter())
        }
        _ => None,
    }
}

/// A box grown a little against rounding at its edges.
fn padded((lo, hi): (DVec2, DVec2)) -> (DVec2, DVec2) {
    let pad = 1e-9 * (1.0 + lo.abs().max(hi.abs()).max_element());
    (lo - DVec2::splat(pad), hi + DVec2::splat(pad))
}

impl Grid {
    fn build(prims: &[Prim]) -> Option<Grid> {
        let boxes: Vec<(u32, (DVec2, DVec2))> =
            prims.iter().enumerate().filter_map(|(i, prim)| Some((i as u32, padded(pick_box(prim)?)))).collect();
        let finite = |(lo, hi): &(DVec2, DVec2)| lo.is_finite() && hi.is_finite();
        let (min, max) = boxes.iter().filter(|(_, b)| finite(b)).fold(
            None,
            |acc: Option<(DVec2, DVec2)>, (_, (lo, hi))| match acc {
                None => Some((*lo, *hi)),
                Some((a, b)) => Some((a.min(*lo), b.max(*hi))),
            },
        )?;
        let size = max - min;
        if !size.is_finite() {
            return None;
        }
        let target = boxes.len().clamp(1, MAX_CELLS) as f64;
        let aspect = if size.y > 0.0 && size.x > 0.0 { size.x / size.y } else { 1.0 };
        let nx = if size.x > 0.0 { ((target * aspect).sqrt().round() as usize).clamp(1, MAX_SIDE) } else { 1 };
        let ny = if size.y > 0.0 { ((target / nx as f64).round() as usize).clamp(1, MAX_SIDE) } else { 1 };
        let cell = DVec2::new(
            if size.x > 0.0 { size.x / nx as f64 } else { 1.0 },
            if size.y > 0.0 { size.y / ny as f64 } else { 1.0 },
        );
        let mut grid = Grid { min, cell, nx, ny, starts: Vec::new(), items: Vec::new(), big: Vec::new() };
        // Count, then fill (one array for every cell's list).
        let mut counts = vec![0u32; nx * ny];
        let mut kept: Vec<(u32, [usize; 4])> = Vec::with_capacity(boxes.len());
        for (i, b) in &boxes {
            let range = grid.range(b.0, b.1);
            let cells = (range[1] - range[0] + 1) * (range[3] - range[2] + 1);
            if !finite(b) || cells > MAX_CELLS_PER_PRIM {
                grid.big.push(*i);
                continue;
            }
            for y in range[2]..=range[3] {
                for x in range[0]..=range[1] {
                    if let Some(c) = counts.get_mut(y * nx + x) {
                        *c += 1;
                    }
                }
            }
            kept.push((*i, range));
        }
        let mut starts = Vec::with_capacity(counts.len() + 1);
        let mut total = 0u32;
        starts.push(0);
        for c in &counts {
            total = total.saturating_add(*c);
            starts.push(total);
        }
        let mut next: Vec<u32> = starts.clone();
        let mut items = vec![0u32; total as usize];
        for (i, [x0, x1, y0, y1]) in kept {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(slot) = next.get_mut(y * nx + x)
                        && let Some(item) = items.get_mut(*slot as usize)
                    {
                        *item = i;
                        *slot += 1;
                    }
                }
            }
        }
        grid.starts = starts;
        grid.items = items;
        Some(grid)
    }

    /// The cells (x from, x to, y from, y to, inclusive) the box `lo`..`hi` touches.
    fn range(&self, lo: DVec2, hi: DVec2) -> [usize; 4] {
        let cell = |v: f64, min: f64, size: f64, n: usize| {
            let c = ((v - min) / size).floor();
            if c.is_nan() || c < 0.0 { 0 } else { (c as usize).min(n - 1) }
        };
        [
            cell(lo.x, self.min.x, self.cell.x, self.nx),
            cell(hi.x, self.min.x, self.cell.x, self.nx),
            cell(lo.y, self.min.y, self.cell.y, self.ny),
            cell(hi.y, self.min.y, self.cell.y, self.ny),
        ]
    }

    /// The primitives whose cells meet the box of `p` grown by `tol`
    /// (sorted, each once), or `None` when that box covers much of the grid.
    fn near(&self, p: DVec2, tol: f64) -> Option<Vec<u32>> {
        let (lo, hi) = padded((p - DVec2::splat(tol.max(0.0)), p + DVec2::splat(tol.max(0.0))));
        let [x0, x1, y0, y1] = self.range(lo, hi);
        if (x1 - x0 + 1) * (y1 - y0 + 1) > (self.nx * self.ny / 4).max(16) {
            return None;
        }
        let mut out = self.big.clone();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = y * self.nx + x;
                let (Some(s), Some(e)) = (self.starts.get(c), self.starts.get(c + 1)) else { continue };
                out.extend(self.items.get(*s as usize..*e as usize).unwrap_or_default());
            }
        }
        out.sort_unstable();
        out.dedup();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::drawing::{Drawing2D, LineStyle, Prim};
    use cc_core::ElementId;
    use glam::DVec2;

    /// A small deterministic generator (no random crate in the workspace).
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
    }

    /// The pick searching every primitive, as before the index.
    fn linear(d: &Drawing2D, p: DVec2, tol: f64) -> Option<ElementId> {
        d.hit_test_all(p, tol)
    }

    fn random_drawing(rng: &mut Lcg, n: usize) -> Drawing2D {
        let mut d = Drawing2D::default();
        d.background.insert(ElementId(1));
        d.background.insert(ElementId(2));
        for _ in 0..n {
            let id = Some(ElementId(rng.range(0.0, 40.0) as u64));
            let at = DVec2::new(rng.range(-50.0, 50.0), rng.range(-30.0, 30.0));
            match (rng.next() * 10.0) as u32 {
                0..=5 => {
                    // Mostly short lines, some long ones across the drawing.
                    let len = if rng.next() < 0.05 { 80.0 } else { 3.0 };
                    let b = at + DVec2::new(rng.range(-len, len), rng.range(-len, len));
                    let element = if rng.next() < 0.05 { None } else { id };
                    d.prims.push(Prim::Line { a: at, b, style: LineStyle::Thin, element, pen: None });
                }
                6..=8 => {
                    let k = 3 + (rng.next() * 4.0) as usize;
                    let r = rng.range(0.5, 8.0);
                    let pts = (0..k)
                        .map(|i| {
                            let a = i as f64 / k as f64 * std::f64::consts::TAU + rng.range(0.0, 0.5);
                            at + DVec2::new(a.cos(), a.sin()) * r * rng.range(0.4, 1.0)
                        })
                        .collect();
                    let alpha = if rng.next() < 0.2 { 0 } else { 255 };
                    d.prims.push(Prim::Fill { pts, color: [1, 2, 3, alpha], element: id, cut: false });
                }
                _ => d.prims.push(Prim::Image {
                    origin: at,
                    u: DVec2::new(rng.range(0.5, 6.0), rng.range(-1.0, 1.0)),
                    v: DVec2::new(rng.range(-1.0, 1.0), rng.range(0.5, 6.0)),
                    image: std::sync::Arc::new(cc_core::raster::RasterImage {
                        format: cc_core::raster::RasterFormat::Png,
                        width: 1,
                        height: 1,
                        data: Vec::new(),
                    }),
                    element: id,
                    faded: false,
                }),
            }
        }
        d
    }

    /// The indexed pick gives what searching every primitive gives.
    #[test]
    fn the_indexed_pick_matches_the_full_search() {
        let mut rng = Lcg(7);
        for n in [70, 500, 5000] {
            let d = random_drawing(&mut rng, n);
            let (mut indexed, mut picked) = (0, 0);
            for _ in 0..3000 {
                let p = DVec2::new(rng.range(-70.0, 70.0), rng.range(-45.0, 45.0));
                let tol = [0.0, 0.05, 0.5, 3.0][(rng.next() * 4.0) as usize];
                indexed += usize::from(d.index.candidates(&d.prims, p, tol).is_some());
                let hit = d.hit_test(p, tol);
                picked += usize::from(hit.is_some());
                assert_eq!(hit, linear(&d, p, tol), "{n} {p} {tol}");
                assert_eq!(d.hit_test_line(p, tol), d.nearest_line_all(p, tol), "{n} {p} {tol}");
            }
            assert!(indexed > 1500 && picked > 100, "{n}: {indexed} indexed, {picked} picked");
            // Points exactly on primitives' ends and corners.
            for prim in d.prims.iter().take(300) {
                let p = match prim {
                    Prim::Line { a, .. } => *a,
                    Prim::Fill { pts, .. } => pts[0],
                    Prim::Image { origin, .. } => *origin,
                    Prim::Text { pos, .. } => *pos,
                };
                assert_eq!(d.hit_test(p, 0.0), linear(&d, p, 0.0));
                assert_eq!(d.hit_test(p, 0.01), linear(&d, p, 0.01));
            }
        }
    }

    #[test]
    fn primitives_of_elements_come_in_drawing_order() {
        let mut rng = Lcg(3);
        let d = random_drawing(&mut rng, 400);
        let ids: std::collections::BTreeSet<_> = [ElementId(3), ElementId(17), ElementId(99)].into_iter().collect();
        let expected: Vec<usize> =
            (0..d.prims.len()).filter(|i| d.prims[*i].element().is_some_and(|e| ids.contains(&e))).collect();
        assert_eq!(d.prims_of(&ids), expected);
    }

    /// Primitives added after the index was built are still found.
    #[test]
    fn a_changed_drawing_is_searched_in_full() {
        let mut rng = Lcg(5);
        let mut d = random_drawing(&mut rng, 300);
        let far = DVec2::new(500.0, 500.0);
        assert_eq!(d.hit_test(far, 0.1), None);
        d.prims.push(Prim::Line {
            a: far,
            b: far + DVec2::X,
            style: LineStyle::Cut,
            element: Some(ElementId(77)),
            pen: None,
        });
        assert_eq!(d.hit_test(far, 0.1), Some(ElementId(77)));
        assert_eq!(d.prims_of(&[ElementId(77)].into_iter().collect()), vec![d.prims.len() - 1]);
    }

    /// Points that are not finite pick nothing, as before.
    #[test]
    fn odd_points_and_primitives_are_searched_in_full() {
        let mut rng = Lcg(9);
        let mut d = random_drawing(&mut rng, 200);
        d.prims.push(Prim::Line {
            a: DVec2::new(f64::NAN, 0.0),
            b: DVec2::ZERO,
            style: LineStyle::Cut,
            element: Some(ElementId(5)),
            pen: None,
        });
        for p in [DVec2::new(f64::NAN, 1.0), DVec2::new(f64::INFINITY, 0.0), DVec2::ZERO] {
            assert_eq!(d.hit_test(p, 0.5), linear(&d, p, 0.5));
        }
    }
}
