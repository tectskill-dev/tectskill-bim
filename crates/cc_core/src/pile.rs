//! Piles and pile caps.
//!
//! A pile ([`Pile`], `ElementKind::Pile`, category `Pile`) is a vertical
//! shaft in the ground at a plan `position`: its head (`cut_off`) and its
//! toe (`toe`) are offsets from its level, its `embedment` how far the head
//! enters the cap above it; its type (`TypeData::Pile`) gives the section
//! ([`PileSection`]), the kind ([`PileKind`]) and the material. The length
//! is derived: `cut_off - toe`.
//!
//! A pile cap is a foundation (`ElementKind::Foundation` placed by a
//! `FoundationShape::Pad`: position and rotation) whose type is of
//! `FoundationKind::PileCap`: the type's [`PileCap`] gives the layout of
//! its piles, their spacing and edge distance, their type, length and
//! embedment; its `thickness` is the cap's. The cap's outline follows from
//! the layout ([`PileCap::outline`]), in the cap's frame: the origin at its
//! position, `u` along its rotation and `v` to the left of it.
//!
//! The piles of a cap are elements pointing at it ([`Pile::cap`], with their
//! index in the layout). Every edit keeps them in step with the cap and its
//! type (see [`crate::EditCtx`]): piles are added or removed when the
//! layout's count changes, and each one stands where the layout puts it
//! through the cap's placement, on the cap's level, its head at the cap's
//! bottom plus the embedment, its toe the pile length below. They are
//! deleted, moved and copied with the cap; moved alone they go back. A pile
//! whose cap is gone (or is no pile cap) is dropped, like a building pad
//! without its terrain.

use crate::{Element, ElementId, ElementKind, ElementTypeId, Project, TypeData};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Most piles under one cap.
pub const MAX_PILES: u32 = 64;
/// Smallest and largest pile diameter (or square side), meters.
pub const MIN_PILE_SIZE: f64 = 0.05;
pub const MAX_PILE_SIZE: f64 = 5.0;
/// Shortest and longest pile, meters.
pub const MIN_PILE_LENGTH: f64 = 0.1;
pub const MAX_PILE_LENGTH: f64 = 200.0;
/// Largest offset of a pile's head or toe from its level, meters.
pub const MAX_OFFSET: f64 = 10_000.0;
/// Deepest embedment of a pile's head into its cap, meters.
pub const MAX_EMBEDMENT: f64 = 5.0;
/// Closest and widest pile spacing in a cap (center to center), meters.
pub const MIN_SPACING: f64 = 0.1;
pub const MAX_SPACING: f64 = 50.0;
/// Smallest and largest edge distance (pile center to cap edge), meters.
pub const MIN_EDGE_DISTANCE: f64 = 0.05;
pub const MAX_EDGE_DISTANCE: f64 = 20.0;
/// Smallest and largest radius of a circle of piles, meters.
pub const MIN_CIRCLE_RADIUS: f64 = 0.1;
pub const MAX_CIRCLE_RADIUS: f64 = 100.0;
/// Defaults of a new pile type and pile cap type.
pub const DEFAULT_DIAMETER: f64 = 0.4;
pub const DEFAULT_PILE_LENGTH: f64 = 12.0;
pub const DEFAULT_EMBEDMENT: f64 = 0.1;
pub const DEFAULT_CAP_THICKNESS: f64 = 0.8;

/// How a pile is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PileKind {
    /// Drilled and cast in place.
    #[default]
    Bored,
    /// Continuous flight auger: concreted through the hollow auger as it is withdrawn.
    ContinuousFlightAuger,
    /// Micropiles and root piles: small drilled and grouted piles.
    Micro,
    /// Precast concrete driven into the ground.
    PrecastDriven,
    /// Steel sections or tubes driven into the ground.
    Steel,
    Other,
}

impl PileKind {
    pub const ALL: [PileKind; 6] = [
        PileKind::Bored,
        PileKind::ContinuousFlightAuger,
        PileKind::Micro,
        PileKind::PrecastDriven,
        PileKind::Steel,
        PileKind::Other,
    ];

