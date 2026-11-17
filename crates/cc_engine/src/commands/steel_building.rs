//! Parametric steel buildings (see `cc_core::steel_building`): the host
//! element and the catalog types its members get by default.

use super::structure_material::type_or_row;
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::steel_building::{self as sb, FrameRoof, RoleKind, Secondary, SteelBuilding};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Columns and rafters by default: (series, designation).
pub const FRAME_DEFAULT: (&str, &str) = ("W", "W310x38,7");
/// Purlins and girts by default.
pub const SECONDARY_DEFAULT: (&str, &str) = ("NBR6355-Ue", "Ue 150x60x20x2,00");

/// The type a project gets with its first steel building.
pub fn new_steel_building_type(id: ElementTypeId) -> ElementType {
    sb::host_type(id, t!("tpl-type-steel-building"))
}

/// Purlins or girts.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SecondaryMembers {
    /// Beam type id or name (default: the catalog's `Ue 150x60x20x2,00`, made when missing).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Distance between lines, meters, 0.3 to 10 (default 1.5): purlins
    /// along the rafter's plan run, girts up the wall.
    #[serde(default)]
    pub spacing: Option<f64>,
}

/// Create a parametric steel building: portal frames (two columns and the
/// rafters of a gable or mono-pitch roof) every `bay_spacing` along it,
/// purlins on the roof and girts on both long sides, as ordinary columns
/// and beams the building keeps in step with its parameters (`element_set`
/// regenerates them, keeping their ids). A member edited or deleted by hand
/// is detached and left alone. Default types come from the profile catalog
/// (`W310x38,7` for columns and rafters, `Ue 150x60x20x2,00` for purlins and
/// girts), made when missing. Out-of-range values are refused. One undo
/// step; returns the building's `id` and its `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SteelBuildingCreate {
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Plan point `[x, y]` of the first frame's left column (default `[0, 0]`).
    #[serde(default)]
    pub origin: Option<Point>,
    /// Turn of the span direction, degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Column axis to column axis, meters, 2 to 100 (default 20).
    #[serde(default)]
    pub span: Option<f64>,
    /// Underside of the rafters at the columns above the level, meters, 1 to 50 (default 6).
    #[serde(default)]
    pub eave_height: Option<f64>,
    /// Roof slope, percent, 0 to 100 (default 10).
    #[serde(default)]
    pub slope_percent: Option<f64>,
    /// `gable` (ridge at mid-span, default) or `mono` (rising toward the far columns).
    #[serde(default)]
    pub roof: Option<String>,
    /// Bays, 1 to 100 (default 5); there is one frame more.
    #[serde(default)]
    pub bays: Option<u32>,
    /// Frame to frame, meters, 1 to 30 (default 6).
    #[serde(default)]
    pub bay_spacing: Option<f64>,
    /// Column type id or name (default: catalog `W310x38,7`).
    #[serde(default)]
    pub column_type: Option<Ref>,
    /// Rafter (beam) type id or name (default: catalog `W310x38,7`).
    #[serde(default)]
    pub rafter_type: Option<Ref>,
    /// Purlins `{ "type", "spacing" }`, or `null` for none (default: on).
    #[serde(default, deserialize_with = "refs::nullable", skip_serializing_if = "Option::is_none")]
    pub purlins: Option<Option<SecondaryMembers>>,
    /// Girts on both long sides `{ "type", "spacing" }`, or `null` for none (default: on).
    #[serde(default, deserialize_with = "refs::nullable", skip_serializing_if = "Option::is_none")]
    pub girts: Option<Option<SecondaryMembers>>,
}

/// The spacing of purlins or girts as asked: `None` for none (`null`).
fn spacing(arg: &Option<Option<SecondaryMembers>>) -> Option<f64> {
    match arg {
        Some(None) => None,
        Some(Some(m)) => Some(m.spacing.unwrap_or(sb::DEFAULT_SECONDARY_SPACING)),
        None => Some(sb::DEFAULT_SECONDARY_SPACING),
    }
}

