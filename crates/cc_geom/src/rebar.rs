//! Reinforcing bars: the center lines of each set of bars in its beam or
//! column, from the host's body, the cover and the bar diameters.
//!
//! Longitudinal bars sit inside the stirrups (cover plus the host's largest
//! stirrup diameter); stirrups are closed loops `cover` inside the faces,
//! spaced evenly and centered along the host. Along an arc beam the bars
//! follow the arc, each at its own radius, and the stirrups are radial. Top
//! and bottom bars that do not fit side by side go in further layers, side
//! bars run between them, and bars longer than the stock length are spliced.
//!
//! A column's bars stand around its section ([`crate::faces`]): a count
//! spread around it, or so many on each face with the corner bars shared;
//! its ties follow the section's outline, cross-ties link the bars between
//! the corners to the bars opposite, and the vertical bars may be cut in
//! lifts that lap.

use crate::faces::{self, FaceBar, SectionFace};
use crate::{BeamEnd, BeamGeom};
use cc_core::profiles_project::SectorPlacement;
use cc_core::{BarFace, ElementId, ElementKind, Project, Rebar, RebarLayout, Splice, TypeData};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// Density of steel, kg/m³.
pub const STEEL_DENSITY: f64 = 7850.0;

/// Segments of round ties.
const ROUND: usize = 16;

/// At most this many stirrups per set (a crafted spacing cannot explode).
const MAX_STIRRUPS: usize = 2000;

/// At most this many layers of top or bottom bars; the rest crowd the last.
const MAX_LAYERS: usize = 4;

/// At most this many bars in one set, splice pieces included.
pub(crate) const MAX_BARS: usize = 20_000;

/// Least clear distance between parallel bars: the bar diameter, and 20 mm.
fn clear_distance(d: f64) -> f64 {
    d.max(0.02)
}

/// One bar: its center line, closed for stirrups.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub points: Vec<DVec3>,
    pub closed: bool,
    /// The bar's true length when its center line is drawn with chords (a
    /// bar following an arc): the arc's length at the bar's radius.
    pub true_length: Option<f64>,
}

impl Bar {
    /// A bar along straight segments.
    pub fn new(points: Vec<DVec3>, closed: bool) -> Self {
        Self { points, closed, true_length: None }
    }

    pub fn length(&self) -> f64 {
        if let Some(length) = self.true_length {
            return length;
        }
        let open: f64 = self.points.windows(2).map(|w| w[0].distance(w[1])).sum();
        let closing = match (self.closed, self.points.first(), self.points.last()) {
            (true, Some(a), Some(b)) => a.distance(*b),
            _ => 0.0,
        };
        open + closing
    }
}

/// The bars of one reinforcement set.
#[derive(Debug, Clone, PartialEq)]
pub struct RebarGeom {
    pub element: ElementId,
    /// The beam or column the set is laid out in (none for shaped bars).
    pub host: Option<ElementId>,
    pub diameter: f64,
    /// The steel grade of the set's type (empty: none).
    pub grade: String,
    /// The type's mass per meter, kg/m, when it gives one.
    pub mass_per_meter: Option<f64>,
    pub bars: Vec<Bar>,
}

impl RebarGeom {
    /// Mass of a meter of bar, kilograms: the type's, else a round bar of
    /// the diameter in steel of [`STEEL_DENSITY`].
    pub fn mass_per_meter(&self) -> f64 {
        self.mass_per_meter
            .filter(|m| m.is_finite() && *m > 0.0)
            .unwrap_or(std::f64::consts::PI * self.diameter * self.diameter / 4.0 * STEEL_DENSITY)
    }

    pub fn count(&self) -> usize {
        self.bars.len()
    }

    pub fn total_length(&self) -> f64 {
        self.bars.iter().map(Bar::length).sum()
    }

    /// Mean length of one bar.
    pub fn bar_length(&self) -> f64 {
        if self.bars.is_empty() { 0.0 } else { self.total_length() / self.bars.len() as f64 }
    }

    /// Steel weight in kilograms.
    pub fn weight(&self) -> f64 {
        self.total_length() * self.mass_per_meter()
    }
}

/// `n` positions spread evenly over `[-half, half]` (the middle for one).
fn spread(n: usize, half: f64) -> Vec<f64> {
    match n {
        0 => Vec::new(),
        1 => vec![0.0],
        _ => (0..n).map(|i| -half + 2.0 * half * i as f64 / (n - 1) as f64).collect(),
    }
}

/// Positions `spacing` apart filling `[a, b]`, centered.
pub(crate) fn stations(a: f64, b: f64, spacing: f64) -> Vec<f64> {
    if b < a || spacing <= 0.0 {
        return Vec::new();
    }
    let count = (((b - a) / spacing).floor() as usize + 1).min(MAX_STIRRUPS);
    let start = a + ((b - a) - spacing * (count - 1) as f64) / 2.0;
    (0..count).map(|i| start + spacing * i as f64).collect()
}

