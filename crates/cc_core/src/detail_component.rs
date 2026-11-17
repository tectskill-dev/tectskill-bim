//! Detail components: reusable 2D components for construction details.
//!
//! A detail component is a component of kind [`ComponentKind::Detail`]
//! (category `DetailComponent`, `TypeData::Component`) made only of 2D
//! pieces, the same [`SheetItem`]s title blocks are drawn with
//! ([`ComponentDef::sheet`]): strokes of lines and arcs (open or closed,
//! with a pen weight in paper millimeters, a color, a dash pattern, and a
//! solid or pattern fill when closed), texts and pictures, plus
//! [`SheetShape::Nested`] pieces that place another detail component
//! inside this one. Unlike a title block's, every length is in meters in
//! the component's own frame (text heights too), and expressions read the
//! component's parameters.
//!
//! An element of kind `ElementKind::DetailComponent` places a type in one
//! view (`view`, like detail lines and text notes; removed with it): its
//! origin at `position` in the view's drawing coordinates, turned
//! `rotation_deg`, sized by `scale` and, when `mirrored`, flipped across
//! its own X axis before turning. It has no phase, no 3D and no IFC.
//!
//! Nested components are named by their component name and read the
//! parameter values of that component's first type; resolving them
//! ([`walk`]) is bounded in depth, in visits and in drawn points, and a
//! component met again inside itself is left out (a cycle).

use crate::component::{ComponentDef, ComponentKind};
use crate::profile::{self, ProfilePoint};
use crate::sketch::SketchCurve;
use crate::title_block::{SheetItem, SheetPieces, SheetShape};
use crate::transform::PlanTransform;
use crate::{Project, TypeData, ViewId};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Range of a placement's scale.
pub const MIN_SCALE: f64 = 1e-4;
pub const MAX_SCALE: f64 = 1e4;
/// Deepest nesting followed (the element's own component is depth 0).
pub const MAX_DEPTH: usize = 8;
/// Most component resolutions one element's drawing makes.
pub const MAX_VISITS: usize = 10_000;
/// Most points of strokes one element's drawing draws in all.
pub const MAX_POINTS: usize = 500_000;

/// Whether `s` is a usable scale.
pub fn scale_ok(s: f64) -> bool {
    s.is_finite() && (MIN_SCALE..=MAX_SCALE).contains(&s)
}

fn one() -> f64 {
    1.0
}

/// A detail component placed in one view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailComponent {
    /// The view that owns (and alone shows) it.
    pub view: ViewId,
    /// Where the component's origin lands, view coordinates (meters).
    pub position: DVec2,
    /// Degrees counter-clockwise.
    #[serde(default)]
    pub rotation_deg: f64,
    /// Size factor of the whole component.
    #[serde(default = "one")]
    pub scale: f64,
    /// Flipped across its own X axis (before turning).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirrored: bool,
    /// Values of instance parameters that differ from the type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, f64>,
}

impl DetailComponent {
    pub fn new(view: ViewId, position: DVec2) -> Self {
        Self { view, position, rotation_deg: 0.0, scale: 1.0, mirrored: false, overrides: BTreeMap::new() }
    }

    /// Finite numbers within bounds and a usable scale.
    pub fn is_valid(&self) -> bool {
        self.position.is_finite()
            && self.position.abs().max_element() <= crate::detail::MAX_COORD
            && self.rotation_deg.is_finite()
            && scale_ok(self.scale)
            && self.overrides.values().all(|v| v.is_finite())
    }

    /// Where the component's frame goes in the view.
    pub fn place(&self) -> Place {
        Place {
            t: PlanTransform {
                angle: self.rotation_deg.to_radians(),
                mirror: self.mirrored,
                translation: self.position,
            },
            scale: self.scale,
        }
    }

    /// Move, rotate or mirror the placement with the view's contents.
    pub fn transform(&mut self, t: &PlanTransform) {
        let place = self.place().then(&Place { t: *t, scale: 1.0 });
        self.position = place.t.translation;
        self.rotation_deg = place.t.angle.to_degrees().rem_euclid(360.0);
        self.mirrored = place.t.mirror;
    }
}

