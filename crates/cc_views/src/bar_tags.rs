//! Bar tags in plans, elevations and sections (see `cc_core::bar_tag`):
//! the text `2x 25 N1 Ø12.5 c/12 C=500` derived from the tagged set, with an
//! optional leader, and the set's bars drawn as thin lines in that view (no
//! view draws reinforcement otherwise, so a tag always points at its bars;
//! bars seen end-on are dots). Sizes are paper millimeters at the view scale.

use crate::{Anchor, Drawing2D, LineStyle, format};
use cc_core::bar_tag::{BarTag, DEFAULT_TEXT_SIZE};
use cc_core::{ElementId, ElementKind, Project, RebarLayout, TypeData, View};
use cc_geom::ModelGeometry;
use cc_geom::bar_marks::BarMarks;
use glam::{DVec2, DVec3};
use std::collections::BTreeSet;

/// Average character width, as a fraction of the text size (as text notes).
const CHAR_WIDTH: f64 = 0.6;
/// Nearly invisible fill that makes the text clickable.
const PICK_AREA: [u8; 4] = [255, 255, 255, 1];
/// Fill of leader dots and bars seen end-on.
const DOT_FILL: [u8; 4] = [20, 20, 20, 255];
/// Most points drawn for one set's bars.
const MAX_POINTS: usize = 200_000;

/// The spacing of a set's bars: one, or several (zones of different spacings).
enum Spacing {
    One(f64),
    Varied,
}

/// The spacing of the set's bars, when its layout has one.
fn spacing(project: &Project, set: ElementId) -> Option<Spacing> {
    let ElementKind::Rebar(r) = &project.element(set)?.kind else { return None };
    match &r.layout {
        RebarLayout::Stirrups { spacing }
        | RebarLayout::Ties { spacing }
        | RebarLayout::WallVertical { spacing, .. }
        | RebarLayout::SlabStraight { spacing, .. }
        | RebarLayout::SlabRing { spacing, .. }
        | RebarLayout::StripTransverse { spacing, .. }
        | RebarLayout::StripLongitudinal { spacing, .. } => Some(Spacing::One(*spacing)),
        RebarLayout::SlabTrim { spacing, count, .. } if *count > 1 => Some(Spacing::One(*spacing)),
        RebarLayout::WallHorizontal { zones, .. } => {
            let first = zones.first()?.spacing;
            if zones.iter().all(|z| (z.spacing - first).abs() < 1e-9) {
                Some(Spacing::One(first))
            } else {
                Some(Spacing::Varied)
            }
        }
        _ => None,
    }
}

/// A spacing in centimeters (the project's length format in imperial projects).
fn spacing_text(meters: f64) -> String {
    if format::imperial() { format::meters(meters) } else { format::trimmed(meters * 100.0, 1) }
}

/// The text of `tag`: its parts, from the tagged set's bars (in `geom`)
/// and their `marks`; `None` when the set has no bars there.
pub fn text(project: &Project, geom: &ModelGeometry, marks: &BarMarks, tag: &BarTag) -> Option<String> {
    let set = geom.rebar.get(&tag.set)?;
    let parts = tag.parts;
    let mut out: Vec<String> = Vec::new();
    let mut lead = String::new();
    if tag.multiplier > 1 {
        lead = format!("{}x", tag.multiplier);
    }
    if parts.count {
        lead = if lead.is_empty() { set.count().to_string() } else { format!("{lead} {}", set.count()) };
    }
    if !lead.is_empty() {
        out.push(lead);
    }
    if parts.mark {
        let of_set = marks.of_set(tag.set);
        if let Some(first) = of_set.first() {
            out.push(if of_set.len() > 1 { format!("{}+", first.mark) } else { first.mark.clone() });
        }
    }
    if parts.diameter {
        out.push(format::bar_diameter(set.diameter));
    }
    if parts.spacing {
        match spacing(project, tag.set) {
            Some(Spacing::One(s)) => out.push(format!("c/{}", spacing_text(s))),
            Some(Spacing::Varied) => out.push("c/var".into()),
            None => {}
        }
    }
    if parts.length {
        let mut lengths: BTreeSet<String> = set.bars.iter().map(|b| format::bar_length(b.length())).collect();
        match (lengths.len(), lengths.pop_first()) {
            (1, Some(l)) => out.push(format!("C={l}")),
            (0, _) => {}
            _ => out.push("C=var".into()),
        }
    }
    Some(out.join(" "))
}

