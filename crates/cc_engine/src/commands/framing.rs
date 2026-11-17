//! Bracing and trusses: sloped members between points in space, braces
//! between columns and trusses laid out from a span, a height and a pattern.

use super::structure_material::{material_arg, type_or_row};
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::truss::{self as tr, Truss, TrussPattern, TrussRoleKind, TrussShape};
use cc_core::{Beam, BeamVertical, Category, ElementKind, ElementType, ElementTypeId, LevelId, StructuralMaterial};
use glam::DVec3;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A point in space: `[x, y, z]`, `z` above the level.
pub type Point3 = [f64; 3];

fn point3(p: Point3) -> Result<DVec3> {
    let v = DVec3::from_array(p);
    v.is_finite().then_some(v).ok_or_else(|| EngineError::invalid("points must be finite numbers"))
}

/// A member from `a` to `b` (elevations above the level), its axis through
/// both: a beam centered on its location line, sloped when the ends differ.
fn member(a: DVec3, b: DVec3) -> Result<Beam> {
    if a.truncate().distance(b.truncate()) < 1e-3 {
        return Err(EngineError::invalid(t!("msg-member-vertical")));
    }
    let mut beam = Beam::new(a.truncate(), b.truncate(), a.z);
    beam.vertical = BeamVertical::Center;
    if (b.z - a.z).abs() > 1e-9 {
        beam.end_offset = Some(b.z);
    }
    Ok(beam)
}

/// Create a brace: a member between two points in space (`[x, y, z]`, `z`
/// above the level), its axis through both. Its ends must not be one above the other.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BraceCreate {
    /// Start point `[x, y, z]`.
    pub start: Point3,
    /// End point `[x, y, z]`.
    pub end: Point3,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Beam type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for BraceCreate {
    const NAME: &'static str = "brace_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = refs::type_of(p, Category::Beam, self.type_ref.as_ref())?;
        let beam = member(point3(self.start)?, point3(self.end)?)?;
        let id = edit(s, t!("tx-create-brace"), |e| Ok(add_element(e, level, type_id, ElementKind::Beam(beam))))?;
        Ok(super::created(s, id))
    }
}

/// Brace the bay between two columns: `x` (default) crosses two members
/// from the base of each column to the top of the other, `diagonal` places
/// one from the base of the first to the top of the second.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BracesBetweenColumns {
    /// The two column ids.
    pub columns: Vec<u64>,
    /// `x` or `diagonal`.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Beam type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for BracesBetweenColumns {
    const NAME: &'static str = "braces_between_columns";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let [a, b] = self.columns[..] else { return Err(EngineError::invalid(t!("msg-columns-two-exactly"))) };
        let axis = |id: u64| -> Result<(LevelId, DVec3, DVec3)> {
            let e = p.element(refs::element(p, id)?).ok_or_else(|| EngineError::not_found("column"))?;
            let ElementKind::Column(c) = &e.kind else {
                return Err(EngineError::invalid(format!("element {id} is not a column")));
            };
            let level_z = p.level_elevation(e.level);
            let (base, top) = p.column_span(e, c);
            Ok((e.level, c.position.extend(base - level_z), c.position.extend(top - level_z)))
        };
        let (level, a0, a1) = axis(a)?;
        let (other, b0, b1) = axis(b)?;
        // Elevations relative to the first column's level.
        let shift = DVec3::Z * (p.level_elevation(other) - p.level_elevation(level));
        let (b0, b1) = (b0 + shift, b1 + shift);
        let type_id = refs::type_of(p, Category::Beam, self.type_ref.as_ref())?;
        let mut members = vec![member(a0, b1)?];
        match self.pattern.as_deref().unwrap_or("x") {
            "x" => members.push(member(b0, a1)?),
            "diagonal" => {}
            other => return Err(EngineError::invalid(format!("pattern must be 'x' or 'diagonal', not '{other}'"))),
        }
        let created = edit(s, t!("tx-create-brace"), |e| {
            Ok(members.into_iter().map(|m| add_element(e, level, type_id, ElementKind::Beam(m))).collect::<Vec<_>>())
        })?;
        Ok(json!({"created": ids_json(&created)}))
    }
}