/// A similarity of the plane: scale by `scale`, then the rigid `t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub t: PlanTransform,
    pub scale: f64,
}

impl Default for Place {
    fn default() -> Self {
        Self { t: PlanTransform::IDENTITY, scale: 1.0 }
    }
}

impl Place {
    pub fn apply(&self, p: DVec2) -> DVec2 {
        self.t.apply(p * self.scale)
    }

    pub fn apply_vector(&self, v: DVec2) -> DVec2 {
        self.t.apply_vector(v * self.scale)
    }

    /// `self` followed by `next`.
    pub fn then(&self, next: &Place) -> Place {
        // next(self(p)) = Tn(sn * Ts(ss * p)): the scale of `next` scales the
        // translation of `self`, rotations and mirrors commute with scaling.
        let inner = PlanTransform { translation: self.t.translation * next.scale, ..self.t };
        Place { t: inner.then(next.t), scale: self.scale * next.scale }
    }

    /// The placement of a nested piece, resolved in the frame `self` maps.
    pub fn nested(&self, n: &crate::title_block::SheetNested) -> Place {
        let local = Place {
            t: PlanTransform { angle: n.rotation_deg.to_radians(), mirror: n.mirrored, translation: n.position },
            scale: n.scale,
        };
        local.then(self)
    }

    /// A direction given in radians, through the placement.
    pub fn apply_angle(&self, angle: f64) -> f64 {
        let v = self.t.apply_vector(DVec2::from_angle(angle));
        v.y.atan2(v.x)
    }
}

impl Project {
    /// The definition of the detail component named `component` (its
    /// first type's), if the project has one.
    pub fn detail_def(&self, component: &str) -> Option<&ComponentDef> {
        self.types.values().find_map(|t| match &t.data {
            TypeData::Component { component: c, def } if c == component && def.kind == ComponentKind::Detail => {
                Some(def)
            }
            _ => None,
        })
    }

    /// Whether the detail component `component` reaches `target` through
    /// its nested pieces (or is it), within [`MAX_DEPTH`].
    pub fn detail_nests(&self, component: &str, target: &str) -> bool {
        let mut pending = vec![(component.to_string(), 0usize)];
        let mut seen = std::collections::BTreeSet::new();
        while let Some((name, depth)) = pending.pop() {
            if name == target {
                return true;
            }
            if depth > MAX_DEPTH || !seen.insert(name.clone()) {
                continue;
            }
            if let Some(def) = self.detail_def(&name) {
                pending.extend(nested_names(def).map(|n| (n.to_string(), depth + 1)));
            }
        }
        false
    }
}

/// The names of the components `def` nests directly.
pub fn nested_names(def: &ComponentDef) -> impl Iterator<Item = &str> {
    def.sheet.iter().filter_map(|i| match &i.shape {
        SheetShape::Nested { component, .. } => Some(component.as_str()),
        _ => None,
    })
}

/// What [`walk`] visits: each resolution's pieces, where they go, and the
/// nesting depth (0 for the element's own component).
pub struct Visit<'a> {
    pub pieces: &'a SheetPieces,
    pub place: Place,
    pub depth: usize,
}

/// Resolve `def` with `overrides` and visit its pieces through `place`,
/// then each nested component's (found by `lookup`), depth first. A
/// component already being resolved (a cycle), nesting past [`MAX_DEPTH`],
/// more than [`MAX_VISITS`] resolutions or strokes past [`MAX_POINTS`]
/// points are left out; returns whether everything was visited.
pub fn walk<'d>(
    lookup: &dyn Fn(&str) -> Option<&'d ComponentDef>,
    name: &str,
    def: &ComponentDef,
    overrides: &BTreeMap<String, f64>,
    place: Place,
    visit: &mut dyn FnMut(Visit),
) -> bool {
    let mut budget = Budget { visits: 0, points: 0, complete: true };
    let mut stack = vec![name.to_string()];
    walk_in(lookup, def, overrides, place, 0, &mut stack, &mut budget, visit);
    budget.complete
}