/// `n` points around a `2a` x `2b` rectangle: the corners first, the rest
/// along the sides, each to the side with the widest gap.
fn perimeter(n: usize, a: f64, b: f64) -> Vec<DVec2> {
    let corners = [DVec2::new(-a, -b), DVec2::new(a, -b), DVec2::new(a, b), DVec2::new(-a, b)];
    if n <= 4 {
        // One or two bars sit on the bottom side's corners; three take three corners.
        return corners.into_iter().take(n).collect();
    }
    let lengths = [2.0 * a, 2.0 * b, 2.0 * a, 2.0 * b];
    let mut extra = [0usize; 4];
    for _ in 4..n {
        let side = (0..4)
            .max_by(|&i, &j| (lengths[i] / (extra[i] + 1) as f64).total_cmp(&(lengths[j] / (extra[j] + 1) as f64)))
            .unwrap_or(0);
        extra[side] += 1;
    }
    let mut points = corners.to_vec();
    for side in 0..4 {
        let (p, q) = (corners[side], corners[(side + 1) % 4]);
        for k in 1..=extra[side] {
            points.push(p + (q - p) * (k as f64 / (extra[side] + 1) as f64));
        }
    }
    points
}

fn circle(n: usize, r: f64) -> Vec<DVec2> {
    (0..n).map(|i| DVec2::from_angle(i as f64 / n as f64 * std::f64::consts::TAU) * r).collect()
}

/// The diameter of a rebar element's type.
fn diameter(project: &Project, e: &cc_core::Element) -> Option<f64> {
    match project.element_type(e.type_id).map(|t| &t.data) {
        Some(TypeData::Rebar { diameter, .. }) if diameter.is_finite() && *diameter > 0.0 => Some(*diameter),
        _ => None,
    }
}

/// The steel grade and the mass per meter of a rebar element's type.
fn steel(project: &Project, e: &cc_core::Element) -> (String, Option<f64>) {
    match project.element_type(e.type_id).map(|t| &t.data) {
        Some(TypeData::Rebar { grade, mass_per_meter, .. }) => (grade.clone(), *mass_per_meter),
        _ => (String::new(), None),
    }
}

