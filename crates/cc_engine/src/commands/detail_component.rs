//! Detail components: reusable 2D components placed in one view (see
//! [`cc_core::detail_component`]). `detail_component_create` makes a type
//! from 2D pieces or from detail lines, regions, text notes and detail
//! components of a view; `detail_component_place` places one. Pieces are
//! added, changed and deleted with the `component_sheet_piece_*` commands,
//! parameters with `component_param_*`, and placed elements move, rotate,
//! mirror, copy and delete with the generic commands; `element_set`
//! changes `position`, `rotation_deg`, `scale`, `mirrored` and `overrides`.
//! `cad_import` with `blocks` brings a DWG or DXF file's blocks in as
//! detail components (see [`cad_blocks`]).

use super::sheets::{SheetPieceArg, check_nested_in};
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::detail_component::{self as dc, DetailComponent, Place, ring_stroke, stroke_of};
use cc_core::profile::{self, Segment};
use cc_core::sketch::SketchCurve;
use cc_core::title_block::{MAX_SHEET_PIECES, MAX_STROKE_POINTS, SheetItem, SheetShape, TextAnchor};
use cc_core::transform::PlanTransform;
use cc_core::{
    ComponentDef, ComponentKind, ElementId, ElementKind, ElementType, ElementTypeId, Pattern, Project, TypeData, ViewId,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The view (and its level) detail components go in: a plan, ceiling plan,
/// elevation, section, legend or drafting view.
fn detail_view(p: &Project, r: &Ref) -> Result<(ViewId, cc_core::LevelId)> {
    let id = refs::view(p, r)?;
    let view = p.view(id).ok_or_else(|| EngineError::not_found("view"))?;
    if !cc_views::hosts_cad(&view.kind) {
        return Err(EngineError::invalid(
            "detail components go in plans, ceiling plans, elevations, sections, legends and drafting views",
        ));
    }
    let level =
        view.level().or_else(|| p.levels.first().map(|l| l.id)).ok_or_else(|| EngineError::rejected("no levels"))?;
    Ok((id, level))
}

/// A detail component type by id or name, with its component name.
fn detail_type(p: &Project, r: &Ref) -> Result<(ElementTypeId, String)> {
    let id = refs::element_type(p, r)?;
    match p.element_type(id).map(|t| &t.data) {
        Some(TypeData::Component { component, def }) if def.kind == ComponentKind::Detail => {
            Ok((id, component.clone()))
        }
        _ => Err(EngineError::invalid("not a detail component type")),
    }
}

/// `name`, refused when a type or a component already has it.
fn new_name(p: &Project, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EngineError::invalid("give the component a name"));
    }
    if p.types.values().any(|t| t.name.trim() == name) {
        return Err(EngineError::invalid(format!("a type is already named '{name}'")));
    }
    if p.types.values().any(|t| matches!(&t.data, TypeData::Component { component, .. } if component.trim() == name)) {
        return Err(EngineError::invalid(format!("a component is already named '{name}'")));
    }
    Ok(name.to_string())
}

fn num(v: f64) -> String {
    format!("{v}")
}

/// A stroke `shape` drawn with a pen.
fn with_pen(shape: SheetShape, weight: f64, color: [u8; 3], pattern: cc_core::LinePattern) -> SheetShape {
    match shape {
        SheetShape::Stroke { points, closed, fill, hatch, .. } => SheetShape::Stroke {
            points,
            closed,
            weight: num(weight),
            color: (color != [0, 0, 0]).then_some(color),
            fill,
            pattern,
            hatch,
        },
        other => other,
    }
}

/// A closed stroke `shape` with a solid fill or a pattern fill.
fn with_fill(shape: SheetShape, solid: Option<[u8; 3]>, pattern: Option<cc_core::PatternFill>) -> SheetShape {
    match shape {
        SheetShape::Stroke { points, closed, weight, color, pattern: dashes, .. } => {
            SheetShape::Stroke { points, closed, weight, color, fill: solid, pattern: dashes, hatch: pattern }
        }
        other => other,
    }
}

