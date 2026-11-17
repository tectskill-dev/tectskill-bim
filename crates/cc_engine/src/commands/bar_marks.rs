//! Bar marks, bending schedules, bar tags and the check of bar lengths (see
//! `cc_core::bar_marks`, `cc_core::bar_tag`, `cc_geom::bar_marks` and
//! `cc_views::bending`).

use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::bar_marks::{BarMarking, FrozenMark, MAX_FROZEN, MAX_MARK_TEXT, MAX_START};
use cc_core::bar_tag::{BarTag, BarTagParts};
use cc_core::{
    BendingOptions, Category, ElementId, ElementKind, ElementType, ElementTypeId, ScheduleDef, TypeData, View, ViewId,
    ViewKind,
};
use cc_geom::bar_marks::{self as marks, BarMarks, Mark, MarkSource, to_mark_shape};
use cc_geom::bar_shape::ShapeSummary;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A shape as the queries show it: lengths in meters, turns in degrees.
pub(crate) fn shape_json(s: &ShapeSummary) -> Value {
    let legs: Vec<Value> = s
        .legs
        .iter()
        .map(|l| json!({"length": l.length, "radius": l.radius, "sweep": l.sweep.to_degrees(), "bend": l.bend.to_degrees()}))
        .collect();
    json!({ "closed": s.closed, "legs": legs })
}

/// The marks of the model's bars, with the project's units set for texts.
fn current_marks(s: &mut Session) -> BarMarks {
    cc_views::format::set_units(&s.project().units);
    let (p, g) = s.project_and_geometry();
    marks::resolve(p, &g.rebar)
}

fn source_name(source: MarkSource) -> &'static str {
    match source {
        MarkSource::Frozen(_) => "frozen",
        MarkSource::Override => "override",
        MarkSource::Numbered => "numbered",
    }
}

/// A mark as the queries show it.
pub(crate) fn mark_json(m: &Mark) -> Value {
    json!({
        "mark": m.mark,
        "diameter": m.diameter,
        "grade": m.grade,
        "code": cc_views::bending::shape_code(&m.shape),
        "shape": shape_json(&m.shape),
        "unit_length": m.unit_length(),
        "count": m.count,
        "total_length": m.total_length,
        "weight": m.weight,
        "longest": m.longest,
        "sets": m.sets.iter().map(|id| id.0).collect::<Vec<_>>(),
        "source": source_name(m.source),
        "frozen": m.frozen(),
        "overridden": m.source == MarkSource::Override,
    })
}

fn settings_json(m: &BarMarking) -> Value {
    json!({
        "prefix": m.prefix,
        "start": m.start,
        "length_tolerance": m.length_tolerance,
        "stock_length": m.stock_length,
        "frozen": m.frozen.len(),
    })
}

/// Every bar mark of the model, sorted naturally (N2 before N10): its
/// diameter and grade, its shape (`code`, e.g. `L 30-470`, and the legs),
/// the length of one bar (`unit_length`, the mean of its bars), how many
/// bars have it, their total length and weight, the longest bar, the sets
/// using it, and where it comes from (`source`: `frozen`, `override` or
/// `numbered`). Frozen marks no bar uses are listed with a count of 0.
/// Also the marking settings. A mark names one shape: the same diameter,
/// grade, legs and bends within the length tolerance (default 5 mm)
/// and 1 degree, read from either end or mirrored. Lengths in meters,
/// angles in degrees, weights in kilograms.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarMarksList {}

impl Command for BarMarksList {
    const NAME: &'static str = "bar_marks_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let marks = current_marks(s);
        Ok(json!({
            "settings": settings_json(&s.project().bar_marking),
            "marks": marks.marks.iter().map(mark_json).collect::<Vec<_>>(),
        }))
    }
}

/// The frozen entry of a numbered mark; none for a shape too long to keep.
fn frozen_entry(m: &Mark) -> Option<FrozenMark> {
    Some(FrozenMark {
        mark: m.mark.clone(),
        diameter: m.diameter,
        grade: m.grade.clone(),
        shape: to_mark_shape(&m.shape)?,
    })
}

