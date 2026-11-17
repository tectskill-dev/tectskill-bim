//! Parametric bracing: `bracing_create` makes a host that keeps the braces
//! of one bay of a frame in step with its pattern, storeys and brace type.

use super::structure_material::{material_arg, type_or_row};
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::bracing::{self as br, Bracing, BracingPattern};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, LevelId, StructuralMaterial};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The catalog row the braces of a material get by default: (series,
/// designation); `None` for concrete (give a beam type).
pub fn brace_row(m: StructuralMaterial) -> Option<(&'static str, &'static str)> {
    match m {
        StructuralMaterial::Steel => Some(("SHS", "SHS 100x100x4")),
        StructuralMaterial::Timber => Some(("Timber-Sawn", "Peça 6x12")),
        StructuralMaterial::Concrete => None,
    }
}

/// The type a project gets with its first bracing.
pub fn new_bracing_type(id: ElementTypeId) -> ElementType {
    br::host_type(id, t!("tpl-type-bracing"))
}

/// Brace one bay of a frame, storey by storey: the bay runs from `start` to
/// `end` in plan (the axes of its two columns), or between two `columns`,
/// whose level, base and height it then takes. `pattern` is `x` (default:
/// both diagonals), `diagonal` (one, rising toward `end`; `flip` turns it,
/// `alternate` turns every other storey), `v` (from the top corners down to
/// the middle of the bay), `chevron` (from the bottom corners up to the
/// middle, an inverted V) or `k` (from the mid-height of each column to the
/// middle of the bay at the top and at the bottom of the storey). `stories`
/// storeys of `height` are braced one above the other from `base_offset`.
/// The braces are ordinary beams the bracing keeps in step with its
/// parameters (`element_set` regenerates them, keeping their ids); a brace
/// edited or deleted by hand is detached and left alone. The default brace
/// type comes from the profile catalog for the `material` (steel:
/// `SHS 100x100x4`; timber: `Peça 6x12`), made when missing; concrete needs
/// a `type`. Out-of-range values are refused. One undo step; returns the
/// bracing's `id` and its `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BracingCreate {
    /// One column axis of the bay in plan (not with `columns`).
    #[serde(default)]
    pub start: Option<Point>,
    /// The other column axis, 0.5 to 200 m from the start (not with `columns`).
    #[serde(default)]
    pub end: Option<Point>,
    /// The two column ids of the bay, instead of `start` and `end`.
    #[serde(default)]
    pub columns: Vec<u64>,
    /// `x` (default), `diagonal`, `v`, `chevron` or `k`.
    #[serde(default)]
    pub pattern: Option<String>,
    /// One storey, meters, 0.5 to 50 (default: the columns' common height over `stories`, else the level's height).
    #[serde(default)]
    pub height: Option<f64>,
    /// Storeys braced one above the other, 1 to 100 (default 1).
    #[serde(default)]
    pub stories: Option<u32>,
    /// The foot of the lowest storey above the level, meters (default: the columns' base, else 0).
    #[serde(default)]
    pub base_offset: Option<f64>,
    /// A `diagonal` rises toward the start instead of the end.
    #[serde(default)]
    pub flip: bool,
    /// `diagonal` braces change direction storey by storey.
    #[serde(default)]
    pub alternate: bool,
    /// Level id or name (default: the first column's level, else the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Beam type of the braces, id or name (default: the material's catalog row).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// `steel` (default) or `timber`: the catalog row of the brace type when `type` is not given.
    #[serde(default)]
    pub material: Option<String>,
}