/// Every point an element of a view is drawn through (for the extents).
fn element_points(p: &Project, e: &cc_core::Element) -> Vec<DVec2> {
    match &e.kind {
        ElementKind::DetailLine(l) => l.curve.points(),
        ElementKind::FilledRegion(r) | ElementKind::MaskingRegion(r) => r.outline.clone(),
        ElementKind::Text(t) => vec![t.position],
        ElementKind::DetailComponent(c) => {
            let mut out = vec![c.position];
            if let Some((name, def)) = cc_views::detail_component::def_of(p, e) {
                let mut d = cc_views::Drawing2D::default();
                cc_views::detail_component::draw(&mut d, p, name, def, &c.overrides, c.place(), 100, None);
                if let Some((a, b)) = d.fit_bounds() {
                    out.extend([a, b]);
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// The 2D pieces of the elements `ids` of `view`, in a frame whose origin is
/// `origin` (view coordinates): detail lines in their style's pen, filled
/// and masking regions as closed strokes with their fill (openings as
/// strokes of their own), text notes as texts (their height in meters at
/// the view's scale, leaders as lines) and detail components as nested
/// pieces.
fn pieces_of(p: &Project, view: ViewId, ids: &[ElementId], origin: DVec2) -> Result<Vec<SheetItem>> {
    let scale = p.view(view).map_or(100, |v| v.scale);
    let mut out: Vec<SheetItem> = Vec::new();
    let mut push = |name: String, shape: SheetShape| out.push(SheetItem { name, shape });
    let thin = cc_core::DetailStyle::Thin.weight_mm();
    for &id in ids {
        let e = p.element(id).ok_or_else(|| EngineError::not_found("element"))?;
        if e.owner_view() != Some(view) {
            return Err(EngineError::invalid(format!("element {} is not in the view", id.0)));
        }
        let name = |what: &str| format!("{what} {}", id.0);
        match &e.kind {
            ElementKind::DetailLine(l) => {
                let mut curve = l.curve;
                curve.transform(&PlanTransform::translate(-origin));
                let (w, color, pattern) = l.style.pen(&p.line_styles);
                if let Some(s) = stroke_of(&curve) {
                    push(name("Line"), with_pen(s, w, color, pattern));
                }
            }
            ElementKind::FilledRegion(r) | ElementKind::MaskingRegion(r) => {
                let masking = matches!(e.kind, ElementKind::MaskingRegion(_));
                let (solid, hatch) = if masking {
                    (Some([255, 255, 255]), None)
                } else {
                    let fill = match p.element_type(e.type_id).map(|t| &t.data) {
                        Some(TypeData::FilledRegion { fill }) => *fill,
                        _ => cc_core::PatternFill::default(),
                    };
                    match (fill.foreground, fill.background) {
                        (Pattern::Solid, _) => (Some(fill.foreground_color), None),
                        (_, Pattern::Solid) if fill.foreground == Pattern::None => (Some(fill.background_color), None),
                        _ => (None, Some(fill)),
                    }
                };
                // A region drawn with no boundary keeps its edge in its fill's color.
                let (w, color, pattern) = match r.boundary {
                    Some(b) => b.pen(&p.line_styles),
                    None => {
                        (thin, solid.or(hatch.map(|h| h.foreground_color)).unwrap_or([0, 0, 0]), Default::default())
                    }
                };
                let ring: Vec<DVec2> = r.outline.iter().map(|q| *q - origin).collect();
                if let Some(s) = ring_stroke(&ring) {
                    push(name("Region"), with_fill(with_pen(s, w, color, pattern), solid, hatch));
                }
                for o in &r.openings {
                    let ring: Vec<DVec2> = o.iter().map(|q| *q - origin).collect();
                    if let Some(s) = ring_stroke(&ring) {
                        push(name("Opening"), with_pen(s, w, color, pattern));
                    }
                }
            }
            ElementKind::Text(note) => {
                let size_mm = match p.element_type(e.type_id).map(|t| &t.data) {
                    Some(TypeData::Text { size, .. }) => *size,
                    _ => 2.5,
                };
                let size = size_mm * f64::from(scale.max(1)) / 1000.0;
                // The note hangs from its top left corner; the piece sits on its first baseline.
                let up = DVec2::from_angle(note.rotation_deg.to_radians()).perp();
                let at = note.position - up * size - origin;
                push(
                    name("Text"),
                    SheetShape::Text {
                        x: num(at.x),
                        y: num(at.y),
                        text: note.text.chars().take(cc_core::title_block::MAX_SHEET_TEXT).collect(),
                        size: num(size),
                        anchor: TextAnchor::Left,
                        rotation: if note.rotation_deg == 0.0 { String::new() } else { num(note.rotation_deg) },
                    },
                );
                for l in &note.leaders {
                    let pts: Vec<DVec2> =
                        std::iter::once(note.position).chain(l.elbow).chain([l.end]).map(|q| q - origin).collect();
                    let vertices: Vec<(DVec2, Segment)> = pts.iter().map(|q| (*q, Segment::Line)).collect();
                    push(
                        name("Leader"),
                        with_pen(
                            SheetShape::stroke(profile::written(&vertices), false),
                            thin,
                            [0; 3],
                            Default::default(),
                        ),
                    );
                }
            }
            ElementKind::DetailComponent(c) => {
                let Some((component, _)) = cc_views::detail_component::def_of(p, e) else { continue };
                let at = c.position - origin;
                push(
                    name("Component"),
                    SheetShape::Nested {
                        component: component.to_string(),
                        x: num(at.x),
                        y: num(at.y),
                        rotation: if c.rotation_deg == 0.0 { String::new() } else { num(c.rotation_deg) },
                        scale: if c.scale == 1.0 { String::new() } else { num(c.scale) },
                        mirrored: c.mirrored,
                    },
                );
            }
            _ => {
                return Err(EngineError::invalid(format!(
                    "element {} is not a detail line, region, text note or detail component",
                    id.0
                )));
            }
        }
    }
    if out.len() > MAX_SHEET_PIECES {
        return Err(EngineError::invalid(format!("a detail component holds at most {MAX_SHEET_PIECES} pieces")));
    }
    Ok(out)
}

/// Make a detail component (a reusable 2D component for construction
/// details) as a new type named `name`, in one undo step: from `pieces`
/// (meters in its frame), or from `elements` of `view` (detail lines,
/// filled and masking regions, text notes and other detail components),
/// whose point `origin` (default: the lower left corner of their extents)
/// becomes the component's origin. With `replace` (the default when
/// elements are given) those elements are deleted and the new component is
/// placed where they were. Detail lines keep their pen, regions their fill
/// (solid or pattern; openings as outlines), text notes their text at its
/// printed height, and detail components become nested pieces.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetailComponentCreate {
    /// Name of the component and of its type.
    pub name: String,
    /// Pieces, as `component_sheet_piece_add` takes them (meters).
    #[serde(default)]
    pub pieces: Vec<SheetPieceArg>,
    /// View id or name of `elements`.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Ids of the view's elements to make the component from.
    #[serde(default)]
    pub elements: Vec<u64>,
    /// The point of the view that becomes the component's origin.
    #[serde(default)]
    pub origin: Option<Point>,
    /// Replace `elements` by the new component (default true).
    #[serde(default)]
    pub replace: Option<bool>,
    /// Description of the type.
    #[serde(default)]
    pub description: Option<String>,
}

impl Command for DetailComponentCreate {
    const NAME: &'static str = "detail_component_create";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = new_name(p, &self.name)?;
        let mut pieces: Vec<SheetItem> = Vec::new();
        for (i, arg) in self.pieces.iter().enumerate() {
            pieces.push(SheetItem { name: format!("Piece{}", i + 1), shape: arg.shape(s)? });
        }
        let ids: Vec<ElementId> = self.elements.iter().map(|i| ElementId(*i)).collect();
        let mut placed: Option<(ViewId, cc_core::LevelId, DVec2)> = None;
        if !ids.is_empty() {
            let view_ref = self.view.as_ref().ok_or_else(|| EngineError::invalid("give the `view` of the elements"))?;
            let (view, level) = detail_view(p, view_ref)?;
            let origin = match self.origin {
                Some(o) => refs::point(o)?,
                None => {
                    let pts: Vec<DVec2> =
                        ids.iter().filter_map(|i| p.element(*i)).flat_map(|e| element_points(p, e)).collect();
                    pts.iter().copied().reduce(DVec2::min).unwrap_or(DVec2::ZERO)
                }
            };
            pieces.extend(pieces_of(p, view, &ids, origin)?);
            if self.replace.unwrap_or(true) {
                placed = Some((view, level, origin));
            }
        }
        if pieces.is_empty() {
            return Err(EngineError::invalid("give `pieces`, or the `view` and `elements` to make it from"));
        }
        let shapes: Vec<SheetShape> = pieces.iter().map(|i| i.shape.clone()).collect();
        check_nested_in(p, &name, &shapes)?;
        let def = dc::def_of(pieces);
        super::types::validate(&def)?;
        let description = self.description.unwrap_or_default();
        let (type_id, element) = edit(s, t!("tx-new-detail-component"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let info = cc_core::TypeInfo { description, ..Default::default() };
            let data = TypeData::Component { component: name.clone(), def };
            let type_id = e.add_type(ElementType { id, name: name.clone(), data, info, uid: Default::default() });
            let element = match placed {
                Some((view, level, origin)) => {
                    for id in &ids {
                        e.remove_element(*id);
                    }
                    let kind = ElementKind::DetailComponent(Box::new(DetailComponent::new(view, origin)));
                    Some(add_element(e, level, type_id, kind))
                }
                None => None,
            };
            Ok((type_id, element))
        })?;
        let mut out = super::types::type_out(s, type_id);
        if let (Some(o), Some(id)) = (out.as_object_mut(), element) {
            o.insert("placed".into(), json!(id.0));
        }
        Ok(out)
    }
}

/// Place a detail component in a 2D view (plan, ceiling plan, elevation,
/// section, legend or drafting view): its origin at `position` (view
/// coordinates, meters), turned `rotation` degrees, sized by `scale` and
/// flipped across its own X axis first when `mirrored`. It is drawn only in
/// that view, over the model and its regions, and goes with the view.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetailComponentPlace {
    /// A detail component type: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// View id or name.
    pub view: Ref,
    /// Where its origin goes, [x, y] in meters.
    pub position: Point,
    /// Degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Size factor (default 1).
    #[serde(default)]
    pub scale: Option<f64>,
    /// Flipped across its own X axis before turning (default false).
    #[serde(default)]
    pub mirrored: Option<bool>,
}

impl Command for DetailComponentPlace {
    const NAME: &'static str = "detail_component_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (type_id, _) = detail_type(p, &self.type_ref)?;
        let (view, level) = detail_view(p, &self.view)?;
        let mut c = DetailComponent::new(view, refs::point(self.position)?);
        c.rotation_deg = refs::finite(self.rotation.unwrap_or(0.0), "rotation")?;
        c.scale = self.scale.unwrap_or(1.0);
        c.mirrored = self.mirrored.unwrap_or(false);
        if !dc::scale_ok(c.scale) {
            return Err(EngineError::invalid(format!("the scale is between {} and {}", dc::MIN_SCALE, dc::MAX_SCALE)));
        }
        if !c.is_valid() {
            return Err(EngineError::invalid("the position is out of bounds"));
        }
        let id = edit(s, t!("tx-place-detail-component"), |e| {
            Ok(add_element(e, level, type_id, ElementKind::DetailComponent(Box::new(c))))
        })?;
        Ok(super::created(s, id))
    }
}

/// Check a patched detail component: the view stays, the placement is valid.
pub(crate) fn check(old: &DetailComponent, new: &DetailComponent) -> Result<()> {
    if new.view != old.view {
        return Err(EngineError::invalid("the view of a detail component cannot be changed"));
    }
    if !new.is_valid() {
        return Err(EngineError::invalid(format!(
            "position and rotation must be finite and the scale between {} and {}",
            dc::MIN_SCALE,
            dc::MAX_SCALE
        )));
    }
    Ok(())
}

/// Derived values of a placed detail component for `element_get`.
pub(crate) fn derived(p: &Project, e: &cc_core::Element, out: &mut serde_json::Map<String, Value>) {
    if let Some((component, def)) = cc_views::detail_component::def_of(p, e) {
        out.insert("component".into(), json!(component));
        out.insert("pieces".into(), json!(def.sheet.len()));
        out.insert("nested".into(), json!(dc::nested_names(def).collect::<Vec<_>>()));
    }
}

/// What [`cad_blocks`] made.
#[derive(Debug, Default)]
pub(crate) struct CadBlocks {
    /// One type per block: its component name (and type name) and definition.
    pub types: Vec<(String, ComponentDef)>,
    /// Pieces left out past the limits.
    pub dropped: usize,
}

/// The color a primitive of an imported drawing is drawn in (its own, else
/// its layer's), darkened for white paper when it is light.
fn prim_color(layers: &[cc_core::CadLayer], prim: &cc_core::CadPrim) -> [u8; 3] {
    let rgb = prim.color().or_else(|| layers.get(prim.layer() as usize).map(|l| l.color)).unwrap_or([0, 0, 0]);
    let luma = 0.299 * f64::from(rgb[0]) + 0.587 * f64::from(rgb[1]) + 0.114 * f64::from(rgb[2]);
    if luma > 190.0 { rgb.map(|v| (f64::from(v) * 0.55).round() as u8) } else { rgb }
}

/// The pieces of one primitive of an imported block, scaled by `f` (drawing
/// units to meters).
fn prim_pieces(prim: &cc_core::CadPrim, f: f64, color: [u8; 3], out: &mut Vec<SheetShape>) {
    use cc_core::cad::{CadHAlign, CadPrim, CadVAlign};
    let pen = |s: SheetShape| with_pen(s, cc_core::DetailStyle::Thin.weight_mm(), color, Default::default());
    match prim {
        CadPrim::Polyline { pts, bulges, closed, .. } => {
            let n = pts.len();
            if n < 2 || (n == 2 && pts[0].distance_squared(pts[1]) < 1e-18) {
                return;
            }
            let segments = if *closed { n } else { n - 1 };
            let mut v: Vec<(DVec2, Segment)> = Vec::with_capacity(n + 2);
            for i in 0..segments {
                let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
                let bulge = bulges.get(i).copied().unwrap_or(0.0);
                match cc_core::arc::Arc::from_bulge(*a * f, *b * f, bulge) {
                    Some(arc) => {
                        let radius = arc.radius * arc.sign();
                        v.push((*a * f, Segment::Arc { radius }));
                        // A radius draws the shorter arc: a longer one goes in two halves.
                        if arc.sweep.abs() > std::f64::consts::PI + 1e-9 {
                            v.push((arc.point(arc.length() / 2.0), Segment::Arc { radius }));
                        }
                    }
                    None => v.push((*a * f, Segment::Line)),
                }
            }
            let mut closed = *closed;
            if !closed {
                v.push((pts[n - 1] * f, Segment::Line));
            } else if v.len() > MAX_STROKE_POINTS {
                // Too long for one stroke: open it and cut it in runs.
                v.push((pts[0] * f, Segment::Line));
                closed = false;
            }
            if v.len() <= MAX_STROKE_POINTS {
                out.push(pen(SheetShape::stroke(profile::written(&v), closed)));
                return;
            }
            let mut start = 0;
            while start + 1 < v.len() {
                let end = (start + MAX_STROKE_POINTS).min(v.len());
                let mut run = v[start..end].to_vec();
                if let Some(last) = run.last_mut() {
                    last.1 = Segment::Line;
                }
                out.push(pen(SheetShape::stroke(profile::written(&run), false)));
                start = end - 1;
            }
        }
        CadPrim::Arc { center, radius, start, sweep, .. } => {
            let (c, r) = (*center * f, radius * f);
            let curve = if sweep.abs() >= std::f64::consts::TAU - 1e-9 {
                SketchCurve::Circle { center: c, radius: r }
            } else {
                let a = c + DVec2::from_angle(*start) * r;
                let b = c + DVec2::from_angle(start + sweep) * r;
                SketchCurve::Arc { a, b, bulge: (sweep / 4.0).tan() }
            };
            if let Some(s) = stroke_of(&curve) {
                out.push(pen(s));
            }
        }
        CadPrim::Fill { pts, .. } => {
            let ring: Vec<DVec2> = pts.iter().take(MAX_STROKE_POINTS).map(|q| *q * f).collect();
            if let Some(s) = ring_stroke(&ring) {
                out.push(with_fill(pen(s), Some(color), None));
            }
        }
        CadPrim::Text { pos, height, rotation, text, h, v, .. } => {
            let size = height * f;
            if !(size.is_finite() && size > 0.0) || text.trim().is_empty() {
                return;
            }
            let up = DVec2::from_angle(*rotation).perp();
            let (anchor, drop) = match (h, v) {
                (CadHAlign::Center, CadVAlign::Middle) => (TextAnchor::Middle, 0.0),
                (h, v) => {
                    let anchor = match h {
                        CadHAlign::Left => TextAnchor::Left,
                        CadHAlign::Center => TextAnchor::Center,
                        CadHAlign::Right => TextAnchor::Right,
                    };
                    let drop = match v {
                        CadVAlign::Baseline | CadVAlign::Bottom => 0.0,
                        CadVAlign::Middle => 0.5,
                        CadVAlign::Top => 1.0,
                    };
                    (anchor, drop)
                }
            };
            let at = *pos * f - up * size * drop;
            let deg = rotation.to_degrees();
            out.push(SheetShape::Text {
                x: num(at.x),
                y: num(at.y),
                text: text.chars().take(cc_core::title_block::MAX_SHEET_TEXT).collect(),
                size: num(size),
                anchor,
                rotation: if deg.abs() < 1e-12 { String::new() } else { num(deg) },
            });
        }
    }
}

/// The kept blocks of a DWG or DXF read as detail component definitions:
/// each block's primitives (on visible layers) scaled to meters by `f`, in
/// its color (its own or its layer's), and its inserts of other kept
/// blocks as nested pieces; `names` gives each block's component name.
pub(crate) fn cad_blocks(
    read: &cc_cad::CadRead,
    layers: &[cc_core::CadLayer],
    f: f64,
    names: &std::collections::BTreeMap<String, String>,
) -> CadBlocks {
    let mut out = CadBlocks::default();
    let visible = |layer: u32| layers.get(layer as usize).is_none_or(|l| l.visible);
    for block in &read.kept {
        let Some(name) = names.get(&block.name) else { continue };
        let mut shapes = Vec::new();
        for prim in block.prims.iter().filter(|p| visible(p.layer())) {
            prim_pieces(prim, f, prim_color(layers, prim), &mut shapes);
        }
        for i in block.inserts.iter().filter(|i| visible(i.layer)) {
            let Some(inner) = names.get(&i.block) else { continue };
            let at = i.position * f;
            let deg = i.rotation.to_degrees();
            shapes.push(SheetShape::Nested {
                component: inner.clone(),
                x: num(at.x),
                y: num(at.y),
                rotation: if deg.abs() < 1e-12 { String::new() } else { num(deg) },
                scale: if (i.scale - 1.0).abs() < 1e-12 { String::new() } else { num(i.scale) },
                mirrored: i.mirrored,
            });
        }
        if shapes.len() > MAX_SHEET_PIECES {
            out.dropped += shapes.len() - MAX_SHEET_PIECES;
            shapes.truncate(MAX_SHEET_PIECES);
        }
        if shapes.is_empty() {
            // An empty block still names a component: keep a point-sized mark out.
            continue;
        }
        let pieces = shapes
            .into_iter()
            .enumerate()
            .map(|(k, shape)| SheetItem { name: format!("Piece{}", k + 1), shape })
            .collect();
        out.types.push((name.clone(), dc::def_of(pieces)));
    }
    // Nested pieces naming a block that made no type are left out.
    let made: std::collections::BTreeSet<String> = out.types.iter().map(|(n, _)| n.clone()).collect();
    for (_, def) in &mut out.types {
        def.sheet.retain(|i| match &i.shape {
            SheetShape::Nested { component, .. } => made.contains(component),
            _ => true,
        });
    }
    out.types.retain(|(_, def)| !def.sheet.is_empty());
    out
}

/// Where an insert of a kept block lands in the view, for an import whose
/// drawing units are `f` meters and whose placement is `import`.
pub(crate) fn insert_place(i: &cc_cad::CadInsert, f: f64, import: &PlanTransform) -> Place {
    let local = Place {
        t: PlanTransform { angle: i.rotation, mirror: i.mirrored, translation: i.position * f },
        scale: i.scale,
    };
    local.then(&Place { t: *import, scale: 1.0 })
}