    /// Translation key of the kind's name.
    pub fn key(self) -> &'static str {
        match self {
            PileKind::Bored => "pile-kind-bored",
            PileKind::ContinuousFlightAuger => "pile-kind-cfa",
            PileKind::Micro => "pile-kind-micro",
            PileKind::PrecastDriven => "pile-kind-precast",
            PileKind::Steel => "pile-kind-steel",
            PileKind::Other => "pile-kind-other",
        }
    }

    /// Short name used by commands (`bored`, `cfa`, `micro`, `precast`, `steel`, `other`).
    pub fn code(self) -> &'static str {
        match self {
            PileKind::Bored => "bored",
            PileKind::ContinuousFlightAuger => "cfa",
            PileKind::Micro => "micro",
            PileKind::PrecastDriven => "precast",
            PileKind::Steel => "steel",
            PileKind::Other => "other",
        }
    }

    /// The kind named by `code` (see [`Self::code`]), ignoring case.
    pub fn from_code(code: &str) -> Option<PileKind> {
        Self::ALL.into_iter().find(|k| k.code().eq_ignore_ascii_case(code.trim()))
    }
}

/// A pile's cross-section, centered on its axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PileSection {
    Round {
        diameter: f64,
    },
    /// A square `side` wide, turned with the pile's rotation.
    Square {
        side: f64,
    },
}

impl Default for PileSection {
    fn default() -> Self {
        PileSection::Round { diameter: DEFAULT_DIAMETER }
    }
}

impl PileSection {
    /// The diameter, or the side of a square.
    pub fn size(&self) -> f64 {
        match *self {
            PileSection::Round { diameter } => diameter,
            PileSection::Square { side } => side,
        }
    }

    /// Exact area of the section.
    pub fn area(&self) -> f64 {
        match *self {
            PileSection::Round { diameter } => PI * diameter * diameter / 4.0,
            PileSection::Square { side } => side * side,
        }
    }

    /// Whether the size is a finite number within the limits.
    pub fn is_valid(&self) -> bool {
        let s = self.size();
        s.is_finite() && (MIN_PILE_SIZE..=MAX_PILE_SIZE).contains(&s)
    }

    /// Bring the size within the limits (the default when not a number);
    /// returns whether it changed.
    pub fn repair(&mut self) -> bool {
        if self.is_valid() {
            return false;
        }
        let fix = |v: f64| if v.is_finite() { v.clamp(MIN_PILE_SIZE, MAX_PILE_SIZE) } else { DEFAULT_DIAMETER };
        match self {
            PileSection::Round { diameter } => *diameter = fix(*diameter),
            PileSection::Square { side } => *side = fix(*side),
        }
        true
    }
}

/// The cap a pile belongs to and its place in the cap's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PileCapRef {
    pub cap: ElementId,
    pub index: u32,
}

/// A pile (see the module documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pile {
    /// Axis in plan.
    pub position: DVec2,
    /// Rotation of a square section in plan, degrees counter-clockwise.
    #[serde(default)]
    pub rotation_deg: f64,
    /// Top of the pile (its cut-off) above the level, meters (negative below).
    pub cut_off: f64,
    /// Bottom of the pile above the level, meters (below `cut_off`).
    pub toe: f64,
    /// How far the head enters the cap above it, meters.
    #[serde(default)]
    pub embedment: f64,
    /// The pile cap that owns the pile (`None`: a free pile).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<PileCapRef>,
}

impl Pile {
    /// A free pile from its head down `length`.
    pub fn new(position: DVec2, cut_off: f64, length: f64) -> Self {
        Self { position, rotation_deg: 0.0, cut_off, toe: cut_off - length, embedment: 0.0, cap: None }
    }

    /// Length from the head to the toe.
    pub fn length(&self) -> f64 {
        self.cut_off - self.toe
    }

    /// Every number finite, the offsets within [`MAX_OFFSET`].
    pub fn is_valid(&self) -> bool {
        self.position.is_finite()
            && self.rotation_deg.is_finite()
            && self.embedment.is_finite()
            && [self.cut_off, self.toe].iter().all(|v| v.is_finite() && v.abs() <= MAX_OFFSET)
    }

