//! Foundations: pads under columns, strips (ground beams) under walls and
//! slabs, placed one by one or under existing columns and walls.

use super::create::{SketchArgs, arc_bulge};
use super::{add_element, created, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, ElementId, ElementKind, ElementTypeId, Foundation, FoundationKind, FoundationShape, LevelId, Project,
    TypeData,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Points closer than this are the same place (1 cm).
const SAME: f64 = 0.01;

/// The foundation type named by `r` (which must be of `kind`), or the first type of that kind.
fn foundation_type(p: &Project, kind: FoundationKind, r: Option<&Ref>) -> Result<ElementTypeId> {
    let of_kind = |id: ElementTypeId| matches!(p.element_type(id).map(|t| &t.data), Some(TypeData::Foundation { kind: k, .. }) if *k == kind);
    match r {
        Some(r) => {
            let id = refs::type_of(p, Category::Foundation, Some(r))?;
            if of_kind(id) {
                Ok(id)
            } else {
                Err(EngineError::invalid(t!("msg-foundation-kind", "kind" => crate::i18n::tr(kind.key()))))
            }
        }
        None => p
            .types_of(Category::Foundation)
            .map(|t| t.id)
            .find(|id| of_kind(*id))
            .ok_or_else(|| EngineError::rejected(t!("msg-foundation-kind", "kind" => crate::i18n::tr(kind.key())))),
    }
}

fn foundation(shape: FoundationShape, top_offset: Option<f64>) -> Result<ElementKind> {
    Ok(ElementKind::Foundation(Foundation::new(shape, finite(top_offset.unwrap_or(0.0), "top_offset")?)))
}

/// Foundations of `level` with their shapes.
fn foundations_on(p: &Project, level: LevelId) -> Vec<&FoundationShape> {
    p.elements
        .values()
        .filter(|e| e.level == level)
        .filter_map(|e| match &e.kind {
            ElementKind::Foundation(f) => Some(&f.shape),
            _ => None,
        })
        .collect()
}

/// Create foundations in one undo step; returns the new ids and how many were skipped.
fn create_all(
    s: &mut Session,
    type_id: ElementTypeId,
    todo: Vec<(LevelId, ElementKind)>,
    skipped: usize,
) -> Result<Value> {
    if todo.is_empty() {
        return Ok(json!({"created": [], "skipped": skipped}));
    }
    let created = edit(s, t!("tx-create-foundation"), |e| {
        Ok(todo.into_iter().map(|(level, kind)| add_element(e, level, type_id, kind)).collect::<Vec<_>>())
    })?;
    Ok(json!({"created": ids_json(&created), "skipped": skipped}))
}

/// Place a pad footing centered at `position`; its top is `top_offset` above the level (default 0).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FootingCreate {
    /// Center of the pad in plan.
    pub position: Point,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Pad footing type id or name (default: the first pad type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Top above the level, meters (default 0).
    #[serde(default)]
    pub top_offset: Option<f64>,
}

impl Command for FootingCreate {
    const NAME: &'static str = "footing_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = foundation_type(p, FoundationKind::Pad, self.type_ref.as_ref())?;
        let shape = FoundationShape::Pad {
            position: point(self.position)?,
            rotation_deg: finite(self.rotation.unwrap_or(0.0), "rotation")?,
        };
        let kind = foundation(shape, self.top_offset)?;
        let id = edit(s, t!("tx-create-foundation"), |e| Ok(add_element(e, level, type_id, kind)))?;
        Ok(created(s, id))
    }
}

/// Half the width of the strip type `type_id` (0 for other types).
fn strip_half(p: &Project, type_id: ElementTypeId) -> f64 {
    match p.element_type(type_id).map(|t| &t.data) {
        Some(TypeData::Foundation { kind: FoundationKind::Strip, width, .. }) => width / 2.0,
        _ => 0.0,
    }
}

/// An arc strip must leave room inside its curve for its width.
pub(crate) fn check_strip_arc(p: &Project, type_id: ElementTypeId, shape: &FoundationShape) -> Result<()> {
    let Some(arc) = shape.arc() else { return Ok(()) };
    if arc.radius <= strip_half(p, type_id) + 1e-3 {
        return Err(EngineError::rejected(t!("msg-strip-arc-tight")));
    }
    Ok(())
}

