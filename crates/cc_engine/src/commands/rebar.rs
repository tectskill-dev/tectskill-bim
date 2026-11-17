//! Reinforcement: sets of bars in beams and columns, and copying a host's sets to others.

use super::{add_element, created, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    BarFace, Category, ElementId, ElementKind, ElementTypeId, FaceBars, Project, Rebar, RebarLayout, Splice, TypeData,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The bar type nearest `diameter`.
pub(crate) fn bar_type(p: &Project, diameter: f64) -> Result<ElementTypeId> {
    p.types_of(Category::Rebar)
        .filter_map(|t| match t.data {
            TypeData::Rebar { diameter: d, .. } => Some((t.id, (d - diameter).abs())),
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
        .ok_or_else(|| EngineError::rejected(t!("msg-no-rebar-type")))
}

/// The host of a set: a beam or a column (walls are reinforced by `wall_rebar_create`).
fn host(p: &Project, id: u64) -> Result<(ElementId, bool)> {
    let id = refs::element(p, id)?;
    match p.element(id).map(|e| &e.kind) {
        Some(ElementKind::Beam(_)) => Ok((id, true)),
        Some(ElementKind::Column(_)) => Ok((id, false)),
        _ => Err(EngineError::invalid(t!("msg-rebar-host"))),
    }
}

fn rebar_kind(host: ElementId, layout: RebarLayout, cover: f64, splice: Option<Splice>) -> ElementKind {
    ElementKind::Rebar(Rebar { splice, ..Rebar::new(host, layout, cover) })
}

/// The bars of a column face by face: `counts` (one per face of its
/// section, corners included), or as many on every face as keep them
/// `spacing` apart.
fn face_bars(p: &Project, host: ElementId, counts: Option<&[u32]>, spacing: Option<f64>) -> Result<Vec<FaceBars>> {
    if counts.is_none() && spacing.is_none() {
        return Ok(Vec::new());
    }
    let data = p.element(host).and_then(|e| p.element_type(e.type_id)).map(|t| &t.data);
    let faces = data.map_or(0, |d| cc_geom::rebar::column_faces(p, d, 0.0).len());
    if faces == 0 {
        return Err(EngineError::invalid("bars by face go in columns with a section"));
    }
    match (counts, spacing) {
        (Some(counts), _) => {
            if counts.len() != faces {
                return Err(EngineError::invalid(format!(
                    "the section has {faces} faces: give `faces` {faces} counts (see `derived.faces` of the column)"
                )));
            }
            if counts.iter().any(|c| !(2..=FaceBars::MAX_COUNT).contains(c)) {
                return Err(EngineError::invalid("each face takes 2 to 200 bars, its two corner bars included"));
            }
            Ok(counts.iter().map(|&count| FaceBars { count, spacing: 0.0 }).collect())
        }
        (None, Some(spacing)) => {
            let spacing = finite(spacing, "face_spacing")?;
            if spacing < 0.02 {
                return Err(EngineError::invalid("face_spacing must be at least 0.02"));
            }
            Ok(vec![FaceBars { count: 0, spacing }; faces])
        }
        (None, None) => Ok(Vec::new()),
    }
}

/// Stock length of bars, meters: 12 m, or 40' in imperial projects.
fn stock_length(p: &Project) -> f64 {
    match p.measurement_system() {
        cc_core::MeasurementSystem::Imperial => 12.192,
        cc_core::MeasurementSystem::Metric => 12.0,
    }
}

/// The lap splices of bars of `type_id` running along a beam: `max_length`
/// (default: the stock length; 0 for whole bars) and `lap` (default: 40 bar
/// diameters).
/// A partition as it is kept: trimmed, at most [`cc_core::bar_marks::MAX_MARK_TEXT`] characters.
pub(crate) fn partition_arg(text: Option<&str>) -> Result<String> {
    let text = text.unwrap_or("").trim();
    if text.chars().count() > cc_core::bar_marks::MAX_MARK_TEXT {
        return Err(EngineError::invalid(format!(
            "partition has at most {} characters",
            cc_core::bar_marks::MAX_MARK_TEXT
        )));
    }
    Ok(text.to_string())
}

pub(crate) fn splice(
    p: &Project,
    type_id: ElementTypeId,
    max_length: Option<f64>,
    lap: Option<f64>,
) -> Result<Option<Splice>> {
    let max_length = finite(max_length.unwrap_or_else(|| stock_length(p)), "max_length")?;
    if max_length == 0.0 {
        return Ok(None);
    }
    let diameter = match p.element_type(type_id).map(|t| &t.data) {
        Some(TypeData::Rebar { diameter, .. }) => *diameter,
        _ => 0.01,
    };
    let lap = finite(lap.unwrap_or(40.0 * diameter), "lap")?.max(Splice::MIN_LAP);
    let splice = Splice { max_length, lap };
    if !splice.is_valid() {
        return Err(EngineError::invalid(
            "max_length must be at least 1 (or 0 for whole bars) and lap at most half of it",
        ));
    }
    Ok(Some(splice))
}

/// Add a set of bars to a beam or column: `layout` is `top` or `bottom`
/// (beams), `perimeter` (columns) with `count` bars, `sides` (beams: `count`
/// side bars on each side face, or as many as keep them `spacing` apart over
/// the height), or `stirrups` every `spacing`. The bars are `cover` inside
/// the faces (default 2.5 cm) and follow the host: along an arc beam they
/// follow the arc, each at its own radius, and the stirrups are radial. Top
/// and bottom bars that do not fit side by side (a clear distance of the bar
/// diameter, at least 2 cm) go in a second layer. Bars along a beam longer
/// than `max_length` are made of pieces overlapping by `lap`, the splices of
/// neighbouring bars staggered.
///
/// A column's `perimeter` bars are `count` spread around its section, or
/// given face by face with `faces` (a bar count for each face of the
/// section, its two corner bars included, the corner bars shared between
/// faces; the column's `derived.faces` lists the faces in order) or
/// `face_spacing`; on a curved face they stand at even angles. Its
/// `stirrups` follow the section's outline, and `ties` adds cross-ties every
/// `spacing`: a hooked bar from each bar between the corners to the bar
/// opposite. With `max_length` a column's vertical bars are cut in lifts
/// that lap by `lap`. `same_type` gives the set to every column (or beam) of
/// the host's type.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RebarCreate {
    /// Beam or column id.
    pub host: u64,
    /// `top`, `bottom`, `perimeter`, `sides`, `stirrups` or `ties`.
    pub layout: String,
    /// Number of longitudinal bars (default: 2 in beams, 4 in columns), or of side bars on each
    /// side face (default 1, unless `spacing` is given).
    #[serde(default)]
    pub count: Option<u32>,
    /// Stirrup spacing, meters (default 0.15; 6" in imperial projects), or the largest distance
    /// between side bars over the height when no `count` is given.
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Bar type id or name (default: the type nearest 10 mm, or 5 mm for stirrups; 1/2" and 3/8" in imperial
    /// projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Concrete cover, meters (default 0.025; 1 1/2" in imperial projects).
    #[serde(default)]
    pub cover: Option<f64>,
    /// Longest bar along a beam, meters (default: the stock length, 12 m, or 40' in imperial
    /// projects): longer bars are lap spliced. 0 keeps whole bars.
    #[serde(default)]
    pub max_length: Option<f64>,
    /// Lap length of the splices, meters (default: 40 bar diameters).
    #[serde(default)]
    pub lap: Option<f64>,
    /// Beams, bars along the beam: run them half a lap past each end joined to another
    /// beam that carries on in the same line (the beams of a ring), so the bars of the
    /// two lap there (default false: they stop the cover short of the end). The lap is `lap`,
    /// or 40 bar diameters where bars are kept whole (`max_length` 0).
    #[serde(default)]
    pub lap_at_joins: Option<bool>,
    /// Columns, `stirrups`: several closed stirrups that overlap, one around each of the
    /// rectangles the section is made of (two in an L or a T, three in a U), instead of one
    /// following the whole outline. The section must have only square corners.
    #[serde(default)]
    pub overlapping: Option<bool>,
    /// The part of the structure the sets belong to (e.g. "FOUNDATION", "WALL"), which
    /// schedules filter and group by (default: none).
    #[serde(default)]
    pub partition: Option<String>,
    /// Columns, `perimeter`: heights above the column's base where its vertical bars are
    /// cut, rising (the construction joints): the bars of each lift run `lap` past the next
    /// height. Instead of the equal lifts of `max_length`.
    #[serde(default)]
    pub lifts: Option<Vec<f64>>,
    /// Columns, `perimeter`: the number of bars on each face of the section, in the order of
    /// the column's `derived.faces`, the two corner bars of a face included (at least 2).
    #[serde(default)]
    pub faces: Option<Vec<u32>>,
    /// Columns, `perimeter`: instead of `faces`, the largest distance between bars on every
    /// face, meters.
    #[serde(default)]
    pub face_spacing: Option<f64>,
    /// Also give the set to every other column (or beam) of the host's type (default false).
    #[serde(default)]
    pub same_type: Option<bool>,
}

impl Command for RebarCreate {
    const NAME: &'static str = "rebar_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let [default_cover, default_spacing, main_bar, _, stirrup_bar] = p.defaults().rebar;
        let (host_id, beam) = host(p, self.host)?;
        let level = p.element(host_id).map(|e| e.level).ok_or_else(|| EngineError::not_found("host"))?;
        let count = self.count.unwrap_or(if beam { 2 } else { 4 });
        if !(1..=200).contains(&count) {
            return Err(EngineError::invalid("count must be between 1 and 200"));
        }
        let longitudinal = |face| RebarLayout::Longitudinal { face, count };
        let layout = match self.layout.as_str() {
            "top" => longitudinal(BarFace::Top),
            "bottom" => longitudinal(BarFace::Bottom),
            "perimeter" => longitudinal(BarFace::Perimeter),
            "sides" => {
                if !beam {
                    return Err(EngineError::invalid("side bars go in beams"));
                }
                match (self.count, self.spacing) {
                    (Some(count), _) => RebarLayout::Sides { count, spacing: 0.0 },
                    (None, Some(spacing)) => {
                        let spacing = finite(spacing, "spacing")?;
                        if spacing < 0.02 {
                            return Err(EngineError::invalid("spacing must be at least 0.02"));
                        }
                        RebarLayout::Sides { count: 0, spacing }
                    }
                    (None, None) => RebarLayout::Sides { count: 1, spacing: 0.0 },
                }
            }
            "stirrups" | "ties" => {
                let spacing = finite(self.spacing.unwrap_or(default_spacing), "spacing")?;
                if spacing < 0.02 {
                    return Err(EngineError::invalid("spacing must be at least 0.02"));
                }
                match self.layout.as_str() {
                    "ties" if beam => return Err(EngineError::invalid("cross-ties go in columns")),
                    "ties" => RebarLayout::Ties { spacing },
                    _ => RebarLayout::Stirrups { spacing },
                }
            }
            other => {
                return Err(EngineError::invalid(format!(
                    "layout must be top, bottom, perimeter, sides, stirrups or ties, not '{other}'"
                )));
            }
        };
        let by_face = self.faces.is_some() || self.face_spacing.is_some();
        if by_face && (beam || !matches!(layout, RebarLayout::Longitudinal { face: BarFace::Perimeter, .. })) {
            return Err(EngineError::invalid("`faces` and `face_spacing` go with the `perimeter` bars of a column"));
        }
        let faces = face_bars(p, host_id, self.faces.as_deref(), self.face_spacing)?;
        let stirrups = matches!(layout, RebarLayout::Stirrups { .. } | RebarLayout::Ties { .. });
        let type_id = match &self.type_ref {
            Some(r) => refs::type_of(p, Category::Rebar, Some(r))?,
            None => bar_type(p, if stirrups { stirrup_bar } else { main_bar })?,
        };
        let cover = finite(self.cover.unwrap_or(default_cover), "cover")?.max(0.0);
        // Bars along a beam are spliced past the stock length; a column's only in the lifts asked for.
        let splice = match (layout.runs_along(), beam, self.max_length) {
            (true, true, _) | (true, false, Some(_)) => splice(p, type_id, self.max_length, self.lap)?,
            _ => None,
        };
        let lap_at_joins = beam && layout.runs_along() && self.lap_at_joins.unwrap_or(false);
        // Overlapping stirrups: in a column whose section is made of rectangles.
        let overlapping = self.overlapping.unwrap_or(false);
        if overlapping {
            let outline = p
                .element(host_id)
                .and_then(|e| p.element_type(e.type_id))
                .and_then(|t| p.type_section(&t.data))
                .map(|s| s.outline.outer)
                .unwrap_or_default();
            if beam || !matches!(layout, RebarLayout::Stirrups { .. }) {
                return Err(EngineError::invalid("`overlapping` goes with the `stirrups` of a column"));
            }
            if cc_geom::faces::rectangles(&outline).len() < 2 {
                return Err(EngineError::invalid(
                    "overlapping stirrups need a section made of rectangles (an L, T or U with square corners, its sides along its own axes)",
                ));
            }
        }
        // Lifts at given heights: a column's vertical bars.
        let lifts = self.lifts.clone().unwrap_or_default();
        if !lifts.is_empty() {
            if beam || !matches!(layout, RebarLayout::Longitudinal { face: BarFace::Perimeter, .. }) {
                return Err(EngineError::invalid("`lifts` go with the `perimeter` bars of a column"));
            }
            if !Rebar::lifts_are_valid(&lifts) {
                return Err(EngineError::invalid(format!(
                    "lifts are heights above the base, positive and rising, at most {}",
                    Rebar::MAX_LIFTS
                )));
            }
        }
        // The lap of the lifts is the splice's: kept even when no stock length was asked for.
        let splice = match (lifts.is_empty(), splice) {
            (false, None) => super::rebar::splice(p, type_id, None, self.lap)?,
            (_, splice) => splice,
        };
        let partition = partition_arg(self.partition.as_deref())?;
        let set = |host| {
            let (layout, faces, lifts) = (layout.clone(), faces.clone(), lifts.clone());
            let partition = partition.clone();
            ElementKind::Rebar(Rebar {
                splice,
                faces,
                lap_at_joins,
                overlapping,
                lifts,
                partition,
                ..Rebar::new(host, layout, cover)
            })
        };
        // The other members of the host's type, each on its own level.
        let host_type = p.element(host_id).map(|e| e.type_id);
        let others: Vec<(ElementId, cc_core::LevelId)> = match self.same_type.unwrap_or(false) {
            true => p
                .elements
                .values()
                .filter(|e| e.id != host_id && Some(e.type_id) == host_type)
                .filter(|e| matches!(e.kind, ElementKind::Beam(_) | ElementKind::Column(_)))
                .map(|e| (e.id, e.level))
                .collect(),
            false => Vec::new(),
        };
        let (id, also) = edit(s, t!("tx-create-rebar"), |e| {
            let id = add_element(e, level, type_id, set(host_id));
            let also: Vec<ElementId> = others.iter().map(|(h, l)| add_element(e, *l, type_id, set(*h))).collect();
            Ok((id, also))
        })?;
        let mut out = created(s, id);
        if self.same_type.unwrap_or(false)
            && let Some(map) = out.as_object_mut()
        {
            map.insert("also".into(), ids_json(&also));
        }
        Ok(out)
    }
}

/// Give beams and columns without reinforcement a typical one: beams two
/// (three when 20 cm wide or more) 10 mm bars at the bottom, two 8 mm at the
/// top (both lap spliced past the stock length) and 5 mm stirrups every 15 cm; columns four 10 mm bars (six in round
/// ones) and 5 mm ties every 15 cm. Default: every beam and column of
/// `level` (or of the project).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RebarTypical {
    /// Beam and column ids (default: all of `level`, or of the project).
    #[serde(default)]
    pub elements: Vec<u64>,
    /// Level id or name, when no elements are given.
    #[serde(default)]
    pub level: Option<Ref>,
}

impl Command for RebarTypical {
    const NAME: &'static str = "rebar_typical";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let hosts: Vec<(ElementId, bool)> = if self.elements.is_empty() {
            let level = self.level.as_ref().map(|r| refs::level(p, r)).transpose()?;
            p.elements
                .values()
                .filter(|e| level.is_none_or(|l| e.level == l))
                .filter_map(|e| match e.kind {
                    ElementKind::Beam(_) => Some((e.id, true)),
                    ElementKind::Column(_) => Some((e.id, false)),
                    _ => None,
                })
                .collect()
        } else {
            self.elements.iter().map(|id| host(p, *id)).collect::<Result<_>>()?
        };
        let [cover, spacing, main_bar, top_bar, stirrup_bar] = p.defaults().rebar;
        let (main, top, ties) = (bar_type(p, main_bar)?, bar_type(p, top_bar)?, bar_type(p, stirrup_bar)?);
        let mut todo = Vec::new();
        let mut skipped = 0;
        for (id, beam) in hosts {
            let Some(e) = p.element(id) else { continue };
            if p.rebar_of(id).next().is_some() {
                skipped += 1;
                continue;
            }
            let stirrups = RebarLayout::Stirrups { spacing };
            if beam {
                let wide = matches!(p.element_type(e.type_id).map(|t| &t.data), Some(TypeData::Beam { width, .. }) if *width >= 0.2 - 1e-9);
                let bottom = RebarLayout::Longitudinal { face: BarFace::Bottom, count: if wide { 3 } else { 2 } };
                todo.push((e.level, main, rebar_kind(id, bottom, cover, splice(p, main, None, None)?)));
                let upper = RebarLayout::Longitudinal { face: BarFace::Top, count: 2 };
                todo.push((e.level, top, rebar_kind(id, upper, cover, splice(p, top, None, None)?)));
            } else {
                let round = p.element_type(e.type_id).is_some_and(|t| p.section_is_round(&t.data));
                let bars = RebarLayout::Longitudinal { face: BarFace::Perimeter, count: if round { 6 } else { 4 } };
                todo.push((e.level, main, rebar_kind(id, bars, cover, None)));
            }
            todo.push((e.level, ties, rebar_kind(id, stirrups, cover, None)));
        }
        if todo.is_empty() {
            return Ok(json!({"created": [], "skipped": skipped}));
        }
        let created = edit(s, t!("tx-create-rebar"), |e| {
            Ok(todo.into_iter().map(|(level, ty, kind)| add_element(e, level, ty, kind)).collect::<Vec<_>>())
        })?;
        Ok(json!({"created": ids_json(&created), "skipped": skipped}))
    }
}

