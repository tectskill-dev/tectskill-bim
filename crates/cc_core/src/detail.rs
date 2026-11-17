//! Detail lines: view-specific 2D curves (lines, arcs, circles and whole or
//! partial ellipses, the [`SketchCurve`] geometry) drawn in one view with a
//! line style. Like text notes they live in their view's drawing
//! coordinates (meters), are owned by it (`Element::owner_view`) and go
//! with it; they belong to no phase-dependent geometry, take no part in the
//! model's 3D or IFC, and are drawn in drafting views and on top of plans,
//! ceiling plans, elevations, sections and legends.
//!
//! A polyline is drawn as one element per segment, so each segment is an
//! ordinary curve that the sketch edits ([`crate::sketch_edit`]: trim,
//! extend, split, offset) and grips change on its own.
//!
//! The line style is a [`LineStyleRef`]: one of the built-in
//! [`DetailStyle`]s or one of the project's own line styles
//! ([`LineStyleDef`]: a name, weight, color and dash pattern, kept in
//! `Project::line_styles` and tracked as project settings). Region
//! boundaries ([`crate::region`]) take the same references. A reference to
//! a project style that no longer exists reads as the thin built-in style,
//! and repair resets it.

use crate::sketch::SketchCurve;
use crate::{LinePattern, LineStyleId, ViewId};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A detail line of one view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetailLine {
    /// The view that owns (and alone shows) the line.
    pub view: ViewId,
    /// The curve, in the view's drawing coordinates (meters).
    pub curve: SketchCurve,
    /// How the line is drawn.
    #[serde(default)]
    pub style: LineStyleRef,
}

impl DetailLine {
    /// Finite numbers and a size (see [`SketchCurve::is_valid`]), within bounds.
    pub fn is_valid(&self) -> bool {
        self.curve.is_valid() && within(&self.curve)
    }

    /// What drawing tools snap to on the line (in the shape of an imported
    /// drawing's snap features): ends, middles, centers, quadrant and axis
    /// points, and the lines and arcs themselves (ellipses by their chords).
    pub fn snap_features(&self, out: &mut crate::cad::CadSnap) {
        use crate::arc::Arc;
        use std::f64::consts::TAU;
        if !self.is_valid() {
            return;
        }
        match self.curve {
            SketchCurve::Line { a, b } => {
                out.ends.extend([a, b]);
                out.mids.push((a + b) * 0.5);
                out.lines.push((a, b));
            }
            SketchCurve::Arc { a, b, bulge } => {
                out.ends.extend([a, b]);
                match Arc::from_bulge(a, b, bulge) {
                    Some(arc) => {
                        out.mids.push(arc.point(arc.length() * 0.5));
                        out.centers.push(arc.center);
                        out.arcs.push(arc);
                    }
                    None => out.lines.push((a, b)),
                }
            }
            SketchCurve::Circle { center, radius } => {
                out.centers.push(center);
                out.ends.extend((0..4).map(|k| center + DVec2::from_angle(k as f64 * TAU / 4.0) * radius));
                out.arcs.push(Arc { center, radius, start_angle: 0.0, sweep: TAU });
            }
            SketchCurve::Ellipse { center, radii, rotation_deg, .. } => {
                out.centers.push(center);
                let turn = DVec2::from_angle(rotation_deg.to_radians());
                if let Some((a, b)) = self.curve.ends() {
                    out.ends.extend([a, b]);
                } else {
                    out.ends.extend(
                        [DVec2::X * radii.x, DVec2::Y * radii.y, -DVec2::X * radii.x, -DVec2::Y * radii.y]
                            .map(|v| center + turn.rotate(v)),
                    );
                }
                out.lines.extend(self.curve.points().windows(2).map(|w| (w[0], w[1])));
            }
        }
    }
}

/// Coordinates and sizes past this (meters) are refused (a crafted file).
pub const MAX_COORD: f64 = 1.0e7;