/// Place a strip footing (a ground beam) centered on the line `start`-`end`,
/// or on an arc through `mid` (or with a `bulge` or a `radius`); its top is
/// `top_offset` above the level (default 0). Strips meet at corners and tees
/// like beams, arcs at the true crossings of their faces.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StripFootingCreate {
    /// Start of the strip's center line.
    pub start: Point,
    /// End of the strip's center line.
    pub end: Point,
    /// A point on the arc between the ends: makes an arc strip through the three points.
    #[serde(default)]
    pub mid: Option<Point>,
    /// Arc strips: tan(sweep / 4), positive when the arc turns counter-clockwise from `start`
    /// to `end` (1 is a half circle; at most 10 either way). Ignored when `mid` is given.
    #[serde(default)]
    pub bulge: Option<f64>,
    /// Arc strips: the radius in meters, positive when the arc turns counter-clockwise from
    /// `start` to `end`, negative clockwise; at least half the distance between the ends.
    /// Ignored when `mid` or `bulge` is given.
    #[serde(default)]
    pub radius: Option<f64>,
    /// With `radius`: take the longer of the two arcs (default: the shorter).
    #[serde(default)]
    pub major_arc: Option<bool>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Strip footing type id or name (default: the first strip type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Top above the level, meters (default 0).
    #[serde(default)]
    pub top_offset: Option<f64>,
}

impl Command for StripFootingCreate {
    const NAME: &'static str = "strip_footing_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (start, end) = (point(self.start)?, point(self.end)?);
        if start.distance(end) < 1e-3 {
            return Err(EngineError::invalid("the strip has no length"));
        }
        let bulge = arc_bulge(start, end, self.mid, self.bulge, self.radius, self.major_arc)?;
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = foundation_type(p, FoundationKind::Strip, self.type_ref.as_ref())?;
        let shape = FoundationShape::Strip { start, end, bulge };
        check_strip_arc(p, type_id, &shape)?;
        let kind = foundation(shape, self.top_offset)?;
        let id = edit(s, t!("tx-create-foundation"), |e| Ok(add_element(e, level, type_id, kind)))?;
        Ok(created(s, id))
    }
}

/// Create a foundation slab (a raft) from an outline, from `curves` (lines,
/// arcs, circles: a circular raft keeps its true curve, area and volume) or
/// under the loop of walls of the level; `offset` is its top above the level
/// (default 0).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct FoundationSlabCreate {
    #[serde(flatten)]
    pub sketch: SketchArgs,
}

impl Command for FoundationSlabCreate {
    const NAME: &'static str = "foundation_slab_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (level, profile, drawn) = super::create::sketched(s, &self.sketch, Category::Floor)?;
        if !profile.openings.is_empty() {
            return Err(EngineError::invalid("a foundation slab takes one outline, with no openings"));
        }
        let type_id = foundation_type(s.project(), FoundationKind::Slab, self.sketch.type_ref.as_ref())?;
        let mut kind = foundation(FoundationShape::Slab { boundary: profile.outline }, self.sketch.offset)?;
        if let ElementKind::Foundation(f) = &mut kind {
            f.sketch = drawn;
        }
        let id = edit(s, t!("tx-create-foundation"), |e| Ok(add_element(e, level, type_id, kind)))?;
        Ok(created(s, id))
    }
}