/// Bars of every reinforcement set whose host has a body.
pub fn build_rebar(
    project: &Project,
    beams: &BTreeMap<ElementId, BeamGeom>,
    walls: &BTreeMap<ElementId, crate::WallGeom>,
    foundations: &BTreeMap<ElementId, crate::FoundationGeom>,
) -> BTreeMap<ElementId, RebarGeom> {
    // The largest stirrup of each host: longitudinal bars sit inside it.
    let mut stirrup: BTreeMap<ElementId, f64> = BTreeMap::new();
    for e in project.elements.values() {
        if let ElementKind::Rebar(r) = &e.kind
            && matches!(r.layout, RebarLayout::Stirrups { .. })
            && let Some(d) = diameter(project, e)
        {
            let s = stirrup.entry(r.host).or_insert(0.0);
            *s = s.max(d);
        }
    }
    // The first longitudinal set of each host: its cross-ties hook around those bars.
    let mut main: BTreeMap<ElementId, (&Rebar, f64)> = BTreeMap::new();
    for e in project.elements.values() {
        if let ElementKind::Rebar(r) = &e.kind
            && matches!(r.layout, RebarLayout::Longitudinal { .. })
            && let Some(d) = diameter(project, e)
        {
            main.entry(r.host).or_insert((r, d));
        }
    }
    // The sets of each wall, by family and face: layers and ties are placed by them.
    let mut wall_sets: BTreeMap<ElementId, Vec<(&Rebar, f64)>> = BTreeMap::new();
    for e in project.elements.values() {
        if let ElementKind::Rebar(r) = &e.kind
            && r.layout.in_wall()
            && let Some(d) = diameter(project, e)
        {
            wall_sets.entry(r.host).or_default().push((r, d));
        }
    }
    // The sets of each slab: the layers of a mesh are stacked by them.
    let mut slab_sets: BTreeMap<ElementId, Vec<(&Rebar, f64)>> = BTreeMap::new();
    for e in project.elements.values() {
        if let ElementKind::Rebar(r) = &e.kind
            && r.layout.in_slab()
            && let Some(d) = diameter(project, e)
        {
            slab_sets.entry(r.host).or_default().push((r, d));
        }
    }
    // The ends of the hosts' axes, when a set laps its bars past them.
    let laps = project.elements.values().any(|e| matches!(&e.kind, ElementKind::Rebar(r) if r.lap_at_joins));
    let ends_of = |g: &BeamGeom| [(g.world(0.0, 0.0), -g.dir_at(0.0)), (g.world(g.length, 0.0), g.dir_at(g.length))];
    let mut wall_ends: AxisEnds = BTreeMap::new();
    let mut beam_ends: AxisEnds = BTreeMap::new();
    let mut strip_ends: AxisEnds = BTreeMap::new();
    if laps {
        wall_ends = walls
            .iter()
            .map(|(id, g)| (*id, [(g.world(0.0, 0.0), -g.tangent(0.0)), (g.world(g.length, 0.0), g.tangent(g.length))]))
            .collect();
        beam_ends = beams.iter().map(|(id, g)| (*id, ends_of(g))).collect();
        strip_ends = foundations.iter().filter_map(|(id, f)| Some((*id, ends_of(f.strip.as_ref()?)))).collect();
    }
    let mut out = BTreeMap::new();
    for e in project.elements.values() {
        let ElementKind::Rebar(r) = &e.kind else { continue };
        let Some(d) = diameter(project, e) else { continue };
        let Some(host) = project.element(r.host) else { continue };
        let ds = stirrup.get(&r.host).copied().unwrap_or(0.0);
        let bars = match &host.kind {
            ElementKind::Beam(_) => {
                let Some(g) = beams.get(&r.host) else { continue };
                let Some(TypeData::Beam { height, .. }) = project.element_type(host.type_id).map(|t| &t.data) else {
                    continue;
                };
                beam_bars(g, *height, r, d, ds, beam_past(r, d, g, &beam_ends))
            }
            // A strip footing: its own bars, or those of a beam in its beam part.
            ElementKind::Foundation(f) if matches!(f.shape, cc_core::FoundationShape::Strip { .. }) => {
                let Some(g) = foundations.get(&r.host).and_then(|f| f.strip.as_ref()) else { continue };
                let Some(data @ TypeData::Foundation { width, thickness, .. }) =
                    project.element_type(host.type_id).map(|t| &t.data)
                else {
                    continue;
                };
                let section = crate::strip_rebar::StripSection {
                    width: *width,
                    thickness: *thickness,
                    haunch: data.strip_haunch(),
                };
                if r.layout.in_strip() {
                    // The bars along the strip lie inside those across it, in the same layer.
                    let across = |s: &Rebar| match (&s.layout, &r.layout) {
                        (
                            RebarLayout::StripTransverse { top: a, .. },
                            RebarLayout::StripLongitudinal { top: b, .. },
                        ) => a == b,
                        _ => false,
                    };
                    let other = project
                        .rebar_of(r.host)
                        .filter(|(_, s)| across(s))
                        .filter_map(|(e, _)| diameter(project, e))
                        .fold(0.0, f64::max);
                    crate::strip_rebar::strip_bars(g, &section, r, d, other, beam_past(r, d, g, &strip_ends))
                } else {
                    // As a beam as wide as the beam part and as deep as the whole section.
                    let part = BeamGeom { half: section.beam_half(), ..g.clone() };
                    beam_bars(&part, data.foundation_depth(), r, d, ds, beam_past(r, d, g, &strip_ends))
                }
            }
            ElementKind::Floor(_) | ElementKind::Foundation(_) => {
                let Some(slab) = crate::slab_rebar::Slab::of(project, host) else { continue };
                // The largest bar of the other sets of the layer that an inner set lies inside.
                let other = slab_sets
                    .get(&r.host)
                    .into_iter()
                    .flatten()
                    .filter(|(s, _)| !std::ptr::eq(*s, r) && s.layout.slab_top() == r.layout.slab_top())
                    .filter(|(s, _)| !matches!(s.layout, RebarLayout::SlabTrim { .. }) && !slab_inner(&s.layout))
                    .map(|(_, d)| *d)
                    .fold(0.0, f64::max);
                crate::slab_rebar::slab_bars(&slab, r, d, other)
            }
            ElementKind::Wall(_) => {
                let Some(g) = walls.get(&r.host) else { continue };
                let all = wall_sets.get(&r.host).map_or(&[][..], |v| v.as_slice());
                let mut context = wall_context(r, all);
                if r.lap_at_joins {
                    let half = half_lap(r, d);
                    let on = |joined: bool, end: usize| {
                        if joined && carries_on(r.host, end, &wall_ends) { half } else { 0.0 }
                    };
                    context.past = (on(g.start_joined, 0), on(g.end_joined, 1));
                }
                crate::wall_rebar::wall_bars(g, r, d, &context)
            }
            ElementKind::Column(c) => {
                let Some(data @ TypeData::Column { section, .. }) = project.element_type(host.type_id).map(|t| &t.data)
                else {
                    continue;
                };
                let outline = project.type_section(data).map(|s| s.outline.outer).unwrap_or_default();
                let shape = ColumnShape {
                    size: section.size(),
                    round: project.section_is_round(data),
                    sector: project.type_sector(data),
                    boxed: is_box(&outline),
                    outline,
                };
                let frame =
                    ColumnFrame { position: c.position, rotation_deg: c.rotation_deg, z: project.column_span(host, c) };
                column_bars(&shape, &frame, r, d, ds, main.get(&r.host).copied())
            }
            _ => continue,
        };
        let (grade, mass_per_meter) = steel(project, e);
        out.insert(e.id, RebarGeom { element: e.id, host: Some(r.host), diameter: d, grade, mass_per_meter, bars });
    }
    // Shaped bars stand where they are drawn.
    for e in project.elements.values() {
        let ElementKind::ShapedRebar(r) = &e.kind else { continue };
        let Some(d) = diameter(project, e) else { continue };
        let z = project.level_elevation(e.level);
        let bars = r.bars.iter().filter_map(|b| shaped_bar(b, z)).collect();
        let (grade, mass_per_meter) = steel(project, e);
        out.insert(e.id, RebarGeom { element: e.id, host: None, diameter: d, grade, mass_per_meter, bars });
    }
    out
}

