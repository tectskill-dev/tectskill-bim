//! Filled and masking regions in the view they belong to (see
//! [`cc_core::region`]), in id order, over the model's linework and under
//! the view's annotations: a filled region as its type's fill (a solid
//! color, or a clear background under pattern lines), a masking region
//! opaque white; then the boundary in its line style, when it has one. A
//! region with openings is one keyhole ring
//! ([`cc_geom::polygon::keyhole`]), or triangles where no ring could be
//! made.

use crate::drawing::{Drawing2D, LineStyle, Pen, Prim};
use cc_core::detail::LineStyles;
use cc_core::region::Region;
use cc_core::{ElementId, ElementKind, Pattern, PatternFill, Project, TypeData, ViewId};

/// Opaque white: masking regions.
pub const MASK: [u8; 4] = [255, 255, 255, 255];
/// The alpha of a region with no background: not seen, but picked.
const CLEAR_ALPHA: u8 = 1;
/// Width of pattern lines, paper millimeters.
const PATTERN_WEIGHT_MM: f64 = 0.13;

/// The regions of `view`, at `1:scale`.
pub fn regions(d: &mut Drawing2D, project: &Project, view: ViewId, scale: u32) {
    for e in project.elements.values() {
        match &e.kind {
            ElementKind::FilledRegion(r) if r.view == view && r.is_valid() => {
                let fill = match project.element_type(e.type_id).map(|t| &t.data) {
                    Some(TypeData::FilledRegion { fill }) => *fill,
                    _ => PatternFill::default(),
                };
                draw_filled(d, r, &fill, &project.line_styles, &project.fill_patterns, scale, Some(e.id));
            }
            ElementKind::MaskingRegion(r) if r.view == view && r.is_valid() => {
                draw_masking(d, r, &project.line_styles, Some(e.id));
            }
            _ => {}
        }
    }
}

/// The color under a fill's pattern lines: a solid pattern's color, else
/// clear.
pub fn base_color(fill: &PatternFill) -> [u8; 4] {
    let rgba = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    match (fill.foreground, fill.background) {
        (Pattern::Solid, _) => rgba(fill.foreground_color),
        (_, Pattern::Solid) => rgba(fill.background_color),
        _ => [255, 255, 255, CLEAR_ALPHA],
    }
}

/// Draw a filled region with `fill`.
pub fn draw_filled(
    d: &mut Drawing2D,
    r: &Region,
    fill: &PatternFill,
    styles: &LineStyles,
    patterns: &cc_core::fill_pattern::FillPatterns,
    scale: u32,
    id: Option<ElementId>,
) {
    area(d, r, base_color(fill), id);
    let spacing = crate::cut::spacing(scale);
    for (pattern, color) in [(fill.background, fill.background_color), (fill.foreground, fill.foreground_color)] {
        if matches!(pattern, Pattern::Solid | Pattern::None) {
            continue;
        }
        let pen = Some(Pen { color: Some(color), weight: Some(PATTERN_WEIGHT_MM), pattern: None, halftone: false });
        for (a, b) in crate::hatch::hatch_with_holes_in(patterns, pattern, &r.outline, &r.openings, spacing) {
            d.prims.push(Prim::Line { a, b, style: LineStyle::Hatch, element: id, pen });
        }
    }
    boundary(d, r, styles, id);
}

/// Draw a masking region.
pub fn draw_masking(d: &mut Drawing2D, r: &Region, styles: &LineStyles, id: Option<ElementId>) {
    area(d, r, MASK, id);
    boundary(d, r, styles, id);
}

/// The region's area as fills of `color`.
fn area(d: &mut Drawing2D, r: &Region, color: [u8; 4], element: Option<ElementId>) {
    if r.openings.is_empty() {
        d.prims.push(Prim::Fill { pts: r.outline.clone(), color, element, cut: false });
        return;
    }
    match cc_geom::polygon::keyhole(&r.outline, &r.openings) {
        Some(pts) => d.prims.push(Prim::Fill { pts, color, element, cut: false }),
        None => {
            for t in cc_geom::polygon::triangulate_with_holes(&r.outline, &r.openings) {
                d.prims.push(Prim::Fill { pts: t.to_vec(), color, element, cut: false });
            }
        }
    }
}

/// The boundary lines of the outline and openings, in the region's style.
fn boundary(d: &mut Drawing2D, r: &Region, styles: &LineStyles, element: Option<ElementId>) {
    let Some(style) = r.boundary else { return };
    let pen = Some(crate::detail::pen(style, styles));
    for ring in std::iter::once(&r.outline).chain(&r.openings) {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            if a.distance_squared(b) > 1e-14 {
                d.prims.push(Prim::Line { a, b, style: LineStyle::Projection, element, pen });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{DetailStyle, LineStyleRef};
    use glam::DVec2;

    fn square(s: f64, at: DVec2) -> Vec<DVec2> {
        [DVec2::ZERO, DVec2::new(s, 0.0), DVec2::new(s, s), DVec2::new(0.0, s)].map(|p| p + at).to_vec()
    }

    fn region(openings: Vec<Vec<DVec2>>, boundary: Option<LineStyleRef>) -> Region {
        Region { view: ViewId(1), outline: square(4.0, DVec2::ZERO), openings, sketch: None, boundary }
    }

    #[test]
    fn filled_regions_draw_their_fill_pattern_and_boundary() {
        let r = region(vec![square(1.0, DVec2::ONE)], Some(LineStyleRef::BuiltIn(DetailStyle::Wide)));
        let fill = PatternFill {
            foreground: Pattern::Diagonal,
            foreground_color: [200, 0, 0],
            background: Pattern::Solid,
            background_color: [0, 0, 200],
        };
        let mut d = Drawing2D::default();
        draw_filled(&mut d, &r, &fill, &Default::default(), &Default::default(), 50, Some(ElementId(3)));
        let fills: Vec<_> = d
            .prims
            .iter()
            .filter_map(|p| if let Prim::Fill { pts, color, .. } = p { Some((pts, *color)) } else { None })
            .collect();
        assert_eq!(fills.len(), 1, "one keyhole ring");
        assert_eq!(fills[0].1, [0, 0, 200, 255]);
        assert!(cc_geom::polygon::contains(fills[0].0, DVec2::new(3.0, 3.0)));
        assert!(!cc_geom::polygon::contains(fills[0].0, DVec2::new(1.5, 1.5)));
        let hatch = d.prims.iter().filter(|p| matches!(p, Prim::Line { style: LineStyle::Hatch, .. })).count();
        assert!(hatch > 10);
        let edges: Vec<_> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { style: LineStyle::Projection, pen: Some(pen), .. } => Some(pen.weight),
                _ => None,
            })
            .collect();
        assert_eq!(edges.len(), 8, "outline and opening");
        assert!(edges.iter().all(|w| *w == Some(DetailStyle::Wide.weight_mm())));
    }

    #[test]
    fn masking_regions_are_opaque_white_and_may_have_no_boundary() {
        let mut d = Drawing2D::default();
        draw_masking(&mut d, &region(vec![], None), &Default::default(), Some(ElementId(4)));
        assert_eq!(d.prims.len(), 1);
        assert!(matches!(&d.prims[0], Prim::Fill { color: MASK, .. }));
        assert_eq!(d.hit_test(DVec2::new(2.0, 2.0), 0.01), Some(ElementId(4)));
    }
}
