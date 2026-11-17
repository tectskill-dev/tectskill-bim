//! Dimensions that follow and drive the model.
//!
//! Placing: each end of a new dimension follows the element (or, in
//! elevations and sections, the level line) whose linework in the view
//! passes through it. Editing the value: the moving end's element moves —
//! elements crossing the measured direction translate; walls, beams and
//! grids along it stretch their nearest end. In elevations and sections a
//! dimension between level lines raises or lowers the upper level.

use crate::error::{EngineError, Result};
use crate::session::Session;
use crate::t;
use cc_core::{Category, DimRef, Dimension, DimensionKind, ElementId, ElementKind, PlanTransform, Project, ViewKind};
use cc_views::{Drawing2D, Prim};
use glam::DVec2;
use std::collections::BTreeSet;

pub fn point_on_segment(a: DVec2, b: DVec2, p: DVec2, tol: f64) -> bool {
    cc_geom::polygon::closest_on_segment(a, b, p).0.distance(p) <= tol
}

/// The dimension's view when it shows the model in known coordinates
/// (plans, elevations and sections).
fn model_view<'a>(p: &'a Project, dim: &Dimension) -> Option<&'a cc_core::View> {
    p.view(dim.view).filter(|v| v.is_plan() || v.projection().is_some())
}

fn rank(c: Category) -> u8 {
    match c {
        Category::Wall | Category::Column | Category::Grid => 0,
        Category::Beam | Category::Door | Category::Window => 1,
        Category::Component => 2,
        _ => 9,
    }
}

/// The element (best ranked) with linework along the line through `p` and `q`.
fn line_element(p: &Project, drawing: &Drawing2D, a: DVec2, b: DVec2) -> Option<ElementId> {
    let mut found: Vec<(u8, ElementId)> = drawing
        .prims
        .iter()
        .filter_map(|prim| match prim {
            Prim::Line { a: s, b: e, element: Some(el), .. } => {
                let u = (*e - *s).try_normalize()?;
                let off = |x: DVec2| (x - *s).perp_dot(u).abs();
                let r = rank(p.element(*el)?.category());
                (off(a) < 1e-4 && off(b) < 1e-4 && r < 9).then_some((r, *el))
            }
            _ => None,
        })
        .collect();
    found.sort();
    found.first().map(|f| f.1)
}

fn end_refs(project: &Project, drawing: &Drawing2D, dim: &Dimension) -> [Option<DimRef>; 2] {
    let Some(view) = model_view(project, dim) else { return [None, None] };
    // Elements with linework through `p` (within `tol`), best first.
    let at = |p: DVec2, tol: f64, across: Option<DVec2>| -> Option<DimRef> {
        let mut found: Vec<(f64, u8, ElementId)> = drawing
            .prims
            .iter()
            .filter_map(|prim| match prim {
                Prim::Line { a, b, element: Some(e), .. } if point_on_segment(*a, *b, p, tol) => {
                    let along = across.zip((*b - *a).try_normalize()).map_or(0.0, |(d, u)| u.dot(d).abs());
                    let category = project.element(*e)?.category();
                    (rank(category) < 9).then_some((along, rank(category), *e))
                }
                _ => None,
            })
            .collect();
        found.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        let element = found.iter().find_map(|(_, _, e)| project.dim_ref_in_view(view, *e, p));
        // In elevations and sections an end on a level line follows the level.
        let level = || {
            let level = project.levels.iter().find(|l| (l.elevation - p.y).abs() < 1e-4)?;
            project.level_ref(view, level.id)
        };
        element.or_else(level)
    };
    match dim.kind {
        DimensionKind::Aligned | DimensionKind::Linear { .. } | DimensionKind::Ordinate { .. } => {
            let dir = dim.direction();
            [at(dim.a, 1e-4, dir), at(dim.b, 1e-4, dir)]
        }
        DimensionKind::Radial | DimensionKind::Diameter => {
            // The leader end lies on the true circle, a little off its chords.
            let r = dim.length();
            let Some(rim) = at(dim.b, 0.02 * r + 1e-4, None) else { return [None, None] };
            let center = rim.element().and_then(|e| project.dim_ref_in_view(view, e, dim.a));
            [center, Some(rim)]
        }
        DimensionKind::Angular { vertex, .. } => {
            let end =
                |p: DVec2| line_element(project, drawing, p, vertex).and_then(|e| project.dim_ref_in_view(view, e, p));
            [end(dim.a), end(dim.b)]
        }
    }
}