/// The two ends of each host's axis: the point, and the direction the axis leaves it by.
type AxisEnds = BTreeMap<ElementId, [(DVec2, DVec2); 2]>;

/// Ends of two axes this near are one joint.
const JOINT_REACH: f64 = 0.05;

/// Whether another host of `ends` carries on from the `end` (0: start, 1:
/// end) of `host`: one of its ends lies there and its axis leaves the
/// joint the opposite way, within some ten degrees.
fn carries_on(host: ElementId, end: usize, ends: &AxisEnds) -> bool {
    let Some((at, out)) = ends.get(&host).and_then(|e| e.get(end)).copied() else { return false };
    let others = ends.iter().filter(|(id, _)| **id != host).flat_map(|(_, e)| e.iter());
    others.into_iter().any(|(p, dir)| p.distance(at) <= JOINT_REACH && dir.dot(out) < -0.985)
}

/// Half the lap of the set `r` of bars `d` thick: its splices', else 40 diameters.
fn half_lap(r: &Rebar, d: f64) -> f64 {
    lap_of(r, d) * 0.5
}

/// The lap of the set `r` of bars `d` thick: its splices', else 40 diameters.
fn lap_of(r: &Rebar, d: f64) -> f64 {
    r.splice.filter(Splice::is_valid).map_or(40.0 * d, |s| s.lap)
}

/// How far the bars of `r` along the beam or strip `g` run past its start
/// and end: half a lap where it is joined to another that carries on.
fn beam_past(r: &Rebar, d: f64, g: &BeamGeom, ends: &AxisEnds) -> (f64, f64) {
    if !r.lap_at_joins {
        return (0.0, 0.0);
    }
    let half = half_lap(r, d);
    let on = |joined: bool, end: usize| if joined && carries_on(g.element, end, ends) { half } else { 0.0 };
    (on(g.start == BeamEnd::Beam, 0), on(g.end == BeamEnd::Beam, 1))
}

/// The stretch of the axis a bar along its host covers: `inside` is the
/// stretch the `cover` clear of both end faces, and where the bar runs
/// `past` an end (a length on its own line, `scale` times the axis's) it
/// starts that far beyond the face instead.
pub(crate) fn run_past(inside: (f64, f64), cover: f64, past: (f64, f64), scale: f64) -> (f64, f64) {
    let scale = scale.max(1e-9);
    let from = if past.0 > 0.0 { inside.0 - cover - past.0 / scale } else { inside.0 };
    let to = if past.1 > 0.0 { inside.1 + cover + past.1 / scale } else { inside.1 };
    (from, to)
}

/// Whether a slab's set lies inside the other bars of its layer.
fn slab_inner(layout: &RebarLayout) -> bool {
    matches!(
        layout,
        RebarLayout::SlabStraight { inner: true, .. }
            | RebarLayout::SlabRing { inner: true, .. }
            | RebarLayout::SlabRadial { inner: true, .. }
    )
}

/// What the set `r` needs to know of `all` the sets of its wall.
fn wall_context<'a>(r: &Rebar, all: &[(&'a Rebar, f64)]) -> crate::wall_rebar::WallSets<'a> {
    use RebarLayout::{WallHorizontal as H, WallVertical as V};
    let horizontal = |s: &Rebar| matches!(s.layout, H { .. });
    let on = |side| move |(s, _): &&(&'a Rebar, f64)| s.layout.wall_side() == Some(side);
    // The other family near the same face.
    let other = all
        .iter()
        .filter(|(s, _)| s.layout.wall_side() == r.layout.wall_side() && horizontal(s) != horizontal(r))
        .filter(|(s, _)| matches!(s.layout, H { .. } | V { .. }))
        .map(|(_, d)| *d)
        .fold(0.0, f64::max);
    let first = |side, wanted: bool| {
        all.iter()
            .filter(on(side))
            .find(|(s, _)| matches!(s.layout, H { .. } | V { .. }) && horizontal(s) == wanted)
            .copied()
    };
    // The interior face's outer layer: the set not marked `inner`, else any.
    let interior = all
        .iter()
        .filter(on(cc_core::WallSide::Interior))
        .filter(|(s, _)| matches!(&s.layout, H { inner: false, .. } | V { inner: false, .. }))
        .chain(all.iter().filter(on(cc_core::WallSide::Interior)))
        .map(|(s, d)| (s.cover, *d))
        .next();
    crate::wall_rebar::WallSets {
        other,
        horizontal: first(cc_core::WallSide::Exterior, true),
        vertical: first(cc_core::WallSide::Exterior, false),
        interior,
        past: (0.0, 0.0),
    }
}

/// A shaped bar in the model, `z` up (its level's elevation): closed when its
/// ends meet (the last point dropped); `None` with less than two points apart.
fn shaped_bar(points: &[DVec3], z: f64) -> Option<Bar> {
    let mut points: Vec<DVec3> = points.iter().filter(|p| p.is_finite()).map(|p| *p + DVec3::Z * z).collect();
    points.dedup_by(|a, b| a.distance(*b) < 1e-9);
    let closed = points.len() > 3 && points.first().zip(points.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-6);
    if closed {
        points.pop();
    }
    (points.len() >= 2).then_some(Bar::new(points, closed))
}

