//! Detail components in the view they belong to (see
//! [`cc_core::detail_component`]), in id order over the model and the
//! view's regions, under its detail lines and annotations: each one's
//! pieces and those of the components it nests, through its placement
//! (position, rotation, mirror, scale). Pictures first, then strokes (a
//! closed one's solid or pattern fill under its lines), then texts, as
//! title blocks draw theirs; lengths are meters, pen weights paper
//! millimeters, text heights meters turned into paper millimeters by the
//! view's scale.

use crate::drawing::{Anchor, Drawing2D, LineStyle, Pen, Prim};
use cc_core::detail_component::{Place, walk};
use cc_core::region::Region;
use cc_core::title_block::{SheetPieces, TextAnchor};
use cc_core::{ComponentDef, ElementId, ElementKind, Project, TypeData, ViewId};
use glam::DVec2;

/// The definition and component name of an element's type, if it is a
/// detail component.
pub fn def_of<'a>(project: &'a Project, e: &cc_core::Element) -> Option<(&'a str, &'a ComponentDef)> {
    match project.element_type(e.type_id).map(|t| &t.data) {
        Some(TypeData::Component { component, def }) if def.kind == cc_core::ComponentKind::Detail => {
            Some((component.as_str(), def))
        }
        _ => None,
    }
}

/// The detail components of `view`, at `1:scale`.
pub fn components(d: &mut Drawing2D, project: &Project, view: ViewId, scale: u32) {
    for e in project.elements.values() {
        let ElementKind::DetailComponent(c) = &e.kind else { continue };
        if c.view != view || !c.is_valid() {
            continue;
        }
        let Some((name, def)) = def_of(project, e) else { continue };
        draw(d, project, name, def, &c.overrides, c.place(), scale, Some(e.id));
    }
}

/// Draw the component `name` (`def`, with `overrides`) and what it nests
/// through `place`; returns whether nothing was left out.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    d: &mut Drawing2D,
    project: &Project,
    name: &str,
    def: &ComponentDef,
    overrides: &std::collections::BTreeMap<String, f64>,
    place: Place,
    scale: u32,
    id: Option<ElementId>,
) -> bool {
    let lookup = |n: &str| project.detail_def(n);
    walk(&lookup, name, def, overrides, place, &mut |v| {
        draw_pieces(d, &project.fill_patterns, v.pieces, &v.place, scale, id)
    })
}

/// Draw one resolution's own pieces (not what it nests) through `place`.
pub fn draw_pieces(
    d: &mut Drawing2D,
    patterns: &cc_core::fill_pattern::FillPatterns,
    pieces: &SheetPieces,
    place: &Place,
    scale: u32,
    id: Option<ElementId>,
) {
    let to = |p: DVec2| place.apply(p);
    for img in &pieces.images {
        let origin = to(img.corner);
        d.prims.push(Prim::Image {
            origin,
            u: place.apply_vector(DVec2::new(img.size.x, 0.0)),
            v: place.apply_vector(DVec2::new(0.0, img.size.y)),
            image: img.image.clone(),
            element: id,
            faded: false,
        });
    }
    for s in &pieces.strokes {
        let pts: Vec<DVec2> = s.points.iter().map(|p| to(*p)).collect();
        if let Some([r, g, b]) = s.fill {
            d.fill(pts.clone(), [r, g, b, 255], id);
        }
        if let Some(fill) = &s.hatch
            && pts.len() >= 3
        {
            let region =
                Region { view: ViewId(0), outline: pts.clone(), openings: Vec::new(), sketch: None, boundary: None };
            if region.is_valid() {
                crate::region::draw_filled(d, &region, fill, &Default::default(), patterns, scale, id);
            }
        }
        let pen = Some(Pen { color: Some(s.color), weight: Some(s.weight), pattern: Some(s.pattern), halftone: false });
        let n = pts.len();
        let segments = if s.closed && n > 2 { n } else { n.saturating_sub(1) };
        for k in 0..segments {
            let (Some(&a), Some(&b)) = (pts.get(k), pts.get((k + 1) % n)) else { continue };
            if a.distance_squared(b) > 1e-14 {
                d.prims.push(Prim::Line { a, b, style: LineStyle::Projection, element: id, pen });
            }
        }
    }
    let paper = 1000.0 / f64::from(scale.max(1));
    for t in &pieces.texts {
        let angle = place.apply_angle(t.rotation_deg.to_radians());
        let size = t.size * place.scale * paper;
        if !(size.is_finite() && size > 0.0) {
            continue;
        }
        let (anchor, mid) = match t.anchor {
            TextAnchor::Left => (Anchor::LeftBottom, false),
            TextAnchor::Center => (Anchor::CenterBottom, false),
            TextAnchor::Right => (Anchor::RightBottom, false),
            TextAnchor::Middle => (Anchor::Center, true),
        };
        let lines: Vec<&str> = t.text.lines().collect();
        // One line down is 1.5 text heights, across the text's direction (meters).
        let step = DVec2::from_angle(angle).perp() * (-1.5 * t.size * place.scale);
        let first = if mid { -step * ((lines.len() as f64 - 1.0) / 2.0) } else { DVec2::ZERO };
        let at = to(t.position);
        for (i, line) in lines.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
            d.tag_text(at + first + step * i as f64, *line, size, anchor, angle, id);
        }
    }
}

