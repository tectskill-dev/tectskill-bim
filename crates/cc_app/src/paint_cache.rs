//! Painted drawings kept between frames.
//!
//! A large drawing (an imported DXF holds 100 000 primitives) is slow to
//! turn into shapes and tessellate, and a 2D view repaints on every pointer
//! move. [`PaintCache`] keeps each view's drawing tessellated at the view's
//! zoom, in screen points relative to where it was built: a frame only
//! moves the meshes by the pan since. It is built again when the drawing
//! (its shared allocation), the zoom, the text scale or the pixel density
//! changes, when the view pans past the area it was built for (the drawing
//! is built for the view and a view's size around it, or all of it when it
//! fits there), or when the frame's vertex budget changes what fits. A
//! change of selection redraws only the chunks holding the elements whose
//! highlight changed. Texts are laid out every frame (egui keeps the
//! layouts), since their glyphs live in a font atlas that may be rebuilt.

use crate::paint::{
    Budget, Drawn, Gate, PaintOptions, Xf, frame_left, frame_use, paint_text, prim_shapes, text_culled,
};
use cc_core::{ElementId, ViewId};
use cc_views::{Drawing2D, Prim};
use egui::epaint::{Mesh, Tessellator};
use egui::{Painter, Rect, Shape, Vec2};
use std::any::Any;
use std::collections::{BTreeSet, HashMap};
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// Primitives per chunk: what a change of highlight redraws at least.
const CHUNK: usize = 2048;

/// Painted drawings by view.
#[derive(Default)]
pub struct PaintCache {
    entries: HashMap<ViewId, Entry>,
    /// Full builds so far (tests).
    builds: usize,
    /// Chunks redrawn for a change of highlight (tests).
    redrawn: usize,
}

/// A part of a chunk, in drawing order.
enum Piece {
    Mesh(Arc<Mesh>),
    /// The text primitive at this index.
    Text(usize),
}

struct Entry {
    /// What the drawing belongs to (its identity; a weak reference keeps the
    /// allocation, so another drawing cannot take its address).
    owner: Weak<dyn Any>,
    /// The frame it was last painted in.
    pass: u64,
    /// The view's transform when it was built: the meshes are in its screen points.
    reference: Xf,
    px_per_mm: f64,
    pixels_per_point: f32,
    /// What was built, in the reference's screen points.
    area: Rect,
    /// The whole drawing lies in `area`.
    complete: bool,
    /// The frame's vertices left when it was built, and whether any did not fit.
    limit: usize,
    refused: bool,
    vertices: usize,
    /// Which primitives were drawn.
    drawn: Vec<bool>,
    chunks: Vec<Vec<Piece>>,
    highlight: BTreeSet<ElementId>,
    /// How far the meshes have been moved from the reference.
    offset: Vec2,
}

impl PaintCache {
    /// Paint `d`, the drawing of `view` belonging to `owner`, with `xf` and
    /// the selection `highlight` (texts at `px_per_mm`). Returns the
    /// transform it was painted with: `xf` moved by less than a pixel, so
    /// the cached meshes stay on whole pixels; what is drawn over the
    /// drawing should use it.
    #[allow(clippy::too_many_arguments)]
    pub fn paint<T: 'static>(
        &mut self,
        painter: &Painter,
        view: ViewId,
        owner: &Rc<T>,
        d: &Drawing2D,
        xf: Xf,
        highlight: &BTreeSet<ElementId>,
        px_per_mm: f64,
    ) -> Xf {
        let ctx = painter.ctx();
        let pass = ctx.cumulative_pass_nr();
        let pixels_per_point = ctx.pixels_per_point();
        let clip = painter.clip_rect().expand(50.0);
        let left = frame_left(ctx);
        self.entries.retain(|v, e| *v == view || (e.pass + 2 >= pass && e.owner.strong_count() > 0));

        let fresh = self.entries.get(&view).is_some_and(|e| {
            let shift = shift(&e.reference, &xf);
            std::ptr::addr_eq(e.owner.as_ptr(), Rc::as_ptr(owner))
                && e.reference.scale == xf.scale
                && e.px_per_mm == px_per_mm
                && e.pixels_per_point == pixels_per_point
                && e.vertices <= left
                && (!e.refused || e.limit == left)
                && (e.complete || e.area.contains_rect(clip.translate(-shift)))
        });
        if !fresh {
            let entry = build(ctx, owner, d, xf, highlight, px_per_mm, clip, left);
            self.builds += 1;
            self.entries.insert(view, entry);
        }
        let Some(entry) = self.entries.get_mut(&view) else { return xf };
        entry.pass = pass;
        if entry.highlight != *highlight {
            self.redrawn += entry.rehighlight(ctx, d, highlight);
        }

        // Move the meshes by whole pixels; the view's transform follows them.
        let exact = shift(&entry.reference, &xf);
        let rounded = (exact * pixels_per_point).round() / pixels_per_point;
        let painted = Xf { screen: xf.screen + (rounded - exact), ..xf };
        if rounded != entry.offset {
            let delta = rounded - entry.offset;
            for piece in entry.chunks.iter_mut().flatten() {
                if let Piece::Mesh(mesh) = piece {
                    Arc::make_mut(mesh).translate(delta);
                }
            }
            entry.offset = rounded;
        }

        let opts = PaintOptions { highlight, px_per_mm, mono: None, only: None };
        for piece in entry.chunks.iter().flatten() {
            match piece {
                Piece::Mesh(mesh) => {
                    painter.add(Shape::Mesh(mesh.clone()));
                }
                Piece::Text(i) => {
                    if let Some(Prim::Text { pos, text, size, anchor, angle, .. }) = d.prims.get(*i) {
                        let px = (size * opts.px_per_mm) as f32;
                        let at = painted.to_screen(*pos);
                        if !text_culled(text, px, at, clip) {
                            paint_text(painter, at, text, px, *anchor, *angle as f32);
                        }
                    }
                }
            }
        }
        frame_use(ctx, entry.vertices);
        painted
    }

    #[cfg(test)]
    fn counts(&self) -> (usize, usize) {
        (self.builds, self.redrawn)
    }
}