    /// Bring the embedment and the length within their limits (a toe not
    /// below the head goes the default length down); returns how many values
    /// changed.
    pub fn repair(&mut self) -> usize {
        let mut repairs = 0;
        let embedment = self.embedment.clamp(0.0, MAX_EMBEDMENT);
        if embedment != self.embedment {
            self.embedment = embedment;
            repairs += 1;
        }
        let length = self.length();
        if length < MIN_PILE_LENGTH - 1e-9 {
            self.toe = self.cut_off - DEFAULT_PILE_LENGTH;
            repairs += 1;
        } else if length > MAX_PILE_LENGTH + 1e-9 {
            self.toe = self.cut_off - MAX_PILE_LENGTH;
            repairs += 1;
        }
        repairs
    }
}

/// Whether `length` is a pile length within the limits.
pub fn length_ok(length: f64) -> bool {
    length.is_finite() && (MIN_PILE_LENGTH..=MAX_PILE_LENGTH).contains(&length)
}

/// Where the piles of a cap stand, in the cap's frame (`spacing` is the
/// distance between neighbors, except on a circle).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PileLayout {
    /// One pile at the center.
    Single,
    /// Two piles along `u`, `spacing` apart.
    Pair,
    /// Three piles at the corners of an equilateral triangle of side
    /// `spacing` centered on the origin, the first on `+u` (the cap points
    /// along its rotation).
    Triangle,
    /// Four piles at the corners of a square of side `spacing`.
    Square,
    /// `count` piles along `u`, `spacing` apart, centered.
    Line { count: u32 },
    /// `count` piles on a circle of `radius`, the first on `+u`: a round
    /// cap (`radius` plus the edge distance), or a ring `ring_width` wide
    /// centered on the circle.
    Circle {
        count: u32,
        radius: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ring_width: Option<f64>,
    },
}

impl PileLayout {
    /// Number of piles.
    pub fn count(&self) -> u32 {
        match *self {
            PileLayout::Single => 1,
            PileLayout::Pair => 2,
            PileLayout::Triangle => 3,
            PileLayout::Square => 4,
            PileLayout::Line { count } | PileLayout::Circle { count, .. } => count,
        }
    }

    /// Translation key of the layout's name.
    pub fn key(&self) -> &'static str {
        match self {
            PileLayout::Single => "pile-layout-single",
            PileLayout::Pair => "pile-layout-pair",
            PileLayout::Triangle => "pile-layout-triangle",
            PileLayout::Square => "pile-layout-square",
            PileLayout::Line { .. } => "pile-layout-line",
            PileLayout::Circle { ring_width: Some(_), .. } => "pile-layout-ring",
            PileLayout::Circle { .. } => "pile-layout-circle",
        }
    }
}

/// The plan of a pile cap in its frame.
#[derive(Debug, Clone, PartialEq)]
pub enum CapOutline {
    /// A convex polygon, counter-clockwise.
    Polygon(Vec<DVec2>),
    /// A disk centered on the origin.
    Disk { radius: f64 },
    /// An annulus centered on the origin.
    Ring { inner: f64, outer: f64 },
}

impl CapOutline {
    /// Exact plan area.
    pub fn area(&self) -> f64 {
        match self {
            CapOutline::Polygon(p) => polygon_area(p),
            CapOutline::Disk { radius } => PI * radius * radius,
            CapOutline::Ring { inner, outer } => PI * (outer * outer - inner * inner),
        }
    }

    /// The bounding box, `(min, max)`.
    pub fn bounds(&self) -> (DVec2, DVec2) {
        match self {
            CapOutline::Polygon(p) => {
                p.iter().fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), q| (lo.min(*q), hi.max(*q)))
            }
            CapOutline::Disk { radius: r } | CapOutline::Ring { outer: r, .. } => (DVec2::splat(-r), DVec2::splat(*r)),
        }
    }

    /// The outer boundary and the holes as polygons (counter-clockwise; a
    /// circle as `segments` chords).
    pub fn loops(&self, segments: usize) -> Vec<Vec<DVec2>> {
        let n = segments.max(8);
        let circle =
            |r: f64| (0..n).map(|i| DVec2::from_angle(i as f64 / n as f64 * std::f64::consts::TAU) * r).collect();
        match self {
            CapOutline::Polygon(p) => vec![p.clone()],
            CapOutline::Disk { radius } => vec![circle(*radius)],
            CapOutline::Ring { inner, outer } => vec![circle(*outer), circle(*inner)],
        }
    }
}