/// What a host of reinforcement is, for copying its sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostKind {
    Column,
    Beam,
    Wall,
    Strip,
}

/// The kind of host `e` is; `None` for floors and slab foundations (their
/// sets keep plan points) and for anything that takes no reinforcement.
fn host_kind(e: &cc_core::Element) -> Option<HostKind> {
    match &e.kind {
        ElementKind::Column(_) => Some(HostKind::Column),
        ElementKind::Beam(_) => Some(HostKind::Beam),
        ElementKind::Wall(_) => Some(HostKind::Wall),
        ElementKind::Foundation(f) if matches!(f.shape, cc_core::FoundationShape::Strip { .. }) => {
            Some(HostKind::Strip)
        }
        _ => None,
    }
}

/// Whether two loops are the same within a hundredth of a millimeter.
fn same_loop(a: &[glam::DVec2], b: &[glam::DVec2]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 1e-5)
}

/// Whether hosts `a` and `b` of one kind take the same sets: the same type,
/// or (columns and beams) types with the same section as placed.
fn same_section(p: &Project, a: &cc_core::Element, b: &cc_core::Element) -> bool {
    if a.type_id == b.type_id {
        return true;
    }
    if !matches!(a.kind, ElementKind::Column(_) | ElementKind::Beam(_)) {
        return false;
    }
    let outline =
        |e: &cc_core::Element| p.element_type(e.type_id).and_then(|t| p.type_section(&t.data)).map(|s| s.outline);
    match (outline(a), outline(b)) {
        (Some(x), Some(y)) => {
            same_loop(&x.outer, &y.outer)
                && x.holes.len() == y.holes.len()
                && x.holes.iter().zip(&y.holes).all(|(h, k)| same_loop(h, k))
        }
        _ => false,
    }
}

