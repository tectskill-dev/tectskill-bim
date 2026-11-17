//! Bar tags: annotations that write a reinforcement set's bars in a plan,
//! an elevation or a section — `25 N1 Ø12.5 c/12 C=500`: the count, the
//! mark, the diameter in millimeters, the spacing in centimeters (sets
//! without one leave it out) and the length of one bar in centimeters
//! (`C=var` when the set's bars differ). A `multiplier` writes `2x` first,
//! for sets repeated on both faces. The text is derived each time
//! (`cc_views::bar_tags`), from the set's bars and marks.
//!
//! Like text notes, a tag lives in one view (`view`); it goes with that view
//! and with its set.

use crate::{ElementId, ViewId};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Largest multiplier.
pub const MAX_MULTIPLIER: u32 = 100;
/// Text size of the default type, paper millimeters.
pub const DEFAULT_TEXT_SIZE: f64 = 2.5;
/// Farthest a tag's points go from the origin, meters.
const MAX_COORD: f64 = 1e7;

/// The parts of a tag's text that are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarTagParts {
    pub count: bool,
    pub mark: bool,
    pub diameter: bool,
    pub spacing: bool,
    pub length: bool,
}

impl Default for BarTagParts {
    fn default() -> Self {
        Self { count: true, mark: true, diameter: true, spacing: true, length: true }
    }
}

fn one() -> u32 {
    1
}

/// A bar tag in a view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarTag {
    pub view: ViewId,
    /// The reinforcement set (or shaped bars) tagged.
    pub set: ElementId,
    /// Where the text starts (its left end, on the baseline), in the view's
    /// drawing coordinates.
    pub position: DVec2,
    /// Where a leader from the text points (none: no leader).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<DVec2>,
    /// The text's angle, degrees counter-clockwise.
    #[serde(default)]
    pub rotation_deg: f64,
    /// How many times the set is repeated (2 writes `2x` first; 1: nothing).
    #[serde(default = "one")]
    pub multiplier: u32,
    #[serde(default)]
    pub parts: BarTagParts,
}

impl BarTag {
    pub fn new(view: ViewId, set: ElementId, position: DVec2) -> Self {
        Self { view, set, position, leader: None, rotation_deg: 0.0, multiplier: 1, parts: BarTagParts::default() }
    }

    /// Whether its points are finite (a tag without one is dropped).
    pub fn is_valid(&self) -> bool {
        self.position.is_finite() && self.leader.is_none_or(|l| l.is_finite())
    }

    /// Bring the values into range (points far away reset, a bad angle or
    /// multiplier reset); returns how many changed.
    pub fn repair(&mut self) -> usize {
        let mut n = 0;
        if !self.position.is_finite() || self.position.abs().max_element() > MAX_COORD {
            self.position = DVec2::ZERO;
            n += 1;
        }
        if self.leader.is_some_and(|l| !l.is_finite() || l.abs().max_element() > MAX_COORD) {
            self.leader = None;
            n += 1;
        }
        if !(self.rotation_deg.is_finite() && self.rotation_deg.abs() <= 360.0) {
            self.rotation_deg = if self.rotation_deg.is_finite() { self.rotation_deg.rem_euclid(360.0) } else { 0.0 };
            n += 1;
        }
        if !(1..=MAX_MULTIPLIER).contains(&self.multiplier) {
            self.multiplier = self.multiplier.clamp(1, MAX_MULTIPLIER);
            n += 1;
        }
        n
    }

    /// Why the values cannot be kept (`None`: they can), as a message.
    pub fn check(&self) -> Option<String> {
        if !(self.position.is_finite() && self.position.abs().max_element() <= MAX_COORD) {
            return Some("position must be a finite point".into());
        }
        if self.leader.is_some_and(|l| !(l.is_finite() && l.abs().max_element() <= MAX_COORD)) {
            return Some("leader must be a finite point".into());
        }
        if !(self.rotation_deg.is_finite() && self.rotation_deg.abs() <= 360.0) {
            return Some("rotation_deg must be between -360 and 360".into());
        }
        if !(1..=MAX_MULTIPLIER).contains(&self.multiplier) {
            return Some(format!("multiplier must be between 1 and {MAX_MULTIPLIER}"));
        }
        None
    }
}

/// Replace an invalid type text size; returns how many changed.
pub fn repair_type(text_size: &mut f64) -> usize {
    if text_size.is_finite() && (0.5..=50.0).contains(text_size) {
        return 0;
    }
    *text_size = DEFAULT_TEXT_SIZE;
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_resets_bad_values() {
        let mut t = BarTag {
            leader: Some(DVec2::new(f64::NAN, 0.0)),
            rotation_deg: 400.0,
            multiplier: 0,
            ..BarTag::new(ViewId(1), ElementId(2), DVec2::splat(1e9))
        };
        assert!(t.check().is_some());
        assert_eq!(t.repair(), 4);
        assert_eq!((t.position, t.leader, t.multiplier), (DVec2::ZERO, None, 1));
        assert!((t.rotation_deg - 40.0).abs() < 1e-9);
        assert_eq!(t.check(), None);
        let mut size = f64::NAN;
        assert_eq!(repair_type(&mut size), 1);
        assert_eq!(size, DEFAULT_TEXT_SIZE);
    }

    #[test]
    fn older_tags_show_every_part() {
        let t: BarTag = ron::from_str("(view: (3), set: (4), position: (1.0, 2.0))").unwrap();
        assert_eq!(t, BarTag::new(ViewId(3), ElementId(4), DVec2::new(1.0, 2.0)));
        assert!(t.parts.count && t.parts.length);
    }
}
