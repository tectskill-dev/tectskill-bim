//! Filled and masking regions: 2D areas drawn in one view (see
//! [`cc_core::region`]), sketched like floors (`curves`: an outline and
//! the loops inside it as openings). They are moved, copied, rotated,
//! mirrored and deleted with the generic commands; `sketch_get`/`sketch_set`
//! read and redraw the boundary; `element_set` changes the boundary style
//! (`"boundary": "<style>"` or `"none"`) and `element_set`'s `type` (or
//! `type_set` on the type) the fill.

use super::sketch::{CurveArg, profile, sketch_of};
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::detail::LineStyleRef;
use cc_core::region::Region;
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, Pattern, PatternFill, Project, TypeData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::fill_patterns::pattern_arg;

/// A boundary style by code; `none` for no boundary.
fn boundary_arg(p: &Project, code: &str) -> Result<Option<LineStyleRef>> {
    if code.trim().eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    super::detail::style_arg(p, code).map(Some)
}

/// The fill of a new filled region type: diagonal lines over white.
fn default_fill() -> PatternFill {
    PatternFill {
        foreground: Pattern::Diagonal,
        foreground_color: [0, 0, 0],
        background: Pattern::Solid,
        background_color: [255, 255, 255],
    }
}

/// The default type of `category`, creating one if the project has none.
fn region_type(e: &mut cc_core::EditCtx, category: Category) -> ElementTypeId {
    if let Some(t) = e.project().default_type(category) {
        return t;
    }
    let id = ElementTypeId(e.alloc_id());
    let (name, data) = match category {
        Category::MaskingRegion => (t!("tpl-type-masking-region"), TypeData::MaskingRegion {}),
        _ => (t!("tpl-type-filled-region"), TypeData::FilledRegion { fill: default_fill() }),
    };
    e.add_type(ElementType { id, name, data, info: Default::default(), uid: Default::default() })
}

/// The view (and its level) regions go in: a plan, ceiling plan,
/// elevation, section, legend or drafting view.
fn region_view(p: &Project, r: &Ref) -> Result<(cc_core::ViewId, cc_core::LevelId)> {
    let id = refs::view(p, r)?;
    let view = p.view(id).ok_or_else(|| EngineError::not_found("view"))?;
    if !cc_views::hosts_cad(&view.kind) {
        return Err(EngineError::invalid(
            "regions go in plans, ceiling plans, elevations, sections, legends and drafting views",
        ));
    }
    let level =
        view.level().or_else(|| p.levels.first().map(|l| l.id)).ok_or_else(|| EngineError::rejected("no levels"))?;
    Ok((id, level))
}

/// The region the curves draw in `view`.
fn region_of(view: cc_core::ViewId, curves: &[CurveArg], boundary: Option<LineStyleRef>) -> Result<Region> {
    let sketch = sketch_of(curves)?;
    let p = profile(&sketch)?;
    super::create::check_openings(&p.outline, &p.openings)?;
    let region = Region { view, outline: p.outline, openings: p.openings, sketch: Some(sketch), boundary };
    if !region.is_valid() {
        return Err(EngineError::invalid("the region has no area or is out of bounds"));
    }
    Ok(region)
}

fn create(
    s: &mut Session,
    view: &Ref,
    curves: &[CurveArg],
    boundary: Option<LineStyleRef>,
    masking: bool,
    ty: Option<&Ref>,
) -> Result<Value> {
    let (view, level) = region_view(s.project(), view)?;
    let region = region_of(view, curves, boundary)?;
    let category = if masking { Category::MaskingRegion } else { Category::FilledRegion };
    let given = ty.map(|r| refs::type_of(s.project(), category, Some(r))).transpose()?;
    let tx = if masking { t!("tx-create-masking-region") } else { t!("tx-create-filled-region") };
    let id = edit(s, tx, |e| {
        let type_id = given.unwrap_or_else(|| region_type(e, category));
        let kind = if masking { ElementKind::MaskingRegion(region) } else { ElementKind::FilledRegion(region) };
        Ok(add_element(e, level, type_id, kind))
    })?;
    Ok(json!({ "id": id.0 }))
}

/// Draw a filled region in a 2D view (plan, ceiling plan, elevation,
/// section, legend or drafting view): the closed loops of `curves` in the
/// view's drawing coordinates (meters), the largest the outline and those
/// inside it openings, filled as its type says (a pattern over a
/// background, or a solid color) and edged in a line style. It is drawn
/// only in that view, over the model and under the view's annotations. A
/// project without a filled region type gets one (diagonal lines over white).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilledRegionCreate {
    /// View id or name.
    pub view: Ref,
    /// The curves, closed into loops, as for sketches (`line`, `arc`,
    /// `circle`, `ellipse`, `rectangle`, `polygon`).
    pub curves: Vec<CurveArg>,
    /// Filled region type id or name (default: the project's first).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Boundary line style (a built-in code or a project style's name;
    /// default `thin`), or `none` for no boundary.
    #[serde(default)]
    pub boundary: Option<String>,
}

