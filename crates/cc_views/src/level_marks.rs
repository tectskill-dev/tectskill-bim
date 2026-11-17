//! Level marks: the finished level above a shelf line and the raw level
//! under it, beside a symbol on the point measured. Plans draw a circle
//! with two filled quarters crossed by the shelf and a vertical line;
//! elevations and sections a triangle pointing down to the point on a stem
//! that the shelf crosses. Sizes are paper millimeters converted with the
//! view scale, like the other annotations.

use crate::drawing::Anchor;
use crate::{Drawing2D, LineStyle};
use cc_core::level_mark::LevelMark;
use cc_core::{ElementId, ElementKind, Project, TypeData, View};
use glam::DVec2;
use std::f64::consts::{FRAC_PI_2, PI};

/// Average character width, as a fraction of the text size (as text notes).
const CHAR_WIDTH: f64 = 0.6;
/// Fill of the symbols.
const SYMBOL_FILL: [u8; 4] = [20, 20, 20, 255];

/// The texts a mark writes: the finished level, then the raw one when it
/// has one, each with its sign, the type's datum added, and its suffix.
pub fn texts(project: &Project, view: &View, mark: &LevelMark, data: &TypeData) -> Option<Vec<String>> {
    let TypeData::LevelMark { datum, finished_suffix, raw_suffix, .. } = data else { return None };
    let levels = cc_geom::level_mark::levels(project, view, mark)?;
    let text = |level: f64, suffix: &str| {
        let value = datum + level;
        let sign = if value < -5e-4 { "-" } else { "+" };
        format!("{sign} {} {suffix}", crate::format::meters(value.abs())).trim_end().to_string()
    };
    let mut out = vec![text(levels.finished, finished_suffix)];
    out.extend(levels.raw.map(|raw| text(raw, raw_suffix)));
    Some(out)
}

/// The level marks of `view`.
pub fn level_marks(d: &mut Drawing2D, project: &Project, view: &View) {
    let plan = view.is_plan();
    for e in project.elements.values() {
        let ElementKind::LevelMark(mark) = &e.kind else { continue };
        if mark.view != view.id {
            continue;
        }
        let Some(data) = project.element_type(e.type_id).map(|t| &t.data) else { continue };
        let TypeData::LevelMark { text_size, .. } = data else { continue };
        let Some(lines) = texts(project, view, mark, data) else { continue };
        let k = view.scale as f64 / 1000.0;
        let symbol = Symbol { at: mark.position, size: *text_size, k, side: if mark.flip { -1.0 } else { 1.0 } };
        if plan {
            symbol.plan(d, &lines, e.id);
        } else {
            symbol.section(d, &lines, e.id);
        }
    }
}

/// Where a mark is drawn: its point, text size (paper millimeters), meters
/// per paper millimeter and the side its values go to (+1 right, -1 left).
struct Symbol {
    at: DVec2,
    size: f64,
    k: f64,
    side: f64,
}

impl Symbol {
    /// A point `x`, `y` paper millimeters from the mark's point, `x` toward its values.
    fn p(&self, x: f64, y: f64) -> DVec2 {
        self.at + DVec2::new(x * self.side, y) * self.k
    }

    /// Paper width of the longest value.
    fn text_width(&self, lines: &[String]) -> f64 {
        lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * self.size * CHAR_WIDTH
    }

    /// The shelf at height `y` from `x0` to past the values starting at
    /// `x1`, the finished level above it and the raw one under it.
    fn shelf(&self, d: &mut Drawing2D, lines: &[String], (x0, x1, y): (f64, f64, f64), id: Option<ElementId>) {
        let gap = 0.25 * self.size;
        d.line(self.p(x0, y), self.p(x1 + self.text_width(lines) + gap, y), LineStyle::Annotation, id);
        let anchor = if self.side > 0.0 { Anchor::LeftBottom } else { Anchor::RightBottom };
        for (i, line) in lines.iter().enumerate().take(2) {
            let baseline = if i == 0 { y + gap } else { y - gap - self.size };
            d.tag_text(self.p(x1, baseline), line.as_str(), self.size, anchor, 0.0, id);
        }
    }

    /// Plan symbol: a circle on the point with its upper quarter on the
    /// values' side and the opposite one filled, a vertical line through it
    /// and the shelf through its center.
    fn plan(&self, d: &mut Drawing2D, lines: &[String], id: ElementId) {
        let id = Some(id);
        let r = 0.6 * self.size;
        let reach = 1.6 * r;
        d.arc(self.at, r * self.k, 0.0, 2.0 * PI, LineStyle::Annotation, id);
        // Quarters as fans: the one over the values and the one opposite it.
        let first = if self.side > 0.0 { 0.0 } else { FRAC_PI_2 };
        for start in [first, first + PI] {
            let mut pts = vec![self.at];
            pts.extend((0..=8).map(|i| {
                let a = start + FRAC_PI_2 * f64::from(i) / 8.0;
                self.at + DVec2::from_angle(a) * r * self.k
            }));
            d.fill(pts, SYMBOL_FILL, id);
        }
        d.line(self.p(0.0, -reach), self.p(0.0, reach), LineStyle::Annotation, id);
        self.shelf(d, lines, (-reach, r + 0.5 * self.size, 0.0), id);
    }

    /// Section symbol: a triangle with its apex on the point, on a stem
    /// rising from it, the shelf across the stem above the triangle.
    fn section(&self, d: &mut Drawing2D, lines: &[String], id: ElementId) {
        let id = Some(id);
        let (h, half) = (0.8 * self.size, 0.6 * self.size);
        let shelf = 3.2 * self.size;
        d.polyline(&[self.at, self.p(-half, h), self.p(half, h)], true, LineStyle::Annotation, id);
        d.line(self.at, self.p(0.0, shelf + 1.2 * self.size), LineStyle::Annotation, id);
        self.shelf(d, lines, (-1.2 * self.size, 1.0 * self.size, shelf), id);
    }
}
