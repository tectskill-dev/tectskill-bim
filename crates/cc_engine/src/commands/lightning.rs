//! Lightning protection: conductors on the roof, down the walls and in the
//! ground (an earth ring), air terminals on the roof, earth electrodes, and
//! a coverage check of the roofs (see `cc_core::lightning`,
//! `cc_geom::lightning`). Elements placed without a system join the
//! project's first lightning protection system, created with the first of them.

use super::devices::{NewDevice, add_device};
use super::mep::{check_fits, joined_offset};
use super::{add_element, created, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::lightning::{EARTH_DEPTH, EARTH_RING_DISTANCE, MAX_CONDUCTOR_DIAMETER, MIN_CONDUCTOR_DIAMETER};
use cc_core::{
    Category, ConductorRole, DeviceKind, EditCtx, ElementKind, ElementType, ElementTypeId, LevelId, LightningConductor,
    LpsClass, MepSegment, MepSystem, Project, SystemClass, SystemId, SystemKind, TypeData,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn parse_role(name: &str) -> Result<ConductorRole> {
    let wanted = name.trim().to_ascii_lowercase();
    ConductorRole::ALL.into_iter().find(|r| r.name() == wanted).ok_or_else(|| {
        let names: Vec<&str> = ConductorRole::ALL.into_iter().map(ConductorRole::name).collect();
        EngineError::invalid(format!("unknown conductor role '{name}' (one of {})", names.join(", ")))
    })
}

/// The system for a new lightning protection element: the one given
/// (checked), else the project's first lightning protection system, else
/// `None` (one is created with the element, see [`ensure_system`]).
fn given_system(p: &Project, r: Option<&Ref>) -> Result<Option<SystemId>> {
    match r {
        Some(r) => {
            let id = refs::system(p, r)?;
            check_fits(p, &[SystemClass::Lightning], id)?;
            Ok(Some(id))
        }
        None => Ok(p.systems.iter().find(|(_, s)| s.kind == SystemKind::Lightning).map(|(id, _)| *id)),
    }
}

/// `system`, or a new lightning protection system.
fn ensure_system(e: &mut EditCtx, system: Option<SystemId>) -> SystemId {
    if let Some(id) = system {
        return id;
    }
    let id = SystemId(e.alloc_id());
    let kind = SystemKind::Lightning;
    e.systems_mut().insert(id, MepSystem::new(crate::i18n::tr(kind.key()), "", kind));
    id
}

/// A conductor type of `diameter`, named by it (in millimeters, or in
/// inches in imperial units).
pub fn new_conductor_type(id: ElementTypeId, diameter: f64, material: String) -> ElementType {
    let name = if cc_views::format::imperial() {
        t!("tpl-imp-type-lightning-conductor", "diameter" => cc_views::format::inch_size(diameter))
    } else {
        t!("tpl-type-lightning-conductor", "diameter" => format!("{:.0}", diameter * 1000.0))
    };
    ElementType {
        id,
        name,
        data: TypeData::LightningConductor { diameter, material },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// The copper of new conductor types: a material named so, else the
/// project's first metal.
fn copper(p: &Project) -> String {
    let name = crate::i18n::tr("tpl-mat-copper");
    if p.material(&name).is_some() { name } else { super::cable_tray::metal(p) }
}

/// The conductor type to use: the one given, else the first, else `None`
/// (a new one is made, see [`ensure_type`]).
fn conductor_type(p: &Project, r: Option<&Ref>) -> Result<Option<ElementTypeId>> {
    match (r, p.default_type(Category::LightningConductor)) {
        (None, None) => Ok(None),
        (r, _) => Ok(Some(refs::type_of(p, Category::LightningConductor, r)?)),
    }
}

fn ensure_type(e: &mut EditCtx, ty: Option<ElementTypeId>, material: &str) -> ElementTypeId {
    match ty {
        Some(t) => t,
        None => {
            let id = ElementTypeId(e.alloc_id());
            // 8 mm, 3/8" in imperial projects.
            let diameter = e.project().defaults().conductor.1;
            e.add_type(new_conductor_type(id, diameter, material.to_string()))
        }
    }
}

/// The height above `level` of the roof (or floor) surface over `at`, if any.
fn roof_offset(s: &mut Session, level: LevelId, at: DVec2) -> Option<f64> {
    let top = cc_geom::lightning::surface_top(s.geometry(), at)?;
    Some(top - s.project().level_elevation(level))
}

/// Create a lightning protection conductor from `start` to `end`, or a
/// down conductor straight down from `start` (no `end`). Without offsets, a
/// conductor lies on the roof under its ends (else where it starts on
/// another conductor, else at the level); a down conductor runs from the
/// roof over `start` (else the level above) down to the earthing,
/// 0.5 m below the level. `role` is `air` (a roof conductor), `down` or
/// `earth`; by default vertical runs are down conductors, runs below the
/// level earth conductors and the others roof conductors. Conductors join
/// each other where their ends meet. A project without a conductor type gets
/// one (8 mm, copper); without a lightning protection system, one is made.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LpsConductorCreate {
    /// Start in plan.
    pub start: Point,
    /// End in plan (default: the start, for a down conductor).
    #[serde(default)]
    pub end: Option<Point>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Conductor type id or name (default: the first conductor type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// `air`, `down` or `earth` (default: by where it runs).
    #[serde(default)]
    pub role: Option<String>,
    /// Elevation of the start above the level, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Elevation of the end above the level, meters.
    #[serde(default)]
    pub end_offset: Option<f64>,
    /// Lightning protection system id, name or code (default: the first).
    #[serde(default)]
    pub system: Option<Ref>,
}

impl Command for LpsConductorCreate {
    const NAME: &'static str = "lps_conductor_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let start = point(self.start)?;
        let end = self.end.map(point).transpose()?;
        let vertical = end.is_none_or(|b| b.distance(start) < 1e-3);
        let roof_start = roof_offset(s, level, start);
        let roof_end = end.and_then(|b| roof_offset(s, level, b));
        let p = s.project();
        let ty = conductor_type(p, self.type_ref.as_ref())?;
        let system = given_system(p, self.system.as_ref())?;
        let (offset, end_offset) = if vertical {
            let top = self.offset.or(roof_start).unwrap_or_else(|| p.level_height(level, p.defaults().ceiling));
            (top, self.end_offset.unwrap_or(-EARTH_DEPTH))
        } else {
            let a = match self.offset.or(roof_start) {
                Some(z) => z,
                None => joined_offset(p, level, self.start, Category::LightningConductor, None, 0.0).unwrap_or(0.0),
            };
            (a, self.end_offset.or(roof_end).unwrap_or(a))
        };
        let mut run = MepSegment::new(start, end.unwrap_or(start), finite(offset, "offset")?);
        run.end_offset = finite(end_offset, "end_offset")?;
        if run.plan_length() < 1e-3 && run.rise().abs() < 1e-3 {
            return Err(EngineError::invalid(t!("msg-segment-empty")));
        }
        let role = match &self.role {
            Some(r) => parse_role(r)?,
            None => ConductorRole::of_run(&run),
        };
        let material = copper(p);
        let id = edit(s, t!("tx-create-lps-conductor"), |e| {
            let type_id = ensure_type(e, ty, &material);
            run.systems = vec![ensure_system(e, system)];
            Ok(add_element(e, level, type_id, ElementKind::LightningConductor(LightningConductor { run, role })))
        })?;
        Ok(created(s, id))
    }
}

/// Create an earth ring: buried conductors (role `earth`) around `points`
/// (a closed outline), or by default around the walls of the level,
/// `distance` (default 1 m) out from their outer faces, `depth` (default
/// 0.5 m) below the level, one conductor per side.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct EarthRingCreate {
    /// The outline in plan (default: around the walls of the level).
    #[serde(default)]
    pub points: Option<Vec<Point>>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Depth below the level, meters.
    #[serde(default)]
    pub depth: Option<f64>,
    /// Distance out from the walls' outer faces, meters (without `points`).
    #[serde(default)]
    pub distance: Option<f64>,
    /// Conductor type id or name (default: the first conductor type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Lightning protection system id, name or code (default: the first).
    #[serde(default)]
    pub system: Option<Ref>,
}

impl Command for EarthRingCreate {
    const NAME: &'static str = "earth_ring_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let depth = positive(self.depth.unwrap_or(EARTH_DEPTH), "depth")?;
        let outline = match &self.points {
            Some(points) => refs::points(points)?,
            None => {
                let distance = finite(self.distance.unwrap_or(EARTH_RING_DISTANCE), "distance")?;
                let walls = crate::defaults::walls_loop(p, level, 1.0)
                    .ok_or_else(|| EngineError::invalid(t!("msg-earth-ring-no-walls")))?;
                cc_geom::polygon::offset(&walls, distance)
            }
        };
        let mut outline: Vec<DVec2> = outline.into_iter().filter(|q| q.is_finite()).collect();
        outline.dedup_by(|a, b| a.distance(*b) < 1e-3);
        if outline.len() > 2 && outline.first().zip(outline.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-3) {
            outline.pop();
        }
        if outline.len() < 3 || cc_geom::polygon::signed_area(&outline).abs() < 1e-6 {
            return Err(EngineError::invalid("the earth ring needs a closed outline of at least 3 points"));
        }
        let ty = conductor_type(p, self.type_ref.as_ref())?;
        let system = given_system(p, self.system.as_ref())?;
        let material = copper(p);
        let n = outline.len();
        let ids = edit(s, t!("tx-create-earth-ring"), |e| {
            let type_id = ensure_type(e, ty, &material);
            let system = ensure_system(e, system);
            Ok((0..n)
                .map(|i| {
                    let mut run = MepSegment::new(outline[i], outline[(i + 1) % n], -depth);
                    run.systems = vec![system];
                    let c = LightningConductor { run, role: ConductorRole::Earth };
                    add_element(e, level, type_id, ElementKind::LightningConductor(c))
                })
                .collect::<Vec<_>>())
        })?;
        let length: f64 = (0..n).map(|i| outline[i].distance(outline[(i + 1) % n])).sum();
        Ok(json!({"created": ids_json(&ids), "length": length}))
    }
}

/// Place an air terminal (a lightning rod) at `position`: on the roof (or
/// floor) surface under it by default, else at the level above; `height` is
/// the rod's height (default: its type's, 1 m for the built-in type). A
/// project without an air terminal type gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct AirTerminalPlace {
    /// Position in plan.
    pub position: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Rod height, meters.
    #[serde(default)]
    pub height: Option<f64>,
    /// Elevation of its base above the level, meters (default: the roof under it).
    #[serde(default)]
    pub offset: Option<f64>,
    /// Air terminal type id or name.
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Lightning protection system id, name or code (default: the first).
    #[serde(default)]
    pub system: Option<Ref>,
}

