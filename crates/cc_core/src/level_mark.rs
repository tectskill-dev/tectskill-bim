//! Level marks: annotations that write the elevation of a point in a plan,
//! an elevation or a section — the finished level above a shelf line and
//! the raw (structural) level under it. Plans draw a circle with two filled
//! quarters on the point; elevations and sections a triangle pointing down
//! to it. Like text notes, a mark lives in one view (`view`).
//!
//! The levels are measured from the model (see `cc_geom::level_mark`) unless
//! the mark sets them; the type adds its `datum` (the value written for the
//! project's zero, e.g. 100.00) and the suffixes after each value.

use crate::ViewId;
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// A level mark in a view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelMark {
    pub view: ViewId,
    /// The point measured, in the view's drawing coordinates: a plan point,
    /// or in elevations and sections a point whose `y` is its elevation.
    pub position: DVec2,
    /// The values on the left of the symbol instead of the right.
    #[serde(default)]
    pub flip: bool,
    /// Finished level written instead of the measured one (meters, an
    /// elevation of the project, before the type's datum is added).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<f64>,
    /// Raw level written instead of the measured one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<f64>,
    /// Whether the raw level is written (when there is one: set, or measured
    /// on a floor with layers over its structure).
    #[serde(default = "yes")]
    pub show_raw: bool,
}

fn yes() -> bool {
    true
}

impl LevelMark {
    pub fn new(view: ViewId, position: DVec2) -> Self {
        Self { view, position, flip: false, finished: None, raw: None, show_raw: true }
    }

    /// Replace invalid values (non-finite, out of range); returns how many changed.
    pub fn repair(&mut self) -> usize {
        let mut n = 0;
        if !self.position.is_finite() || self.position.abs().max_element() > MAX_COORD {
            self.position = DVec2::ZERO;
            n += 1;
        }
        for v in [&mut self.finished, &mut self.raw] {
            if v.is_some_and(|x| !x.is_finite() || x.abs() > MAX_LEVEL) {
                *v = None;
                n += 1;
            }
        }
        n
    }
}

/// Farthest a mark's point goes from the origin, meters.
const MAX_COORD: f64 = 1e7;

/// Largest level a mark writes either way from zero, meters (datum included).
pub const MAX_LEVEL: f64 = 1e5;

/// Text size of the default type, paper millimeters.
pub const DEFAULT_TEXT_SIZE: f64 = 2.5;

/// The type a project gets with its first level mark, with the suffixes
/// `name` gives the keys `tpl-level-mark-finished` and `tpl-level-mark-raw`
/// (in the interface language).
pub fn default_type(name: &dyn Fn(&str) -> String) -> crate::TypeData {
    crate::TypeData::LevelMark {
        text_size: DEFAULT_TEXT_SIZE,
        datum: 0.0,
        finished_suffix: name("tpl-level-mark-finished"),
        raw_suffix: name("tpl-level-mark-raw"),
    }
}

/// Replace invalid type values; returns how many changed.
pub fn repair_type(text_size: &mut f64, datum: &mut f64) -> usize {
    let mut n = 0;
    if !text_size.is_finite() || !(0.5..=50.0).contains(text_size) {
        *text_size = DEFAULT_TEXT_SIZE;
        n += 1;
    }
    if !datum.is_finite() || datum.abs() > MAX_LEVEL {
        *datum = 0.0;
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_resets_bad_values() {
        let mut m = LevelMark { finished: Some(f64::NAN), raw: Some(1e9), ..LevelMark::new(ViewId(1), DVec2::NAN) };
        assert_eq!(m.repair(), 3);
        assert_eq!((m.position, m.finished, m.raw), (DVec2::ZERO, None, None));
        let (mut size, mut datum) = (0.0, f64::INFINITY);
        assert_eq!(repair_type(&mut size, &mut datum), 2);
        assert_eq!((size, datum), (DEFAULT_TEXT_SIZE, 0.0));
    }

    #[test]
    fn old_marks_show_the_raw_level() {
        let m: LevelMark = ron::from_str("(view: (3), position: (1.0, 2.0))").unwrap();
        assert!(m.show_raw && !m.flip && m.finished.is_none());
    }
}
