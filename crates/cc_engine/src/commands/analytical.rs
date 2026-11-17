//! The analytical model of the structure, for structural analysis programs.

use crate::error::{EngineError, Result};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use cc_core::{ElementKind, TypeData};
use cc_geom::analytical::{MemberKind, analytical_model};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Format version of the analytical model document.
/// Version 2: a member's `section` holds its profile and properties.
const FORMAT: u32 = 2;

/// The analytical model as JSON: nodes (`id`, `x`, `y`, `z` in meters,
/// `support` at column bases) and members (`element`, `kind`, `start` and
/// `end` nodes, `length`, `type`, `section` and `material`). A section is
/// the `profile`'s name (null for a type drawn from its own section), the
/// type's `width` and `height` (m, the profile's box), its `area` (m²), second moments `ix`, `iy` and torsion constant `j` (m⁴), and
/// a column's `rotation` (degrees).
fn model_json(s: &mut Session) -> Value {
    let geom = s.geometry().clone();
    let p = s.project();
    let model = analytical_model(p, &geom);
    let nodes: Vec<Value> = model
        .nodes
        .iter()
        .map(|n| json!({"id": n.id, "x": n.position.x, "y": n.position.y, "z": n.position.z, "support": n.support}))
        .collect();
    let members: Vec<Value> = model
        .members
        .iter()
        .filter_map(|m| {
            let (a, b) = model.line(m)?;
            let e = p.element(m.element)?;
            let ty = p.element_type(e.type_id)?;
            let section = match super::profiles::section(p, &ty.data) {
                Some(sec) => {
                    let props = &sec.properties;
                    let (width, height) = match &ty.data {
                        TypeData::Column { section, .. } => section.size(),
                        TypeData::Beam { width, height, .. } => (*width, *height),
                        _ => (0.0, 0.0),
                    };
                    let mut out = json!({
                        "profile": sec.profile,
                        "width": width,
                        "height": height,
                        "area": props.area,
                        "ix": props.ix,
                        "iy": props.iy,
                        "j": props.j,
                    });
                    if let (TypeData::Column { .. }, ElementKind::Column(c), Some(o)) =
                        (&ty.data, &e.kind, out.as_object_mut())
                    {
                        o.insert("rotation".into(), json!(c.rotation_deg));
                    }
                    out
                }
                None => Value::Null,
            };
            let kind = match m.kind {
                MemberKind::Column => "column",
                MemberKind::Beam => "beam",
            };
            Some(json!({
                "id": m.id,
                "element": m.element.0,
                "uid": e.uid.to_string(),
                "kind": kind,
                "start": m.start,
                "end": m.end,
                "length": a.distance(b),
                "type": ty.name,
                "section": section,
                "material": ty.main_material(),
            }))
        })
        .collect();
    json!({
        "format": "constructcad-analytical",
        "version": FORMAT,
        "project": p.info.name,
        "units": {"length": "m", "angle": "deg"},
        "nodes": nodes,
        "members": members,
    })
}

/// The analytical model of the structure: structural columns and beams as
/// members along their center lines between shared nodes. Beams framed into
/// columns end on the column axis, beams ending on beams end on their line,
/// and members are split where others connect. Each member carries its
/// `section`: the profile's name, `width` and `height` (m), `area` (m²), `ix`, `iy` and `j` (m⁴).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct AnalyticalModelGet {}

impl Command for AnalyticalModelGet {
    const NAME: &'static str = "analytical_model";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        Ok(model_json(s))
    }
}

/// Write the analytical model (as `analytical_model` returns it) to a JSON file.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyticalExport {
    /// Path of the .json file to write.
    pub path: PathBuf,
}

impl Command for AnalyticalExport {
    const NAME: &'static str = "analytical_export";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let model = model_json(s);
        let text = serde_json::to_string_pretty(&model).map_err(|e| EngineError::invalid(e.to_string()))?;
        std::fs::write(&self.path, text).map_err(|e| EngineError::io(e.to_string()))?;
        let count = |k: &str| model[k].as_array().map_or(0, Vec::len);
        Ok(json!({"path": self.path, "nodes": count("nodes"), "members": count("members")}))
    }
}
