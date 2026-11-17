//! Crop regions and callout marks (see [`cc_core::callout`]).

use crate::{Anchor, Drawing2D, LineStyle, Pen, Prim};
use cc_core::{CropBox, ElementId, Project, View, ViewId};
use glam::DVec2;
use std::collections::BTreeSet;

/// Diameter of a callout's tag on paper, millimeters (as a section head).
const TAG_MM: f64 = 12.0;
/// Pen of a callout's outline, paper millimeters.
const CALLOUT_PEN_MM: f64 = 0.35;
/// Largest corner radius of the outline on paper, millimeters.
const CORNER_MM: f64 = 4.0;
/// Paper height of the detail number and of the sheet number, millimeters.
const NUMBER_MM: f64 = 3.5;
const SHEET_MM: f64 = 2.5;
/// Straight pieces per rounded corner.
const CORNER_STEPS: usize = 6;

/// A callout's mark in its parent, as drawn (meters on the parent's plane).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalloutHandle {
    /// The detail (or drafting) view the callout stands for.
    pub view: ViewId,
    pub rect: CropBox,
    /// Center of the tag's circle.
    pub tag: DVec2,
    pub radius: f64,
}

impl CalloutHandle {
    /// Whether `p` lies on the mark: within `tol` of the rectangle's sides or in the tag.
    pub fn hits(&self, p: DVec2, tol: f64) -> bool {
        if self.tag.distance(p) <= self.radius + tol {
            return true;
        }
        let c = self.rect.corners();
        (0..4).any(|i| cc_geom::polygon::closest_on_segment(c[i], c[(i + 1) % 4], p).0.distance(p) <= tol)
    }
}

/// The marks of the callouts drawn in `view`, each listed in `d.callouts`.
pub(crate) fn marks(d: &mut Drawing2D, project: &Project, view: &View) {
    if !view.kind.croppable() {
        return;
    }
    let k = f64::from(view.scale.max(1)) / 1000.0;
    for detail in project.callouts_in(view.id) {
        let Some(rect) = detail.callout_rect().filter(|r| r.is_valid()) else { continue };
        let (number, sheet) = crate::plan::section_labels(project, detail);
        let handle = mark(d, rect, k, (&number, &sheet));
        d.callouts.push(CalloutHandle { view: detail.id, ..handle });
    }
}

