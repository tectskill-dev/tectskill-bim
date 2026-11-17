//! Column and beam types by structural material (concrete, steel, timber):
//! the first type of a material, else a default made from the profile
//! catalog (steel `W200x26,6`, timber `MLC 9x32`) or a 20 x 40 concrete
//! rectangle, in one undo step.

use super::edit;
use super::profiles::{ProfileCreate, ProfileImport, ProfileTypeCreate};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{Category, ElementTypeId, ProfileMaterial, ProfileShape, StructuralMaterial, TypeData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The catalog row of the steel default: (series, designation).
const STEEL_DEFAULT: (&str, &str) = ("W", "W200x26,6");
/// The catalog row of the timber default.
const TIMBER_DEFAULT: (&str, &str) = ("Glulam", "MLC 9x32");
/// The name of the concrete default profile, a 0.20 x 0.40 rectangle.
const CONCRETE_DEFAULT: &str = "20x40";

pub(crate) fn material_arg(code: &str) -> Result<StructuralMaterial> {
    StructuralMaterial::from_code(code).ok_or_else(|| {
        let codes: Vec<&str> = StructuralMaterial::ALL.iter().map(|m| m.code()).collect();
        EngineError::invalid(format!("unknown structural material '{code}' ({})", codes.join(", ")))
    })
}

pub(crate) fn member_category(code: &str) -> Result<Category> {
    match refs::category(code)? {
        c @ (Category::Column | Category::Beam) => Ok(c),
        c => Err(EngineError::invalid(format!("a column or beam, not {}", refs::category_name(c)))),
    }
}

/// The type of `category` new `material` members get: the first one of the
/// material, else a default made in one undo step. Returns it and whether it was made.
pub(crate) fn structural_type(
    s: &mut Session,
    category: Category,
    material: StructuralMaterial,
) -> Result<(ElementTypeId, bool)> {
    if let Some(t) = s.project().types_of_material(category, material).next() {
        return Ok((t.id, false));
    }
    let depth = s.doc.undo_depth();
    match make_default(s, category, material) {
        Ok(id) => {
            s.doc.squash_since(depth, t!("tx-structural-type"));
            Ok((id, true))
        }
        Err(e) => {
            s.undo_to(depth);
            Err(e)
        }
    }
}

/// The first `category` type whose profile is catalog row `designation` of
/// `series`, its profile not turned, else one made from the row (the
/// profile imported when the project lacks it). What it makes takes undo
/// steps of its own: callers squash them into theirs.
pub(crate) fn catalog_type(
    s: &mut Session,
    category: Category,
    series: &str,
    designation: &str,
) -> Result<ElementTypeId> {
    catalog_type_turned(s, category, (series, designation), 0.0)
}

/// [`catalog_type`] with the profile turned `rotation_deg` (90 lays a
/// beam's section on its side: its depth runs across, its width up).
pub(crate) fn catalog_type_turned(
    s: &mut Session,
    category: Category,
    (series, designation): (&str, &str),
    rotation_deg: f64,
) -> Result<ElementTypeId> {
    let p = s.project();
    let is_row = |d: &cc_core::ProfileDef| {
        d.catalog.as_ref().is_some_and(|c| c.series == series && c.designation == designation) || d.name == designation
    };
    let turned = |data: &TypeData| match data {
        TypeData::Column { profile_rotation_deg: r, .. } | TypeData::Beam { profile_rotation_deg: r, .. } => {
            (r - rotation_deg).abs() < 1e-9
        }
        _ => false,
    };
    let found =
        p.types_of(category).find(|t| turned(&t.data) && p.type_profile_def(&t.data).is_some_and(is_row)).map(|t| t.id);
    if let Some(id) = found {
        return Ok(id);
    }
    let material = cc_core::profile_catalog::series(series)
        .map_or(StructuralMaterial::Steel, |x| StructuralMaterial::of_profile(x.material));
    let profile = catalog_profile(s, (series, designation), material)?;
    let code = if category == Category::Column { "column" } else { "beam" };
    let out = ProfileTypeCreate { profile: Ref::Name(profile), category: code.into(), ..Default::default() }.run(s)?;
    let id = out["id"].as_u64().map(ElementTypeId).ok_or_else(|| EngineError::not_found("the new type"))?;
    if rotation_deg != 0.0 {
        edit(s, t!("tx-profile-type-create"), |e| {
            if let Some(TypeData::Column { profile_rotation_deg, .. } | TypeData::Beam { profile_rotation_deg, .. }) =
                e.type_mut(id).map(|t| &mut t.data)
            {
                *profile_rotation_deg = rotation_deg;
            }
            Ok(())
        })?;
    }
    Ok(id)
}

fn make_default(s: &mut Session, category: Category, material: StructuralMaterial) -> Result<ElementTypeId> {
    match material {
        StructuralMaterial::Steel => catalog_type(s, category, STEEL_DEFAULT.0, STEEL_DEFAULT.1),
        StructuralMaterial::Timber => catalog_type(s, category, TIMBER_DEFAULT.0, TIMBER_DEFAULT.1),
        StructuralMaterial::Concrete => {
            let profile = concrete_profile(s)?;
            let category = if category == Category::Column { "column" } else { "beam" };
            let out =
                ProfileTypeCreate { profile: Ref::Name(profile), category: category.into(), ..Default::default() }
                    .run(s)?;
            out["id"].as_u64().map(ElementTypeId).ok_or_else(|| EngineError::not_found("the new type"))
        }
    }
}

/// The project's profile of a catalog row, imported when missing.
fn catalog_profile(
    s: &mut Session,
    (series, designation): (&str, &str),
    material: StructuralMaterial,
) -> Result<String> {
    let existing = s.project().profile_named(designation);
    if let Some(p) = existing.filter(|p| StructuralMaterial::of_profile(p.material) == material) {
        return Ok(p.name.clone());
    }
    // A same-named profile of another material is left alone: the row comes in under a free name.
    let out = ProfileImport {
        series: series.into(),
        designations: vec![designation.into()],
        on_clash: Some("rename".into()),
    }
    .run(s)?;
    out["imported"][0]["name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| EngineError::not_found(format!("catalog row {designation}")))
}

/// The 0.20 x 0.40 concrete rectangle: the project's `20x40` when it is that, else a new one.
fn concrete_profile(s: &mut Session) -> Result<String> {
    let wanted = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 };
    let existing = s.project().profile_named(CONCRETE_DEFAULT);
    if let Some(p) = existing.filter(|p| p.shape == wanted && p.material == ProfileMaterial::Concrete) {
        return Ok(p.name.clone());
    }
    let name = existing.is_none().then(|| CONCRETE_DEFAULT.to_string());
    let params = BTreeMap::from([("b".to_string(), 0.2), ("h".to_string(), 0.4)]);
    let out = ProfileCreate {
        name,
        material: "concrete".into(),
        shape: Some("rect".into()),
        params: Some(params),
        ..Default::default()
    }
    .run(s)?;
    out["name"].as_str().map(str::to_string).ok_or_else(|| EngineError::not_found("the new profile"))
}

/// The column or beam type new members of a structural material get: the
/// first type of that material, else a default made in one undo step
/// (steel `W200x26,6`, timber `MLC 9x32` from the profile catalog, concrete
/// a 0.20 x 0.40 rectangle named `20x40`). Returns its `id`, `name` and
/// whether it was `created`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StructuralTypeFor {
    /// `column` or `beam`.
    pub category: String,
    /// `concrete`, `steel` or `timber`.
    pub material: String,
}

impl Command for StructuralTypeFor {
    const NAME: &'static str = "structural_type_for";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let category = member_category(&self.category)?;
        let material = material_arg(&self.material)?;
        let (id, created) = structural_type(s, category, material)?;
        let name = s.project().type_name(id).to_string();
        Ok(json!({ "id": id.0, "name": name, "created": created }))
    }
}

/// The given type, else the catalog row's (made when missing).
pub(crate) fn type_or_row(
    s: &mut Session,
    given: Option<ElementTypeId>,
    category: Category,
    (series, row): (&str, &str),
) -> Result<ElementTypeId> {
    match given {
        Some(t) => Ok(t),
        None => catalog_type(s, category, series, row),
    }
}

/// [`type_or_row`] with the row's type turned `rotation_deg` (see [`catalog_type_turned`]).
pub(crate) fn type_or_turned_row(
    s: &mut Session,
    given: Option<ElementTypeId>,
    category: Category,
    row: (&str, &str),
    rotation_deg: f64,
) -> Result<ElementTypeId> {
    match given {
        Some(t) => Ok(t),
        None => catalog_type_turned(s, category, row, rotation_deg),
    }
}