struct Budget {
    visits: usize,
    points: usize,
    complete: bool,
}

#[allow(clippy::too_many_arguments)]
fn walk_in<'d>(
    lookup: &dyn Fn(&str) -> Option<&'d ComponentDef>,
    def: &ComponentDef,
    overrides: &BTreeMap<String, f64>,
    place: Place,
    depth: usize,
    stack: &mut Vec<String>,
    budget: &mut Budget,
    visit: &mut dyn FnMut(Visit),
) {
    budget.visits += 1;
    if budget.visits > MAX_VISITS || !(place.scale.is_finite() && place.scale > 0.0) {
        budget.complete = false;
        return;
    }
    let scope = def.scope(overrides, &[]);
    let mut pieces = def.resolve_2d(&|n: &str| scope.get(n));
    let points: usize = pieces.strokes.iter().map(|s| s.points.len()).sum();
    if budget.points + points > MAX_POINTS {
        budget.complete = false;
        pieces.strokes.clear();
    }
    budget.points += points;
    visit(Visit { pieces: &pieces, place, depth });
    for n in &pieces.nested {
        if depth + 1 > MAX_DEPTH || stack.contains(&n.component) {
            budget.complete = false;
            continue;
        }
        let Some(inner) = lookup(&n.component) else {
            budget.complete = false;
            continue;
        };
        stack.push(n.component.clone());
        walk_in(lookup, inner, &BTreeMap::new(), place.nested(n), depth + 1, stack, budget, visit);
        stack.pop();
    }
}

/// A stroke piece that draws `curve` (in the component's frame, meters):
/// lines and arcs as two points (a longer arc three), circles and whole
/// ellipses as closed strokes.
pub fn stroke_of(curve: &SketchCurve) -> Option<SheetShape> {
    let closed = match curve {
        SketchCurve::Circle { .. } => true,
        SketchCurve::Ellipse { sweep_deg, .. } => sweep_deg.abs() >= 360.0 - 1e-9,
        _ => false,
    };
    let vertices = profile::from_curves(std::slice::from_ref(curve), closed).ok()?;
    let points: Vec<ProfilePoint> = profile::written(&vertices);
    (points.len() >= 2).then(|| SheetShape::stroke(points, closed))
}

/// A closed stroke along `ring` (straight sides).
pub fn ring_stroke(ring: &[DVec2]) -> Option<SheetShape> {
    let vertices: Vec<(DVec2, profile::Segment)> = ring.iter().map(|p| (*p, profile::Segment::Line)).collect();
    (ring.len() >= 3).then(|| SheetShape::stroke(profile::written(&vertices), true))
}

