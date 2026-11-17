//! Piles and pile caps (see `cc_core::pile`): free piles, pile and pile cap
//! types, caps placed one by one or under columns (their piles made with
//! them), and tie beams between caps.

use super::{add_element, created, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::pile::{Pile, PileCap, PileKind, PileLayout, PileSection};
use cc_core::{
    Beam, Category, EditCtx, ElementId, ElementKind, ElementType, ElementTypeId, Foundation, FoundationKind,
    FoundationShape, LevelId, Project, TypeData,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Points closer than this are the same place (1 cm).
const SAME: f64 = 0.01;

/// The material new pile and cap types are made of: the first foundation
/// type's (the template's concrete), else none.
fn concrete(p: &Project) -> String {
    p.types_of(Category::Foundation).next().map(|t| t.main_material().to_string()).unwrap_or_default()
}

/// A pile type's name: `Ø 40` for round piles, `30 x 30` for square ones (centimeters).
fn pile_type_name(section: &PileSection) -> String {
    let cm = |m: f64| format!("{:.0}", m * 100.0);
    match *section {
        PileSection::Round { diameter } => t!("tpl-type-pile", "size" => format!("Ø{}", cm(diameter))),
        PileSection::Square { side } => t!("tpl-type-pile", "size" => format!("{}x{}", cm(side), cm(side))),
    }
}

/// A pile type of `section` and `kind`.
fn new_pile_type(id: ElementTypeId, section: PileSection, kind: PileKind, material: String) -> ElementType {
    ElementType {
        id,
        name: pile_type_name(&section),
        data: TypeData::Pile { section, kind, material },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// The pile type named by `r`, else the first one; `None` when the project
/// has none (the caller makes one).
fn pile_type(p: &Project, r: Option<&Ref>) -> Result<Option<ElementTypeId>> {
    match (r, p.default_type(Category::Pile)) {
        (None, None) => Ok(None),
        (r, _) => Ok(Some(refs::type_of(p, Category::Pile, r)?)),
    }
}

/// The pile type `wanted`, or a new round one of the default size.
fn ensure_pile_type(e: &mut EditCtx, wanted: Option<ElementTypeId>) -> ElementTypeId {
    match wanted {
        Some(t) => t,
        None => {
            let id = ElementTypeId(e.alloc_id());
            let material = concrete(e.project());
            e.add_type(new_pile_type(id, PileSection::default(), PileKind::default(), material))
        }
    }
}

/// The section of a pile type.
fn section_of(p: &Project, type_id: ElementTypeId) -> Option<PileSection> {
    match p.element_type(type_id).map(|t| &t.data) {
        Some(TypeData::Pile { section, .. }) => Some(*section),
        _ => None,
    }
}

/// A pile kind from its code.
fn kind_arg(code: Option<&str>) -> Result<PileKind> {
    match code {
        None => Ok(PileKind::default()),
        Some(c) => PileKind::from_code(c).ok_or_else(|| {
            EngineError::invalid(format!("unknown pile kind '{c}' (bored, cfa, micro, precast, steel, other)"))
        }),
    }
}

/// A pile section from a diameter or a side.
fn section_arg(diameter: Option<f64>, side: Option<f64>) -> Result<PileSection> {
    let section = match (diameter, side) {
        (Some(_), Some(_)) => return Err(EngineError::invalid("give diameter or side, not both")),
        (Some(d), None) => PileSection::Round { diameter: d },
        (None, Some(s)) => PileSection::Square { side: s },
        (None, None) => PileSection::default(),
    };
    if !section.is_valid() {
        return Err(EngineError::invalid(format!(
            "the size must lie between {} and {} m",
            cc_core::pile::MIN_PILE_SIZE,
            cc_core::pile::MAX_PILE_SIZE
        )));
    }
    Ok(section)
}

/// A pile's toe from a `toe` or a `length` below `cut_off` (default: the default length).
pub(crate) fn toe_arg(cut_off: f64, toe: Option<f64>, length: Option<f64>) -> Result<f64> {
    let length = match (toe, length) {
        (Some(_), Some(_)) => return Err(EngineError::invalid("give toe or length, not both")),
        (Some(t), None) => cut_off - finite(t, "toe")?,
        (None, Some(l)) => l,
        (None, None) => cc_core::pile::DEFAULT_PILE_LENGTH,
    };
    if !cc_core::pile::length_ok(length) {
        return Err(EngineError::invalid(format!(
            "the pile's length must lie between {} and {} m",
            cc_core::pile::MIN_PILE_LENGTH,
            cc_core::pile::MAX_PILE_LENGTH
        )));
    }
    Ok(cut_off - length)
}

/// Create a pile type: round (`diameter`) or square (`side`), made as `kind` says.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PileTypeCreate {
    /// Type name (default: from the size, e.g. `Estaca Ø40`).
    #[serde(default)]
    pub name: Option<String>,
    /// Diameter of a round pile, meters (default 0.4).
    #[serde(default)]
    pub diameter: Option<f64>,
    /// Side of a square pile, meters (instead of `diameter`).
    #[serde(default)]
    pub side: Option<f64>,
    /// How the piles are made: `bored` (default), `cfa` (continuous flight auger), `micro`
    /// (micropiles and root piles), `precast` (precast concrete, driven), `steel` (driven), `other`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Material name (default: the first foundation type's, the template's concrete).
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for PileTypeCreate {
    const NAME: &'static str = "pile_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let section = section_arg(self.diameter, self.side)?;
        let kind = kind_arg(self.kind.as_deref())?;
        let p = s.project();
        let material = match self.material {
            Some(m) => refs::material(p, &Ref::Name(m))?,
            None => concrete(p),
        };
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-pile-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_pile_type(id, section, kind, material);
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

/// Create a free pile at `position`: its head (`cut_off`) above the level and
/// its toe given by `toe` or by a `length` below the head. A project without
/// a pile type gets one (round, 40 cm, in concrete).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PileCreate {
    /// Axis of the pile in plan.
    pub position: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Pile type id or name (default: the first pile type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Head of the pile (its cut-off) above the level, meters (default 0).
    #[serde(default)]
    pub cut_off: Option<f64>,
    /// Toe of the pile above the level, meters (negative: below it); or `length`.
    #[serde(default)]
    pub toe: Option<f64>,
    /// Length of the pile below its head, meters (default 12); or `toe`.
    #[serde(default)]
    pub length: Option<f64>,
    /// How far the head enters a cap or slab above it, meters (default 0).
    #[serde(default)]
    pub embedment: Option<f64>,
    /// Rotation of a square pile in plan, degrees (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
}

impl Command for PileCreate {
    const NAME: &'static str = "pile_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let ty = pile_type(p, self.type_ref.as_ref())?;
        let cut_off = finite(self.cut_off.unwrap_or(0.0), "cut_off")?;
        let toe = toe_arg(cut_off, self.toe, self.length)?;
        let embedment = finite(self.embedment.unwrap_or(0.0), "embedment")?;
        if !(0.0..=cc_core::pile::MAX_EMBEDMENT).contains(&embedment) {
            return Err(EngineError::invalid(format!(
                "the embedment must lie between 0 and {} m",
                cc_core::pile::MAX_EMBEDMENT
            )));
        }
        let pile = Pile {
            position: point(self.position)?,
            rotation_deg: finite(self.rotation.unwrap_or(0.0), "rotation")?,
            embedment,
            ..Pile::new(DVec2::ZERO, cut_off, cut_off - toe)
        };
        if !pile.is_valid() {
            return Err(EngineError::invalid(format!(
                "the head and the toe lie within {} m of the level",
                cc_core::pile::MAX_OFFSET
            )));
        }
        let id = edit(s, t!("tx-create-pile"), |e| {
            let type_id = ensure_pile_type(e, ty);
            let id = add_element(e, level, type_id, ElementKind::Pile(pile));
            let mark = e.project().next_mark(Category::Pile);
            if let Some(x) = e.element_mut(id) {
                x.mark = mark;
            }
            Ok(id)
        })?;
        Ok(created(s, id))
    }
}

/// A pile layout from its name and sizes.
fn layout_arg(name: &str, count: Option<u32>, radius: Option<f64>, ring_width: Option<f64>) -> Result<PileLayout> {
    let layout = match name.trim().to_ascii_lowercase().as_str() {
        "single" | "1" => PileLayout::Single,
        "pair" | "2" => PileLayout::Pair,
        "triangle" | "3" => PileLayout::Triangle,
        "square" | "4" => PileLayout::Square,
        "line" => PileLayout::Line { count: count.ok_or_else(|| EngineError::invalid("a line needs a count"))? },
        "circle" => PileLayout::Circle {
            count: count.ok_or_else(|| EngineError::invalid("a circle needs a count"))?,
            radius: finite(radius.ok_or_else(|| EngineError::invalid("a circle needs a radius"))?, "radius")?,
            ring_width,
        },
        other => {
            return Err(EngineError::invalid(format!(
                "unknown layout '{other}' (single, pair, triangle, square, line, circle)"
            )));
        }
    };
    if ring_width.is_some() && !matches!(layout, PileLayout::Circle { .. }) {
        return Err(EngineError::invalid("only a circle of piles has a ring width"));
    }
    Ok(layout)
}

/// A pile cap type's name: how many piles it stands on.
fn cap_type_name(cap: &PileCap) -> String {
    match cap.layout {
        PileLayout::Circle { ring_width: Some(_), count, .. } => t!("tpl-type-pile-ring", "count" => count),
        _ => t!("tpl-type-pile-cap", "count" => cap.layout.count()),
    }
}

/// A pile cap type `thickness` deep holding `cap`.
fn new_cap_type(id: ElementTypeId, cap: PileCap, thickness: f64, material: String) -> ElementType {
    let (lo, hi) = cap.outline().bounds();
    ElementType {
        id,
        name: cap_type_name(&cap),
        data: TypeData::Foundation {
            kind: FoundationKind::PileCap,
            width: hi.x - lo.x,
            length: hi.y - lo.y,
            thickness,
            material,
            haunch: None,
            pile_cap: Some(cap),
        },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// Why a cap of `thickness` cannot hold `cap` with piles `pile_size` across.
fn check_cap(cap: &PileCap, thickness: f64, pile_size: f64) -> Result<()> {
    if !(thickness.is_finite() && thickness > 0.0 && thickness <= cc_core::StripHaunch::MAX_HEIGHT) {
        return Err(EngineError::invalid("the thickness must be positive (at most 20 m)"));
    }
    if let Some(why) = cap.problem(pile_size) {
        return Err(EngineError::invalid(why));
    }
    if cap.embedment >= thickness {
        return Err(EngineError::invalid("the embedment must be less than the cap's thickness"));
    }
    Ok(())
}

/// Create a pile cap type: the layout of its piles (`single`, `pair`,
/// `triangle`, `square`, `line` of `count`, `circle` of `count` on a
/// `radius`, a ring cap with `ring_width`), their spacing and edge distance,
/// the cap's thickness, and the type, length and embedment of its piles. The
/// cap's outline follows: a rectangle around one, two, four and a line of
/// piles; for three, a triangle with its corners cut square; on a circle, a
/// round cap or a ring. A project without a pile type gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PileCapTypeCreate {
    /// Type name (default: from the number of piles, e.g. `Bloco 3 estacas`).
    #[serde(default)]
    pub name: Option<String>,
    /// `single`, `pair` (default), `triangle`, `square`, `line` or `circle`.
    #[serde(default)]
    pub layout: Option<String>,
    /// Number of piles of a `line` (2 to 64) or a `circle` (3 to 64).
    #[serde(default)]
    pub count: Option<u32>,
    /// Radius of a `circle` of piles (their axes), meters.
    #[serde(default)]
    pub radius: Option<f64>,
    /// Width of a ring cap centered on the `circle` of piles, meters (default: a round cap).
    #[serde(default)]
    pub ring_width: Option<f64>,
    /// Distance between neighboring piles' axes, meters (default: three pile sizes).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Distance from a pile's axis to the cap's edge, meters (default: one pile size).
    #[serde(default)]
    pub edge_distance: Option<f64>,
    /// Thickness of the cap, meters (default 0.8).
    #[serde(default)]
    pub thickness: Option<f64>,
    /// The piles' type id or name (default: the first pile type).
    #[serde(default)]
    pub pile_type: Option<Ref>,
    /// Length of each pile below its head, meters (default 12).
    #[serde(default)]
    pub pile_length: Option<f64>,
    /// How far each pile's head enters the cap, meters (default 0.1).
    #[serde(default)]
    pub embedment: Option<f64>,
    /// Material name (default: the first foundation type's, the template's concrete).
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for PileCapTypeCreate {
    const NAME: &'static str = "pile_cap_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let piles = pile_type(p, self.pile_type.as_ref())?;
        let size = piles.and_then(|t| section_of(p, t)).unwrap_or_default().size();
        let layout = layout_arg(self.layout.as_deref().unwrap_or("pair"), self.count, self.radius, self.ring_width)?;
        let cap = PileCap {
            layout,
            spacing: self.spacing.unwrap_or(3.0 * size),
            edge_distance: self.edge_distance.unwrap_or(size),
            pile_type: piles,
            pile_length: self.pile_length.unwrap_or(cc_core::pile::DEFAULT_PILE_LENGTH),
            embedment: self.embedment.unwrap_or(cc_core::pile::DEFAULT_EMBEDMENT),
        };
        let thickness = self.thickness.unwrap_or(cc_core::pile::DEFAULT_CAP_THICKNESS);
        check_cap(&cap, thickness, size)?;
        let material = match self.material {
            Some(m) => refs::material(p, &Ref::Name(m))?,
            None => concrete(p),
        };
        let name = self.name.filter(|n| !n.trim().is_empty());
        let (id, pile_type) = edit(s, t!("tx-create-pile-cap-type"), |e| {
            let pile_type = ensure_pile_type(e, piles);
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_cap_type(id, PileCap { pile_type: Some(pile_type), ..cap }, thickness, material);
            if let Some(name) = name {
                ty.name = name;
            }
            Ok((e.add_type(ty), pile_type))
        })?;
        Ok(json!({ "id": id.0, "pile_type": pile_type.0 }))
    }
}

/// The pile cap type named by `r` (which must be one), else the first pile
/// cap type; `None` when there is none (the caller makes one).
fn cap_type(p: &Project, r: Option<&Ref>) -> Result<Option<ElementTypeId>> {
    let is_cap = |id: ElementTypeId| p.element_type(id).is_some_and(|t| cc_core::pile::cap_of_type(&t.data).is_some());
    match r {
        Some(r) => {
            let id = refs::type_of(p, Category::Foundation, Some(r))?;
            if is_cap(id) {
                Ok(Some(id))
            } else {
                Err(EngineError::invalid(
                    t!("msg-foundation-kind", "kind" => crate::i18n::tr(FoundationKind::PileCap.key())),
                ))
            }
        }
        None => Ok(p.types_of(Category::Foundation).map(|t| t.id).find(|id| is_cap(*id))),
    }
}

/// The pile cap type `wanted`, or a new two-pile cap on the first pile type
/// (made too when there is none).
fn ensure_cap_type(e: &mut EditCtx, wanted: Option<ElementTypeId>) -> ElementTypeId {
    if let Some(t) = wanted {
        return t;
    }
    let existing = e.project().default_type(Category::Pile);
    let piles = ensure_pile_type(e, existing);
    let size = section_of(e.project(), piles).unwrap_or_default().size();
    let cap = PileCap { spacing: 3.0 * size, edge_distance: size, pile_type: Some(piles), ..PileCap::default() };
    let id = ElementTypeId(e.alloc_id());
    let material = concrete(e.project());
    e.add_type(new_cap_type(id, cap, cc_core::pile::DEFAULT_CAP_THICKNESS, material))
}

/// A pile cap's element kind.
fn cap_kind(position: DVec2, rotation_deg: f64, top_offset: f64) -> ElementKind {
    ElementKind::Foundation(Foundation::new(FoundationShape::Pad { position, rotation_deg }, top_offset))
}

/// Place a pile cap centered at `position`, turned by `rotation` (its
/// layout's `u` axis along it), its top `top_offset` above the level. Its
/// piles are made with it, in the same undo step, and follow it: they move,
/// turn and are copied and deleted with it, and change with its type. A
/// project without a pile cap type gets one (two piles).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PileCapCreate {
    /// Center of the cap (the origin of its layout) in plan.
    pub position: Point,
    /// Rotation in plan, degrees (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Pile cap type id or name (default: the first pile cap type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Top of the cap above the level, meters (default 0).
    #[serde(default)]
    pub top_offset: Option<f64>,
}

impl Command for PileCapCreate {
    const NAME: &'static str = "pile_cap_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let ty = cap_type(p, self.type_ref.as_ref())?;
        let kind = cap_kind(
            point(self.position)?,
            finite(self.rotation.unwrap_or(0.0), "rotation")?,
            finite(self.top_offset.unwrap_or(0.0), "top_offset")?,
        );
        let id = edit(s, t!("tx-create-pile-cap"), |e| {
            let type_id = ensure_cap_type(e, ty);
            Ok(add_element(e, level, type_id, kind))
        })?;
        Ok(created(s, id))
    }
}

/// How caps under columns are turned: as their column, by a fixed angle, or
/// pointing away from a center.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum CapRotation {
    /// A fixed angle, degrees counter-clockwise.
    Angle(f64),
    /// `column` (as each column turns) or `radial` (pointing away from `center`).
    Mode(String),
}

/// Place a pile cap under each column (default: every column of the level),
/// centered on it, its top at the column's base; its piles are made with it,
/// all in one undo step. `rotation` turns each cap: `column` (default) as its
/// column, an angle in degrees, or `radial`: its layout's `u` axis pointing
/// away from `center`, so caps under columns on a circle point outward (a
/// three-pile cap's first pile outside). Columns that already stand on a pad
/// footing or a pile cap are skipped.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PileCapsUnderColumns {
    /// Column ids (default: every column of `level`).
    #[serde(default)]
    pub columns: Vec<u64>,
    /// Level id or name, when no columns are given (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Pile cap type id or name (default: the first pile cap type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// `column` (default), `radial`, or an angle in degrees.
    #[serde(default)]
    pub rotation: Option<CapRotation>,
    /// The center radial caps point away from (default: the middle of the columns).
    #[serde(default)]
    pub center: Option<Point>,
}

impl Command for PileCapsUnderColumns {
    const NAME: &'static str = "pile_caps_under_columns";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let ty = cap_type(p, self.type_ref.as_ref())?;
        let ids: Vec<ElementId> = if self.columns.is_empty() {
            let level = refs::level_or_first(p, self.level.as_ref())?;
            p.elements.values().filter(|e| e.level == level && e.category() == Category::Column).map(|e| e.id).collect()
        } else {
            refs::elements(p, &self.columns)?
        };
        let mut columns = Vec::new();
        for id in ids {
            let Some(e) = p.element(id) else { continue };
            let ElementKind::Column(c) = &e.kind else {
                return Err(EngineError::invalid(format!("element {} is not a column", id.0)));
            };
            columns.push((e.level, c.clone()));
        }
        enum Turn {
            Column,
            Fixed(f64),
            Radial(DVec2),
        }
        let turn = match &self.rotation {
            None => Turn::Column,
            Some(CapRotation::Angle(a)) => Turn::Fixed(finite(*a, "rotation")?),
            Some(CapRotation::Mode(m)) if m.eq_ignore_ascii_case("column") => Turn::Column,
            Some(CapRotation::Mode(m)) if m.eq_ignore_ascii_case("radial") => {
                let center = match self.center {
                    Some(c) => point(c)?,
                    None if !columns.is_empty() => {
                        columns.iter().map(|(_, c)| c.position).sum::<DVec2>() / columns.len() as f64
                    }
                    None => DVec2::ZERO,
                };
                Turn::Radial(center)
            }
            Some(CapRotation::Mode(m)) => {
                return Err(EngineError::invalid(format!("unknown rotation '{m}' (column, radial or an angle)")));
            }
        };
        // Pads and pile caps already under a point of a level.
        let taken = |level: LevelId, at: DVec2| {
            p.elements.values().any(|e| {
                e.level == level
                    && matches!(&e.kind, ElementKind::Foundation(Foundation { shape: FoundationShape::Pad { position, .. }, .. })
                        if position.distance(at) < SAME)
            })
        };
        let (mut todo, mut skipped): (Vec<(LevelId, ElementKind)>, usize) = (Vec::new(), 0);
        for (level, c) in columns {
            let planned = todo.iter().any(|(l, k)| {
                *l == level && matches!(k, ElementKind::Foundation(f) if f.shape.points().first().is_some_and(|q| q.distance(c.position) < SAME))
            });
            if planned || taken(level, c.position) {
                skipped += 1;
                continue;
            }
            let rotation = match turn {
                Turn::Column => c.rotation_deg,
                Turn::Fixed(a) => a,
                Turn::Radial(center) => {
                    let d = c.position - center;
                    if d.length() < 1e-9 { 0.0 } else { d.y.atan2(d.x).to_degrees() }
                }
            };
            todo.push((level, cap_kind(c.position, rotation, c.base_offset)));
        }
        if todo.is_empty() {
            return Ok(json!({ "created": [], "piles": [], "skipped": skipped }));
        }
        let created = edit(s, t!("tx-create-pile-cap"), |e| {
            let type_id = ensure_cap_type(e, ty);
            Ok(todo.into_iter().map(|(level, kind)| add_element(e, level, type_id, kind)).collect::<Vec<_>>())
        })?;
        let p = s.project();
        let piles: Vec<ElementId> = created.iter().flat_map(|cap| p.piles_of(*cap).map(|(e, _)| e.id)).collect();
        Ok(json!({ "created": ids_json(&created), "piles": ids_json(&piles), "skipped": skipped }))
    }
}