impl Command for AirTerminalPlace {
    const NAME: &'static str = "air_terminal_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let position = point(self.position)?;
        let roof = roof_offset(s, level, position);
        let p = s.project();
        let base_offset =
            finite(self.offset.or(roof).unwrap_or_else(|| p.level_height(level, p.defaults().ceiling)), "offset")?;
        let size = self.height.map(|h| positive(h, "height")).transpose()?;
        place(
            s,
            self.type_ref.as_ref(),
            self.system.as_ref(),
            DeviceKind::AirTerminal,
            level,
            position,
            base_offset,
            size,
        )
    }
}

/// Place an earth electrode (a driven rod) at `position`: its top `depth`
/// (default 0.5 m) below the level, `length` long (default: its type's,
/// 2.4 m for the built-in type). A project without an earth electrode type
/// gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct EarthElectrodePlace {
    /// Position in plan.
    pub position: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Rod length, meters.
    #[serde(default)]
    pub length: Option<f64>,
    /// Depth of its top below the level, meters.
    #[serde(default)]
    pub depth: Option<f64>,
    /// Earth electrode type id or name.
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Lightning protection system id, name or code (default: the first).
    #[serde(default)]
    pub system: Option<Ref>,
}

impl Command for EarthElectrodePlace {
    const NAME: &'static str = "earth_electrode_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let position = point(self.position)?;
        let depth = finite(self.depth.unwrap_or(EARTH_DEPTH), "depth")?;
        let size = self.length.map(|l| positive(l, "length")).transpose()?;
        let kind = DeviceKind::EarthElectrode;
        place(s, self.type_ref.as_ref(), self.system.as_ref(), kind, level, position, -depth, size)
    }
}