/// References for the ends of a dimension being placed: each end follows
/// the element whose linework passes through it, preferring lines across
/// the measured direction (the faces being measured) and, among those,
/// walls, columns and grids. Radial and diameter dimensions follow the
/// element of their arc; angular ones the elements along each measured line.
pub fn associate(project: &Project, drawing: &Drawing2D, dim: &Dimension) -> Dimension {
    let mut out = Dimension { refs: end_refs(project, drawing, dim), ..dim.clone() };
    if let DimensionKind::Angular { vertex, rays } = &mut out.kind
        && let Some(view) = model_view(project, dim)
    {
        for (ray, end) in rays.iter_mut().zip([dim.a, dim.b]) {
            *ray = line_element(project, drawing, end, *vertex).and_then(|e| project.dim_ref_in_view(view, e, *vertex));
        }
    }
    out
}

fn dimension(p: &Project, id: ElementId) -> Option<Dimension> {
    match p.element(id).map(|e| &e.kind) {
        Some(ElementKind::Dimension(d)) => Some((**d).clone()),
        _ => None,
    }
}

fn invalid() -> EngineError {
    EngineError::invalid(t!("msg-invalid-value"))
}

/// A vertical dimension on level lines: the level at the moving end (the
/// upper one between two levels) moves so the dimension measures `value`.
fn set_level(s: &mut Session, dim: &Dimension, value: f64) -> Result<()> {
    let level_of = |r: &Option<DimRef>| match r.map(|r| r.target) {
        Some(cc_core::RefTarget::Level(l)) => Some(l),
        _ => None,
    };
    let (at_b, level) = match (level_of(&dim.refs[0]), level_of(&dim.refs[1])) {
        (Some(la), Some(lb)) => {
            if dim.b.y >= dim.a.y {
                (true, lb)
            } else {
                (false, la)
            }
        }
        (_, Some(l)) => (true, l),
        (Some(l), None) => (false, l),
        (None, None) => return Err(invalid()),
    };
    let dir = dim
        .direction()
        .filter(|d| d.y.abs() > 0.99)
        .ok_or_else(|| EngineError::rejected(t!("msg-dim-level-vertical")))?;
    let measured = (dim.b - dim.a).dot(dir);
    let target = value * if measured < 0.0 { -1.0 } else { 1.0 };
    let dz = dir.y * (target - measured) * if at_b { 1.0 } else { -1.0 };
    crate::commands::edit(s, t!("tx-dim-value"), |e| {
        let levels = e.levels_mut();
        if let Some(l) = levels.iter_mut().find(|l| l.id == level) {
            l.elevation += dz;
        }
        levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
        Ok(())
    })
}

/// Elements (not dimensions, not `me`) whose linework passes through `p` but not through `other`.
fn attached(drawing: &Drawing2D, p: &Project, me: ElementId, at: DVec2, other: DVec2) -> Vec<ElementId> {
    let tol = 1e-3;
    let through = |q: DVec2| -> BTreeSet<ElementId> {
        drawing
            .prims
            .iter()
            .filter_map(|prim| match prim {
                Prim::Line { a, b, element: Some(e), .. } if point_on_segment(*a, *b, q, tol) => Some(*e),
                _ => None,
            })
            .filter(|e| *e != me)
            .filter(|e| p.element(*e).is_some_and(|x| x.category() != Category::Dimension))
            .collect()
    };
    let far = through(other);
    through(at).into_iter().filter(|e| !far.contains(e)).collect()
}