/// Create tie beams between consecutive pile caps of `caps` (and from the
/// last back to the first when `closed`): straight, or arcs around `center`
/// (each between two caps at the same distance from it, the shorter way),
/// or of a signed `radius` (positive: counter-clockwise). Each beam runs
/// between the caps' centers and stops at their outlines where it frames
/// into their depth; its top is flush with the lower of its two caps' tops
/// unless `top_offset` is given. Tie beams are ordinary beams.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TieBeamsCreate {
    /// Pile cap ids, in order (at least two).
    pub caps: Vec<u64>,
    /// Also join the last cap to the first.
    #[serde(default)]
    pub closed: bool,
    /// Arc beams: the center of the circle the caps stand on.
    #[serde(default)]
    pub center: Option<Point>,
    /// Arc beams: the radius in meters, positive when the arcs turn counter-clockwise from cap to
    /// cap, negative clockwise (ignored with `center`).
    #[serde(default)]
    pub radius: Option<f64>,
    /// Beam type id or name (default: the first beam type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Level id or name of the beams (default: the first cap's level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Top of the beams above their level, meters (default: flush with the caps' tops).
    #[serde(default)]
    pub top_offset: Option<f64>,
}

impl Command for TieBeamsCreate {
    const NAME: &'static str = "tie_beams_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let ids = refs::elements(p, &self.caps)?;
        if ids.len() < 2 {
            return Err(EngineError::invalid("tie beams join at least two pile caps"));
        }
        // Each cap's center and top elevation.
        let mut caps = Vec::new();
        for id in &ids {
            let e = p.element(*id).ok_or_else(|| EngineError::not_found("element"))?;
            let c = p.pile_cap(e).ok_or_else(|| EngineError::invalid(format!("element {} is no pile cap", id.0)))?;
            caps.push((e.level, c.position, p.level_elevation(e.level) + c.top_offset));
        }
        let level = match &self.level {
            Some(l) => refs::level(p, l)?,
            None => caps.first().map(|c| c.0).ok_or_else(|| EngineError::invalid("no caps"))?,
        };
        let type_id = refs::type_of(p, Category::Beam, self.type_ref.as_ref())?;
        let center = self.center.map(point).transpose()?;
        let radius = self.radius.map(|r| finite(r, "radius")).transpose()?;
        let top_offset = self.top_offset.map(|t| finite(t, "top_offset")).transpose()?;
        let mut pairs: Vec<(usize, usize)> = (1..caps.len()).map(|i| (i - 1, i)).collect();
        if self.closed && caps.len() > 2 {
            pairs.push((caps.len() - 1, 0));
        }
        let level_z = p.level_elevation(level);
        let mut beams = Vec::new();
        for (i, j) in pairs {
            let ((_, a, za), (_, b, zb)) = (caps[i], caps[j]);
            if a.distance(b) < 1e-3 {
                return Err(EngineError::invalid("two consecutive caps stand at the same place"));
            }
            let bulge = match (center, radius) {
                (Some(c), _) => {
                    let (ra, rb) = (a.distance(c), b.distance(c));
                    if (ra - rb).abs() > SAME || ra < 1e-3 {
                        return Err(EngineError::invalid("the caps do not stand on one circle about the center"));
                    }
                    let sweep = (a - c).angle_to(b - c);
                    (sweep / 4.0).tan()
                }
                (None, Some(r)) => cc_core::arc::Arc::bulge_radius(a, b, r.abs(), r > 0.0, false).ok_or_else(|| {
                    EngineError::invalid("the radius must be at least half the distance between the caps")
                })?,
                (None, None) => 0.0,
            };
            let mut beam = Beam::new(a, b, top_offset.unwrap_or(za.min(zb) - level_z));
            beam.bulge = super::create::check_bulge(bulge)?;
            super::create::check_beam_arc(p, type_id, &beam)?;
            beams.push(beam);
        }
        let created = edit(s, t!("tx-create-tie-beams"), |e| {
            Ok(beams.into_iter().map(|b| add_element(e, level, type_id, ElementKind::Beam(b))).collect::<Vec<_>>())
        })?;
        Ok(json!({ "created": ids_json(&created) }))
    }
}