/// Place a lightning protection device of `device` (checking a given type).
#[allow(clippy::too_many_arguments)]
fn place(
    s: &mut Session,
    type_ref: Option<&Ref>,
    system: Option<&Ref>,
    device: DeviceKind,
    level: LevelId,
    position: DVec2,
    base_offset: f64,
    size: Option<f64>,
) -> Result<Value> {
    let p = s.project();
    let type_id = match type_ref {
        Some(r) => {
            let id = refs::type_of(p, Category::Component, Some(r))?;
            if super::devices::type_mep(p, id).and_then(|m| m.device()) != Some(device) {
                return Err(EngineError::invalid(t!("msg-wrong-device-type", "kind" => crate::i18n::tr(device.key()))));
            }
            Some(id)
        }
        None => None,
    };
    let system = given_system(p, system)?;
    let tx =
        if device == DeviceKind::AirTerminal { t!("tx-place-air-terminal") } else { t!("tx-place-earth-electrode") };
    let id = edit(s, tx, |e| {
        let system = Some(ensure_system(e, system));
        let d = NewDevice {
            device,
            level,
            position,
            rotation: 0.0,
            base_offset,
            system,
            type_id,
            ports: None,
            size,
            mark: String::new(),
        };
        add_device(e, d)
    })?;
    Ok(created(s, id))
}