/// The bay between two columns: its level, the column axes, and the base
/// (above the level) and height both columns share.
fn bay_of(p: &cc_core::Project, columns: &[u64]) -> Result<(LevelId, DVec2, DVec2, f64, f64)> {
    let [a, b] = columns[..] else { return Err(EngineError::invalid(t!("msg-columns-two-exactly"))) };
    let span = |id: u64| -> Result<(LevelId, DVec2, f64, f64)> {
        let e = p.element(refs::element(p, id)?).ok_or_else(|| EngineError::not_found("column"))?;
        let ElementKind::Column(c) = &e.kind else {
            return Err(EngineError::invalid(format!("element {id} is not a column")));
        };
        let (base, top) = p.column_span(e, c);
        Ok((e.level, c.position, base, top))
    };
    let (level, pa, base_a, top_a) = span(a)?;
    let (_, pb, base_b, top_b) = span(b)?;
    let (base, top) = (base_a.max(base_b), top_a.min(top_b));
    Ok((level, pa, pb, base - p.level_elevation(level), top - base))
}

impl Command for BracingCreate {
    const NAME: &'static str = "bracing_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        let pattern = match &self.pattern {
            Some(c) => BracingPattern::from_code(c).ok_or_else(|| {
                EngineError::invalid(format!("pattern must be x, diagonal, v, chevron or k, not '{c}'"))
            })?,
            None => BracingPattern::X,
        };
        let stories = self.stories.unwrap_or(1);
        if !br::STORIES.contains(&stories) {
            return Err(EngineError::invalid(format!(
                "stories must be between {} and {}",
                br::STORIES.start(),
                br::STORIES.end()
            )));
        }
        let (level, start, end, base, height) = if self.columns.is_empty() {
            let (Some(start), Some(end)) = (self.start, self.end) else {
                return Err(EngineError::invalid("give start and end, or the two columns of the bay"));
            };
            let level = refs::level_or_first(p, self.level.as_ref())?;
            (level, point(start)?, point(end)?, 0.0, p.level_height(level, p.defaults().story))
        } else {
            if self.start.is_some() || self.end.is_some() {
                return Err(EngineError::invalid("give start and end, or columns, not both"));
            }
            let (own, a, b, base, total) = bay_of(p, &self.columns)?;
            let level = match &self.level {
                Some(r) => refs::level_or_first(p, Some(r))?,
                None => own,
            };
            let base = base + p.level_elevation(own) - p.level_elevation(level);
            (level, a, b, base, total / f64::from(stories))
        };
        let given = self.type_ref.as_ref().map(|r| refs::type_of(p, Category::Beam, Some(r))).transpose()?;
        let material = match &self.material {
            Some(m) => material_arg(m)?,
            None => StructuralMaterial::Steel,
        };
        let row = match (given, brace_row(material)) {
            (None, None) => {
                return Err(EngineError::invalid(
                    "braces have no default concrete type: give a beam type, or steel or timber",
                ));
            }
            (_, row) => row.unwrap_or(("", "")),
        };
        let mut bracing = Bracing {
            start,
            end,
            base_offset: self.base_offset.unwrap_or(base),
            height: self.height.unwrap_or(height),
            stories,
            pattern,
            flip: self.flip,
            alternate: self.alternate,
            brace_type: ElementTypeId(0),
            members: Vec::new(),
            detached: Vec::new(),
        };
        bracing.check().map_err(EngineError::invalid)?;
        let host_type = p.default_type(Category::Bracing);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            bracing.brace_type = type_or_row(s, given, Category::Beam, row)?;
            bracing.check_types(s.project()).map_err(EngineError::invalid)?;
            let kind = ElementKind::Bracing(Box::new(bracing));
            let id = edit(s, t!("tx-create-bracing"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_bracing_type(id))
                    }
                };
                Ok(add_element(e, level, type_id, kind))
            })?;
            let members: Vec<u64> = s.project().host_members_of(id).iter().map(|m| m.0).collect();
            Ok(json!({ "id": id.0, "members": members }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-create-bracing"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a bracing: its brace ids, how many
/// roles are detached, the bay width and the height of all its storeys.
pub(crate) fn derived(b: &Bracing, out: &mut serde_json::Map<String, Value>) {
    let ids: Vec<u64> = b.members.iter().map(|m| m.id.0).collect();
    out.insert("members".into(), json!(ids));
    out.insert("detached".into(), json!(b.detached.len()));
    out.insert("length".into(), json!(b.length()));
    out.insert("total_height".into(), json!(b.total_height()));
}
