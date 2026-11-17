//! Fills of cut regions (plans and sections) from the material's cut pattern.

use crate::{Drawing2D, LineStyle};
use cc_core::{ElementId, Pattern, PatternFill, Project};
use glam::DVec2;

/// Neutral fill of cut regions whose material has no cut pattern.
pub(crate) const POCHE: [u8; 4] = [196, 196, 192, 255];

/// Base spacing of hatch patterns on paper, millimeters.
pub(crate) const PATTERN_MM: f64 = 1.8;

/// Hatch spacing in drawing units for a view at `1:scale`.
pub(crate) fn spacing(scale: u32) -> f64 {
    PATTERN_MM * scale.max(1) as f64 / 1000.0
}

/// The color under a cut fill: the solid color of `fill`, white under
/// pattern lines (a background keeps the region selectable), or the
/// neutral poche when the material has no cut pattern.
fn base_color(fill: Option<PatternFill>) -> [u8; 4] {
    let rgba = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    match fill {
        None => POCHE,
        Some(f) if f.foreground == Pattern::Solid => rgba(f.foreground_color),
        Some(f) if f.background == Pattern::Solid => rgba(f.background_color),
        Some(_) => [255, 255, 255, 255],
    }
}

/// Fill `poly` with the cut pattern of `material` (or the neutral poche).
pub(crate) fn cut_fill(
    d: &mut Drawing2D,
    project: &Project,
    material: &str,
    poly: Vec<DVec2>,
    spacing: f64,
    id: Option<ElementId>,
) {
    cut_fill_with_holes(d, project, material, &poly, &[], spacing, id);
}

/// [`cut_fill`] of a region with `holes`: one keyhole ring (triangles when
/// no ring can be made), the pattern lines clipped out of the holes.
pub(crate) fn cut_fill_with_holes(
    d: &mut Drawing2D,
    project: &Project,
    material: &str,
    outer: &[DVec2],
    holes: &[Vec<DVec2>],
    spacing: f64,
    id: Option<ElementId>,
) {
    let fill = project.material(material).map(|m| m.graphics.cut).filter(|f| !f.is_empty());
    let base = base_color(fill);
    if holes.is_empty() {
        d.cut_fill(outer.to_vec(), base, id);
    } else {
        match cc_geom::polygon::keyhole(outer, holes) {
            Some(ring) => d.cut_fill(ring, base, id),
            None => {
                for t in cc_geom::polygon::triangulate_with_holes(outer, holes) {
                    d.cut_fill(t.to_vec(), base, id);
                }
            }
        }
    }
    let Some(fill) = fill else { return };
    for pattern in [fill.background, fill.foreground] {
        if matches!(pattern, Pattern::Solid | Pattern::None) {
            continue;
        }
        for (a, b) in crate::hatch::hatch_with_holes_in(&project.fill_patterns, pattern, outer, holes, spacing) {
            // Tagged with the element, so joined geometry can trim them.
            d.line(a, b, LineStyle::Hatch, id);
        }
    }
}