/// Area of a simple polygon of either winding.
fn polygon_area(p: &[DVec2]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f64>().abs() * 0.5
}

/// What a pile cap type holds besides its thickness and material: its piles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PileCap {
    pub layout: PileLayout,
    /// Center-to-center distance between neighboring piles (unused on a circle).
    pub spacing: f64,
    /// Distance from a pile's center to the cap's edge (on a circle: to the
    /// outer edge of a round cap; a ring has its width).
    pub edge_distance: f64,
    /// The type of the cap's piles (`None`: the project's first pile type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pile_type: Option<ElementTypeId>,
    /// Length of each pile, from its head to its toe.
    pub pile_length: f64,
    /// How far each pile's head enters the cap.
    #[serde(default)]
    pub embedment: f64,
}

impl Default for PileCap {
    fn default() -> Self {
        Self {
            layout: PileLayout::Pair,
            spacing: 3.0 * DEFAULT_DIAMETER,
            edge_distance: DEFAULT_DIAMETER,
            pile_type: None,
            pile_length: DEFAULT_PILE_LENGTH,
            embedment: DEFAULT_EMBEDMENT,
        }
    }
}

impl PileCap {
    /// Pile positions in the cap's frame, in layout order.
    pub fn positions(&self) -> Vec<DVec2> {
        let s = self.spacing;
        match self.layout {
            PileLayout::Single => vec![DVec2::ZERO],
            PileLayout::Pair => vec![DVec2::new(-s / 2.0, 0.0), DVec2::new(s / 2.0, 0.0)],
            PileLayout::Triangle => {
                let r = s / 3f64.sqrt();
                (0..3).map(|i| DVec2::from_angle(f64::from(i) * 2.0 * PI / 3.0) * r).collect()
            }
            PileLayout::Square => {
                let h = s / 2.0;
                vec![DVec2::new(-h, -h), DVec2::new(h, -h), DVec2::new(h, h), DVec2::new(-h, h)]
            }
            PileLayout::Line { count } => {
                let n = count.clamp(1, MAX_PILES);
                let mid = f64::from(n - 1) / 2.0;
                (0..n).map(|i| DVec2::new((f64::from(i) - mid) * s, 0.0)).collect()
            }
            PileLayout::Circle { count, radius, .. } => {
                let n = count.clamp(1, MAX_PILES);
                (0..n).map(|i| DVec2::from_angle(f64::from(i) * 2.0 * PI / f64::from(n)) * radius).collect()
            }
        }
    }

    /// The cap's plan in its frame: a rectangle `edge_distance` around the
    /// piles of one, two, four and a line; for three, the triangle whose
    /// sides lie `edge_distance` off the piles with its corners cut square
    /// `edge_distance` past the corner piles; on a circle a disk, or the
    /// ring.
    pub fn outline(&self) -> CapOutline {
        let (s, e) = (self.spacing, self.edge_distance);
        let rect = |hu: f64, hv: f64| {
            CapOutline::Polygon(vec![
                DVec2::new(-hu, -hv),
                DVec2::new(hu, -hv),
                DVec2::new(hu, hv),
                DVec2::new(-hu, hv),
            ])
        };
        match self.layout {
            PileLayout::Single => rect(e, e),
            PileLayout::Pair => rect(s / 2.0 + e, e),
            PileLayout::Square => rect(s / 2.0 + e, s / 2.0 + e),
            PileLayout::Line { count } => rect(f64::from(count.clamp(1, MAX_PILES) - 1) * s / 2.0 + e, e),
            PileLayout::Triangle => {
                // Half-planes every 60°: the cuts across the corners (along the
                // piles' directions, R + e out) and the sides (r + e out).
                let (big, small) = (s / 3f64.sqrt() + e, s / (2.0 * 3f64.sqrt()) + e);
                let plane = |k: usize| {
                    let n = DVec2::from_angle(k as f64 * PI / 3.0);
                    (n, if k.is_multiple_of(2) { big } else { small })
                };
                let corner = |k: usize| {
                    let ((n1, d1), (n2, d2)) = (plane(k), plane((k + 1) % 6));
                    let det = n1.perp_dot(n2);
                    DVec2::new((d1 * n2.y - d2 * n1.y) / det, (n1.x * d2 - n2.x * d1) / det)
                };
                CapOutline::Polygon((0..6).map(corner).collect())
            }
            PileLayout::Circle { radius, ring_width: Some(w), .. } => {
                CapOutline::Ring { inner: radius - w / 2.0, outer: radius + w / 2.0 }
            }
            PileLayout::Circle { radius, .. } => CapOutline::Disk { radius: radius + e },
        }
    }

