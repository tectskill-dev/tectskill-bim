//! Detail lines: 2D lines, arcs, circles and ellipses drawn in one view
//! (see [`cc_core::detail`]). They are moved, copied, rotated, mirrored,
//! trimmed, split, offset and deleted with the generic commands, and
//! `element_get`/`element_set` show and change their curve and style.

use super::sketch::CurveArg;
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::detail::{DetailLine, DetailStyle, LineStyleRef};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, TypeData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Most curves one `detail_line_create` makes.
pub const MAX_CURVES: usize = cc_core::sketch::MAX_CURVES;

/// The detail line type, creating one if the project has none.
fn detail_type(e: &mut cc_core::EditCtx) -> ElementTypeId {
    if let Some(t) = e.project().default_type(Category::DetailLine) {
        return t;
    }
    let id = ElementTypeId(e.alloc_id());
    e.add_type(ElementType {
        id,
        name: t!("tpl-type-detail-line"),
        data: TypeData::DetailLine {},
        info: Default::default(),
        uid: Default::default(),
    })
}

/// A line style named by its code: a built-in style's code, or the name
/// of one of the project's line styles.
pub fn style_arg(p: &cc_core::Project, code: &str) -> Result<LineStyleRef> {
    if let Some(s) = DetailStyle::from_code(code) {
        return Ok(LineStyleRef::BuiltIn(s));
    }
    if let Some(s) = p.line_styles.values().find(|s| s.name.trim().eq_ignore_ascii_case(code.trim())) {
        return Ok(LineStyleRef::Project(s.id));
    }
    let codes: Vec<String> = DetailStyle::ALL
        .iter()
        .map(|s| s.code().to_string())
        .chain(p.line_styles.values().map(|s| s.name.clone()))
        .collect();
    Err(EngineError::invalid(format!("unknown line style '{code}' ({})", codes.join(", "))))
}

/// The view (and its level) detail lines go in: a plan, ceiling plan,
/// elevation, section, legend or drafting view.
fn detail_view(p: &cc_core::Project, r: &Ref) -> Result<(cc_core::ViewId, cc_core::LevelId)> {
    let id = refs::view(p, r)?;
    let view = p.view(id).ok_or_else(|| EngineError::not_found("view"))?;
    if !cc_views::hosts_cad(&view.kind) {
        return Err(EngineError::invalid(
            "detail lines go in plans, ceiling plans, elevations, sections, legends and drafting views",
        ));
    }
    let level =
        view.level().or_else(|| p.levels.first().map(|l| l.id)).ok_or_else(|| EngineError::rejected("no levels"))?;
    Ok((id, level))
}

/// Draw detail lines in a 2D view (plan, ceiling plan, elevation, section,
/// legend or drafting view): lines, arcs, circles and ellipses in the
/// view's drawing coordinates (meters, as for dimensions and text notes),
/// shown only in that view, on top of the model. Each curve becomes one
/// element (a polyline is its segments; rectangles and polygons their
/// sides), all in one undo step. Detail lines are left out of 3D views and
/// IFC export; DXF export puts them on a layer named after their style.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetailLineCreate {
    /// View id or name.
    pub view: Ref,
    /// The curves to draw, as for sketches (`line`, `arc`, `circle`,
    /// `ellipse`, `rectangle`, `polygon`).
    pub curves: Vec<CurveArg>,
    /// Line style: thin (default), medium, wide, hidden, dashed, center or
    /// dotted, or the name of one of the project's line styles
    /// (`line_styles_list`).
    #[serde(default)]
    pub style: Option<String>,
}

impl Command for DetailLineCreate {
    const NAME: &'static str = "detail_line_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (view, level) = detail_view(s.project(), &self.view)?;
        let style = self.style.as_deref().map(|c| style_arg(s.project(), c)).transpose()?.unwrap_or_default();
        let mut lines = Vec::new();
        for arg in &self.curves {
            for curve in arg.curves()? {
                let line = DetailLine { view, curve, style };
                if !line.is_valid() {
                    return Err(EngineError::invalid("a curve is out of bounds"));
                }
                lines.push(line);
            }
            if lines.len() > MAX_CURVES {
                return Err(EngineError::invalid(format!("at most {MAX_CURVES} curves at once")));
            }
        }
        if lines.is_empty() {
            return Err(EngineError::invalid("give at least one curve"));
        }
        let ids = edit(s, t!("tx-create-detail-lines"), |e| {
            let type_id = detail_type(e);
            Ok(lines
                .into_iter()
                .map(|l| add_element(e, level, type_id, ElementKind::DetailLine(l)))
                .collect::<Vec<_>>())
        })?;
        Ok(json!({ "created": ids_json(&ids) }))
    }
}

/// A `params` patch for a detail line with its style given by code
/// (`"style": "wide"`) turned into the model's form.
pub(crate) fn normalize_patch(p: &cc_core::Project, patch: &Value) -> Result<Value> {
    let mut patch = patch.clone();
    if let Some(code) = patch.get("style").and_then(Value::as_str) {
        let style = serde_json::to_value(style_arg(p, code)?).map_err(|e| EngineError::invalid(e.to_string()))?;
        if let Some(o) = patch.as_object_mut() {
            o.insert("style".into(), style);
        }
    }
    Ok(patch)
}

/// Check a patched detail line: the view stays, the curve is valid.
pub(crate) fn check(old: &DetailLine, new: &DetailLine) -> Result<()> {
    if new.view != old.view {
        return Err(EngineError::invalid("the view of a detail line cannot be changed"));
    }
    if !new.is_valid() {
        return Err(EngineError::invalid("the curve has no size or is out of bounds"));
    }
    Ok(())
}