/// How far the screen points of `to` are from those of `from` (same scale).
fn shift(from: &Xf, to: &Xf) -> Vec2 {
    to.to_screen(from.origin) - from.screen
}

/// A tessellator for the frame's pixel density and options, culling nothing.
fn tessellator(ctx: &egui::Context) -> Tessellator {
    let options = ctx.tessellation_options(|o| *o);
    let font = ctx.fonts(|f| f.font_image_size());
    Tessellator::new(ctx.pixels_per_point(), options, font, Vec::new())
}

#[allow(clippy::too_many_arguments)]
fn build<T: 'static>(
    ctx: &egui::Context,
    owner: &Rc<T>,
    d: &Drawing2D,
    xf: Xf,
    highlight: &BTreeSet<ElementId>,
    px_per_mm: f64,
    clip: Rect,
    left: usize,
) -> Entry {
    let margin = clip.width().max(clip.height()).max(0.0);
    let area = clip.expand(margin);
    let complete = d.bounds().is_some_and(|(lo, hi)| {
        let (a, b) = (xf.to_screen(lo), xf.to_screen(hi));
        area.contains(a) && area.contains(b)
    }) || d.prims.is_empty();
    let opts = PaintOptions { highlight, px_per_mm, mono: None, only: None };
    let mut budget = Budget::with_left(left);
    let mut drawn = vec![false; d.prims.len()];
    let mut tess = tessellator(ctx);
    let mut chunks = Vec::with_capacity(d.prims.len().div_ceil(CHUNK));
    for start in (0..d.prims.len()).step_by(CHUNK) {
        let end = (start + CHUNK).min(d.prims.len());
        let pieces = chunk(ctx, d, start..end, &xf, &opts, area, &mut tess, Gates::Budget(&mut budget));
        for (i, was) in pieces.1 {
            if let Some(slot) = drawn.get_mut(i) {
                *slot = was;
            }
        }
        chunks.push(pieces.0);
    }
    Entry {
        owner: Rc::downgrade(owner) as Weak<dyn Any>,
        pass: ctx.cumulative_pass_nr(),
        reference: xf,
        px_per_mm,
        pixels_per_point: ctx.pixels_per_point(),
        area,
        complete,
        limit: left,
        refused: budget.refused,
        vertices: budget.used,
        drawn,
        chunks,
        highlight: highlight.clone(),
        offset: Vec2::ZERO,
    }
}