/// Freeze the bar marks: the marks numbered for shapes the frozen marks
/// table does not hold yet are added to it, so they stay when the model
/// changes (entries already there are kept, overrides stay on their sets).
/// With `drop_unused`, entries no bar uses any more are dropped. Changes
/// nothing (and makes no undo step) when there is nothing to freeze or drop.
/// `bar_marks_list` shows the result.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarMarksUpdate {
    /// Drop frozen marks that no bar uses (default false).
    #[serde(default)]
    pub drop_unused: bool,
}

impl Command for BarMarksUpdate {
    const NAME: &'static str = "bar_marks_update";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let marks = current_marks(s);
        let mut marking = s.project().bar_marking.clone();
        let dropped: Vec<String> = if self.drop_unused {
            let unused: Vec<usize> = marks
                .marks
                .iter()
                .filter_map(|m| match m.source {
                    MarkSource::Frozen(i) if m.count == 0 => Some(i),
                    _ => None,
                })
                .collect();
            let names = unused.iter().filter_map(|i| marking.frozen.get(*i)).map(|f| f.mark.clone()).collect();
            let mut i = 0;
            marking.frozen.retain(|_| {
                i += 1;
                !unused.contains(&(i - 1))
            });
            names
        } else {
            Vec::new()
        };
        let mut added = Vec::new();
        let mut skipped = 0;
        for m in marks.marks.iter().filter(|m| m.source == MarkSource::Numbered) {
            match frozen_entry(m) {
                Some(f) => {
                    added.push(f.mark.clone());
                    marking.frozen.push(f);
                }
                None => skipped += 1,
            }
        }
        if marking.frozen.len() > MAX_FROZEN {
            return Err(EngineError::invalid(format!("the frozen marks table holds at most {MAX_FROZEN} marks")));
        }
        if !added.is_empty() || !dropped.is_empty() {
            edit(s, t!("tx-bar-marks-update"), |e| {
                *e.bar_marking_mut() = marking.clone();
                Ok(())
            })?;
        }
        Ok(json!({
            "frozen": added,
            "dropped": dropped,
            "skipped": skipped,
            "settings": settings_json(&s.project().bar_marking),
        }))
    }
}

/// Number the bar marks afresh: the frozen marks table is cleared, and so
/// are the sets' `bar_mark` overrides unless `keep_overrides`; then every
/// shape is numbered from `start` (default: the current one) with `prefix`
/// (default: the current one) in the stable order (partition, diameter
/// descending, length descending, shape) and the numbering is frozen. One
/// undo step; returns the new marks.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarMarksRenumber {
    /// Text before the numbers (e.g. "N"; at most 40 characters).
    #[serde(default)]
    pub prefix: Option<String>,
    /// First number (0 to 1000000).
    #[serde(default)]
    pub start: Option<u32>,
    /// Keep the marks the sets set themselves (default false: they are cleared).
    #[serde(default)]
    pub keep_overrides: bool,
}

impl Command for BarMarksRenumber {
    const NAME: &'static str = "bar_marks_renumber";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let mut marking = s.project().bar_marking.clone();
        if let Some(prefix) = &self.prefix {
            if prefix.chars().count() > MAX_MARK_TEXT {
                return Err(EngineError::invalid(format!("the prefix has at most {MAX_MARK_TEXT} characters")));
            }
            marking.prefix = prefix.trim().to_string();
        }
        if let Some(start) = self.start {
            if start > MAX_START {
                return Err(EngineError::invalid(format!("start must be at most {MAX_START}")));
            }
            marking.start = start;
        }
        marking.frozen.clear();
        cc_views::format::set_units(&s.project().units);
        let (p, g) = s.project_and_geometry();
        let fresh = marks::resolve_with(p, &g.rebar, &marking, self.keep_overrides);
        marking.frozen =
            fresh.marks.iter().filter(|m| m.source == MarkSource::Numbered).filter_map(frozen_entry).collect();
        if marking.frozen.len() > MAX_FROZEN {
            return Err(EngineError::invalid(format!("the frozen marks table holds at most {MAX_FROZEN} marks")));
        }
        let overridden: Vec<ElementId> = if self.keep_overrides {
            Vec::new()
        } else {
            p.elements.values().filter(|e| e.rebar_bar_mark().is_some()).map(|e| e.id).collect()
        };
        edit(s, t!("tx-bar-marks-renumber"), |e| {
            *e.bar_marking_mut() = marking.clone();
            for id in &overridden {
                match e.element_mut(*id).map(|x| &mut x.kind) {
                    Some(ElementKind::Rebar(r)) => r.bar_mark = None,
                    Some(ElementKind::ShapedRebar(r)) => r.bar_mark = None,
                    _ => {}
                }
            }
            Ok(())
        })?;
        let marks = current_marks(s);
        Ok(json!({
            "settings": settings_json(&s.project().bar_marking),
            "marks": marks.marks.iter().map(mark_json).collect::<Vec<_>>(),
        }))
    }
}

