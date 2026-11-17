//! Regions: 2D areas drawn in one view, owned by it like detail lines and
//! text notes (`Element::owner_view`), in the view's drawing coordinates
//! (meters). An outline with openings, usually drawn as a sketch
//! ([`crate::sketch::Sketch::profile`]), and an optional boundary line
//! style (none: no boundary is drawn).
//!
//! - A *filled region* (`ElementKind::FilledRegion`) is filled as its type
//!   says (`TypeData::FilledRegion { fill }`: a foreground and a background
//!   pattern, each with its color, as for material cut patterns; a solid
//!   pattern fills with its color).
//! - A *masking region* (`ElementKind::MaskingRegion`, `TypeData::MaskingRegion {}`)
//!   is opaque white: it hides the model's linework of the view under it,
//!   while the view's annotations (detail lines, dimensions, texts, tags)
//!   stay on top.
//!
//! Regions of a view are drawn in id order over the model and under its
//! annotations; they take no part in 3D views or IFC.

use crate::ViewId;
use crate::detail::{LineStyleRef, MAX_COORD};
use crate::sketch::Sketch;
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Most points of an outline or an opening, and most openings (a crafted file).
pub const MAX_POINTS: usize = 20_000;
pub const MAX_OPENINGS: usize = 1000;

/// A filled or masking region of one view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// The view that owns (and alone shows) the region.
    pub view: ViewId,
    /// The outline, in the view's drawing coordinates (meters).
    pub outline: Vec<DVec2>,
    /// Holes inside the outline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<Vec<DVec2>>,
    /// The curves the outline was drawn with (see [`crate::sketch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<Sketch>,
    /// The style of the boundary lines; `None` draws no boundary.
    #[serde(default)]
    pub boundary: Option<LineStyleRef>,
}

impl Region {
    /// An outline of three or more finite points within bounds, with some
    /// area, and openings of the same kind.
    pub fn is_valid(&self) -> bool {
        let ring_ok = |r: &[DVec2]| {
            (3..=MAX_POINTS).contains(&r.len())
                && r.iter().all(|p| p.is_finite() && p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD)
        };
        ring_ok(&self.outline)
            && area(&self.outline) > 1e-12
            && self.openings.len() <= MAX_OPENINGS
            && self.openings.iter().all(|o| ring_ok(o))
    }

    /// Area of the outline less its openings, square meters.
    pub fn area(&self) -> f64 {
        (area(&self.outline) - self.openings.iter().map(|o| area(o)).sum::<f64>()).max(0.0)
    }

    /// Length of the outline and of the openings' edges, meters.
    pub fn perimeter(&self) -> f64 {
        std::iter::once(&self.outline).chain(&self.openings).map(|r| perimeter(r)).sum()
    }

    /// Every point of the outline and openings, to move them.
    pub fn points_mut(&mut self) -> impl Iterator<Item = &mut DVec2> {
        self.outline.iter_mut().chain(self.openings.iter_mut().flatten())
    }
}

fn area(r: &[DVec2]) -> f64 {
    let n = r.len();
    (0..n).map(|i| r[i].perp_dot(r[(i + 1) % n])).sum::<f64>().abs() * 0.5
}

fn perimeter(r: &[DVec2]) -> f64 {
    let n = r.len();
    (0..n).map(|i| r[i].distance(r[(i + 1) % n])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64, at: DVec2) -> Vec<DVec2> {
        [DVec2::ZERO, DVec2::new(s, 0.0), DVec2::new(s, s), DVec2::new(0.0, s)].map(|p| p + at).to_vec()
    }

    #[test]
    fn area_takes_out_openings_and_bad_outlines_are_invalid() {
        let r = Region {
            view: ViewId(1),
            outline: square(4.0, DVec2::ZERO),
            openings: vec![square(1.0, DVec2::ONE)],
            sketch: None,
            boundary: None,
        };
        assert!(r.is_valid());
        assert!((r.area() - 15.0).abs() < 1e-9);
        assert!((r.perimeter() - 20.0).abs() < 1e-9);
        let mut bad = r.clone();
        bad.outline[0].x = f64::NAN;
        assert!(!bad.is_valid());
        let mut flat = r.clone();
        flat.outline = vec![DVec2::ZERO, DVec2::X, DVec2::X * 2.0];
        assert!(!flat.is_valid());
        let mut far = r;
        far.openings.push(vec![DVec2::splat(1e9); 3]);
        assert!(!far.is_valid());
    }

    #[test]
    fn reads_without_optional_fields() {
        let r: Region = ron::from_str("(view: (2), outline: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)])").unwrap();
        assert!(r.openings.is_empty() && r.sketch.is_none() && r.boundary.is_none());
    }
}