/// A new detail component definition from `pieces`.
pub fn def_of(pieces: Vec<SheetItem>) -> ComponentDef {
    ComponentDef { kind: ComponentKind::Detail, sheet: pieces, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementType, ElementTypeId};

    fn nested(component: &str, x: &str) -> SheetItem {
        SheetItem {
            name: "N".into(),
            shape: SheetShape::Nested {
                component: component.into(),
                x: x.into(),
                y: "0".into(),
                rotation: "90".into(),
                scale: "2".into(),
                mirrored: false,
            },
        }
    }

    fn line() -> SheetItem {
        let shape = stroke_of(&SketchCurve::Line { a: DVec2::ZERO, b: DVec2::X }).unwrap();
        SheetItem { name: "L".into(), shape }
    }

    fn project(defs: &[(&str, ComponentDef)]) -> Project {
        let mut p = Project::empty();
        for (i, (name, def)) in defs.iter().enumerate() {
            let id = ElementTypeId(1000 + i as u64);
            p.types.insert(
                id,
                ElementType {
                    id,
                    name: name.to_string(),
                    data: TypeData::Component { component: name.to_string(), def: def.clone() },
                    info: Default::default(),
                    uid: Default::default(),
                },
            );
        }
        p
    }

    #[test]
    fn places_compose_scale_turn_and_mirror() {
        let a = Place { t: PlanTransform { angle: 0.0, mirror: true, translation: DVec2::new(1.0, 0.0) }, scale: 2.0 };
        let b = Place {
            t: PlanTransform { angle: std::f64::consts::FRAC_PI_2, mirror: false, translation: DVec2::new(0.0, 5.0) },
            scale: 3.0,
        };
        let p = DVec2::new(0.5, 0.25);
        let ab = a.then(&b);
        assert!((ab.apply(p) - b.apply(a.apply(p))).length() < 1e-12);
        assert!((ab.scale - 6.0).abs() < 1e-12);
        let mut e = DetailComponent::new(ViewId(1), DVec2::new(1.0, 2.0));
        e.transform(&PlanTransform::translate(DVec2::new(1.0, 1.0)));
        assert_eq!(e.position, DVec2::new(2.0, 3.0));
        e.transform(&PlanTransform::mirror_across(DVec2::ZERO, DVec2::X));
        assert!(e.mirrored && (e.position - DVec2::new(2.0, -3.0)).length() < 1e-12);
    }

    #[test]
    fn nested_components_are_placed_and_cycles_stop() {
        let leaf = def_of(vec![line()]);
        let mid = def_of(vec![line(), nested("Leaf", "3")]);
        // A component that holds itself, and one that holds a missing one.
        let looped = def_of(vec![line(), nested("Loop", "1")]);
        let p = project(&[("Leaf", leaf), ("Mid", mid.clone()), ("Loop", looped.clone())]);
        let lookup = |n: &str| p.detail_def(n);
        let mut seen = Vec::new();
        let done = walk(&lookup, "Mid", &mid, &BTreeMap::new(), Place::default(), &mut |v| {
            seen.push((v.depth, v.place.apply(DVec2::X)));
        });
        assert!(done);
        assert_eq!(seen.len(), 2);
        // The leaf's (1, 0) turned 90° and doubled, from (3, 0).
        assert!((seen[1].1 - DVec2::new(3.0, 2.0)).length() < 1e-12, "{seen:?}");
        let mut count = 0;
        assert!(!walk(&lookup, "Loop", &looped, &BTreeMap::new(), Place::default(), &mut |_| count += 1));
        assert_eq!(count, 1);
        assert!(p.detail_nests("Mid", "Leaf") && !p.detail_nests("Leaf", "Mid") && p.detail_nests("Loop", "Loop"));
    }

    #[test]
    fn deep_chains_are_bounded() {
        let defs: Vec<(String, ComponentDef)> =
            (0..20).map(|i| (format!("C{i}"), def_of(vec![line(), nested(&format!("C{}", i + 1), "1")]))).collect();
        let refs: Vec<(&str, ComponentDef)> = defs.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        let p = project(&refs);
        let lookup = |n: &str| p.detail_def(n);
        let mut depth = 0;
        let done = walk(&lookup, "C0", &defs[0].1, &BTreeMap::new(), Place::default(), &mut |v| depth = v.depth);
        assert!(!done);
        assert_eq!(depth, MAX_DEPTH);
    }

    #[test]
    fn curves_become_strokes() {
        let arc = SketchCurve::Arc { a: DVec2::ZERO, b: DVec2::new(2.0, 0.0), bulge: 1.0 };
        let SheetShape::Stroke { points, closed, .. } = stroke_of(&arc).unwrap() else { panic!() };
        assert!(!closed && points.len() == 2 && !points[0].radius.is_empty());
        let circle = SketchCurve::Circle { center: DVec2::ZERO, radius: 1.0 };
        assert!(matches!(stroke_of(&circle), Some(SheetShape::Stroke { closed: true, .. })));
        let def = def_of(vec![SheetItem { name: "A".into(), shape: stroke_of(&arc).unwrap() }]);
        let r = def.resolve_2d(&|_| None);
        assert!(r.errors.is_empty());
        let mid = r.strokes[0].points.iter().map(|p| p.y.abs()).fold(0.0, f64::max);
        assert!((mid - 1.0).abs() < 0.05, "a half circle of radius 1: {mid}");
    }
}