fn within(c: &SketchCurve) -> bool {
    let ok = |v: f64| v.abs() <= MAX_COORD;
    match *c {
        SketchCurve::Line { a, b } | SketchCurve::Arc { a, b, .. } => ok(a.x) && ok(a.y) && ok(b.x) && ok(b.y),
        SketchCurve::Circle { center, radius } => ok(center.x) && ok(center.y) && ok(radius),
        SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg, .. } => {
            [center.x, center.y, radii.x, radii.y, rotation_deg, start_deg, sweep_deg].into_iter().all(ok)
        }
    }
}

/// The project's line styles, by id.
pub type LineStyles = BTreeMap<LineStyleId, LineStyleDef>;

/// The style a detail line or a region's boundary is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LineStyleRef {
    /// One of the built-in styles.
    BuiltIn(DetailStyle),
    /// One of the project's line styles (`Project::line_styles`).
    Project(LineStyleId),
}

impl Default for LineStyleRef {
    fn default() -> Self {
        LineStyleRef::BuiltIn(DetailStyle::default())
    }
}

impl LineStyleRef {
    /// Width on paper (millimeters), color and dash pattern; a project
    /// style that is gone reads as the default built-in style.
    pub fn pen(self, styles: &LineStyles) -> (f64, [u8; 3], LinePattern) {
        match self {
            LineStyleRef::BuiltIn(s) => (s.weight_mm(), [0, 0, 0], s.pattern()),
            LineStyleRef::Project(id) => match styles.get(&id) {
                Some(s) => (s.weight_mm, s.color, s.pattern),
                None => LineStyleRef::default().pen(styles),
            },
        }
    }

    /// The style's name as a code (DXF layers, commands): a built-in
    /// style's code, a project style's name.
    pub fn code(self, styles: &LineStyles) -> String {
        match self {
            LineStyleRef::BuiltIn(s) => s.code().to_string(),
            LineStyleRef::Project(id) => match styles.get(&id) {
                Some(s) => s.name.clone(),
                None => DetailStyle::default().code().to_string(),
            },
        }
    }

    /// Whether the reference is to a style that exists.
    pub fn exists(self, styles: &LineStyles) -> bool {
        match self {
            LineStyleRef::BuiltIn(_) => true,
            LineStyleRef::Project(id) => styles.contains_key(&id),
        }
    }
}

/// Most project line styles (a crafted file).
pub const MAX_LINE_STYLES: usize = 1000;
/// Range of a line style's weight, paper millimeters.
pub const MIN_WEIGHT_MM: f64 = 0.05;
pub const MAX_WEIGHT_MM: f64 = 5.0;

/// One of the project's line styles: a name, a weight on paper, a color and
/// a dash pattern. Names are unique and differ from the built-in codes, so
/// commands can name either.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineStyleDef {
    pub id: LineStyleId,
    pub name: String,
    /// Width on paper, millimeters.
    pub weight_mm: f64,
    #[serde(default)]
    pub color: [u8; 3],
    #[serde(default)]
    pub pattern: LinePattern,
}

impl LineStyleDef {
    /// Bring the weight into range (a crafted file); returns whether it changed.
    pub fn repair(&mut self) -> bool {
        let mut changed = false;
        if !self.weight_mm.is_finite() {
            self.weight_mm = DetailStyle::Medium.weight_mm();
            changed = true;
        } else if !(MIN_WEIGHT_MM..=MAX_WEIGHT_MM).contains(&self.weight_mm) {
            self.weight_mm = self.weight_mm.clamp(MIN_WEIGHT_MM, MAX_WEIGHT_MM);
            changed = true;
        }
        if self.name.trim().is_empty() {
            self.name = format!("{}", self.id);
            changed = true;
        }
        changed
    }
}

/// The built-in line styles of detail lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DetailStyle {
    /// Fine continuous line.
    #[default]
    Thin,
    Medium,
    Wide,
    /// Short dashes.
    Hidden,
    /// Long dashes.
    Dashed,
    /// Dash-dot (center lines, axes).
    Center,
    Dotted,
}