impl Command for FilledRegionCreate {
    const NAME: &'static str = "filled_region_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let boundary = match &self.boundary {
            Some(code) => boundary_arg(s.project(), code)?,
            None => Some(LineStyleRef::default()),
        };
        create(s, &self.view, &self.curves, boundary, false, self.type_ref.as_ref())
    }
}

/// Draw a masking region in a 2D view: an opaque white area (the closed
/// loops of `curves`, openings inside the outline) that hides the model's
/// linework of the view under it, while the view's annotations (detail
/// lines, dimensions, texts, tags) stay on top. Drawn only in that view.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskingRegionCreate {
    /// View id or name.
    pub view: Ref,
    /// The curves, closed into loops, as for sketches.
    pub curves: Vec<CurveArg>,
    /// Boundary line style (a built-in code or a project style's name), or
    /// `none` (the default) for no boundary.
    #[serde(default)]
    pub boundary: Option<String>,
}

impl Command for MaskingRegionCreate {
    const NAME: &'static str = "masking_region_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let boundary = match &self.boundary {
            Some(code) => boundary_arg(s.project(), code)?,
            None => None,
        };
        create(s, &self.view, &self.curves, boundary, true, None)
    }
}

/// Create a filled region type: a foreground pattern over a background
/// pattern, each with its color (a `solid` pattern fills with its color; a
/// `none` background leaves the model visible between the pattern lines).
/// Change it later with `type_set` (`{"fill": {...}}`).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FilledRegionTypeCreate {
    /// Type name (default: the template's filled region name).
    #[serde(default)]
    pub name: Option<String>,
    /// Foreground pattern (default `diagonal`): none, solid, diagonal,
    /// diagonal_back, crosshatch, horizontal, vertical, grid, brick,
    /// concrete, sand, insulation or earth.
    #[serde(default)]
    pub foreground: Option<String>,
    /// Foreground color `[r, g, b]` (default black).
    #[serde(default)]
    pub foreground_color: Option<[u8; 3]>,
    /// Background pattern (default `solid`), as for `foreground`.
    #[serde(default)]
    pub background: Option<String>,
    /// Background color `[r, g, b]` (default white).
    #[serde(default)]
    pub background_color: Option<[u8; 3]>,
}

impl Command for FilledRegionTypeCreate {
    const NAME: &'static str = "filled_region_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let mut fill = default_fill();
        if let Some(p) = &self.foreground {
            fill.foreground = pattern_arg(s.project(), p)?;
        }
        if let Some(p) = &self.background {
            fill.background = pattern_arg(s.project(), p)?;
        }
        if let Some(c) = self.foreground_color {
            fill.foreground_color = c;
        }
        if let Some(c) = self.background_color {
            fill.background_color = c;
        }
        let name = self.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| t!("tpl-type-filled-region"));
        let id = edit(s, t!("tx-create-filled-region-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            Ok(e.add_type(ElementType {
                id,
                name,
                data: TypeData::FilledRegion { fill },
                info: Default::default(),
                uid: Default::default(),
            }))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

/// A `params` patch for a region with its boundary given by code
/// (`"boundary": "wide"`, or `"none"`) turned into the model's form.
pub(crate) fn normalize_patch(p: &Project, patch: &Value) -> Result<Value> {
    let mut patch = patch.clone();
    if let Some(code) = patch.get("boundary").and_then(Value::as_str) {
        let style = match boundary_arg(p, code)? {
            Some(b) => serde_json::to_value(b).map_err(|e| EngineError::invalid(e.to_string()))?,
            None => Value::Null,
        };
        if let Some(o) = patch.as_object_mut() {
            o.insert("boundary".into(), style);
        }
    }
    Ok(patch)
}

/// Check a patched region: the view stays, the outline is valid, the
/// openings lie inside it apart.
pub(crate) fn check(old: &Region, new: &Region) -> Result<()> {
    if new.view != old.view {
        return Err(EngineError::invalid("the view of a region cannot be changed"));
    }
    if !new.is_valid() {
        return Err(EngineError::invalid("the region has no area or is out of bounds"));
    }
    super::create::check_openings(&new.outline, &new.openings)
}

/// Redraw a region's outline and openings from a sketch (`sketch_set`).
pub(crate) fn redraw(r: &mut Region, sketch: cc_core::sketch::Sketch) -> Result<()> {
    let p = profile(&sketch)?;
    super::create::check_openings(&p.outline, &p.openings)?;
    (r.outline, r.openings) = (p.outline, p.openings);
    r.sketch = Some(sketch);
    if !r.is_valid() {
        return Err(EngineError::invalid("the region has no area or is out of bounds"));
    }
    Ok(())
}