/// The catalog rows a truss of a material gets by default: (series,
/// designation) of the chords and of the web (the posts are a column type of
/// the web's row).
#[derive(Debug, Clone, Copy)]
pub struct TrussRows {
    pub chord: (&'static str, &'static str),
    pub web: (&'static str, &'static str),
}

/// Steel and timber rows; `None` for concrete (trusses are not offered).
pub fn truss_rows(m: StructuralMaterial) -> Option<TrussRows> {
    match m {
        StructuralMaterial::Steel => {
            Some(TrussRows { chord: ("NBR6355-Ue", "Ue 150x60x20x2,00"), web: ("NBR6355-Ue", "Ue 100x50x17x2,00") })
        }
        StructuralMaterial::Timber => {
            Some(TrussRows { chord: ("Timber-Sawn", "Peça 6x16"), web: ("Timber-Sawn", "Peça 6x12") })
        }
        StructuralMaterial::Concrete => None,
    }
}

/// The type a project gets with its first truss.
pub fn new_truss_type(id: ElementTypeId) -> ElementType {
    tr::host_type(id, t!("tpl-type-truss"))
}

/// Lay out a parametric plane truss from `start` to `end`: a `flat`
/// (default), `mono` or `gable` outline `height` deep (the rise at the apex
/// or high end for mono and gable), `panels` bottom-chord panels and the web
/// of `pattern` (`pratt`, the default, `howe`, `warren`, or for gables
/// `kingpost`, `queenpost` and `fink`; a gable-only pattern on another shape
/// is laid out as Pratt). Chords and diagonals are beams, verticals are
/// columns, all ordinary elements the truss keeps in step with its
/// parameters (`element_set` regenerates them, keeping their ids); a member
/// edited or deleted by hand is detached and left alone. `copies` trusses
/// are made `spacing` apart to the left of start → end. Default types come
/// from the profile catalog for the `material` (steel: `Ue 150x60x20x2,00`
/// chords, `Ue 100x50x17x2,00` web and posts; timber: `Peça 6x16` chords,
/// `Peça 6x12` web and posts), made when missing; concrete is refused.
/// Out-of-range values are refused. One undo step; returns the truss's `id`
/// and its `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TrussCreate {
    /// Start of the span line in plan (a support).
    pub start: Point,
    /// End of the span line in plan (the other support), 0.5 to 200 m from the start.
    pub end: Point,
    /// Chord axis to chord axis (flat), or the rise at the apex or high end (mono, gable), meters, 0.1 to 20.
    pub height: f64,
    /// Bottom-chord panels, 2 to 64, even on a gable (default: about one per 1.5 m).
    #[serde(default)]
    pub panels: Option<u32>,
    /// `pratt` (default), `howe`, `warren`, `kingpost`, `queenpost` or `fink`.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Elevation of the bottom chord's axis above the level (default: the level height minus `height`).
    #[serde(default)]
    pub bottom_offset: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Beam type of both chords (default: the material's catalog chord).
    #[serde(default)]
    pub chord_type: Option<Ref>,
    /// Beam type of the diagonals (default: the material's catalog web row).
    #[serde(default)]
    pub web_type: Option<Ref>,
    /// Column type of the verticals (default: a column type of the material's catalog web row).
    #[serde(default)]
    pub post_type: Option<Ref>,
    /// `flat` (default), `mono` (rising toward `end`) or `gable` (rising toward mid-span).
    #[serde(default)]
    pub shape: Option<String>,
    /// Beam type of the top chord (default: `chord_type`, else the catalog chord).
    #[serde(default)]
    pub top_chord_type: Option<Ref>,
    /// Beam type of the bottom chord (default: `chord_type`, else the catalog chord).
    #[serde(default)]
    pub bottom_chord_type: Option<Ref>,
    /// How many trusses, 1 to 200 (default 1).
    #[serde(default)]
    pub copies: Option<u32>,
    /// Distance between copies, to the left of start → end, meters, 0.3 to 20 (default 1.5).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// `steel` (default) or `timber`: the catalog rows of the types not given (a given `chord_type` does not change it, so give `material` with a timber chord).
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for TrussCreate {
    const NAME: &'static str = "truss_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let material = match &self.material {
            Some(m) => material_arg(m)?,
            None => StructuralMaterial::Steel,
        };
        let rows = truss_rows(material)
            .ok_or_else(|| EngineError::invalid("trusses are made of steel or timber, not concrete"))?;
        let shape = match &self.shape {
            Some(c) => TrussShape::from_code(c)
                .ok_or_else(|| EngineError::invalid(format!("unknown shape '{c}' (flat, mono, gable)")))?,
            None => TrussShape::Flat,
        };
        let pattern = match &self.pattern {
            Some(c) => TrussPattern::from_code(c).ok_or_else(|| {
                EngineError::invalid(format!(
                    "pattern must be pratt, howe, warren, kingpost, queenpost or fink, not '{c}'"
                ))
            })?,
            None => TrussPattern::Pratt,
        };
        let (start, end) = (point(self.start)?, point(self.end)?);
        let bottom = self.bottom_offset.unwrap_or_else(|| p.level_height(level, p.defaults().story) - self.height);
        let placeholder = ElementTypeId(0);
        let mut truss = Truss {
            start,
            end,
            base_offset: bottom,
            shape,
            height: self.height,
            pattern,
            panels: self.panels.unwrap_or_else(|| tr::default_panels(start.distance(end), shape)),
            top_chord_type: placeholder,
            bottom_chord_type: placeholder,
            web_type: placeholder,
            post_type: placeholder,
            copies: self.copies.unwrap_or(1),
            spacing: self.spacing.unwrap_or(tr::DEFAULT_SPACING),
            members: Vec::new(),
            detached: Vec::new(),
        };
        truss.check().map_err(EngineError::invalid)?;
        let given = |r: Option<&Ref>, c: Category| r.map(|r| refs::type_of(p, c, Some(r))).transpose();
        let chord = self.chord_type.as_ref();
        let top_given = given(self.top_chord_type.as_ref().or(chord), Category::Beam)?;
        let bottom_given = given(self.bottom_chord_type.as_ref().or(chord), Category::Beam)?;
        let web_given = given(self.web_type.as_ref(), Category::Beam)?;
        let post_given = given(self.post_type.as_ref(), Category::Column)?;
        let host_type = p.default_type(Category::Truss);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            truss.top_chord_type = type_or_row(s, top_given, Category::Beam, rows.chord)?;
            truss.bottom_chord_type = type_or_row(s, bottom_given, Category::Beam, rows.chord)?;
            truss.web_type = type_or_row(s, web_given, Category::Beam, rows.web)?;
            truss.post_type = type_or_row(s, post_given, Category::Column, rows.web)?;
            truss.check_types(s.project()).map_err(EngineError::invalid)?;
            let kind = ElementKind::Truss(Box::new(truss));
            let id = edit(s, t!("tx-create-truss"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_truss_type(id))
                    }
                };
                Ok(add_element(e, level, type_id, kind))
            })?;
            let members: Vec<u64> = s.project().host_members_of(id).iter().map(|m| m.0).collect();
            Ok(json!({ "id": id.0, "members": members }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-create-truss"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a truss: its member ids by role kind,
/// how many roles are detached, its span length and the pattern it is laid
/// out with (a gable-only pattern falls back to Pratt).
pub(crate) fn derived(t: &Truss, out: &mut serde_json::Map<String, Value>) {
    let ids =
        |k: TrussRoleKind| -> Vec<u64> { t.members.iter().filter(|m| m.role.kind == k).map(|m| m.id.0).collect() };
    out.insert(
        "members".into(),
        json!({
            "top_chords": ids(TrussRoleKind::TopChord),
            "bottom_chords": ids(TrussRoleKind::BottomChord),
            "diagonals": ids(TrussRoleKind::Diagonal),
            "verticals": ids(TrussRoleKind::Vertical),
        }),
    );
    out.insert("detached".into(), json!(t.detached.len()));
    out.insert("length".into(), json!(t.length()));
    out.insert("pattern_used".into(), json!(t.pattern_used().code()));
}