/// The derived values `element_get` shows for a pile or a pile cap.
pub(crate) fn derived(p: &Project, e: &cc_core::Element, out: &mut serde_json::Map<String, Value>) {
    let level = p.level_elevation(e.level);
    match &e.kind {
        ElementKind::Pile(pile) => {
            out.insert("length".into(), json!(pile.length()));
            out.insert("cut_off_elevation".into(), json!(level + pile.cut_off));
            out.insert("toe_elevation".into(), json!(level + pile.toe));
            if let Some(TypeData::Pile { section, kind, .. }) = p.element_type(e.type_id).map(|t| &t.data) {
                out.insert("size".into(), json!(section.size()));
                out.insert("section_area".into(), json!(section.area()));
                out.insert("kind".into(), json!(kind.code()));
            }
            if let Some(r) = pile.cap {
                out.insert("cap".into(), json!({ "id": r.cap.0, "index": r.index }));
            }
        }
        ElementKind::Foundation(_) => {
            let Some(c) = p.pile_cap(e) else { return };
            let piles: Vec<u64> = p.piles_of(e.id).map(|(x, _)| x.id.0).collect();
            out.insert("piles".into(), json!(piles));
            out.insert("pile_count".into(), json!(c.cap.layout.count()));
            let positions: Vec<[f64; 2]> = c.pile_positions().iter().map(|q| q.to_array()).collect();
            out.insert("pile_positions".into(), json!(positions));
            out.insert("top_elevation".into(), json!(level + c.top_offset));
            out.insert("bottom_elevation".into(), json!(level + c.top_offset - c.thickness));
            out.insert("area".into(), json!(c.cap.outline().area()));
        }
        _ => {}
    }
}