/// How many bars go in each layer: `count` bars `d` thick across a width of
/// `across` between the outermost bar centers. Bars that do not fit side by
/// side at the least clear distance go in further layers, the last of
/// [`MAX_LAYERS`] taking what is left.
fn layers(count: usize, across: f64, d: f64) -> Vec<usize> {
    let pitch = d + clear_distance(d);
    let fit = if across.is_finite() && pitch > 0.0 { (across.max(0.0) / pitch).floor() as usize + 1 } else { 1 };
    // Two corner bars always share a layer.
    let fit = fit.max(2);
    let mut out = Vec::new();
    let mut left = count;
    while left > 0 {
        let n = if out.len() + 1 == MAX_LAYERS { left } else { left.min(fit) };
        out.push(n);
        left -= n;
    }
    out
}

/// Heights of side bars between `lo` and `hi` (the lowest and highest bar
/// lines of the section): `count` evenly spread, or with a count of 0 as
/// many as keep them at most `spacing` apart.
fn side_heights(count: u32, spacing: f64, lo: f64, hi: f64) -> Vec<f64> {
    if hi <= lo {
        return Vec::new();
    }
    let n = if count > 0 {
        count as usize
    } else if spacing.is_finite() && spacing > 0.0 {
        (((hi - lo) / spacing) - 1e-9).ceil().max(1.0) as usize - 1
    } else {
        0
    };
    let n = n.min(200);
    (1..=n).map(|i| lo + (hi - lo) * i as f64 / (n + 1) as f64).collect()
}

/// Bars along a beam: longitudinal and side bars from end to end inside the
/// cover (following the arc of an arc beam), stirrups in vertical planes
/// across it (radial on an arc).
fn beam_bars(g: &BeamGeom, height: f64, r: &Rebar, d: f64, ds: f64, past: (f64, f64)) -> Vec<Bar> {
    let (layout, cover, splice) = (&r.layout, r.cover, r.splice);
    let k = g.stretch();
    let u0 = g.start_ext.0.max(g.start_ext.1) + cover;
    let u1 = g.end_ext.0.min(g.end_ext.1) - cover;
    if u1 <= u0 {
        return Vec::new();
    }
    let at = |u: f64, v: f64, z: f64| g.world(u, v).extend(g.z_at(u).0 + z * k);
    // The bars along the lines `(v, z)` of the section, spliced where longer than the stock.
    let runs = |lines: Vec<(f64, f64)>| {
        let mut bars = Vec::new();
        for (index, (v, z)) in lines.into_iter().enumerate() {
            // Length of the bar per meter of the axis: its own radius on an arc, the slope.
            let scale = g.arc.as_ref().map_or(k, |a| (a.radius_at(v) / a.radius.max(1e-9)).max(1e-9));
            let (u0, u1) = run_past((u0, u1), cover, past, scale);
            let length = (u1 - u0) * scale;
            let pieces = splice.map_or_else(|| vec![(0.0, length)], |s| s.pieces(length, index));
            for (from, to) in pieces {
                if bars.len() >= MAX_BARS {
                    return bars;
                }
                let (a, b) = (u0 + from / scale, u0 + to / scale);
                bars.push(match &g.arc {
                    Some(arc) => {
                        let n = cc_core::arc::segments((b - a) / arc.radius.max(1e-9), cc_core::arc::ARC_STEP);
                        let points = (0..=n).map(|i| at(a + (b - a) * i as f64 / n as f64, v, z)).collect();
                        Bar { points, closed: false, true_length: Some(to - from) }
                    }
                    None => Bar::new(vec![at(a, v, z), at(b, v, z)], false),
                });
            }
        }
        bars
    };
    let inset = cover + ds + d / 2.0;
    match *layout {
        RebarLayout::Longitudinal { face, count } => {
            let half = (g.half - inset).max(0.0);
            let step = d + clear_distance(d);
            let mut lines = Vec::new();
            for (layer, n) in layers(count as usize, 2.0 * half, d).into_iter().enumerate() {
                let z = match face {
                    BarFace::Top => (height - inset - step * layer as f64).max(inset),
                    BarFace::Bottom | BarFace::Perimeter => {
                        (inset + step * layer as f64).min((height - inset).max(inset))
                    }
                };
                lines.extend(spread(n, half).into_iter().map(|v| (v, z)));
            }
            runs(lines)
        }
        RebarLayout::Sides { count, spacing } => {
            let v = (g.half - inset).max(0.0);
            let heights = side_heights(count, spacing, inset, height - inset);
            runs(heights.into_iter().flat_map(|z| [(-v, z), (v, z)]).collect())
        }
        // Cross-ties belong to columns, wall bars to walls, slab bars to slabs.
        RebarLayout::Ties { .. }
        | RebarLayout::WallHorizontal { .. }
        | RebarLayout::WallVertical { .. }
        | RebarLayout::WallTies { .. }
        | RebarLayout::SlabStraight { .. }
        | RebarLayout::SlabRing { .. }
        | RebarLayout::SlabRadial { .. }
        | RebarLayout::SlabTrim { .. }
        | RebarLayout::StripTransverse { .. }
        | RebarLayout::StripLongitudinal { .. } => Vec::new(),
        RebarLayout::Stirrups { spacing } => {
            let inset = cover + d / 2.0;
            let (v, z0, z1) = ((g.half - inset).max(0.0), inset, (height - inset).max(inset));
            stations(u0, u1, spacing)
                .into_iter()
                .map(|u| Bar::new(vec![at(u, -v, z0), at(u, v, z0), at(u, v, z1), at(u, -v, z1)], true))
                .collect()
        }
    }
}

