//! Schedule definitions (see `cc_core::schedule`): the fields a schedule
//! shows, its filters, its sorting and grouping, how each column is written
//! and how the table looks on paper.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, ColumnFormat, ScheduleAppearance, ScheduleDef, ScheduleField, ScheduleFilter, SortLevel, ViewId, ViewKind,
};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The schedule view `view` names: its id, category and definition.
fn schedule_of(s: &Session, view: &Ref) -> Result<(ViewId, Category, ScheduleDef)> {
    let p = s.project();
    let id = refs::view(p, view)?;
    match p.view(id).map(|v| &v.kind) {
        Some(ViewKind::Schedule { category, def }) => Ok((id, *category, def.clone())),
        _ => Err(EngineError::invalid("not a schedule view")),
    }
}

/// A field by its name, as the model writes it (`Mark`, `Type`, `Length`…).
fn field(name: &str) -> Result<ScheduleField> {
    serde_json::from_value(Value::String(name.trim().to_string()))
        .map_err(|_| EngineError::invalid(format!("unknown field `{name}` (see `schedule_get`'s `available`)")))
}

/// `value` read as part of a definition, with what it is in the message.
fn part<T: DeserializeOwned>(value: &Value, what: &str) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|e| EngineError::invalid(format!("{what}: {e}")))
}

fn name_of(f: ScheduleField) -> Value {
    serde_json::to_value(f).unwrap_or(Value::Null)
}

/// A definition as the queries show it: every part written out, the older
/// single filter and sort field read as the first of `filters` and `sorting`.
fn def_json(category: Category, def: &ScheduleDef) -> Value {
    let fields = def.fields_for(category);
    json!({
        "fields": fields.iter().map(|f| name_of(*f)).collect::<Vec<_>>(),
        "filters": def.all_filters(),
        "sorting": def.all_sorting(),
        "itemize": !def.collapse,
        "totals": !def.hide_totals,
        "columns": fields.iter().map(|f| def.format_of(*f)).collect::<Vec<_>>(),
        "appearance": def.appearance,
        "bending": def.bending,
    })
}

/// A schedule's definition: the `fields` it shows in order, its `filters`
/// (`field`, `op`, `value`), its `sorting` levels (`field`, `descending`,
/// `header`, `footer`, `blank_line`), whether every element has its own row
/// (`itemize`), whether the totals row shows (`totals`), each column's
/// format (`columns`: `heading`, `align`, `hidden`, `total`) and the table's
/// `appearance` on paper; and the fields `available` to schedules of its
/// category. `schedule_set` changes any of them.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduleGet {
    /// Schedule view id or name.
    pub view: Ref,
}

impl Command for ScheduleGet {
    const NAME: &'static str = "schedule_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (id, category, def) = schedule_of(s, &self.view)?;
        let mut out = def_json(category, &def);
        out["view"] = json!(id.0);
        out["category"] = json!(category);
        out["available"] = ScheduleField::available(category).into_iter().map(name_of).collect();
        Ok(out)
    }
}