/// Change how bars are marked: the `prefix` and `start` of numbered marks
/// (used by marks numbered from now on; `bar_marks_renumber` renumbers the
/// frozen ones), the length tolerance within which bars are one shape, and
/// the stock length bars are bought in (`bar_length_check` and bending
/// schedules flag longer bars). One undo step.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarMarkingSet {
    /// Text before the numbers (at most 40 characters).
    #[serde(default)]
    pub prefix: Option<String>,
    /// First number (0 to 1000000).
    #[serde(default)]
    pub start: Option<u32>,
    /// Legs of bars of one mark differ by at most this, meters (0.0001 to 0.05; default 0.005).
    #[serde(default)]
    pub length_tolerance: Option<f64>,
    /// Length bars are bought in, meters (1 to 100; default 12).
    #[serde(default)]
    pub stock_length: Option<f64>,
}

impl Command for BarMarkingSet {
    const NAME: &'static str = "bar_marking_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let mut marking = s.project().bar_marking.clone();
        if let Some(prefix) = &self.prefix {
            marking.prefix = prefix.trim().to_string();
        }
        if let Some(start) = self.start {
            marking.start = start;
        }
        if let Some(t) = self.length_tolerance {
            marking.length_tolerance = finite(t, "length_tolerance")?;
        }
        if let Some(l) = self.stock_length {
            marking.stock_length = finite(l, "stock_length")?;
        }
        if let Some(message) = marking.check() {
            return Err(EngineError::invalid(message));
        }
        edit(s, t!("tx-bar-marking-set"), |e| {
            *e.bar_marking_mut() = marking.clone();
            Ok(())
        })?;
        Ok(settings_json(&s.project().bar_marking))
    }
}

/// Bars longer than `max_length` (default: the project's stock length, 12 m
/// unless `bar_marking_set` changed it): the bars that need splices, listed
/// by set and mark with how many there are and the longest. A set with a
/// `max_length` of its own cuts its bars into lapped pieces, which then pass.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarLengthCheck {
    /// Longest bar allowed, meters (default: the stock length).
    #[serde(default)]
    pub max_length: Option<f64>,
}

impl Command for BarLengthCheck {
    const NAME: &'static str = "bar_length_check";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let max = match self.max_length {
            Some(m) => {
                let m = finite(m, "max_length")?;
                if m <= 0.0 {
                    return Err(EngineError::invalid("max_length must be above 0"));
                }
                m
            }
            None => s.project().bar_marking.stock_length,
        };
        let marks = current_marks(s);
        let (p, g) = s.project_and_geometry();
        let mut out = Vec::new();
        let mut total = 0;
        for (id, set) in &g.rebar {
            // Per mark: the bars over and the longest.
            let mut over: Vec<(String, usize, f64)> = Vec::new();
            for (i, bar) in set.bars.iter().enumerate() {
                let length = bar.length();
                if length <= max + 1e-9 {
                    continue;
                }
                let mark = marks.of_bar(*id, i).map_or(String::new(), |m| m.mark.clone());
                match over.iter_mut().find(|(m, ..)| *m == mark) {
                    Some(o) => {
                        o.1 += 1;
                        o.2 = o.2.max(length);
                    }
                    None => over.push((mark, 1, length)),
                }
            }
            let e = p.element(*id);
            for (mark, count, longest) in over {
                total += count;
                out.push(json!({
                    "set": id.0,
                    "host": set.host.map(|h| h.0),
                    "partition": e.and_then(|e| e.rebar_partition()).unwrap_or(""),
                    "mark": mark,
                    "count": count,
                    "longest": longest,
                }));
            }
        }
        Ok(json!({ "max_length": max, "count": total, "bars": out }))
    }
}

