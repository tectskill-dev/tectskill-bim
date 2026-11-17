//! Object snaps (issue #317): which points the tools snap to (a preference),
//! and the snap points of the elements a view draws, so that every element
//! shown has them, not only those the tools know by their geometry.

use cc_core::cad::CadSnap;
use cc_views::Drawing2D;
use glam::DVec2;

/// Which object snaps are on. Rounding free points to the zoom's increment
/// and the angle increments from the point a segment starts at stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapSettings {
    /// Off: no object snap at all, whatever the others say.
    pub enabled: bool,
    /// Every element the view draws, by its lines; otherwise only the points
    /// and lines the tools take from the model (walls, grids, outlines).
    pub elements: bool,
    pub endpoint: bool,
    pub midpoint: bool,
    pub center: bool,
    pub intersection: bool,
    pub perpendicular: bool,
    pub tangent: bool,
    /// The nearest point of a line or an arc.
    pub nearest: bool,
    /// Guides from points acquired by hovering them.
    pub alignment: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            elements: true,
            endpoint: true,
            midpoint: true,
            center: true,
            intersection: true,
            perpendicular: true,
            tangent: true,
            nearest: true,
            alignment: true,
        }
    }
}

/// A snap setting: its storage code and the key of its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapOption {
    Enabled,
    Elements,
    Endpoint,
    Midpoint,
    Center,
    Intersection,
    Perpendicular,
    Tangent,
    Nearest,
    Alignment,
}

impl SnapOption {
    pub const ALL: [SnapOption; 10] = [
        SnapOption::Enabled,
        SnapOption::Elements,
        SnapOption::Endpoint,
        SnapOption::Midpoint,
        SnapOption::Center,
        SnapOption::Intersection,
        SnapOption::Perpendicular,
        SnapOption::Tangent,
        SnapOption::Nearest,
        SnapOption::Alignment,
    ];
    /// The kinds of snap point, which the master switch turns off together.
    pub const KINDS: [SnapOption; 8] = [
        SnapOption::Endpoint,
        SnapOption::Midpoint,
        SnapOption::Center,
        SnapOption::Intersection,
        SnapOption::Perpendicular,
        SnapOption::Tangent,
        SnapOption::Nearest,
        SnapOption::Alignment,
    ];

    pub fn code(self) -> &'static str {
        match self {
            SnapOption::Enabled => "enabled",
            SnapOption::Elements => "elements",
            SnapOption::Endpoint => "endpoint",
            SnapOption::Midpoint => "midpoint",
            SnapOption::Center => "center",
            SnapOption::Intersection => "intersection",
            SnapOption::Perpendicular => "perpendicular",
            SnapOption::Tangent => "tangent",
            SnapOption::Nearest => "nearest",
            SnapOption::Alignment => "alignment",
        }
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            SnapOption::Enabled => "pref-snap-enabled",
            SnapOption::Elements => "pref-snap-elements",
            SnapOption::Endpoint => "pref-snap-endpoint",
            SnapOption::Midpoint => "pref-snap-midpoint",
            SnapOption::Center => "pref-snap-center",
            SnapOption::Intersection => "pref-snap-intersection",
            SnapOption::Perpendicular => "pref-snap-perpendicular",
            SnapOption::Tangent => "pref-snap-tangent",
            SnapOption::Nearest => "pref-snap-nearest",
            SnapOption::Alignment => "pref-snap-alignment",
        }
    }
}

impl SnapSettings {
    pub fn get(&self, option: SnapOption) -> bool {
        match option {
            SnapOption::Enabled => self.enabled,
            SnapOption::Elements => self.elements,
            SnapOption::Endpoint => self.endpoint,
            SnapOption::Midpoint => self.midpoint,
            SnapOption::Center => self.center,
            SnapOption::Intersection => self.intersection,
            SnapOption::Perpendicular => self.perpendicular,
            SnapOption::Tangent => self.tangent,
            SnapOption::Nearest => self.nearest,
            SnapOption::Alignment => self.alignment,
        }
    }

    pub fn get_mut(&mut self, option: SnapOption) -> &mut bool {
        match option {
            SnapOption::Enabled => &mut self.enabled,
            SnapOption::Elements => &mut self.elements,
            SnapOption::Endpoint => &mut self.endpoint,
            SnapOption::Midpoint => &mut self.midpoint,
            SnapOption::Center => &mut self.center,
            SnapOption::Intersection => &mut self.intersection,
            SnapOption::Perpendicular => &mut self.perpendicular,
            SnapOption::Tangent => &mut self.tangent,
            SnapOption::Nearest => &mut self.nearest,
            SnapOption::Alignment => &mut self.alignment,
        }
    }

    /// The settings the tools go by: with the master switch off, no kind is on.
    pub fn effective(self) -> Self {
        if self.enabled {
            return self;
        }
        let mut off = self;
        for option in SnapOption::ALL {
            *off.get_mut(option) = false;
        }
        off
    }

    /// As stored: the codes of the options that are off, separated by commas
    /// (so an option added later is on for settings stored before it).
    pub fn to_text(self) -> String {
        let off: Vec<&str> = SnapOption::ALL.into_iter().filter(|o| !self.get(*o)).map(SnapOption::code).collect();
        off.join(",")
    }

    /// Read what [`Self::to_text`] wrote; unknown codes are ignored.
    pub fn from_text(text: &str) -> Self {
        let mut settings = Self::default();
        for code in text.split(',').map(str::trim).take(4 * SnapOption::ALL.len()) {
            if let Some(option) = SnapOption::ALL.into_iter().find(|o| o.code() == code) {
                *settings.get_mut(option) = false;
            }
        }
        settings
    }
}

