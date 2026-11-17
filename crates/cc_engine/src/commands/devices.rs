//! Device types the engine provides on demand: a project that has no type
//! for a kind of device (a data outlet, a rack, an air terminal, an earth
//! electrode…) gets the built-in one with the first device placed, in the
//! same undo step (see `cc_core::component::device_library`).

use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::material::MaterialClass;
use cc_core::{
    Component, DeviceKind, EditCtx, ElementId, ElementKind, ElementType, ElementTypeId, LevelId, Material, MepData,
    Project, SystemId, TypeData,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) fn parse_device(name: &str) -> Result<DeviceKind> {
    let wanted = name.trim().to_ascii_lowercase();
    DeviceKind::ALL
        .into_iter()
        .find(|k| k.name() == wanted || format!("{k:?}").to_ascii_lowercase() == wanted)
        .ok_or_else(|| {
            let names: Vec<&str> = DeviceKind::ALL.into_iter().map(DeviceKind::name).collect();
            EngineError::invalid(format!("unknown device '{name}' (one of {})", names.join(", ")))
        })
}

/// The MEP data of a component type.
pub(crate) fn type_mep(p: &Project, id: ElementTypeId) -> Option<MepData> {
    match p.element_type(id).map(|t| &t.data) {
        Some(TypeData::Component { def, .. }) => def.mep,
        _ => None,
    }
}

/// The first component type that is a `device` (and, for data outlets, has
/// `ports` jacks when given).
pub(crate) fn device_type(p: &Project, device: DeviceKind, ports: Option<u32>) -> Option<ElementTypeId> {
    p.types_of(cc_core::Category::Component)
        .find(|t| {
            let mep = type_mep(p, t.id);
            mep.and_then(|m| m.device()) == Some(device) && ports.is_none_or(|n| mep.is_some_and(|m| m.cables() == n))
        })
        .map(|t| t.id)
}

/// The material named after template key `key`, added to the project (with
/// the usual properties of `class` and `color`) when it has none.
fn ensure_material(e: &mut EditCtx, key: &str, class: MaterialClass, color: [u8; 3]) -> String {
    let name = crate::i18n::tr(key);
    if e.project().material(&name).is_none() {
        let mut m = Material::new(name.as_str(), color);
        cc_core::material::template_properties(&mut m, class);
        e.materials_mut().push(m);
    }
    name
}

/// The built-in type of `device` (data outlets with `ports` jacks, default
/// two) added to the project: its materials too, when missing.
pub(crate) fn add_device_type(e: &mut EditCtx, device: DeviceKind, ports: Option<u32>) -> Result<ElementTypeId> {
    let n = |key: &str| crate::i18n::tr(key);
    let found = cc_core::component::device_library(device, &n).or_else(|| {
        cc_core::component::mep_library(&n).into_iter().find(|(_, _, d)| d.mep.and_then(|m| m.device()) == Some(device))
    });
    let Some((component, mut name, mut def)) = found else {
        return Err(EngineError::invalid(format!("no built-in type for {}", device.name())));
    };
    if let (Some(n), Some(MepData::Device { ports, .. })) = (ports, def.mep.as_mut()) {
        *ports = n.clamp(1, cc_core::mep::MAX_PORTS);
    }
    if device == DeviceKind::DataOutlet {
        name = t!("type-data-outlet-ports", "ports" => def.mep.map_or(1, |m| m.cables()));
    }
    if e.project().measurement_system() == cc_core::MeasurementSystem::Imperial {
        // Round inches and feet: a 60 cm rack is 2'-0", a 2.4 m rod 8'-0".
        for p in def.params.iter_mut().filter(|p| p.kind == cc_core::ParamKind::Length) {
            p.value = cc_core::units::round_imperial(p.value);
        }
    }
    ensure_material(e, "tpl-mat-steel", MaterialClass::Metal, [160, 165, 170]);
    ensure_material(e, "tpl-mat-pvc", MaterialClass::Plastic, [236, 236, 230]);
    ensure_material(e, "tpl-mat-copper", MaterialClass::Metal, [184, 115, 51]);
    let id = ElementTypeId(e.alloc_id());
    let ty = ElementType {
        id,
        name,
        data: TypeData::Component { component, def },
        info: Default::default(),
        uid: Default::default(),
    };
    Ok(e.add_type(ty))
}

/// A device to add to the project.
pub(crate) struct NewDevice {
    pub device: DeviceKind,
    pub level: LevelId,
    pub position: DVec2,
    pub rotation: f64,
    pub base_offset: f64,
    pub system: Option<SystemId>,
    /// Its type (default: the first of its kind, else the built-in one).
    pub type_id: Option<ElementTypeId>,
    /// Data outlets: jacks of the type to use when no type is given.
    pub ports: Option<u32>,
    /// Value of the type's first instance parameter (an air terminal's
    /// height, an earth electrode's length).
    pub size: Option<f64>,
    pub mark: String,
}

/// Add a device (and its type, when the project has none).
pub(crate) fn add_device(e: &mut EditCtx, d: NewDevice) -> Result<ElementId> {
    let type_id = match d.type_id.or_else(|| device_type(e.project(), d.device, d.ports)) {
        Some(t) => t,
        None => add_device_type(e, d.device, d.ports)?,
    };
    let mut component = Component {
        position: d.position,
        rotation_deg: d.rotation,
        base_offset: d.base_offset,
        system: d.system,
        ..Default::default()
    };
    if let Some(size) = d.size {
        let name = match e.project().element_type(type_id).map(|t| &t.data) {
            Some(TypeData::Component { def, .. }) => def.params.iter().find(|p| p.instance).map(|p| p.name.clone()),
            _ => None,
        };
        if let Some(name) = name {
            component.overrides.insert(name, size);
        }
    }
    let id = add_element(e, d.level, type_id, ElementKind::Component(component));
    if let Some(el) = e.element_mut(id) {
        el.mark = d.mark;
    }
    Ok(id)
}

/// Give the project the built-in component type of a kind of device, if it
/// has none: `device` is `outlet`, `dedicated_outlet`, `switch`,
/// `ceiling_light`, `wall_light`, `panel`, `data_outlet` (with `ports`
/// jacks, default 2), `rack`, `air_terminal` or `earth_electrode`. Returns
/// the type (the existing one when there is one).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DeviceTypeCreate {
    /// Kind of device.
    pub device: String,
    /// Data outlets: jacks per outlet (1 to 48).
    #[serde(default)]
    pub ports: Option<u32>,
}

impl Command for DeviceTypeCreate {
    const NAME: &'static str = "device_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let device = parse_device(&self.device)?;
        let ports = if device == DeviceKind::DataOutlet { self.ports } else { None };
        if let Some(n) = ports
            && !(1..=cc_core::mep::MAX_PORTS).contains(&n)
        {
            return Err(EngineError::invalid(format!("ports must be between 1 and {}", cc_core::mep::MAX_PORTS)));
        }
        let id = match device_type(s.project(), device, ports) {
            Some(id) => id,
            None => edit(s, t!("tx-create-device-type"), |e| add_device_type(e, device, ports))?,
        };
        Ok(json!({"id": id.0, "name": s.project().type_name(id)}))
    }
}