/// A callout mark: the rectangle with rounded corners in a medium pen and,
/// off its lower right corner on a leader, a circle split by a horizontal
/// line with the detail number above it and the sheet number below; `k`
/// turns paper millimeters into meters on the view.
fn mark(d: &mut Drawing2D, rect: CropBox, k: f64, (number, sheet): (&str, &str)) -> CalloutHandle {
    let size = rect.size();
    let rc = (CORNER_MM * k).min(size.min_element() * 0.25).max(0.0);
    let pen = Pen { weight: Some(CALLOUT_PEN_MM), ..Default::default() };
    let outline = rounded(rect, rc);
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        if a.distance_squared(b) > 1e-14 {
            d.prims.push(Prim::Line { a, b, style: LineStyle::Annotation, element: None, pen: Some(pen) });
        }
    }
    let r = TAG_MM * 0.5 * k;
    let diagonal = DVec2::new(1.0, -1.0).normalize();
    let corner = DVec2::new(rect.max.x - rc, rect.min.y + rc) + diagonal * rc;
    let tag = corner + diagonal * (r * 2.5);
    d.line(corner, tag - diagonal * r, LineStyle::Annotation, None);
    let circle: Vec<DVec2> = (0..32)
        .map(|i| {
            let a = f64::from(i) / 32.0 * std::f64::consts::TAU;
            tag + DVec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    d.fill(circle.clone(), [255, 255, 255, 255], None);
    d.polyline(&circle, true, LineStyle::Annotation, None);
    d.line(tag - DVec2::X * r, tag + DVec2::X * r, LineStyle::Annotation, None);
    d.text_at(tag + DVec2::Y * r * 0.48, number, NUMBER_MM, Anchor::Center, 0.0);
    d.text_at(tag - DVec2::Y * r * 0.5, sheet, SHEET_MM, Anchor::Center, 0.0);
    CalloutHandle { view: ViewId(0), rect, tag, radius: r }
}

/// The outline of `rect` with corners rounded to radius `rc`, counter-clockwise.
fn rounded(rect: CropBox, rc: f64) -> Vec<DVec2> {
    if rc <= 0.0 {
        return rect.corners().to_vec();
    }
    let centers = [
        (DVec2::new(rect.max.x - rc, rect.min.y + rc), -0.5),
        (DVec2::new(rect.max.x - rc, rect.max.y - rc), 0.0),
        (DVec2::new(rect.min.x + rc, rect.max.y - rc), 0.5),
        (DVec2::new(rect.min.x + rc, rect.min.y + rc), 1.0),
    ];
    let mut out = Vec::with_capacity(4 * (CORNER_STEPS + 1));
    for (c, start) in centers {
        for i in 0..=CORNER_STEPS {
            let a = (start + 0.5 * i as f64 / CORNER_STEPS as f64) * std::f64::consts::PI;
            out.push(c + DVec2::new(a.cos(), a.sin()) * rc);
        }
    }
    out
}

/// Crop the drawing of `view` to `rect`: lines and fills of the model (and
/// of anything not owned by the view) are clipped to it, texts and pictures
/// outside it dropped; what the view owns (detail lines, regions, notes,
/// dimensions…) is kept whole.
pub(crate) fn crop(d: &mut Drawing2D, project: &Project, view: ViewId, rect: CropBox) {
    let owned: BTreeSet<ElementId> =
        project.elements.values().filter(|e| e.owner_view() == Some(view)).map(|e| e.id).collect();
    let mut under = 0;
    let mut out = Vec::with_capacity(d.prims.len());
    for (i, prim) in std::mem::take(&mut d.prims).into_iter().enumerate() {
        let kept = if prim.element().is_some_and(|e| owned.contains(&e)) { Some(prim) } else { clip(prim, rect) };
        if let Some(p) = kept {
            under += usize::from(i < d.underlay);
            out.push(p);
        }
    }
    d.prims = out;
    d.underlay = under;
    d.tags.retain(|t| rect.contains(t.center));
    d.sections.retain(|s| rect.contains(s.start) || rect.contains(s.end));
    d.callouts.retain(|c| rect.contains(c.tag) || rect.contains(c.rect.min) || rect.contains(c.rect.max));
}

/// One primitive clipped to `rect` (`None` when nothing of it is left).
fn clip(prim: Prim, rect: CropBox) -> Option<Prim> {
    match prim {
        Prim::Line { a, b, style, element, pen } => {
            let (a, b) = clip_segment(a, b, rect)?;
            Some(Prim::Line { a, b, style, element, pen })
        }
        Prim::Fill { pts, color, element, cut } => {
            let pts = clip_polygon(&pts, rect);
            (pts.len() >= 3).then_some(Prim::Fill { pts, color, element, cut })
        }
        Prim::Text { pos, .. } => rect.contains(pos).then_some(prim),
        Prim::Image { origin, u, v, .. } => rect.contains(origin + (u + v) * 0.5).then_some(prim),
    }
}

/// The part of segment `a`–`b` inside `rect` (Liang–Barsky).
fn clip_segment(a: DVec2, b: DVec2, rect: CropBox) -> Option<(DVec2, DVec2)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [(-d.x, a.x - rect.min.x), (d.x, rect.max.x - a.x), (-d.y, a.y - rect.min.y), (d.y, rect.max.y - a.y)]
    {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    let (p0, p1) = (a + d * t0, a + d * t1);
    (p0.distance_squared(p1) > 1e-14).then_some((p0, p1))
}

/// A polygon clipped to `rect` (Sutherland–Hodgman: a convex clip region
/// keeps any polygon's area exactly; concave parts may leave zero-width
/// bridges along the sides, which fill nothing).
fn clip_polygon(pts: &[DVec2], rect: CropBox) -> Vec<DVec2> {
    let mut poly = pts.to_vec();
    let edges: [fn(DVec2, &CropBox) -> f64; 4] =
        [|p, r| p.x - r.min.x, |p, r| r.max.x - p.x, |p, r| p.y - r.min.y, |p, r| r.max.y - p.y];
    for inside in edges {
        if poly.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(poly.len() + 4);
        for i in 0..poly.len() {
            let (cur, prev) = (poly[i], poly[(i + poly.len() - 1) % poly.len()]);
            let (dc, dp) = (inside(cur, &rect), inside(prev, &rect));
            if dc >= 0.0 {
                if dp < 0.0 {
                    next.push(prev + (cur - prev) * (dp / (dp - dc)));
                }
                next.push(cur);
            } else if dp >= 0.0 {
                next.push(prev + (cur - prev) * (dp / (dp - dc)));
            }
        }
        poly = next;
    }
    poly
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit() -> CropBox {
        CropBox::new(DVec2::ZERO, DVec2::ONE)
    }

    #[test]
    fn segments_clip_to_the_box() {
        let (a, b) = clip_segment(DVec2::new(-1.0, 0.5), DVec2::new(2.0, 0.5), unit()).unwrap();
        assert!(a.distance(DVec2::new(0.0, 0.5)) < 1e-12 && b.distance(DVec2::new(1.0, 0.5)) < 1e-12);
        assert!(clip_segment(DVec2::new(-1.0, 2.0), DVec2::new(2.0, 2.0), unit()).is_none());
        assert!(clip_segment(DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0), unit()).is_none());
    }

    #[test]
    fn polygons_clip_to_the_box() {
        let square = [DVec2::new(-1.0, -1.0), DVec2::new(0.5, -1.0), DVec2::new(0.5, 0.5), DVec2::new(-1.0, 0.5)];
        let clipped = clip_polygon(&square, unit());
        let area = cc_core::mass::area(&clipped);
        assert!((area - 0.25).abs() < 1e-12, "{area}");
        let outside = [DVec2::new(2.0, 2.0), DVec2::new(3.0, 2.0), DVec2::new(3.0, 3.0)];
        assert!(clip_polygon(&outside, unit()).len() < 3);
    }

    #[test]
    fn rounded_outlines_stay_in_the_box() {
        let pts = rounded(CropBox::new(DVec2::ZERO, DVec2::new(2.0, 1.0)), 0.2);
        assert_eq!(pts.len(), 4 * (CORNER_STEPS + 1));
        assert!(pts.iter().all(|p| p.x > -1e-9 && p.x < 2.0 + 1e-9 && p.y > -1e-9 && p.y < 1.0 + 1e-9));
    }
}