impl DetailStyle {
    pub const ALL: [DetailStyle; 7] = [
        DetailStyle::Thin,
        DetailStyle::Medium,
        DetailStyle::Wide,
        DetailStyle::Hidden,
        DetailStyle::Dashed,
        DetailStyle::Center,
        DetailStyle::Dotted,
    ];

    /// Width on paper, millimeters.
    pub fn weight_mm(self) -> f64 {
        match self {
            DetailStyle::Thin => 0.13,
            DetailStyle::Medium => 0.25,
            DetailStyle::Wide => 0.5,
            DetailStyle::Hidden | DetailStyle::Dashed | DetailStyle::Center | DetailStyle::Dotted => 0.18,
        }
    }

    pub fn pattern(self) -> LinePattern {
        match self {
            DetailStyle::Thin | DetailStyle::Medium | DetailStyle::Wide => LinePattern::Solid,
            DetailStyle::Hidden => LinePattern::ShortDash,
            DetailStyle::Dashed => LinePattern::Dash,
            DetailStyle::Center => LinePattern::DashDot,
            DetailStyle::Dotted => LinePattern::Dot,
        }
    }

    /// The name used by commands and DXF layers.
    pub fn code(self) -> &'static str {
        match self {
            DetailStyle::Thin => "thin",
            DetailStyle::Medium => "medium",
            DetailStyle::Wide => "wide",
            DetailStyle::Hidden => "hidden",
            DetailStyle::Dashed => "dashed",
            DetailStyle::Center => "center",
            DetailStyle::Dotted => "dotted",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        DetailStyle::ALL.into_iter().find(|s| s.code().eq_ignore_ascii_case(code.trim()))
    }

    /// Translation key of the name.
    pub fn key(self) -> &'static str {
        match self {
            DetailStyle::Thin => "line-style-thin",
            DetailStyle::Medium => "line-style-medium",
            DetailStyle::Wide => "line-style-wide",
            DetailStyle::Hidden => "line-style-hidden",
            DetailStyle::Dashed => "line-style-dashed",
            DetailStyle::Center => "line-style-center",
            DetailStyle::Dotted => "line-style-dotted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_bad_curves_are_invalid() {
        for s in DetailStyle::ALL {
            assert_eq!(DetailStyle::from_code(s.code()), Some(s));
        }
        let line = |b: DVec2| DetailLine {
            view: ViewId(1),
            curve: SketchCurve::Line { a: DVec2::ZERO, b },
            style: LineStyleRef::default(),
        };
        assert!(line(DVec2::X).is_valid());
        assert!(!line(DVec2::ZERO).is_valid());
        assert!(!line(DVec2::new(f64::NAN, 0.0)).is_valid());
        assert!(!line(DVec2::new(1e9, 0.0)).is_valid());
    }

    #[test]
    fn project_styles_resolve_and_fall_back() {
        let mut styles = LineStyles::new();
        let id = LineStyleId(9);
        styles.insert(
            id,
            LineStyleDef { id, name: "Red".into(), weight_mm: 0.7, color: [200, 0, 0], pattern: LinePattern::Dot },
        );
        assert_eq!(LineStyleRef::Project(id).pen(&styles), (0.7, [200, 0, 0], LinePattern::Dot));
        assert_eq!(LineStyleRef::Project(id).code(&styles), "Red");
        let gone = LineStyleRef::Project(LineStyleId(10));
        assert!(!gone.exists(&styles));
        assert_eq!(gone.pen(&styles), LineStyleRef::default().pen(&styles));
        let mut bad =
            LineStyleDef { id, name: " ".into(), weight_mm: f64::NAN, color: [0; 3], pattern: LinePattern::Solid };
        assert!(bad.repair());
        assert!(bad.weight_mm.is_finite() && !bad.name.trim().is_empty());
    }

    #[test]
    fn style_reads_from_older_text_without_it() {
        let l: DetailLine = ron::from_str("(view: (3), curve: Line(a: (0.0, 0.0), b: (1.0, 0.0)))").unwrap();
        assert_eq!(l.style, LineStyleRef::BuiltIn(DetailStyle::Thin));
    }
}