/// Most blocks one DXF export defines.
const MAX_BLOCKS: usize = 2000;

/// The detail components of `view` that `drawing` shows, as DXF blocks:
/// one block per component (an element with its own parameter values gets
/// its own), drawn at `1:scale` with what it nests as inserts of further
/// blocks (a nested piece that would close a cycle is left out), and an
/// insert per element on the layer `layer` names.
pub fn dxf_blocks(
    project: &Project,
    view: ViewId,
    drawing: &Drawing2D,
    scale: u32,
    layer: &dyn Fn(&cc_core::Element) -> String,
) -> crate::dxf::DxfBlocks {
    use crate::dxf::{DxfBlock, DxfBlocks, DxfInsert, layer_name};
    use std::collections::{BTreeMap, BTreeSet};
    let shown: BTreeSet<ElementId> = drawing.prims.iter().filter_map(Prim::element).collect();
    let mut out = DxfBlocks::default();
    // Block names by key (the component, or the element with overrides), unique once sanitized.
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut name_of = |key: &str, base: &str| -> String {
        if let Some(n) = names.get(key) {
            return n.clone();
        }
        let stem = layer_name(base);
        let mut name = stem.clone();
        let mut k = 2;
        while taken.contains(&name) {
            let suffix = format!("_{k}");
            name = format!("{}{suffix}", stem.chars().take(31 - suffix.len()).collect::<String>());
            k += 1;
        }
        taken.insert(name.clone());
        names.insert(key.to_string(), name.clone());
        name
    };
    let insert = |block: String, place: (DVec2, f64, f64, bool), layer: String| DxfInsert {
        block,
        position: place.0,
        rotation_deg: place.1,
        scale: DVec2::new(place.2, if place.3 { -place.2 } else { place.2 }),
        layer,
    };
    // Blocks to define: (key, component, overrides).
    let mut pending: Vec<(String, String, BTreeMap<String, f64>)> = Vec::new();
    for e in project.elements.values() {
        let ElementKind::DetailComponent(c) = &e.kind else { continue };
        if c.view != view || !c.is_valid() || !shown.contains(&e.id) {
            continue;
        }
        let Some((component, _)) = def_of(project, e) else { continue };
        let key = if c.overrides.is_empty() { component.to_string() } else { format!("{component}#{}", e.id.0) };
        let base = if c.overrides.is_empty() { component.to_string() } else { format!("{component}_{}", e.id.0) };
        let block = name_of(&key, &base);
        out.inserts.push(insert(block, (c.position, c.rotation_deg, c.scale, c.mirrored), layer(e)));
        out.skip.insert(e.id);
        pending.push((key, component.to_string(), c.overrides.clone()));
    }
    let mut defined: BTreeSet<String> = BTreeSet::new();
    while let Some((key, component, overrides)) = pending.pop() {
        if out.blocks.len() >= MAX_BLOCKS || !defined.insert(key.clone()) {
            continue;
        }
        let Some(def) = project.detail_def(&component) else { continue };
        let scope = def.scope(&overrides, &[]);
        let pieces = def.resolve_2d(&|n: &str| scope.get(n));
        let mut d = Drawing2D::default();
        draw_pieces(&mut d, &project.fill_patterns, &pieces, &Place::default(), scale, None);
        let mut block = DxfBlock { name: name_of(&key, &component), prims: d.prims, inserts: Vec::new() };
        for n in &pieces.nested {
            if project.detail_def(&n.component).is_none() || project.detail_nests(&n.component, &component) {
                continue;
            }
            let inner = name_of(&n.component, &n.component);
            block.inserts.push(insert(inner, (n.position, n.rotation_deg, n.scale, n.mirrored), "0".into()));
            pending.push((n.component.clone(), n.component.clone(), BTreeMap::new()));
        }
        out.blocks.push(block);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::detail_component::{DetailComponent, def_of as new_def, stroke_of};
    use cc_core::sketch::SketchCurve;
    use cc_core::title_block::{SheetItem, SheetShape};
    use cc_core::{Element, ElementType, ElementTypeId};

    #[test]
    fn components_draw_placed_with_nested_ones() {
        let mut p = Project::from_template();
        let view = p.views.keys().next().copied().unwrap();
        let line = SheetItem {
            name: "L".into(),
            shape: stroke_of(&SketchCurve::Line { a: DVec2::ZERO, b: DVec2::X }).unwrap(),
        };
        let text = SheetItem {
            name: "T".into(),
            shape: SheetShape::Text {
                x: "0".into(),
                y: "0".into(),
                text: "A".into(),
                size: "0.1".into(),
                anchor: TextAnchor::Left,
                rotation: String::new(),
            },
        };
        let nested = SheetItem {
            name: "N".into(),
            shape: SheetShape::Nested {
                component: "Leaf".into(),
                x: "0".into(),
                y: "1".into(),
                rotation: String::new(),
                scale: String::new(),
                mirrored: false,
            },
        };
        for (i, (name, def)) in
            [("Leaf", new_def(vec![line.clone()])), ("Top", new_def(vec![line, text, nested]))].into_iter().enumerate()
        {
            let id = ElementTypeId(90_000 + i as u64);
            let data = TypeData::Component { component: name.into(), def };
            p.types.insert(
                id,
                ElementType { id, name: name.into(), data, info: Default::default(), uid: Default::default() },
            );
        }
        let level = p.levels[0].id;
        let mut c = DetailComponent::new(view, DVec2::new(10.0, 0.0));
        c.rotation_deg = 90.0;
        c.scale = 2.0;
        let e = Element {
            id: ElementId(91_000),
            level,
            type_id: ElementTypeId(90_001),
            mark: String::new(),
            comments: String::new(),
            kind: ElementKind::DetailComponent(Box::new(c)),
            uid: Default::default(),
            phase_created: Default::default(),
            phase_demolished: None,
            design_option: None,
            paints: Vec::new(),
        };
        p.elements.insert(e.id, e);
        let mut d = Drawing2D::default();
        components(&mut d, &p, view, 50);
        let lines: Vec<(DVec2, DVec2)> = d
            .prims
            .iter()
            .filter_map(|q| if let Prim::Line { a, b, .. } = q { Some((*a, *b)) } else { None })
            .collect();
        assert_eq!(lines.len(), 2);
        // The top's line (0,0)-(1,0) doubled and turned: (10,0)-(10,2); the
        // leaf's, one up in the top's frame: (8,0)-(8,2).
        assert!((lines[0].1 - DVec2::new(10.0, 2.0)).length() < 1e-9, "{lines:?}");
        assert!(
            (lines[1].0 - DVec2::new(8.0, 0.0)).length() < 1e-9 && (lines[1].1 - DVec2::new(8.0, 2.0)).length() < 1e-9
        );
        let size = d.prims.iter().find_map(|q| if let Prim::Text { size, .. } = q { Some(*size) } else { None });
        // 0.1 m doubled is 0.2 m: 4 mm on paper at 1:50.
        assert!((size.unwrap() - 4.0).abs() < 1e-9);
        assert!(d.prims.iter().all(|q| q.element() == Some(ElementId(91_000))));
    }
}