/// A column's section as its bars see it, in the column's own frame.
struct ColumnShape {
    /// Width and depth of its box.
    size: (f64, f64),
    round: bool,
    /// An annular sector's ring (its faces are then exact arcs).
    sector: Option<SectorPlacement>,
    /// Whether the outline is that box and nothing else.
    boxed: bool,
    outline: Vec<DVec2>,
}

impl ColumnShape {
    /// The section's faces `inset` inside it.
    fn faces(&self, inset: f64) -> Vec<SectionFace> {
        match &self.sector {
            Some(s) => faces::sector_faces(s, inset),
            None => faces::outline_faces(&self.outline, inset),
        }
    }

    /// The longitudinal bars of the set `r` (bars `d` thick inside ties `ds` thick).
    fn bars(&self, r: &Rebar, d: f64, ds: f64) -> Vec<FaceBar> {
        let RebarLayout::Longitudinal { count, .. } = r.layout else { return Vec::new() };
        let inset = r.cover + ds + d / 2.0;
        let (w, depth) = self.size;
        let bar = |at: DVec2, inward: DVec2, corner: bool| FaceBar { at, inward, corner };
        if self.round {
            return circle(count as usize, (w / 2.0 - inset).max(0.0))
                .into_iter()
                .map(|p| bar(p, -p.normalize_or_zero(), false))
                .collect();
        }
        if self.boxed && r.faces.is_empty() {
            let (a, b) = ((w / 2.0 - inset).max(0.0), (depth / 2.0 - inset).max(0.0));
            return perimeter(count as usize, a, b)
                .into_iter()
                .enumerate()
                .map(|(i, p)| {
                    // A bar between corners looks across the side it stands on.
                    let inward =
                        if (p.y.abs() - b).abs() < 1e-9 { -DVec2::Y * p.y.signum() } else { -DVec2::X * p.x.signum() };
                    bar(p, inward, i < 4)
                })
                .collect();
        }
        faces::face_bars(&self.faces(inset), &r.faces, count as usize)
    }
}

/// The faces of a column type's section `inset` inside it, in the order
/// bars are given face by face: counter-clockwise, an annular sector's
/// from its outer arc. Empty for other types and failed sections.
pub fn column_faces(project: &Project, data: &TypeData, inset: f64) -> Vec<SectionFace> {
    let TypeData::Column { section, .. } = data else { return Vec::new() };
    let outline = project.type_section(data).map(|s| s.outline.outer).unwrap_or_default();
    let shape = ColumnShape {
        size: section.size(),
        round: project.section_is_round(data),
        sector: project.type_sector(data),
        boxed: is_box(&outline),
        outline,
    };
    shape.faces(inset)
}

/// Whether `outline` is its own bounding box: four corners, nothing else.
fn is_box(outline: &[DVec2]) -> bool {
    let lo = outline.iter().fold(DVec2::INFINITY, |m, p| m.min(*p));
    let hi = outline.iter().fold(DVec2::NEG_INFINITY, |m, p| m.max(*p));
    let at = |v: f64, a: f64, b: f64| (v - a).abs() < 1e-9 || (v - b).abs() < 1e-9;
    outline.len() == 4 && outline.iter().all(|p| at(p.x, lo.x, hi.x) && at(p.y, lo.y, hi.y))
}

/// Where a column stands: its point, turn and base and top elevations.
struct ColumnFrame {
    position: DVec2,
    rotation_deg: f64,
    z: (f64, f64),
}

/// Length of a tie's hook bent by `turn` radians: the bend around a pin
/// three bar diameters across, then a straight end of five diameters (at
/// least 5 cm).
pub(crate) fn hook(turn: f64, d: f64) -> f64 {
    turn * 2.0 * d + (5.0 * d).max(0.05)
}

