//! Cable trays: open rectangular runs carrying electrical circuits, joined
//! like conduits (see `cc_core::mep`, `cc_geom::mep_net`).

use super::mep::{check_fits, joined_offset, segment};
use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::mep::{MAX_TRAY_HEIGHT, MAX_TRAY_WIDTH};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, Project, TrayStyle, TypeData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The material of new cable trays and valves: the project's first metal, else its first material.
pub(crate) fn metal(p: &Project) -> String {
    p.materials
        .iter()
        .find(|m| m.identity.class == cc_core::MaterialClass::Metal)
        .or(p.materials.first())
        .map(|m| m.name.clone())
        .unwrap_or_default()
}

/// A cable tray type of `width` × `height` and `style`, named by its
/// section (in millimeters, or in inches in imperial units).
pub fn new_tray_type(id: ElementTypeId, width: f64, height: f64, style: TrayStyle, material: String) -> ElementType {
    let mm = |v: f64| {
        if cc_views::format::imperial() { cc_views::format::inch_size(v) } else { format!("{:.0}", v * 1000.0) }
    };
    ElementType {
        id,
        name: t!("tpl-type-cable-tray", "width" => mm(width), "height" => mm(height)),
        data: TypeData::CableTray { width, height, style, material },
        info: Default::default(),
        uid: Default::default(),
    }
}

pub(crate) fn parse_style(name: &str) -> Result<TrayStyle> {
    let wanted = name.trim().to_ascii_lowercase();
    TrayStyle::ALL
        .into_iter()
        .find(|s| s.name() == wanted || format!("{s:?}").to_ascii_lowercase() == wanted)
        .ok_or_else(|| {
            let names: Vec<&str> = TrayStyle::ALL.into_iter().map(TrayStyle::name).collect();
            EngineError::invalid(format!("unknown tray style '{name}' (one of {})", names.join(", ")))
        })
}

/// A section size in (0, `max`].
fn size(v: f64, what: &str, max: f64) -> Result<f64> {
    let v = positive(v, what)?;
    if v > max {
        return Err(EngineError::invalid(format!("{what} must be at most {max} m")));
    }
    Ok(v)
}

/// Create a cable tray from `start` to `end` (or a vertical run at `start`
/// to `end_offset`) carrying `circuits`. Its center line runs `offset` above
/// the level (default: where it starts on another cable tray, such as the
/// end of a run; else 0.30 m under the level above). Trays meet other trays
/// at their ends or along them (elbows, tees and crosses are derived), not
/// conduits or pipes. A project without a cable tray type gets one (200 ×
/// 100 mm, 8" × 4" in imperial projects; ladder).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CableTrayCreate {
    /// Start of the center line in plan.
    pub start: Point,
    /// End in plan (default: the start, for a vertical run).
    #[serde(default)]
    pub end: Option<Point>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Cable tray type id or name (default: the first cable tray type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Circuits it carries: ids, names or codes.
    #[serde(default)]
    pub circuits: Vec<Ref>,
    /// Elevation of the start above the level, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Elevation of the end above the level, meters (default: as the start).
    #[serde(default)]
    pub end_offset: Option<f64>,
}

impl Command for CableTrayCreate {
    const NAME: &'static str = "cable_tray_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let ty = match (&self.type_ref, p.default_type(Category::CableTray)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::CableTray, r.as_ref())?),
        };
        let mut circuits = Vec::new();
        for r in &self.circuits {
            let id = refs::system(p, r)?;
            check_fits(p, super::mep::CABLED, id)?;
            if !circuits.contains(&id) {
                circuits.push(id);
            }
        }
        let offset = self.offset.unwrap_or_else(|| {
            let wanted = crate::defaults::tray_offset(p, level);
            joined_offset(p, level, self.start, Category::CableTray, None, wanted).unwrap_or(wanted)
        });
        let mut seg = segment(self.start, self.end, offset, self.end_offset)?;
        seg.systems = circuits;
        let material = metal(p);
        let id = edit(s, t!("tx-create-cable-tray"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    let (width, height) = e.project().defaults().tray;
                    e.add_type(new_tray_type(id, width, height, TrayStyle::Ladder, material))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::CableTray(seg)))
        })?;
        Ok(created(s, id))
    }
}

/// Create a cable tray type: an open channel `width` × `height` (meters;
/// default 0.2 × 0.1, 8" × 4" in imperial projects; at most 2 × 1), built as `style` says (`ladder`, the
/// default, `solid_bottom` or `perforated`), in `material` (default: the
/// project's first metal). The name defaults to its section.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CableTrayTypeCreate {
    /// Type name (default: by its section, e.g. "Cable tray 200×100").
    #[serde(default)]
    pub name: Option<String>,
    /// Outside width, meters.
    #[serde(default)]
    pub width: Option<f64>,
    /// Outside height, meters.
    #[serde(default)]
    pub height: Option<f64>,
    /// `ladder`, `solid_bottom` or `perforated`.
    #[serde(default)]
    pub style: Option<String>,
    /// Material name.
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for CableTrayTypeCreate {
    const NAME: &'static str = "cable_tray_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (default_width, default_height) = p.defaults().tray;
        let width = size(self.width.unwrap_or(default_width), "width", MAX_TRAY_WIDTH)?;
        let height = size(self.height.unwrap_or(default_height), "height", MAX_TRAY_HEIGHT)?;
        let style = self.style.as_deref().map(parse_style).transpose()?.unwrap_or_default();
        let material = match &self.material {
            Some(m) => refs::material(p, &Ref::Name(m.clone()))?,
            None => metal(p),
        };
        let name = self.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let id = edit(s, t!("tx-create-cable-tray-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_tray_type(id, width, height, style, material);
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0, "name": s.project().type_name(id) }))
    }
}