/// `element_set` params for a pile: `length` stands for the toe that far
/// below the head; a pile cap's piles follow their cap and take no params.
pub(crate) fn normalize_patch(e: &cc_core::Element, patch: &Value) -> Result<Value> {
    let ElementKind::Pile(pile) = &e.kind else { return Ok(patch.clone()) };
    if pile.cap.is_some() {
        return Err(EngineError::rejected("the piles of a pile cap follow their cap: change the cap or its type"));
    }
    let Value::Object(fields) = patch else { return Ok(patch.clone()) };
    let mut fields = fields.clone();
    if let Some(length) = fields.remove("length") {
        let length = length.as_f64().ok_or_else(|| EngineError::invalid("length must be a number"))?;
        let cut_off = match fields.get("cut_off") {
            Some(v) => v.as_f64().ok_or_else(|| EngineError::invalid("cut_off must be a number"))?,
            None => pile.cut_off,
        };
        if fields.contains_key("toe") {
            return Err(EngineError::invalid("give toe or length, not both"));
        }
        fields.insert("toe".into(), json!(toe_arg(cut_off, None, Some(length))?));
    }
    Ok(Value::Object(fields))
}

/// Check a pile changed by `element_set`: a length within the limits.
pub(crate) fn check(pile: &Pile) -> Result<()> {
    if !pile.is_valid() {
        return Err(EngineError::invalid("the pile's numbers must be finite, within 10 km of the level"));
    }
    if !cc_core::pile::length_ok(pile.length()) {
        return Err(EngineError::invalid(format!(
            "the pile's length must lie between {} and {} m",
            cc_core::pile::MIN_PILE_LENGTH,
            cc_core::pile::MAX_PILE_LENGTH
        )));
    }
    if !(0.0..=cc_core::pile::MAX_EMBEDMENT).contains(&pile.embedment) {
        return Err(EngineError::invalid("the embedment must lie between 0 and 5 m"));
    }
    Ok(())
}

/// Element ids of piles in `ids` that belong to a cap not in `ids` (they go with their cap only).
pub(crate) fn owned_piles(p: &Project, ids: &[ElementId]) -> Vec<ElementId> {
    ids.iter()
        .copied()
        .filter(|id| {
            matches!(p.element(*id).map(|e| &e.kind),
                Some(ElementKind::Pile(Pile { cap: Some(c), .. })) if !ids.contains(&c.cap) && p.element(c.cap).is_some())
        })
        .collect()
}