/// How the view maps the model: plans from above, elevations and sections
/// through their projection.
fn mapping(view: &View) -> Option<Box<dyn Fn(DVec3) -> DVec2>> {
    if view.is_plan() {
        return Some(Box::new(|p: DVec3| p.truncate()));
    }
    let projection = view.projection()?;
    Some(Box::new(move |p: DVec3| projection.project(p.truncate(), p.z)))
}

/// Whether `view` takes bar tags: plans, elevations and sections.
pub fn takes_tags(view: &View) -> bool {
    view.is_plan() || view.projection().is_some()
}

/// The bar tags of `view`, with the bars of the sets they tag.
pub fn bar_tags(d: &mut Drawing2D, project: &Project, geom: &ModelGeometry, view: &View) {
    let tags: Vec<(ElementId, cc_core::ElementTypeId, &BarTag)> = project
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            ElementKind::BarTag(t) if t.view == view.id => Some((e.id, e.type_id, t.as_ref())),
            _ => None,
        })
        .collect();
    if tags.is_empty() {
        return;
    }
    let Some(map) = mapping(view) else { return };
    let marks = cc_geom::bar_marks::resolve(project, &geom.rebar);
    let k = view.scale.max(1) as f64 / 1000.0;
    let mut drawn = BTreeSet::new();
    for (id, type_id, tag) in tags {
        // The set's bars, once per view.
        if drawn.insert(tag.set)
            && let Some(set) = geom.rebar.get(&tag.set)
        {
            let mut budget = MAX_POINTS;
            for bar in &set.bars {
                let pts: Vec<DVec2> = bar.points.iter().map(|p| map(*p)).collect();
                if pts.len() > budget {
                    break;
                }
                budget -= pts.len();
                let run: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
                if run < 1e-6 {
                    // Seen end-on: a dot.
                    let Some(&at) = pts.first() else { continue };
                    let r = (set.diameter / 2.0).max(0.35 * k);
                    let dot = (0..16).map(|i| at + DVec2::from_angle(i as f64 / 16.0 * std::f64::consts::TAU) * r);
                    d.fill(dot.collect(), DOT_FILL, Some(tag.set));
                } else {
                    d.polyline(&pts, bar.closed, LineStyle::Thin, Some(tag.set));
                }
            }
        }
        let Some(line) = text(project, geom, &marks, tag) else { continue };
        let size = match project.element_type(type_id).map(|t| &t.data) {
            Some(TypeData::BarTag { text_size }) => *text_size,
            _ => DEFAULT_TEXT_SIZE,
        };
        let angle = tag.rotation_deg.to_radians();
        let (u, v) = (DVec2::from_angle(angle), DVec2::from_angle(angle).perp());
        let (w, h) = (line.chars().count() as f64 * size * CHAR_WIDTH * k, size * k);
        let at = tag.position;
        d.fill(vec![at, at + u * w, at + u * w + v * h, at + v * h], PICK_AREA, Some(id));
        d.tag_text(at, line, size, Anchor::LeftBottom, angle, Some(id));
        if let Some(end) = tag.leader {
            // From the nearer end of the text's baseline.
            let from = if end.distance(at) <= end.distance(at + u * w) { at } else { at + u * w };
            d.line(from, end, LineStyle::Annotation, Some(id));
            let r = 0.5 * k;
            let dot = (0..16).map(|i| end + DVec2::from_angle(i as f64 / 16.0 * std::f64::consts::TAU) * r);
            d.fill(dot.collect(), DOT_FILL, Some(id));
        }
    }
}