/// Change a schedule's definition; every part is optional and replaces the
/// part it names, the others staying as they are. `fields` are the columns
/// in order. `filters` keep the rows that pass every one: each compares a
/// `field` with a `value` by `op` (`equals`, `not_equals`, `contains`,
/// `not_contains`, `begins_with`, `not_begins_with`, `greater`,
/// `greater_or_equal`, `less`, `less_or_equal`, `has_value`,
/// `has_no_value`); numbers compare as numbers in SI units (lengths in
/// meters), texts without case. `sorting` orders the rows by up to four
/// levels, the first outermost; a level with `header`, `footer` or
/// `blank_line` groups the rows alike in it under a heading, over a footer
/// (the group's count and totals) or apart by a blank line. With `itemize`
/// false, the elements alike in every sorting level share one row: values
/// that differ among them read as varying and totaled columns add up (the
/// `Count` field says how many). `columns` formats the columns of the fields
/// named (`heading`, `align`: `Auto`, `Left`, `Center`, `Right`; `hidden`: a
/// hidden field still sorts and filters; `total`: add the column up in
/// footers and totals), `totals` shows or hides the totals row, and
/// `appearance` is how the table is drawn on sheets and exports
/// (`grid_lines`, `outline`, `show_title`, `show_headers`, and the
/// `title_size`, `header_size` and `text_size` in millimeters). One undo
/// step; returns the definition as `schedule_get` does.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ScheduleSet {
    /// Schedule view id or name.
    pub view: Ref,
    /// The fields shown, in order (names as `schedule_get` lists them in `available`).
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    /// The filters, `[{"field": "Level", "op": "equals", "value": "Térreo"}, …]` (at most 8; `[]` for none).
    #[serde(default)]
    pub filters: Option<Value>,
    /// The sorting levels, `[{"field": "Type", "header": true, "footer": true}, …]` (at most 4; `[]` for none).
    #[serde(default)]
    pub sorting: Option<Value>,
    /// One row per element (default true); false gathers the elements alike in every sorting level in one row.
    #[serde(default)]
    pub itemize: Option<bool>,
    /// Show the totals row.
    #[serde(default)]
    pub totals: Option<bool>,
    /// Column formats, `[{"field": "Area", "heading": "Net area", "align": "Right", "total": true}, …]`: the fields named take the format given, the others keep theirs (`[]` clears every format).
    #[serde(default)]
    pub columns: Option<Value>,
    /// How the table looks on paper: a patch over `{grid_lines, outline, show_title, show_headers, title_size, header_size, text_size}`.
    #[serde(default)]
    pub appearance: Option<Value>,
}

impl Command for ScheduleSet {
    const NAME: &'static str = "schedule_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (id, category, mut def) = schedule_of(s, &self.view)?;
        let available = ScheduleField::available(category);
        let known = |f: ScheduleField| match available.contains(&f) {
            true => Ok(f),
            false => Err(EngineError::invalid(format!("schedules of this category have no `{}` field", name_of(f)))),
        };
        if let Some(names) = &self.fields {
            let mut fields = Vec::new();
            for name in names {
                let f = known(field(name)?)?;
                if fields.contains(&f) {
                    return Err(EngineError::invalid(format!("the field `{name}` is listed twice")));
                }
                fields.push(f);
            }
            if fields.is_empty() {
                return Err(EngineError::invalid("a schedule shows at least one field"));
            }
            def.fields = fields;
        }
        if let Some(value) = &self.filters {
            let filters: Vec<ScheduleFilter> = part(value, "filters")?;
            for f in &filters {
                known(f.field)?;
            }
            // The older single filter is one of these now.
            (def.filters, def.filter) = (filters, None);
        }
        if let Some(value) = &self.sorting {
            let sorting: Vec<SortLevel> = part(value, "sorting")?;
            for l in &sorting {
                known(l.field)?;
            }
            (def.sorting, def.sort_by, def.descending) = (sorting, None, false);
        }
        if let Some(itemize) = self.itemize {
            def.collapse = !itemize;
        }
        if let Some(totals) = self.totals {
            def.hide_totals = !totals;
        }
        if let Some(value) = &self.columns {
            let columns: Vec<ColumnFormat> = part(value, "columns")?;
            if columns.is_empty() {
                def.columns.clear();
            }
            for c in columns {
                known(c.field)?;
                def.columns.retain(|x| x.field != c.field);
                // A format that says nothing is not kept.
                if !c.is_plain() {
                    def.columns.push(ColumnFormat { heading: c.heading.map(|h| h.trim().to_string()), ..c });
                }
            }
        }
        if let Some(patch) = &self.appearance {
            let mut value = serde_json::to_value(def.appearance).unwrap_or(Value::Null);
            super::merge(&mut value, patch);
            let appearance: ScheduleAppearance = part(&value, "appearance")?;
            def.appearance = appearance;
        }
        if let Some(message) = def.check() {
            return Err(EngineError::invalid(message));
        }
        edit(s, t!("tx-modify-view"), |e| {
            if let Some(ViewKind::Schedule { def: d, .. }) = e.view_mut(id).map(|v| &mut v.kind) {
                *d = def.clone();
            }
            Ok(())
        })?;
        let mut out = def_json(category, &def);
        out["view"] = json!(id.0);
        Ok(out)
    }
}
