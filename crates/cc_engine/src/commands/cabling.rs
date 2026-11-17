//! Structured cabling: racks, data outlets and the horizontal cables
//! between them. Each rack feeds a data system (`SystemKind::Data`, its
//! `panel` the rack), created with the rack; outlets belong to it like
//! devices to a circuit, and each jack takes a cable to the rack, whose
//! length is estimated along the ceiling (see `cc_core::mep::cable_length`).
//! Cable trays and conduits carry the data systems like circuits.

use super::devices::{NewDevice, add_device, type_mep};
use super::mep::check_fits;
use super::{created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::mep::{MAX_LINK_LENGTH, MAX_PORTS};
use cc_core::{Category, DeviceKind, ElementId, MepSystem, Project, SystemClass, SystemId, SystemKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Whether `id` is a rack.
fn is_rack(p: &Project, id: ElementId) -> bool {
    p.element(id).and_then(|e| p.mep_data(e)).and_then(|m| m.device()) == Some(DeviceKind::Rack)
}

/// The data systems fed by `rack`.
fn systems_of(p: &Project, rack: ElementId) -> impl Iterator<Item = (&SystemId, &MepSystem)> {
    p.systems.iter().filter(move |(_, s)| s.kind == SystemKind::Data && s.panel == Some(rack))
}

/// The next free rack code (`R1`, `R2`…), among element marks and system codes.
fn next_rack_code(p: &Project) -> String {
    let taken = |code: &str| p.elements.values().any(|e| e.mark == code) || p.systems.values().any(|s| s.code == code);
    (1..).map(|n| format!("R{n}")).find(|c| !taken(c)).unwrap_or_default()
}

/// Place a communications rack at `position` (on the wall, `offset` above
/// the level, default 1.40 m, 4'-6" in imperial projects) and create the data system its outlets are
/// cabled to. The rack's mark and the system's code are `mark` (default
/// the next free `R1`, `R2`…). A project without a rack type gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RackPlace {
    /// Insertion point in plan (the back of the rack).
    pub position: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Elevation above the level, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Mark of the rack and code of its cabling.
    #[serde(default)]
    pub mark: Option<String>,
    /// Rack type id or name.
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for RackPlace {
    const NAME: &'static str = "rack_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = typed(p, self.type_ref.as_ref(), DeviceKind::Rack)?;
        let mark =
            self.mark.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or_else(|| next_rack_code(p));
        let d = NewDevice {
            device: DeviceKind::Rack,
            level,
            position: point(self.position)?,
            rotation: finite(self.rotation.unwrap_or(0.0), "rotation")?,
            base_offset: finite(
                self.offset.unwrap_or_else(|| crate::defaults::device_height(p, level, DeviceKind::Rack)),
                "offset",
            )?,
            system: None,
            type_id,
            ports: None,
            size: None,
            mark: mark.clone(),
        };
        let (id, system) = edit(s, t!("tx-place-rack"), |e| {
            let id = add_device(e, d)?;
            let mut system = MepSystem::new(t!("default-cabling-name", "n" => mark.clone()), mark, SystemKind::Data);
            system.panel = Some(id);
            let sid = SystemId(e.alloc_id());
            e.systems_mut().insert(sid, system);
            Ok((id, sid))
        })?;
        let mut out = created(s, id);
        if let Some(o) = out.as_object_mut() {
            o.insert("system".into(), json!(system.0));
        }
        Ok(out)
    }
}

/// A given component type, checked to be a `device`.
fn typed(p: &Project, r: Option<&Ref>, device: DeviceKind) -> Result<Option<cc_core::ElementTypeId>> {
    let Some(r) = r else { return Ok(None) };
    let id = refs::type_of(p, Category::Component, Some(r))?;
    if type_mep(p, id).and_then(|m| m.device()) != Some(device) {
        return Err(EngineError::invalid(t!("msg-wrong-device-type", "kind" => crate::i18n::tr(device.key()))));
    }
    Ok(Some(id))
}

/// The data system of `rack` (its first).
fn rack_system(p: &Project, rack: u64) -> Result<SystemId> {
    let id = refs::element(p, rack)?;
    if !is_rack(p, id) {
        return Err(EngineError::invalid(t!("msg-not-a-rack", "id" => rack)));
    }
    systems_of(p, id).next().map(|(s, _)| *s).ok_or_else(|| EngineError::invalid(t!("msg-rack-without-cabling")))
}

/// Place a data (or voice) outlet at `position` (on the wall, `offset`
/// above the level, default 0.30 m, 1'-6" in imperial projects), cabled to `rack` (a rack element: its
/// data system) or to the data system `system`; by default to the
/// project's only rack, if it has one. `ports` picks a type with that many
/// jacks (one cable each), made when the project has none; a project
/// without a data outlet type gets one (two jacks).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DataOutletPlace {
    /// Insertion point in plan (the back of the outlet).
    pub position: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Elevation above the level, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Rack element id.
    #[serde(default)]
    pub rack: Option<u64>,
    /// Data system id, name or code.
    #[serde(default)]
    pub system: Option<Ref>,
    /// Jacks per outlet (1 to 48).
    #[serde(default)]
    pub ports: Option<u32>,
    /// Data outlet type id or name.
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for DataOutletPlace {
    const NAME: &'static str = "data_outlet_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = typed(p, self.type_ref.as_ref(), DeviceKind::DataOutlet)?;
        if let Some(n) = self.ports
            && !(1..=MAX_PORTS).contains(&n)
        {
            return Err(EngineError::invalid(format!("ports must be between 1 and {MAX_PORTS}")));
        }
        let system = match (self.rack, &self.system) {
            (Some(_), Some(_)) => return Err(EngineError::invalid("give rack or system, not both")),
            (Some(rack), None) => Some(rack_system(p, rack)?),
            (None, Some(r)) => {
                let id = refs::system(p, r)?;
                check_fits(p, &[SystemClass::Data], id)?;
                Some(id)
            }
            (None, None) => {
                let racks: Vec<SystemId> =
                    p.systems.iter().filter(|(_, s)| s.kind == SystemKind::Data).map(|(id, _)| *id).collect();
                match racks[..] {
                    [only] => Some(only),
                    _ => None,
                }
            }
        };
        let d = NewDevice {
            device: DeviceKind::DataOutlet,
            level,
            position: point(self.position)?,
            rotation: finite(self.rotation.unwrap_or(0.0), "rotation")?,
            base_offset: finite(
                self.offset.unwrap_or_else(|| crate::defaults::device_height(p, level, DeviceKind::DataOutlet)),
                "offset",
            )?,
            system,
            type_id,
            ports: self.ports,
            size: None,
            mark: String::new(),
        };
        let id = edit(s, t!("tx-place-data-outlet"), |e| add_device(e, d))?;
        Ok(created(s, id))
    }
}