/// Copy the reinforcement sets of one host (all, or those of `sets`) to other
/// hosts of the same kind and section in one step: columns and beams of the
/// same type or of types with the same section, walls and strip footings of
/// the same type. Each copy keeps its bar type, layout, cover, splices,
/// partition and hand-given mark. Floors and slab foundations, whose sets keep
/// plan points, are not copied. With `replace`, each target's own sets go
/// first. Nothing is changed when a target does not match. Returns the
/// created set ids and those replaced.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RebarCopy {
    /// The host whose sets are copied: a column, beam, wall or strip footing.
    pub from: u64,
    /// The hosts that receive them.
    pub to: Vec<u64>,
    /// The sets of `from` to copy (default all).
    #[serde(default)]
    pub sets: Vec<u64>,
    /// Delete the targets' own sets first (default false: add beside them).
    #[serde(default)]
    pub replace: Option<bool>,
}

impl Command for RebarCopy {
    const NAME: &'static str = "rebar_copy";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let from = refs::element(p, self.from)?;
        let source = p.element(from).ok_or_else(|| EngineError::not_found("host"))?;
        let kind = host_kind(source).ok_or_else(|| {
            EngineError::invalid(
                "copy from a column, beam, wall or strip footing (sets of floors and slab foundations keep plan points)",
            )
        })?;
        let own: Vec<ElementId> = p.rebar_of(from).map(|(e, _)| e.id).collect();
        let mut sets: Vec<ElementId> = Vec::new();
        for id in &self.sets {
            let id = refs::element(p, *id)?;
            if !own.contains(&id) {
                return Err(EngineError::invalid(format!("set {} is not one of host {}'s", id.0, from.0)));
            }
            if !sets.contains(&id) {
                sets.push(id);
            }
        }
        if self.sets.is_empty() {
            sets = own;
        }
        if sets.is_empty() {
            return Err(EngineError::invalid("the host has no reinforcement to copy"));
        }
        if self.to.is_empty() {
            return Err(EngineError::invalid("give at least one host to copy to"));
        }
        let mut targets: Vec<(ElementId, cc_core::LevelId)> = Vec::new();
        for id in &self.to {
            let id = refs::element(p, *id)?;
            let e = p.element(id).ok_or_else(|| EngineError::not_found("host"))?;
            if id == from {
                return Err(EngineError::invalid("a host cannot be copied to itself"));
            }
            if host_kind(e) != Some(kind) || !same_section(p, source, e) {
                return Err(EngineError::invalid(format!(
                    "element {} is not a host of the same kind and section as {}",
                    id.0, from.0
                )));
            }
            if !targets.iter().any(|(t, _)| *t == id) {
                targets.push((id, e.level));
            }
        }
        let originals: Vec<(ElementTypeId, Rebar)> = sets
            .iter()
            .filter_map(|id| match p.element(*id).map(|e| (e.type_id, &e.kind)) {
                Some((ty, ElementKind::Rebar(r))) => Some((ty, r.clone())),
                _ => None,
            })
            .collect();
        let replaced: Vec<ElementId> = match self.replace.unwrap_or(false) {
            true => targets.iter().flat_map(|(t, _)| p.rebar_of(*t).map(|(e, _)| e.id)).collect(),
            false => Vec::new(),
        };
        let created = edit(s, t!("tx-copy-rebar"), |e| {
            for id in &replaced {
                e.remove_element(*id);
            }
            let mut out = Vec::new();
            for (target, level) in &targets {
                for (ty, set) in &originals {
                    let copy = Rebar { host: *target, ..set.clone() };
                    out.push(add_element(e, *level, *ty, ElementKind::Rebar(copy)));
                }
            }
            Ok(out)
        })?;
        Ok(json!({"created": ids_json(&created), "replaced": ids_json(&replaced)}))
    }
}
