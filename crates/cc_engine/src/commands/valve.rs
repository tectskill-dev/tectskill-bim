//! Valves on pipes: inline accessories hosted by a pipe at a distance along
//! its run (see `cc_core::mep::Valve`). A valve takes its pipe's diameter
//! and system, is a node of its network, and moves, is copied and is
//! deleted with the pipe.

use super::cable_tray::metal;
use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, Project, TypeData, Valve, ValveKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A clicked point farther than this from the pipe in plan is refused, meters.
const REACH: f64 = 0.5;

pub(crate) fn parse_kind(name: &str) -> Result<ValveKind> {
    let wanted = name.trim().to_ascii_lowercase();
    ValveKind::ALL
        .into_iter()
        .find(|k| k.name() == wanted || format!("{k:?}").to_ascii_lowercase() == wanted)
        .ok_or_else(|| {
            let names: Vec<&str> = ValveKind::ALL.into_iter().map(ValveKind::name).collect();
            EngineError::invalid(format!("unknown valve kind '{name}' (one of {})", names.join(", ")))
        })
}

/// A valve type of `kind`, named after it.
pub fn new_valve_type(id: ElementTypeId, kind: ValveKind, material: String) -> ElementType {
    ElementType {
        id,
        name: crate::i18n::tr(kind.key()),
        data: TypeData::Valve { kind, material },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// The first valve type of `kind`.
fn type_of_kind(p: &Project, kind: ValveKind) -> Option<ElementTypeId> {
    p.types_of(Category::Valve).find(|t| t.valve_kind() == Some(kind)).map(|t| t.id)
}

/// Place a valve on a pipe, at `point` (the nearest point of the pipe in
/// plan, within 0.5 m) or `distance` meters along the pipe from its start
/// (default: its middle). `type` picks a valve type; else `kind` (`gate`,
/// `ball`, `check` or `globe`) picks the first type of that kind, created
/// when the project has none; else the first valve type (a gate valve type
/// is created when there is none). The valve takes the pipe's diameter and
/// system and splits its network there; it moves, is copied and is deleted
/// with the pipe.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ValvePlace {
    /// Pipe element id.
    pub pipe: u64,
    /// A point in plan on (or near) the pipe.
    #[serde(default)]
    pub point: Option<Point>,
    /// Distance from the pipe's start along it, meters.
    #[serde(default)]
    pub distance: Option<f64>,
    /// `gate`, `ball`, `check` or `globe`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Valve type id or name.
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for ValvePlace {
    const NAME: &'static str = "valve_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let host = refs::element(p, self.pipe)?;
        let (level, run) = match p.element(host).map(|e| (e.level, &e.kind)) {
            Some((level, ElementKind::Pipe(run))) => (level, run.clone()),
            _ => return Err(EngineError::invalid(t!("msg-not-a-pipe", "id" => self.pipe))),
        };
        let length = run.length();
        let at = match (self.point, self.distance) {
            (Some(_), Some(_)) => return Err(EngineError::invalid("give point or distance, not both")),
            (None, Some(d)) => {
                let d = finite(d, "distance")?;
                if !(0.0..=length + 1e-9).contains(&d) {
                    return Err(EngineError::invalid(format!("distance must be between 0 and {length:.3} m")));
                }
                d.min(length)
            }
            (Some(q), None) => {
                let q = point(q)?;
                if run.is_vertical() {
                    return Err(EngineError::invalid("give a distance for a vertical pipe"));
                }
                let d = run.end - run.start;
                let t = ((q - run.start).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                if (run.start + d * t).distance(q) > REACH {
                    return Err(EngineError::invalid(t!("msg-point-off-pipe")));
                }
                t * length
            }
            (None, None) => length * 0.5,
        };
        let kind = self.kind.as_deref().map(parse_kind).transpose()?;
        let chosen = match (&self.type_ref, kind) {
            (Some(_), Some(_)) => return Err(EngineError::invalid("give type or kind, not both")),
            (Some(r), None) => Some(refs::type_of(p, Category::Valve, Some(r))?),
            (None, Some(k)) => type_of_kind(p, k),
            (None, None) => p.default_type(Category::Valve),
        };
        let material = metal(p);
        let valve = Valve { host, at };
        let id = edit(s, t!("tx-place-valve"), |e| {
            let type_id = match chosen {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_valve_type(id, kind.unwrap_or_default(), material))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::Valve(valve)))
        })?;
        Ok(created(s, id))
    }
}

/// Create valve types, one per kind in `kinds` (`gate`, `ball`, `check`,
/// `globe`; default: a gate valve), named after their kind (or `name`, for
/// one kind), in `material` (default: the project's first metal).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ValveTypeCreate {
    /// Kinds of valve to create a type for.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Type name (only with one kind).
    #[serde(default)]
    pub name: Option<String>,
    /// Material name.
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for ValveTypeCreate {
    const NAME: &'static str = "valve_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let mut kinds = Vec::new();
        for k in &self.kinds {
            let k = parse_kind(k)?;
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        if kinds.is_empty() {
            kinds.push(ValveKind::Gate);
        }
        let name = self.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        if name.is_some() && kinds.len() > 1 {
            return Err(EngineError::invalid("a name needs a single kind"));
        }
        let material = match &self.material {
            Some(m) => refs::material(p, &Ref::Name(m.clone()))?,
            None => metal(p),
        };
        let ids = edit(s, t!("tx-create-valve-type"), |e| {
            Ok(kinds
                .iter()
                .map(|k| {
                    let id = ElementTypeId(e.alloc_id());
                    let mut ty = new_valve_type(id, *k, material.clone());
                    if let Some(name) = &name {
                        ty.name = name.clone();
                    }
                    e.add_type(ty)
                })
                .collect::<Vec<_>>())
        })?;
        let p = s.project();
        let out: Vec<Value> = ids.iter().map(|id| json!({"id": id.0, "name": p.type_name(*id)})).collect();
        Ok(json!(out))
    }
}