/// Place a pad footing under each column (default: every column of the
/// level), centered and turned with it, its top at the column's base.
/// Columns that already stand on a pad are skipped.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FootingsUnderColumns {
    /// Column ids (default: every column of `level`).
    #[serde(default)]
    pub columns: Vec<u64>,
    /// Level id or name, when no columns are given (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Pad footing type id or name (default: the first pad type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for FootingsUnderColumns {
    const NAME: &'static str = "footings_under_columns";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let type_id = foundation_type(p, FoundationKind::Pad, self.type_ref.as_ref())?;
        let ids: Vec<ElementId> = if self.columns.is_empty() {
            let level = refs::level_or_first(p, self.level.as_ref())?;
            p.elements.values().filter(|e| e.level == level && e.category() == Category::Column).map(|e| e.id).collect()
        } else {
            refs::elements(p, &self.columns)?
        };
        let (mut todo, mut skipped) = (Vec::new(), 0);
        for id in ids {
            let Some(e) = p.element(id) else { continue };
            let ElementKind::Column(c) = &e.kind else {
                return Err(EngineError::invalid(format!("element {} is not a column", id.0)));
            };
            let taken = |shapes: Vec<&FoundationShape>| {
                shapes
                    .iter()
                    .any(|f| matches!(f, FoundationShape::Pad { position, .. } if position.distance(c.position) < SAME))
            };
            let planned = todo.iter().any(|(l, k): &(LevelId, ElementKind)| {
                *l == e.level
                    && matches!(k, ElementKind::Foundation(f) if f.shape.points().first().is_some_and(|q| q.distance(c.position) < SAME))
            });
            if planned || taken(foundations_on(p, e.level)) {
                skipped += 1;
                continue;
            }
            let shape = FoundationShape::Pad { position: c.position, rotation_deg: c.rotation_deg };
            todo.push((e.level, foundation(shape, Some(c.base_offset))?));
        }
        create_all(s, type_id, todo, skipped)
    }
}

/// Place a strip footing under each wall (default: every wall of the level),
/// along its body's centerline (an arc under an arc wall), its top at the
/// wall's base. Walls that already have a strip, and arc walls too tight for
/// the strip's width, are skipped; strips meet at corners like beams.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StripFootingsUnderWalls {
    /// Wall ids (default: every wall of `level`).
    #[serde(default)]
    pub walls: Vec<u64>,
    /// Level id or name, when no walls are given (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Strip footing type id or name (default: the first strip type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for StripFootingsUnderWalls {
    const NAME: &'static str = "strip_footings_under_walls";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let type_id = foundation_type(p, FoundationKind::Strip, self.type_ref.as_ref())?;
        let ids: Vec<ElementId> = if self.walls.is_empty() {
            let level = refs::level_or_first(p, self.level.as_ref())?;
            p.walls().filter(|(e, _)| e.level == level).map(|(e, _)| e.id).collect()
        } else {
            refs::elements(p, &self.walls)?
        };
        // The same line or arc, drawn either way (a reversed arc turns the other way).
        let same = |a: (DVec2, DVec2, f64), b: (DVec2, DVec2, f64)| {
            (a.0.distance(b.0) < SAME && a.1.distance(b.1) < SAME && (a.2 - b.2).abs() < 1e-6)
                || (a.0.distance(b.1) < SAME && a.1.distance(b.0) < SAME && (a.2 + b.2).abs() < 1e-6)
        };
        let half = strip_half(p, type_id);
        let (mut todo, mut skipped) = (Vec::new(), 0);
        let mut planned: Vec<(LevelId, (DVec2, DVec2, f64))> = Vec::new();
        for id in ids {
            let Some(e) = p.element(id) else { continue };
            let ElementKind::Wall(w) = &e.kind else {
                return Err(EngineError::invalid(format!("element {} is not a wall", id.0)));
            };
            // Along the body's centerline: concentric with an arc wall's location line, with its bulge.
            let (start, end) = p.wall_axis(e, w);
            let bulge = if w.arc().is_some() { w.bulge } else { 0.0 };
            let shape = FoundationShape::Strip { start, end, bulge };
            // A strip wider than the curve of its wall allows is left out.
            if shape.arc().is_some_and(|a| a.radius <= half + 1e-3) {
                skipped += 1;
                continue;
            }
            let line = (start, end, bulge);
            let existing = foundations_on(p, e.level).into_iter().any(|f| match f {
                FoundationShape::Strip { start, end, bulge } => same((*start, *end, *bulge), line),
                _ => false,
            });
            if existing || planned.iter().any(|(l, x)| *l == e.level && same(*x, line)) {
                skipped += 1;
                continue;
            }
            planned.push((e.level, line));
            todo.push((e.level, foundation(shape, Some(w.base_offset))?));
        }
        create_all(s, type_id, todo, skipped)
    }
}