    /// Why the layout cannot hold piles `pile_size` across (`None` when it
    /// can): piles apart, inside the cap, numbers within the limits.
    pub fn problem(&self, pile_size: f64) -> Option<String> {
        let in_range = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !in_range(self.spacing, MIN_SPACING, MAX_SPACING) {
            return Some(format!("the spacing must lie between {MIN_SPACING} and {MAX_SPACING} m"));
        }
        if !in_range(self.edge_distance, MIN_EDGE_DISTANCE, MAX_EDGE_DISTANCE) {
            return Some(format!("the edge distance must lie between {MIN_EDGE_DISTANCE} and {MAX_EDGE_DISTANCE} m"));
        }
        if !length_ok(self.pile_length) {
            return Some(format!("the pile length must lie between {MIN_PILE_LENGTH} and {MAX_PILE_LENGTH} m"));
        }
        if !in_range(self.embedment, 0.0, MAX_EMBEDMENT) {
            return Some(format!("the embedment must lie between 0 and {MAX_EMBEDMENT} m"));
        }
        match self.layout {
            PileLayout::Line { count } if !(2..=MAX_PILES).contains(&count) => {
                return Some(format!("a line holds 2 to {MAX_PILES} piles"));
            }
            PileLayout::Circle { count, radius, ring_width } => {
                if !(3..=MAX_PILES).contains(&count) {
                    return Some(format!("a circle holds 3 to {MAX_PILES} piles"));
                }
                if !in_range(radius, MIN_CIRCLE_RADIUS, MAX_CIRCLE_RADIUS) {
                    return Some(format!(
                        "the circle's radius must lie between {MIN_CIRCLE_RADIUS} and {MAX_CIRCLE_RADIUS} m"
                    ));
                }
                if let Some(w) = ring_width
                    && !(w.is_finite() && w >= pile_size && w / 2.0 < radius - MIN_EDGE_DISTANCE)
                {
                    return Some("a ring must be at least a pile wide and leave a hole".into());
                }
                // Neighbors on the circle: the chord between them.
                if 2.0 * radius * (PI / f64::from(count)).sin() < pile_size {
                    return Some("the piles on the circle overlap".into());
                }
            }
            _ => {}
        }
        let spaced = matches!(self.layout, PileLayout::Single | PileLayout::Circle { .. }) || self.spacing >= pile_size;
        if !spaced {
            return Some("the spacing is less than the piles' size".into());
        }
        if self.edge_distance < pile_size / 2.0
            && !matches!(self.layout, PileLayout::Circle { ring_width: Some(_), .. })
        {
            return Some("the edge distance is less than half the piles' size".into());
        }
        None
    }

    /// Bring every value within its limits (a number that is not one takes
    /// the default); returns how many changed. The pile type is checked
    /// against the project by [`Project::repair`].
    pub fn repair(&mut self) -> usize {
        let d = Self::default();
        let fix = |v: &mut f64, lo: f64, hi: f64, default: f64| {
            let fixed = if v.is_finite() { v.clamp(lo, hi) } else { default };
            let changed = fixed.to_bits() != v.to_bits();
            *v = fixed;
            usize::from(changed)
        };
        let mut repairs = fix(&mut self.spacing, MIN_SPACING, MAX_SPACING, d.spacing)
            + fix(&mut self.edge_distance, MIN_EDGE_DISTANCE, MAX_EDGE_DISTANCE, d.edge_distance)
            + fix(&mut self.pile_length, MIN_PILE_LENGTH, MAX_PILE_LENGTH, d.pile_length)
            + fix(&mut self.embedment, 0.0, MAX_EMBEDMENT, d.embedment);
        match &mut self.layout {
            PileLayout::Line { count } => {
                let c = (*count).clamp(2, MAX_PILES);
                if c != *count {
                    *count = c;
                    repairs += 1;
                }
            }
            PileLayout::Circle { count, radius, ring_width } => {
                let c = (*count).clamp(3, MAX_PILES);
                if c != *count {
                    *count = c;
                    repairs += 1;
                }
                repairs += fix(radius, MIN_CIRCLE_RADIUS, MAX_CIRCLE_RADIUS, 1.0);
                if ring_width.is_some_and(|w| !(w.is_finite() && w > 0.0 && w / 2.0 < *radius - MIN_EDGE_DISTANCE)) {
                    *ring_width = None;
                    repairs += 1;
                }
            }
            _ => {}
        }
        repairs
    }
}