/// Create a lightning protection conductor type: a round conductor of
/// `diameter` (meters, 2 to 50 mm; default 8 mm, 3/8" in imperial projects) in `material` (default:
/// copper, else the first metal), named by its diameter unless `name` is given.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LpsConductorTypeCreate {
    /// Diameter, meters (default 0.008; 3/8" in imperial projects).
    #[serde(default)]
    pub diameter: Option<f64>,
    /// Material name.
    #[serde(default)]
    pub material: Option<String>,
    /// Type name.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for LpsConductorTypeCreate {
    const NAME: &'static str = "lps_conductor_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let diameter = positive(self.diameter.unwrap_or(p.defaults().conductor.1), "diameter")?;
        if !(MIN_CONDUCTOR_DIAMETER..=MAX_CONDUCTOR_DIAMETER).contains(&diameter) {
            return Err(EngineError::invalid(format!(
                "diameter must be between {MIN_CONDUCTOR_DIAMETER} and {MAX_CONDUCTOR_DIAMETER} m"
            )));
        }
        let material = match &self.material {
            Some(m) => refs::material(p, &Ref::Name(m.clone()))?,
            None => copper(p),
        };
        let name = self.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let id = edit(s, t!("tx-create-lps-conductor-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_conductor_type(id, diameter, material);
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({"id": id.0, "name": s.project().type_name(id)}))
    }
}

/// Check how the air termination (air terminals and roof conductors)
/// covers the roofs, for a protection `class` (`I` to `IV`, default `III`):
/// the exposed upper faces of roofs (and of floors above the lowest level
/// that nothing covers) are sampled about `spacing` meters apart (default
/// 1 m; more on large roofs) and each point is protected when one air
/// termination point protects it by itself, by the rolling sphere method
/// (sphere radius 20, 30, 45 or 60 m by class) over a horizontal plane at
/// the roof's lowest exposed point, or, on flat parts, when it lies within
/// half the class's mesh width (5, 10, 15 or 20 m) of a roof conductor.
/// Mutual protection between terminals and the shielding by the building
/// itself are left out, which errs on the safe side. Returns the counts,
/// the share protected and up to 200 unprotected points.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LpsCheck {
    /// Protection class, `I` to `IV`.
    #[serde(default)]
    pub class: Option<String>,
    /// Distance between the sampled points, meters.
    #[serde(default)]
    pub spacing: Option<f64>,
}

/// Most unprotected points listed by `lps_check`.
const MAX_LISTED: usize = 200;

impl Command for LpsCheck {
    const NAME: &'static str = "lps_check";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let class = match &self.class {
            Some(c) => {
                LpsClass::parse(c).ok_or_else(|| EngineError::invalid(format!("unknown class '{c}' (I to IV)")))?
            }
            None => LpsClass::default(),
        };
        let spacing = positive(self.spacing.unwrap_or(1.0), "spacing")?;
        let (p, geom) = s.project_and_geometry();
        let c = cc_geom::lightning::check(p, geom, class, spacing);
        let unprotected: Vec<Value> = c
            .unprotected
            .iter()
            .take(MAX_LISTED)
            .map(|(id, q)| json!({"element": id.0, "point": q.to_array()}))
            .collect();
        Ok(json!({
            "class": class.name(),
            "sphere_radius": c.radius,
            "mesh_width": c.mesh,
            "spacing": c.spacing,
            "air_points": c.air_points,
            "roofs": ids_json(&c.roofs),
            "checked": c.checked,
            "protected": c.protected,
            "ratio": c.ratio(),
            "ok": c.checked > 0 && c.protected == c.checked,
            "unprotected": unprotected,
        }))
    }
}