/// Lines of one element meeting at less than this angle carry on into each
/// other (the chords of a curve, a line drawn in pieces): where they meet is
/// not a corner, and they have no midpoint of their own.
const SMOOTH_DEG: f64 = 30.0;
/// How close two line ends are to be the same point (drawing units).
const SAME: f64 = 1e-6;

/// Snap features of the elements `drawing` shows near `raw`: the lines
/// passing within `r`, their ends where they are corners and their
/// midpoints. See [`Drawing2D::lines_near`] for what is left out.
pub fn drawing_snap(drawing: &Drawing2D, raw: DVec2, r: f64, out: &mut CadSnap) {
    if !(raw.is_finite() && r.is_finite() && r > 0.0) {
        return;
    }
    for (a, b, element) in drawing.lines_near(raw, r) {
        if !(a.is_finite() && b.is_finite()) || a.distance(b) < SAME {
            continue;
        }
        out.lines.push((a, b));
        // Whether the line carries on smoothly past its end `at` (coming from `from`).
        let smooth = |at: DVec2, from: DVec2| {
            let along = (at - from).normalize_or_zero();
            drawing.lines_near(at, SAME).into_iter().any(|(c, d, other)| {
                let next = if c.distance(at) <= SAME {
                    d
                } else if d.distance(at) <= SAME {
                    c
                } else {
                    return false;
                };
                let onward = (next - at).normalize_or_zero();
                other == element && next.distance(from) > SAME && along.dot(onward) > SMOOTH_DEG.to_radians().cos()
            })
        };
        let (smooth_a, smooth_b) = (smooth(a, b), smooth(b, a));
        if a.distance(raw) <= r && !smooth_a {
            out.ends.push(a);
        }
        if b.distance(raw) <= r && !smooth_b {
            out.ends.push(b);
        }
        let mid = (a + b) * 0.5;
        if mid.distance(raw) <= r && !smooth_a && !smooth_b {
            out.mids.push(mid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::ElementId;
    use cc_views::LineStyle;

    #[test]
    fn settings_round_trip_and_new_options_default_on() {
        assert_eq!(SnapSettings::default().to_text(), "");
        assert_eq!(SnapSettings::from_text(""), SnapSettings::default());
        let some = SnapSettings { midpoint: false, tangent: false, elements: false, ..Default::default() };
        assert_eq!(SnapSettings::from_text(&some.to_text()), some);
        // Hostile or older text: what is not a code is ignored.
        assert_eq!(
            SnapSettings::from_text("nonsense,, midpoint ,\u{0}"),
            SnapSettings { midpoint: false, ..Default::default() }
        );
        // The master switch turns every kind off without forgetting them.
        let off = SnapSettings { enabled: false, ..Default::default() };
        assert!(SnapOption::ALL.into_iter().all(|o| !off.effective().get(o)));
        assert!(off.endpoint);
        for o in SnapOption::ALL {
            assert!(SnapSettings::default().get(o), "{o:?} is on by default");
        }
    }

    /// A rectangle (a footing in plan) gives its corners, its sides and
    /// their midpoints; pattern lines and annotations give nothing.
    #[test]
    fn drawn_elements_give_corners_sides_and_midpoints() {
        let mut d = Drawing2D::default();
        let id = Some(ElementId(7));
        let corners = [DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0), DVec2::new(0.0, 1.0)];
        d.polyline(&corners, true, LineStyle::Hidden, id);
        d.line(DVec2::new(0.9, -1.0), DVec2::new(1.1, 1.0), LineStyle::Hatch, id);
        d.line(DVec2::new(1.0, -1.0), DVec2::new(1.0, 1.0), LineStyle::Annotation, Some(ElementId(8)));

        let mut at_corner = CadSnap::default();
        drawing_snap(&d, DVec2::new(2.02, 0.01), 0.05, &mut at_corner);
        assert_eq!(at_corner.ends, [DVec2::new(2.0, 0.0), DVec2::new(2.0, 0.0)], "once for each side");
        assert!(at_corner.mids.is_empty());
        assert_eq!(at_corner.lines.len(), 2);

        let mut at_mid = CadSnap::default();
        drawing_snap(&d, DVec2::new(1.01, 0.02), 0.05, &mut at_mid);
        assert_eq!(at_mid.mids, [DVec2::new(1.0, 0.0)]);
        assert_eq!(at_mid.lines, [(DVec2::ZERO, DVec2::new(2.0, 0.0))], "only the footing's side");
        assert!(at_mid.ends.is_empty());
    }

    /// The chords of a curve are one line: no end or midpoint at every chord.
    #[test]
    fn chords_of_a_curve_have_no_corners() {
        let mut d = Drawing2D::default();
        let arc: Vec<DVec2> = (0..=18).map(|i| DVec2::from_angle(f64::from(i) * 5.0_f64.to_radians()) * 3.0).collect();
        d.polyline(&arc, false, LineStyle::Cut, Some(ElementId(1)));
        let mut on_arc = CadSnap::default();
        drawing_snap(&d, arc[9] * 1.001, 0.05, &mut on_arc);
        assert!(on_arc.ends.is_empty() && on_arc.mids.is_empty(), "{on_arc:?}");
        assert!(!on_arc.lines.is_empty());
        // Its two ends are ends.
        let mut at_end = CadSnap::default();
        drawing_snap(&d, arc[0], 0.05, &mut at_end);
        assert_eq!(at_end.ends, [arc[0]]);
        // Another element's line ending on it makes a corner of its own.
        d.line(arc[9] * 2.0, arc[9], LineStyle::Cut, Some(ElementId(2)));
        let mut joined = CadSnap::default();
        drawing_snap(&d, arc[9], 0.05, &mut joined);
        assert_eq!(joined.ends, [arc[9]]);
    }
}