/// Give dimension `id` the measured `value` (meters, or radians for angles),
/// moving what its moving end touches (see the module docs). One undo step.
pub fn set_value(s: &mut Session, id: ElementId, value: f64) -> Result<()> {
    let dim = dimension(s.project(), id)
        .ok_or_else(|| EngineError::invalid(format!("element {} is not a dimension", id.0)))?;
    if !value.is_finite() || value <= 1e-6 {
        return Err(invalid());
    }
    let plan = s
        .project()
        .view(dim.view)
        .is_some_and(|v| matches!(v.kind, ViewKind::FloorPlan { .. } | ViewKind::CeilingPlan { .. }));
    let drawing = s.drawing(dim.view).unwrap_or_default();
    let p = s.project();
    // The referenced element of an end moves first; otherwise what the
    // view's linework shows at the ends.
    let element_end = |r: &Option<DimRef>| r.and_then(|r| r.element());
    // An ordinate dimension keeps its origin: its point moves.
    let ordinate = matches!(dim.kind, DimensionKind::Ordinate { .. });
    let (at_b, ids) = match (element_end(&dim.refs[0]), element_end(&dim.refs[1])) {
        (_, Some(e)) => (true, vec![e]),
        (_, None) if ordinate && dim.refs[1].is_none() => (true, attached(&drawing, p, id, dim.b, dim.a)),
        (_, None) if ordinate => return set_level(s, &dim, value),
        (Some(e), None) => (false, vec![e]),
        (None, None) if dim.refs.iter().any(Option::is_some) => return set_level(s, &dim, value),
        (None, None) => {
            let b_side = attached(&drawing, p, id, dim.b, dim.a);
            let a_side = attached(&drawing, p, id, dim.a, dim.b);
            if !b_side.is_empty() || a_side.is_empty() { (true, b_side) } else { (false, a_side) }
        }
    };
    if !ids.is_empty() && !plan {
        return Err(EngineError::rejected(t!("msg-dim-edit-plan")));
    }
    let (free, pinned): (Vec<_>, Vec<_>) = ids.into_iter().partition(|e| !p.is_pinned(*e));
    if free.is_empty() && !pinned.is_empty() {
        return Err(EngineError::rejected(t!("msg-pinned-skipped", "count" => pinned.len())));
    }
    let moving = if at_b { dim.b } else { dim.a };
    match dim.kind {
        DimensionKind::Aligned | DimensionKind::Linear { .. } | DimensionKind::Ordinate { .. } => {
            let dir = dim.direction().ok_or_else(invalid)?;
            let measured = (dim.b - dim.a).dot(dir);
            let target = value * if measured < 0.0 { -1.0 } else { 1.0 };
            let shift = dir * (target - measured) * if at_b { 1.0 } else { -1.0 };
            crate::commands::edit(s, t!("tx-dim-value"), |e| {
                let mut translate = Vec::new();
                for el in &free {
                    let line = e.project().element(*el).and_then(cc_core::wall_ops::line_of);
                    let along =
                        line.and_then(|(a, b)| (b - a).try_normalize()).is_some_and(|u| u.dot(dir).abs() > 0.99);
                    match line {
                        // Along the move: stretch the end at the dimension point.
                        Some((a, b)) if along => {
                            let at_start = a.distance(moving) <= b.distance(moving);
                            let end = if at_start { a } else { b };
                            if end.distance(moving) <= 1.0 {
                                e.move_endpoint(*el, at_start, end + shift)
                                    .map_err(|_| EngineError::rejected(t!("msg-degenerate")))?;
                            }
                        }
                        _ => translate.push(*el),
                    }
                }
                e.transform_elements(&translate, &PlanTransform::translate(shift));
                if let Some(ElementKind::Dimension(d)) = e.element_mut(id).map(|x| &mut x.kind) {
                    if at_b {
                        d.b += shift;
                    } else {
                        d.a += shift;
                    }
                }
                Ok(())
            })
        }
        DimensionKind::Angular { vertex, .. } => {
            if value >= std::f64::consts::PI {
                return Err(invalid());
            }
            let sweep = (dim.a - vertex).angle_to(dim.b - vertex);
            let target = value * if sweep < 0.0 { -1.0 } else { 1.0 };
            let turn = (target - sweep) * if at_b { 1.0 } else { -1.0 };
            let t = PlanTransform::rotate_about(vertex, turn);
            crate::commands::edit(s, t!("tx-dim-value"), |e| {
                e.transform_elements(&free, &t);
                if let Some(ElementKind::Dimension(d)) = e.element_mut(id).map(|x| &mut x.kind) {
                    if at_b {
                        d.b = t.apply(d.b);
                    } else {
                        d.a = t.apply(d.a);
                    }
                }
                Ok(())
            })
        }
        DimensionKind::Radial | DimensionKind::Diameter => Err(EngineError::rejected(t!("msg-dim-measured"))),
    }
}