/// Bars of a column: vertical bars around the section over its height
/// (cut in lifts by a splice), horizontal ties up the column following its
/// outline, and cross-ties between the bars of its longitudinal set `main`.
fn column_bars(
    shape: &ColumnShape,
    frame: &ColumnFrame,
    r: &Rebar,
    d: f64,
    ds: f64,
    main: Option<(&Rebar, f64)>,
) -> Vec<Bar> {
    let rot = DVec2::from_angle(frame.rotation_deg.to_radians());
    let place = |p: DVec2, z: f64| (frame.position + rot.rotate(p)).extend(z);
    let (base, top) = frame.z;
    let (w, depth) = shape.size;
    match r.layout {
        RebarLayout::Longitudinal { .. } => {
            // Every bar is cut at the same heights: the lifts the column is cast
            // in, at the heights given or equal ones under the stock length.
            let lifts = match r.lifts.is_empty() {
                false => Rebar::lift_pieces(&r.lifts, top - base, lap_of(r, d)),
                true => r.splice.map_or_else(|| vec![(0.0, top - base)], |s| s.pieces(top - base, 0)),
            };
            let mut bars = Vec::new();
            for bar in shape.bars(r, d, ds) {
                for (from, to) in &lifts {
                    if bars.len() >= MAX_BARS {
                        return bars;
                    }
                    bars.push(Bar::new(vec![place(bar.at, base + from), place(bar.at, base + to)], false));
                }
            }
            bars
        }
        // Side bars belong to beams, wall bars to walls, slab bars to slabs.
        RebarLayout::Sides { .. }
        | RebarLayout::WallHorizontal { .. }
        | RebarLayout::WallVertical { .. }
        | RebarLayout::WallTies { .. }
        | RebarLayout::SlabStraight { .. }
        | RebarLayout::SlabRing { .. }
        | RebarLayout::SlabRadial { .. }
        | RebarLayout::SlabTrim { .. }
        | RebarLayout::StripTransverse { .. }
        | RebarLayout::StripLongitudinal { .. } => Vec::new(),
        RebarLayout::Stirrups { spacing } => {
            let inset = r.cover + d / 2.0;
            // Overlapping stirrups: one around each rectangle the section is made of.
            let rects = if r.overlapping && !shape.round { faces::rectangles(&shape.outline) } else { Vec::new() };
            if rects.len() > 1 {
                let loops: Vec<[DVec2; 4]> = rects
                    .iter()
                    .map(|(lo, hi)| (*lo + DVec2::splat(inset), *hi - DVec2::splat(inset)))
                    .filter(|(lo, hi)| hi.x - lo.x > 1e-6 && hi.y - lo.y > 1e-6)
                    .map(|(lo, hi)| [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)])
                    .collect();
                let mut out = Vec::new();
                for z in stations(base + r.cover, top - r.cover, spacing) {
                    for l in &loops {
                        if out.len() >= MAX_BARS {
                            return out;
                        }
                        out.push(Bar::new(l.iter().map(|p| place(*p, z)).collect(), true));
                    }
                }
                return out;
            }
            let (outline, length) = if shape.round {
                (circle(ROUND, (w / 2.0 - inset).max(0.0)), None)
            } else if shape.boxed {
                let (a, b) = ((w / 2.0 - inset).max(0.0), (depth / 2.0 - inset).max(0.0));
                (vec![DVec2::new(-a, -b), DVec2::new(a, -b), DVec2::new(a, b), DVec2::new(-a, b)], None)
            } else {
                // Around the outline, its curved faces as curves.
                let (points, length) = faces::face_loop(&shape.faces(inset));
                (points, Some(length))
            };
            if outline.len() < 3 {
                return Vec::new();
            }
            stations(base + r.cover, top - r.cover, spacing)
                .into_iter()
                .map(|z| Bar {
                    points: outline.iter().map(|p| place(*p, z)).collect(),
                    closed: true,
                    true_length: length,
                })
                .collect()
        }
        RebarLayout::Ties { spacing } => {
            let Some((set, dm)) = main else { return Vec::new() };
            let bars = shape.bars(set, dm, ds);
            // The lines the main bars stand on, to find the bar opposite.
            let lines = match shape.round || (shape.boxed && set.faces.is_empty()) {
                true => faces::outline_faces(&bars.iter().filter(|b| b.corner).map(|b| b.at).collect::<Vec<_>>(), 0.0),
                false => shape.faces(set.cover + ds + dm / 2.0),
            };
            let near = (3.0 * dm).max(0.03);
            let mut ties: Vec<(DVec2, DVec2)> = Vec::new();
            for bar in bars.iter().filter(|b| !b.corner) {
                let Some(hit) = faces::across(&lines, bar.at, bar.inward, near) else { continue };
                // Onto the bar that stands there, when one does.
                let to = bars
                    .iter()
                    .map(|b| b.at)
                    .filter(|p| p.distance(hit) < near)
                    .min_by(|p, q| p.distance(hit).total_cmp(&q.distance(hit)))
                    .unwrap_or(hit);
                // One tie between two bars, whichever it was found from.
                let same = |a: DVec2, b: DVec2| a.distance(b) < near;
                if !ties.iter().any(|(p, q)| same(*p, to) && same(*q, bar.at)) {
                    ties.push((bar.at, to));
                }
            }
            let hooks = hook(0.75 * std::f64::consts::PI, d) + hook(std::f64::consts::PI, d);
            let mut out = Vec::new();
            for z in stations(base + r.cover, top - r.cover, spacing) {
                for (a, b) in &ties {
                    if out.len() >= MAX_BARS {
                        return out;
                    }
                    let points = vec![place(*a, z), place(*b, z)];
                    out.push(Bar { points, closed: false, true_length: Some(a.distance(*b) + hooks) });
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stirrups_fill_the_length_centered() {
        // 1.00 m at 0.15: 7 stirrups spanning 0.90, starting 0.05 in.
        let s = stations(0.0, 1.0, 0.15);
        assert_eq!(s.len(), 7);
        assert!((s[0] - 0.05).abs() < 1e-12 && (s[6] - 0.95).abs() < 1e-12);
        assert!(stations(1.0, 0.0, 0.15).is_empty());
    }

    #[test]
    fn perimeter_bars_take_the_corners_then_the_longer_sides() {
        let p = perimeter(6, 0.2, 0.1);
        assert_eq!(p.len(), 6);
        // The extra two go to the long (0.4 m) sides, at their middles.
        assert!(p.contains(&DVec2::new(0.0, -0.1)) && p.contains(&DVec2::new(0.0, 0.1)));
        assert_eq!(perimeter(4, 0.2, 0.1).len(), 4);
    }

    #[test]
    fn bars_that_do_not_fit_take_a_second_layer() {
        // 0.14 m between the outer bar centers, 16 mm bars 36 mm apart: four fit.
        assert_eq!(layers(4, 0.14, 0.016), vec![4]);
        assert_eq!(layers(6, 0.14, 0.016), vec![4, 2]);
        // Two corner bars share a layer however narrow the beam.
        assert_eq!(layers(3, 0.01, 0.016), vec![2, 1]);
        // The last layer takes what is left.
        assert_eq!(layers(11, 0.01, 0.016), vec![2, 2, 2, 5]);
        assert!(layers(0, 0.14, 0.016).is_empty());
    }

    #[test]
    fn side_bars_come_from_a_count_or_a_spacing() {
        let h = side_heights(2, 0.0, 0.05, 0.65);
        assert!((h[0] - 0.25).abs() < 1e-12 && (h[1] - 0.45).abs() < 1e-12, "{h:?}");
        // 0.60 m at most 0.20 apart: two bars, 0.20 from each other and the ends.
        assert_eq!(side_heights(0, 0.2, 0.05, 0.65).len(), 2);
        assert_eq!(side_heights(0, 0.25, 0.05, 0.65).len(), 2);
        assert!(side_heights(0, 0.6, 0.05, 0.65).is_empty());
        assert!(side_heights(3, 0.0, 0.5, 0.5).is_empty());
    }

    #[test]
    fn long_bars_are_cut_in_the_fewest_pieces_and_staggered() {
        let s = Splice { max_length: 12.0, lap: 0.5 };
        // Within the stock: one piece, whatever the bar.
        assert_eq!(s.pieces(12.0, 0), vec![(0.0, 12.0)]);
        assert_eq!(s.pieces(8.0, 1), vec![(0.0, 8.0)]);
        // 23.5 m is exactly two pieces of 12 m lapping 0.5 m.
        let two = s.pieces(23.5, 0);
        assert_eq!(two.len(), 2);
        assert!((two[0].1 - 12.0).abs() < 1e-9 && (two[1].0 - 11.5).abs() < 1e-9 && (two[1].1 - 23.5).abs() < 1e-9);
        // A little more takes three, each as long as the others.
        let three = s.pieces(23.6, 0);
        assert_eq!(three.len(), 3);
        let lengths: Vec<f64> = three.iter().map(|(a, b)| b - a).collect();
        assert!(lengths.iter().all(|l| (l - lengths[0]).abs() < 1e-9 && *l <= 12.0), "{lengths:?}");
        assert!((lengths.iter().sum::<f64>() - (23.6 + 2.0 * 0.5)).abs() < 1e-9);
        // The next bar is cut half a piece off: one more piece, short ones at both ends.
        let odd = s.pieces(23.6, 1);
        assert_eq!(odd.len(), 4);
        assert!((odd[0].0).abs() < 1e-12 && (odd[3].1 - 23.6).abs() < 1e-9);
        assert!(odd.windows(2).all(|w| (w[0].1 - w[1].0 - 0.5).abs() < 1e-9), "{odd:?}");
        assert!(((odd[0].1 - odd[0].0) - (odd[3].1 - odd[3].0)).abs() < 1e-9);
        // Values a file may hold: no splices rather than a loop.
        assert_eq!(Splice { max_length: 0.0, lap: 0.5 }.pieces(30.0, 0), vec![(0.0, 30.0)]);
        assert_eq!(Splice { max_length: 12.0, lap: 12.0 }.pieces(30.0, 0), vec![(0.0, 30.0)]);
        assert_eq!(s.pieces(f64::NAN, 0).len(), 1);
        assert!(s.pieces(1e9, 0).len() <= 10_001);
    }

    #[test]
    fn a_stirrup_is_a_closed_loop() {
        let bar = Bar::new(
            vec![DVec3::ZERO, DVec3::new(0.1, 0.0, 0.0), DVec3::new(0.1, 0.0, 0.3), DVec3::new(0.0, 0.0, 0.3)],
            true,
        );
        assert!((bar.length() - 0.8).abs() < 1e-12);
    }
}