/// The pile cap a foundation type describes, with the cap's thickness.
pub fn cap_of_type(data: &TypeData) -> Option<(&PileCap, f64)> {
    match data {
        TypeData::Foundation { kind: crate::FoundationKind::PileCap, pile_cap: Some(cap), thickness, .. } => {
            Some((cap, *thickness))
        }
        _ => None,
    }
}

/// A pile cap element's placement: position, rotation (degrees) and top above its level.
pub fn cap_placement(e: &Element) -> Option<(DVec2, f64, f64)> {
    match &e.kind {
        ElementKind::Foundation(crate::Foundation {
            shape: crate::FoundationShape::Pad { position, rotation_deg },
            top_offset,
            ..
        }) => Some((*position, *rotation_deg, *top_offset)),
        _ => None,
    }
}

/// A pile cap element as the sync and the geometry see it.
#[derive(Debug, Clone, PartialEq)]
pub struct CapPlacement<'a> {
    pub cap: &'a PileCap,
    pub thickness: f64,
    pub position: DVec2,
    pub rotation_deg: f64,
    pub top_offset: f64,
}

impl CapPlacement<'_> {
    /// A point of the cap's frame in plan.
    pub fn world(&self, local: DVec2) -> DVec2 {
        self.position + DVec2::from_angle(self.rotation_deg.to_radians()).rotate(local)
    }

    /// The piles' axes in plan, in layout order.
    pub fn pile_positions(&self) -> Vec<DVec2> {
        self.cap.positions().into_iter().map(|p| self.world(p)).collect()
    }

    /// The head of the cap's piles above its level: the cap's bottom plus the embedment.
    pub fn cut_off(&self) -> f64 {
        self.top_offset - self.thickness + self.cap.embedment
    }
}