/// The structured cabling per rack: for each data system, its rack, its
/// outlets with their jacks and the estimated length of each cable (up to
/// the ceiling of the outlet's level, across along the two axes, down to
/// the rack, plus 3 m of slack), the totals, and the cables longer than the
/// 90 m of a permanent link. `rack` limits it to one rack.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CablingList {
    /// Rack element id.
    #[serde(default)]
    pub rack: Option<u64>,
}

impl Command for CablingList {
    const NAME: &'static str = "cabling_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let rack = self.rack.map(|r| refs::element(p, r)).transpose()?;
        let mut rows = Vec::new();
        for (id, system) in p.systems.iter().filter(|(_, s)| s.kind == SystemKind::Data) {
            if rack.is_some() && system.panel != rack {
                continue;
            }
            let outlets: Vec<Value> = p
                .system_members(*id)
                .filter(|e| p.mep_data(e).and_then(|m| m.device()) == Some(DeviceKind::DataOutlet))
                .map(|e| {
                    let (ports, length) = p.outlet_cable(e).map_or((None, None), |(n, l)| (Some(n), Some(l)));
                    json!({
                        "id": e.id.0,
                        "mark": e.mark,
                        "level": p.level_name(e.level),
                        "ports": ports,
                        "cable_length": length,
                        "ok": length.is_none_or(|l| l <= MAX_LINK_LENGTH),
                    })
                })
                .collect();
            let over = outlets.iter().filter(|o| o["ok"] == json!(false)).count();
            let mut row = json!({
                "id": id.0,
                "code": system.code,
                "name": system.name,
                "rack": system.panel.map(|r| r.0),
                "rack_mark": system.panel.and_then(|r| p.element(r)).map(|e| e.mark.clone()),
                "over_limit": over,
                "outlet_list": outlets,
            });
            if let Some(o) = row.as_object_mut() {
                o.extend(super::mep::cabling_totals(p, *id));
            }
            rows.push(row);
        }
        Ok(json!({"max_link_length": MAX_LINK_LENGTH, "racks": rows}))
    }
}
