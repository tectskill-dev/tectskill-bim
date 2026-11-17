//! Steel and timber connections (see `cc_core::connection`): placing them
//! on member ends, the checks `element_set` runs on them and the values
//! `element_get` derives.

use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::connection::{self as cn, Connection, ConnectionKind, ConnectionParams, MemberEnd};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, LevelId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The type a project gets with its first connection.
pub fn new_connection_type(id: ElementTypeId) -> ElementType {
    cn::host_type(id, t!("tpl-type-connection"))
}

fn kind_arg(code: &str) -> Result<ConnectionKind> {
    ConnectionKind::from_code(code).ok_or_else(|| {
        let codes: Vec<&str> = ConnectionKind::ALL.iter().map(|k| k.code()).collect();
        EngineError::invalid(format!("unknown kind '{code}' ({})", codes.join(", ")))
    })
}

/// `kind`'s defaults with `patch` merged in, every name known and every value in range.
pub(crate) fn params_arg(kind: ConnectionKind, patch: &Value) -> Result<ConnectionParams> {
    let defaults = kind.defaults();
    let known = serde_json::to_value(defaults).map_err(|e| EngineError::invalid(e.to_string()))?;
    let (Value::Object(given), Value::Object(names)) = (patch, &known) else {
        return Err(EngineError::invalid("params must be an object"));
    };
    if let Some(bad) = given.keys().find(|k| !names.contains_key(*k)) {
        let all: Vec<&String> = names.keys().collect();
        return Err(EngineError::invalid(format!("unknown parameter '{bad}' ({all:?})")));
    }
    let params: ConnectionParams = super::patch_value(&defaults, patch)?;
    params.check().map_err(EngineError::invalid)?;
    Ok(params)
}

/// Place steel or timber connections of one `kind` at every member end that
/// suits it: `base_plate` under columns; `end_plate` and `timber_bolts` at
/// beam ends that stop at a column; `fin_plate` at beam ends at a column or
/// another beam; `hanger` at beam ends at another beam. Ends that already
/// have a connection are skipped. Plates and bolts follow their member.
/// One undo step; returns the new ids (`created`) and how many ends were
/// `skipped`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsCreate {
    /// `base_plate`, `end_plate`, `fin_plate`, `timber_bolts` or `hanger`.
    pub kind: String,
    /// Column ids (base plates) or beam ids (the other kinds); default: every one of `level`.
    #[serde(default)]
    pub members: Vec<u64>,
    /// Level id or name when no `members` are given (default: the active level, else every level); ignored with `members`.
    #[serde(default)]
    pub level: Option<Ref>,
    /// Sizes to change from the kind's defaults, meters: `thickness`, `width` and `length` (0: from the
    /// member's section plus `edge`), `bolt_diameter`, `rows` and `columns` (1 to 10), `spacing`, `edge`,
    /// `anchor_length`. Lengths are 0.001 to 2.
    #[serde(default)]
    pub params: Option<Value>,
}

impl Command for ConnectionsCreate {
    const NAME: &'static str = "connections_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let kind = kind_arg(&self.kind)?;
        let params = match &self.params {
            Some(v) => params_arg(kind, v)?,
            None => kind.defaults(),
        };
        let wanted = if kind.at_column() { Category::Column } else { Category::Beam };
        let p = s.project();
        let mut members: Vec<ElementId> = if self.members.is_empty() {
            let level = match &self.level {
                Some(r) => Some(refs::level(p, r)?),
                None => s.active_level(),
            };
            p.elements_of(wanted).filter(|e| level.is_none_or(|l| e.level == l)).map(|e| e.id).collect()
        } else {
            let mut out = Vec::new();
            for &raw in &self.members {
                let id = refs::element(p, raw)?;
                if p.element(id).map(|e| e.category()) != Some(wanted) {
                    let what = if kind.at_column() { "column" } else { "beam" };
                    return Err(EngineError::invalid(format!("element {raw} is not a {what}")));
                }
                out.push(id);
            }
            out
        };
        members.sort();
        members.dedup();
        let (p, geom) = s.project_and_geometry();
        let mut todo: Vec<(LevelId, Connection)> = Vec::new();
        let mut skipped = 0usize;
        for id in members {
            let Some(e) = p.element(id) else { continue };
            let ends: Vec<MemberEnd> = if kind.at_column() {
                vec![MemberEnd::Base]
            } else {
                // Not in the session geometry (a beam of an option not being shown): nothing to place on.
                let Some(g) = geom.beams.get(&id) else {
                    skipped += 1;
                    continue;
                };
                [MemberEnd::Start, MemberEnd::End]
                    .into_iter()
                    .filter(|end| {
                        cc_geom::connection::beam_end_join(g, *end).is_some_and(|j| cc_geom::connection::suits(kind, j))
                    })
                    .collect()
            };
            for end in ends {
                if p.connection_at(id, end).is_some() {
                    skipped += 1;
                    continue;
                }
                let mut c = Connection::new(id, end, kind);
                c.params = params;
                todo.push((e.level, c));
            }
        }
        if todo.is_empty() {
            return Ok(json!({"created": [], "skipped": skipped}));
        }
        let host_type = p.default_type(Category::Connection);
        let created = edit(s, t!("tx-create-connections"), |e| {
            let type_id = match host_type {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_connection_type(id))
                }
            };
            Ok(todo
                .into_iter()
                .map(|(level, c)| add_element(e, level, type_id, ElementKind::Connection(c)))
                .collect::<Vec<_>>())
        })?;
        Ok(json!({"created": ids_json(&created), "skipped": skipped}))
    }
}

/// `element_set` on a connection: it keeps its member, end and kind, and
/// its sizes stay in range.
pub(crate) fn check(old: &Connection, new: &Connection) -> Result<()> {
    if (new.member, new.end, new.kind) != (old.member, old.end, old.kind) {
        return Err(EngineError::invalid("a connection keeps its member, end and kind: delete it and place another"));
    }
    new.params.check().map_err(EngineError::invalid)
}

/// `element_get`'s derived values of a connection.
pub(crate) fn derived(g: &cc_geom::connection::ConnectionGeom, out: &mut serde_json::Map<String, Value>) {
    out.insert("plate_volume".into(), json!(g.plate_volume()));
    out.insert("plate_mass".into(), json!(g.plate_mass()));
    out.insert("bolt_count".into(), json!(g.bolt_count()));
    out.insert("bolt_diameter".into(), json!(g.bolt_diameter));
    out.insert("bolt_length".into(), json!(g.bolt_length()));
    out.insert("warning".into(), json!(g.warning.map(|w| w.code())));
}