impl Project {
    /// `e` as a pile cap: a foundation placed as a pad whose type is a pile cap type.
    pub fn pile_cap(&self, e: &Element) -> Option<CapPlacement<'_>> {
        let (position, rotation_deg, top_offset) = cap_placement(e)?;
        let (cap, thickness) = cap_of_type(&self.element_type(e.type_id)?.data)?;
        Some(CapPlacement { cap, thickness, position, rotation_deg, top_offset })
    }

    /// Whether element `id` is a pile cap.
    pub fn is_pile_cap(&self, id: ElementId) -> bool {
        self.element(id).is_some_and(|e| self.pile_cap(e).is_some())
    }

    /// The piles of cap `cap`, in id order.
    pub fn piles_of(&self, cap: ElementId) -> impl Iterator<Item = (&Element, &Pile)> {
        self.elements.values().filter_map(move |e| match &e.kind {
            ElementKind::Pile(p) if p.cap.is_some_and(|c| c.cap == cap) => Some((e, p)),
            _ => None,
        })
    }

    /// The type a cap's piles take: its own when it names a pile type, else
    /// the project's first pile type.
    pub fn cap_pile_type(&self, cap: &PileCap) -> Option<ElementTypeId> {
        let is_pile =
            |id: ElementTypeId| self.element_type(id).is_some_and(|t| matches!(t.data, TypeData::Pile { .. }));
        cap.pile_type.filter(|id| is_pile(*id)).or_else(|| self.default_type(crate::Category::Pile))
    }

    /// The piles cap `e` should have: index, type and the pile itself.
    pub fn cap_piles(&self, e: &Element) -> Vec<(u32, ElementTypeId, Pile)> {
        let Some(c) = self.pile_cap(e) else { return Vec::new() };
        let Some(type_id) = self.cap_pile_type(c.cap) else { return Vec::new() };
        let cut_off = c.cut_off();
        c.pile_positions()
            .into_iter()
            .zip(0u32..)
            .map(|(position, index)| {
                let pile = Pile {
                    position,
                    rotation_deg: c.rotation_deg,
                    cut_off,
                    toe: cut_off - c.cap.pile_length,
                    embedment: c.cap.embedment,
                    cap: Some(PileCapRef { cap: e.id, index }),
                };
                (index, type_id, pile)
            })
            .collect()
    }

    /// What keeps the piles in step with their caps (see [`PileSync`]).
    pub(crate) fn pile_sync(&self) -> PileSync {
        let mut out = PileSync::default();
        // Cap types show their outline's size as their width and length.
        for t in self.types.values() {
            if let TypeData::Foundation { width, length, .. } = &t.data
                && let Some((cap, _)) = cap_of_type(&t.data)
            {
                let (lo, hi) = cap.outline().bounds();
                let size = hi - lo;
                if size.is_finite() && ((size.x - width).abs() > 1e-9 || (size.y - length).abs() > 1e-9) {
                    out.types.push((t.id, size.x, size.y));
                }
            }
        }
        if !self.elements.values().any(|e| matches!(e.kind, ElementKind::Pile(_)) || self.pile_cap(e).is_some()) {
            return out;
        }
        // Wanted piles by (cap, index).
        let mut wanted: std::collections::BTreeMap<(ElementId, u32), (crate::LevelId, ElementTypeId, Pile)> =
            Default::default();
        for e in self.elements.values().filter(|e| self.pile_cap(e).is_some()) {
            for (index, type_id, pile) in self.cap_piles(e) {
                wanted.insert((e.id, index), (e.level, type_id, pile));
            }
        }
        let mut found = std::collections::BTreeSet::new();
        for e in self.elements.values() {
            let ElementKind::Pile(p) = &e.kind else { continue };
            let Some(r) = p.cap else { continue };
            match wanted.get(&(r.cap, r.index)) {
                // A second pile at the same place goes.
                Some(_) if found.contains(&(r.cap, r.index)) => out.remove.push(e.id),
                Some((level, type_id, want)) => {
                    found.insert((r.cap, r.index));
                    if e.level != *level || e.type_id != *type_id || p != want {
                        out.update.push((e.id, *level, *type_id, want.clone()));
                    }
                }
                None => out.remove.push(e.id),
            }
        }
        for (key, (level, type_id, pile)) in wanted {
            if !found.contains(&key) {
                out.add.push((level, type_id, pile));
            }
        }
        out
    }
}

/// The changes that bring piles in step with their caps.
#[derive(Debug, Default)]
pub(crate) struct PileSync {
    /// Cap types whose width and length differ from their outline's size.
    pub types: Vec<(ElementTypeId, f64, f64)>,
    /// Piles of no cap (or of a place another pile takes).
    pub remove: Vec<ElementId>,
    /// Piles to put where their cap wants them: level, type and pile.
    pub update: Vec<(ElementId, crate::LevelId, ElementTypeId, Pile)>,
    /// Missing piles of caps.
    pub add: Vec<(crate::LevelId, ElementTypeId, Pile)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(layout: PileLayout) -> PileCap {
        PileCap { layout, spacing: 1.2, edge_distance: 0.4, ..PileCap::default() }
    }

    #[test]
    fn layouts_place_their_piles() {
        let close = |a: DVec2, b: DVec2| a.distance(b) < 1e-12;
        assert_eq!(cap(PileLayout::Single).positions(), vec![DVec2::ZERO]);
        let pair = cap(PileLayout::Pair).positions();
        assert!(close(pair[0], DVec2::new(-0.6, 0.0)) && close(pair[1], DVec2::new(0.6, 0.0)));
        // A triangle of side 1.2: every pair 1.2 apart, centered.
        let tri = cap(PileLayout::Triangle).positions();
        assert_eq!(tri.len(), 3);
        for i in 0..3 {
            assert!((tri[i].distance(tri[(i + 1) % 3]) - 1.2).abs() < 1e-12);
        }
        assert!(tri.iter().sum::<DVec2>().length() < 1e-12 && tri[0].y.abs() < 1e-12 && tri[0].x > 0.0);
        let square = cap(PileLayout::Square).positions();
        assert!(close(square[2], DVec2::new(0.6, 0.6)));
        let line = cap(PileLayout::Line { count: 3 }).positions();
        assert!(close(line[0], DVec2::new(-1.2, 0.0)) && close(line[2], DVec2::new(1.2, 0.0)));
        let ring = cap(PileLayout::Circle { count: 8, radius: 2.0, ring_width: None }).positions();
        assert!(ring.iter().all(|p| (p.length() - 2.0).abs() < 1e-12));
        assert!(close(ring[2], DVec2::new(0.0, 2.0)));
    }