/// Create a bending schedule: a reinforcement schedule view with one row
/// per bar mark (mark, diameter, grade, a sketch of the shape with its
/// dimensions, the length of one bar in centimeters, the count, the total
/// length and the weight, from the type's mass per meter or the diameter;
/// rows of bars longer than the stock length noted) and, under it, a
/// summary per diameter and grade (total length and weight; with
/// `waste_percent`, the weight with the waste added). Totals add the bars
/// exactly. It is placed on sheets, exported (`view_export`) and written to
/// CSV (`schedule_export_csv`, shapes as codes such as `L 30-470`) like any
/// schedule; `view_set` changes its `def.bending` options later.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BendingScheduleCreate {
    /// View name (default: the language's).
    #[serde(default)]
    pub name: Option<String>,
    /// Only the sets of these partitions (default: every set; "" stands for sets without one).
    #[serde(default)]
    pub partitions: Vec<String>,
    /// Rows under a heading per partition, each partition's marks counted on their own (default false).
    #[serde(default)]
    pub group_by_partition: bool,
    /// Waste added in the summary, percent (0 to 100; default: no waste columns).
    #[serde(default)]
    pub waste_percent: Option<f64>,
    /// Hide the totals row of the marks (default false).
    #[serde(default)]
    pub hide_totals: bool,
}

impl Command for BendingScheduleCreate {
    const NAME: &'static str = "bending_schedule_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let options = BendingOptions {
            partitions: self.partitions.iter().map(|p| p.trim().to_string()).collect(),
            group_by_partition: self.group_by_partition,
            waste_percent: self.waste_percent.map(|w| finite(w, "waste_percent")).transpose()?,
        };
        if let Some(message) = options.check() {
            return Err(EngineError::invalid(message));
        }
        let def = ScheduleDef { bending: Some(options), hide_totals: self.hide_totals, ..Default::default() };
        let name = self.name.unwrap_or_else(|| t!("default-bending-schedule-name"));
        let id = edit(s, t!("tx-new-view"), |e| {
            let id = ViewId(e.alloc_id());
            Ok(e.add_view(View::new(id, name, 1, ViewKind::Schedule { category: Category::Rebar, def })))
        })?;
        Ok(super::views::view_out(s, id))
    }
}

/// Whether `view` takes bar tags: plans, elevations and sections.
pub(crate) fn takes_tags(view: &View) -> bool {
    cc_views::bar_tags::takes_tags(view)
}

/// The parts of a tag's text to write.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct TagParts {
    /// The number of bars (default true).
    #[serde(default)]
    pub count: Option<bool>,
    /// The bar mark (default true).
    #[serde(default)]
    pub mark: Option<bool>,
    /// The diameter in millimeters (default true).
    #[serde(default)]
    pub diameter: Option<bool>,
    /// The spacing in centimeters, for sets with one (default true).
    #[serde(default)]
    pub spacing: Option<bool>,
    /// The length of one bar in centimeters (default true).
    #[serde(default)]
    pub length: Option<bool>,
}

/// Tag a reinforcement set in a plan, an elevation or a section: a text
/// such as `25 N1 Ø12.5 c/12 C=500` (the count, the mark, the diameter in
/// millimeters, the spacing in centimeters for sets with one, the length of
/// one bar in centimeters, `C=var` when they differ; a set of several marks
/// shows its first and `+`), derived from the set each time; `multiplier`
/// writes `2x` first for a set repeated on both faces. The view draws the
/// set's bars as thin lines (dots where seen end-on), so the tag points at
/// them. The tag goes with its view and its set. A project gets a bar tag
/// type (2.5 mm text) with its first tag; `element_get` gives the text
/// (`derived.text`) and `element_set` changes the tag's `params`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarTagCreate {
    /// View id or name: a floor or ceiling plan, an elevation or a section.
    pub view: Ref,
    /// The reinforcement set (or shaped bars) to tag.
    pub set: u64,
    /// Where the text starts (its left end, on the baseline), in the view's drawing coordinates.
    pub position: Point,
    /// Where a leader from the text points (default: no leader).
    #[serde(default)]
    pub leader: Option<Point>,
    /// How many times the set is repeated: 2 writes `2x` first (1 to 100; default 1).
    #[serde(default)]
    pub multiplier: Option<u32>,
    /// Angle of the text, degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation_deg: Option<f64>,
    /// The parts of the text to write, `{count, mark, diameter, spacing, length}`, each true or false (default: all).
    #[serde(default)]
    pub parts: Option<TagParts>,
    /// Bar tag type id or name (default: the default bar tag type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