fn type_arg(arg: &Option<Option<SecondaryMembers>>) -> Option<&Ref> {
    arg.as_ref().and_then(|o| o.as_ref()).and_then(|m| m.type_ref.as_ref())
}

impl Command for SteelBuildingCreate {
    const NAME: &'static str = "steel_building_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let roof = match &self.roof {
            Some(r) => FrameRoof::from_code(r)
                .ok_or_else(|| EngineError::invalid(format!("unknown roof '{r}' (gable, mono)")))?,
            None => FrameRoof::Gable,
        };
        let placeholder = ElementTypeId(0);
        let line = |spacing: Option<f64>| spacing.map(|spacing| Secondary { type_id: placeholder, spacing });
        let mut building = SteelBuilding {
            origin: point(self.origin.unwrap_or([0.0, 0.0]))?,
            rotation_deg: self.rotation.unwrap_or(0.0),
            span: self.span.unwrap_or(sb::DEFAULT_SPAN),
            eave_height: self.eave_height.unwrap_or(sb::DEFAULT_EAVE_HEIGHT),
            slope_percent: self.slope_percent.unwrap_or(sb::DEFAULT_SLOPE),
            roof,
            bays: self.bays.unwrap_or(sb::DEFAULT_BAYS),
            bay_spacing: self.bay_spacing.unwrap_or(sb::DEFAULT_BAY_SPACING),
            column_type: placeholder,
            rafter_type: placeholder,
            purlins: line(spacing(&self.purlins)),
            girts: line(spacing(&self.girts)),
            members: Vec::new(),
            detached: Vec::new(),
        };
        building.check().map_err(EngineError::invalid)?;
        let given = |r: Option<&Ref>, c: Category| r.map(|r| refs::type_of(p, c, Some(r))).transpose();
        let column_given = given(self.column_type.as_ref(), Category::Column)?;
        let rafter_given = given(self.rafter_type.as_ref(), Category::Beam)?;
        let purlin_given = given(type_arg(&self.purlins), Category::Beam)?;
        let girt_given = given(type_arg(&self.girts), Category::Beam)?;
        let host_type = p.default_type(Category::SteelBuilding);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            building.column_type = type_or_row(s, column_given, Category::Column, FRAME_DEFAULT)?;
            building.rafter_type = type_or_row(s, rafter_given, Category::Beam, FRAME_DEFAULT)?;
            if let Some(line) = &mut building.purlins {
                line.type_id = type_or_row(s, purlin_given, Category::Beam, SECONDARY_DEFAULT)?;
            }
            if let Some(line) = &mut building.girts {
                line.type_id = type_or_row(s, girt_given, Category::Beam, SECONDARY_DEFAULT)?;
            }
            building.check_types(s.project()).map_err(EngineError::invalid)?;
            let kind = ElementKind::SteelBuilding(Box::new(building));
            let id = edit(s, t!("tx-steel-building"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_steel_building_type(id))
                    }
                };
                Ok(add_element(e, level, type_id, kind))
            })?;
            let members: Vec<u64> = s.project().host_members_of(id).iter().map(|m| m.0).collect();
            Ok(json!({ "id": id.0, "members": members }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-steel-building"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a building: its member ids by role
/// kind, how many roles are detached and whether the layout reached the
/// member limit (`at_limit`).
pub(crate) fn derived(p: &cc_core::Project, b: &SteelBuilding, out: &mut serde_json::Map<String, Value>) {
    let ids = |k: RoleKind| -> Vec<u64> { b.members.iter().filter(|m| m.role.kind == k).map(|m| m.id.0).collect() };
    out.insert(
        "members".into(),
        json!({
            "columns": ids(RoleKind::Column),
            "rafters": ids(RoleKind::Rafter),
            "purlins": ids(RoleKind::Purlin),
            "girts": ids(RoleKind::Girt),
        }),
    );
    out.insert("detached".into(), json!(b.detached.len()));
    out.insert("length".into(), json!(b.length()));
    // The layout stopped at the member limit: the last girts or purlins are left out.
    out.insert("at_limit".into(), json!(b.layout(p).len() >= sb::MAX_MEMBERS));
}