/// How a chunk's primitives are let in: by the budget of a build, or as it decided.
enum Gates<'a> {
    Budget(&'a mut Budget),
    Decided(&'a [bool]),
}

/// The pieces of the primitives in `range`, each let in by `gate`, with
/// whether each was drawn.
#[allow(clippy::too_many_arguments)]
fn chunk(
    ctx: &egui::Context,
    d: &Drawing2D,
    range: std::ops::Range<usize>,
    xf: &Xf,
    opts: &PaintOptions,
    area: Rect,
    tess: &mut Tessellator,
    mut gates: Gates,
) -> (Vec<Piece>, Vec<(usize, bool)>) {
    let mut pieces = Vec::new();
    let mut drawn = Vec::with_capacity(range.len());
    let mut mesh = Mesh::default();
    let mut shapes = Vec::new();
    let flush = |mesh: &mut Mesh, pieces: &mut Vec<Piece>| {
        if !mesh.is_empty() {
            pieces.push(Piece::Mesh(Arc::new(std::mem::take(mesh))));
        }
    };
    for i in range {
        let Some(prim) = d.prims.get(i) else { break };
        let mut gate = match &mut gates {
            Gates::Budget(budget) => Gate::Budget(budget),
            Gates::Decided(flags) => Gate::Decided(flags.get(i).copied().unwrap_or(false)),
        };
        let result = prim_shapes(ctx, d, prim, xf, opts, area, &mut gate, &mut shapes);
        drawn.push((i, !matches!(result, Drawn::Nothing)));
        match result {
            Drawn::Nothing => {}
            Drawn::Shapes => {
                for shape in shapes.drain(..) {
                    if !mesh.is_empty() && mesh.texture_id != shape.texture_id() {
                        flush(&mut mesh, &mut pieces);
                    }
                    tess.tessellate_shape(shape, &mut mesh);
                }
            }
            Drawn::Text { .. } => {
                flush(&mut mesh, &mut pieces);
                pieces.push(Piece::Text(i));
            }
        }
    }
    flush(&mut mesh, &mut pieces);
    (pieces, drawn)
}

impl Entry {
    /// Redraw the chunks holding elements whose highlight changed; returns how many.
    fn rehighlight(&mut self, ctx: &egui::Context, d: &Drawing2D, highlight: &BTreeSet<ElementId>) -> usize {
        let changed: BTreeSet<ElementId> = self.highlight.symmetric_difference(highlight).copied().collect();
        let mut affected: Vec<usize> = d.prims_of(&changed).into_iter().map(|i| i / CHUNK).collect();
        affected.dedup();
        let opts = PaintOptions { highlight, px_per_mm: self.px_per_mm, mono: None, only: None };
        let mut tess = tessellator(ctx);
        for c in &affected {
            let start = c * CHUNK;
            let end = (start + CHUNK).min(d.prims.len());
            let gates = Gates::Decided(&self.drawn);
            let (mut pieces, _) = chunk(ctx, d, start..end, &self.reference, &opts, self.area, &mut tess, gates);
            for piece in &mut pieces {
                if let Piece::Mesh(mesh) = piece {
                    Arc::make_mut(mesh).translate(self.offset);
                }
            }
            if let Some(slot) = self.chunks.get_mut(*c) {
                *slot = pieces;
            }
        }
        self.highlight = highlight.clone();
        affected.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::paint_drawing;
    use cc_views::{Anchor, LineStyle};
    use egui::Pos2;
    use glam::DVec2;

    /// Lines of each kind, fills (convex and not), texts between them and
    /// an imported drawing's lines.
    fn drawing() -> Drawing2D {
        let mut d = Drawing2D::default();
        let cad = ElementId(900);
        d.background.insert(cad);
        for i in 0..3000 {
            let x = (i % 60) as f64 * 0.3;
            let y = (i / 60) as f64 * 0.3;
            d.line(DVec2::new(x, y), DVec2::new(x + 0.25, y + 0.1), LineStyle::Thin, Some(cad));
        }
        for i in 0..40u64 {
            let x = (i % 8) as f64 * 2.0;
            let y = (i / 8) as f64 * 2.0;
            let style = [LineStyle::Cut, LineStyle::Hidden, LineStyle::Center, LineStyle::Projection][i as usize % 4];
            d.line(DVec2::new(x, y), DVec2::new(x + 1.7, y + 0.4), style, Some(ElementId(i)));
            d.fill(
                vec![DVec2::new(x, y), DVec2::new(x + 1.0, y), DVec2::new(x + 0.5, y + 0.3), DVec2::new(x, y + 1.0)],
                [200, 180, 160, 255],
                Some(ElementId(i)),
            );
            d.tag_text(DVec2::new(x, y + 1.2), format!("T{i}"), 2.5, Anchor::LeftBottom, 0.3, Some(ElementId(i)));
        }
        d
    }

    /// The vertices egui makes of what `paint` paints in one frame.
    fn frame(ctx: &egui::Context, paint: impl FnOnce(&Painter)) -> Vec<egui::epaint::Vertex> {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 900.0))),
            ..Default::default()
        });
        paint(&ctx.layer_painter(egui::LayerId::background()));
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        ctx.tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .flat_map(|p| match p.primitive {
                egui::epaint::Primitive::Mesh(m) => m.vertices,
                _ => Vec::new(),
            })
            .collect()
    }

    fn same(a: &[egui::epaint::Vertex], b: &[egui::epaint::Vertex]) {
        assert_eq!(a.len(), b.len());
        for (u, v) in a.iter().zip(b) {
            assert!(u.pos.distance(v.pos) < 1e-3 && u.color == v.color && u.uv == v.uv, "{u:?} {v:?}");
        }
    }

    /// The cached drawing paints what painting it afresh paints, panned
    /// and with a selection, and pans and selections do not build it again.
    #[test]
    fn the_cached_drawing_paints_what_painting_it_does() {
        let ctx = egui::Context::default();
        let d = Rc::new(drawing());
        let view = ViewId(1);
        let mut cache = PaintCache::default();
        let none = BTreeSet::new();
        let selected = BTreeSet::from([ElementId(3), ElementId(900)]);
        let xf = Xf { screen: Pos2::new(300.0, 700.0), origin: DVec2::ZERO, scale: 40.0 };
        let fresh = |xf: Xf, highlight: &BTreeSet<ElementId>| {
            frame(&ctx, |p| {
                paint_drawing(p, &d, &xf, &PaintOptions { highlight, px_per_mm: 4.0, mono: None, only: None })
            })
        };
        let cached = |cache: &mut PaintCache, xf: Xf, highlight: &BTreeSet<ElementId>| {
            let mut painted = None;
            let v = frame(&ctx, |p| painted = Some(cache.paint(p, view, &d, &d, xf, highlight, 4.0)));
            (v, painted)
        };
        let (first, painted) = cached(&mut cache, xf, &none);
        assert!(painted.is_some_and(|p| p.screen == xf.screen));
        same(&first, &fresh(xf, &none));
        // Panned by whole pixels, then with a selection.
        let panned = Xf { origin: DVec2::new(-2.5, 1.25), ..xf };
        let (v, _) = cached(&mut cache, panned, &none);
        same(&v, &fresh(panned, &none));
        let (v, _) = cached(&mut cache, panned, &selected);
        same(&v, &fresh(panned, &selected));
        let (v, _) = cached(&mut cache, xf, &none);
        same(&v, &fresh(xf, &none));
        let (builds, redrawn) = cache.counts();
        assert_eq!(builds, 1, "pans and selections keep the build");
        // The imported drawing spans both chunks; element 3 one of them: twice each way.
        assert_eq!(redrawn, 4);
        // A zoom builds it again.
        // (Still all on screen: egui leaves out of a frame what is off it.)
        let zoomed = Xf { scale: 45.0, ..xf };
        let (v, _) = cached(&mut cache, zoomed, &none);
        same(&v, &fresh(zoomed, &none));
        assert_eq!(cache.counts().0, 2);
    }

    /// A pan past the area built for builds again; a drawing that fits the
    /// area never does.
    #[test]
    fn far_pans_build_again() {
        let ctx = egui::Context::default();
        let mut d = Drawing2D::default();
        for i in 0..200 {
            let x = i as f64 * 10.0;
            d.line(DVec2::new(x, 0.0), DVec2::new(x + 5.0, 3.0), LineStyle::Cut, Some(ElementId(i)));
        }
        let d = Rc::new(d);
        let mut cache = PaintCache::default();
        let none = BTreeSet::new();
        let at = |x: f64| Xf { screen: Pos2::new(600.0, 450.0), origin: DVec2::new(x, 0.0), scale: 100.0 };
        for x in [0.0, 5.0, 1000.0, 1005.0] {
            frame(&ctx, |p| {
                cache.paint(p, ViewId(1), &d, &d, at(x), &none, 1.0);
            });
        }
        assert_eq!(cache.counts().0, 2);
        let whole = |x: f64| Xf { scale: 0.5, ..at(x) };
        for x in [0.0, 500.0, 1500.0] {
            frame(&ctx, |p| {
                cache.paint(p, ViewId(1), &d, &d, whole(x), &none, 1.0);
            });
        }
        assert_eq!(cache.counts().0, 3);
        // Another drawing (a new revision) builds again.
        let other = Rc::new((*d).clone());
        frame(&ctx, |p| {
            cache.paint(p, ViewId(1), &other, &other, whole(0.0), &none, 1.0);
        });
        assert_eq!(cache.counts().0, 4);
    }
}