    #[test]
    fn outlines_have_their_areas() {
        let (s, e) = (1.2, 0.4);
        let area = |l: PileLayout| cap(l).outline().area();
        assert!((area(PileLayout::Single) - 4.0 * e * e).abs() < 1e-12);
        assert!((area(PileLayout::Pair) - (s + 2.0 * e) * 2.0 * e).abs() < 1e-12);
        assert!((area(PileLayout::Square) - (s + 2.0 * e).powi(2)).abs() < 1e-12);
        assert!((area(PileLayout::Line { count: 4 }) - (3.0 * s + 2.0 * e) * 2.0 * e).abs() < 1e-12);
        // The triangle with its sides r + e out, its corners cut e past the
        // corner piles: the big triangle less three small ones of height e.
        let r = s / (2.0 * 3f64.sqrt());
        let expected = 3.0 * 3f64.sqrt() * (r + e).powi(2) - 3f64.sqrt() * e * e;
        assert!((area(PileLayout::Triangle) - expected).abs() < 1e-12);
        let CapOutline::Polygon(hex) = cap(PileLayout::Triangle).outline() else { panic!("polygon") };
        assert_eq!(hex.len(), 6);
        // Every pile is `e` from the two sides and the cut nearest it.
        let piles = cap(PileLayout::Triangle).positions();
        for p in &piles {
            let nearest = (0..6)
                .map(|i| {
                    let (a, b) = (hex[i], hex[(i + 1) % 6]);
                    (b - a).normalize().perp_dot(*p - a)
                })
                .fold(f64::INFINITY, f64::min);
            assert!((nearest - e).abs() < 1e-12, "{nearest}");
        }
        let disk = area(PileLayout::Circle { count: 6, radius: 2.0, ring_width: None });
        assert!((disk - PI * 2.4 * 2.4).abs() < 1e-12);
        let ring = area(PileLayout::Circle { count: 6, radius: 2.0, ring_width: Some(0.8) });
        assert!((ring - PI * (2.4f64.powi(2) - 1.6f64.powi(2))).abs() < 1e-12);
    }

    #[test]
    fn problems_and_repairs() {
        assert!(cap(PileLayout::Triangle).problem(0.4).is_none());
        assert!(cap(PileLayout::Triangle).problem(1.5).is_some(), "piles overlap");
        let edge = PileCap { edge_distance: 0.1, ..cap(PileLayout::Pair) };
        assert!(edge.problem(0.4).is_some());
        let mut bad = PileCap {
            spacing: f64::NAN,
            pile_length: -1.0,
            layout: PileLayout::Circle { count: 1000, radius: 2.0, ring_width: Some(9.0) },
            ..cap(PileLayout::Pair)
        };
        assert_eq!(bad.repair(), 4);
        assert_eq!(bad.spacing, PileCap::default().spacing);
        assert_eq!(bad.pile_length, MIN_PILE_LENGTH);
        assert_eq!(bad.layout, PileLayout::Circle { count: MAX_PILES, radius: 2.0, ring_width: None });
        assert_eq!(bad.repair(), 0);
        let mut p = Pile { embedment: -1.0, toe: 3.0, ..Pile::new(DVec2::ZERO, 0.0, 10.0) };
        assert_eq!(p.repair(), 2);
        assert_eq!((p.embedment, p.length()), (0.0, DEFAULT_PILE_LENGTH));
        assert!(!Pile { cut_off: f64::INFINITY, ..p.clone() }.is_valid());
    }
}