/// A bar tag type in the interface language: text 2.5 mm.
fn new_type(id: ElementTypeId) -> ElementType {
    ElementType {
        id,
        name: t!("tpl-type-bar-tag"),
        data: TypeData::BarTag { text_size: cc_core::bar_tag::DEFAULT_TEXT_SIZE },
        info: Default::default(),
        uid: Default::default(),
    }
}

impl Command for BarTagCreate {
    const NAME: &'static str = "bar_tag_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let view_id = refs::view(p, &self.view)?;
        let view = p.view(view_id).ok_or_else(|| EngineError::not_found("view"))?;
        if !takes_tags(view) {
            return Err(EngineError::invalid(t!("msg-bar-tag-view")));
        }
        let set = refs::element(p, self.set)?;
        if !p.element(set).is_some_and(|e| matches!(e.kind, ElementKind::Rebar(_) | ElementKind::ShapedRebar(_))) {
            return Err(EngineError::invalid(t!("msg-bar-tag-set")));
        }
        let level = view
            .level()
            .or_else(|| p.levels.first().map(|l| l.id))
            .ok_or_else(|| EngineError::rejected("no levels"))?;
        // A project gets its bar tag type with its first tag.
        let ty = match (&self.type_ref, p.default_type(Category::BarTag)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::BarTag, r.as_ref())?),
        };
        let parts = self.parts.unwrap_or_default();
        let all = BarTagParts::default();
        let tag = BarTag {
            leader: self.leader.map(point).transpose()?,
            rotation_deg: self.rotation_deg.map(|a| finite(a, "rotation_deg")).transpose()?.unwrap_or(0.0),
            multiplier: self.multiplier.unwrap_or(1),
            parts: BarTagParts {
                count: parts.count.unwrap_or(all.count),
                mark: parts.mark.unwrap_or(all.mark),
                diameter: parts.diameter.unwrap_or(all.diameter),
                spacing: parts.spacing.unwrap_or(all.spacing),
                length: parts.length.unwrap_or(all.length),
            },
            ..BarTag::new(view_id, set, point(self.position)?)
        };
        if let Some(message) = tag.check() {
            return Err(EngineError::invalid(message));
        }
        let kind = ElementKind::BarTag(Box::new(tag));
        let id = edit(s, t!("tx-create-bar-tag"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_type(id))
                }
            };
            Ok(add_element(e, level, type_id, kind))
        })?;
        Ok(created(s, id))
    }
}

/// Refuse a changed tag a view or set would not take, or with bad values.
pub(crate) fn check_tag(p: &cc_core::Project, tag: &BarTag) -> Result<()> {
    if let Some(message) = tag.check() {
        return Err(EngineError::invalid(message));
    }
    if !p.view(tag.view).is_some_and(takes_tags) {
        return Err(EngineError::invalid(t!("msg-bar-tag-view")));
    }
    if !p.element(tag.set).is_some_and(|e| matches!(e.kind, ElementKind::Rebar(_) | ElementKind::ShapedRebar(_))) {
        return Err(EngineError::invalid(t!("msg-bar-tag-set")));
    }
    Ok(())
}

/// The text of tag `tag` as its view writes it.
pub(crate) fn tag_text(s: &mut Session, tag: &BarTag) -> Option<String> {
    cc_views::format::set_units(&s.project().units);
    let view = tag.view;
    s.view_geometry(view);
    let p = s.project();
    let g = s.geom_for(view);
    let marks = marks::resolve(p, &g.rebar);
    cc_views::bar_tags::text(p, g, &marks, tag)
}
