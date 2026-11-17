//! Bar marks: the names (N1, N2…) bending schedules and bar tags give the
//! bars of reinforcement, one per bar shape: the same diameter, steel grade
//! and shape (legs, radii, bends and hooks, within a length tolerance and
//! [`ANGLE_TOLERANCE_DEG`], read from either end or mirrored).
//!
//! Marks are derived (`cc_geom::bar_marks` resolves them, a pure
//! function): a set's `bar_mark` override names all its bars; otherwise a
//! bar whose shape matches an entry of the frozen marks table
//! ([`BarMarking::frozen`], in table order) takes its mark; the shapes left
//! are numbered on the fly with the `prefix`, after the highest frozen number
//! (and from `start`), in a stable order: partition, then diameter
//! descending, then length descending, then shape. So every bar has a mark,
//! and frozen marks never shift when the model changes. Freezing writes the
//! marks shown into the table; renumbering clears it and freezes a fresh
//! numbering.
//!
//! The settings live in [`crate::Project::bar_marking`] (tracked as project
//! settings) with the `stock_length` bars are bought in (bars longer than it
//! need splices).

use serde::{Deserialize, Serialize};

/// Prefix of numbered marks when none is set.
pub const DEFAULT_PREFIX: &str = "N";
/// First number when none is set.
pub const DEFAULT_START: u32 = 1;
/// Largest first number.
pub const MAX_START: u32 = 1_000_000;
/// Legs of two bars of one mark differ by at most this when none is set, meters.
pub const DEFAULT_LENGTH_TOLERANCE: f64 = 0.005;
/// Smallest and largest length tolerance, meters.
pub const MIN_LENGTH_TOLERANCE: f64 = 0.0001;
pub const MAX_LENGTH_TOLERANCE: f64 = 0.05;
/// Turns of two bars of one mark differ by at most this, degrees.
pub const ANGLE_TOLERANCE_DEG: f64 = 1.0;
/// Length bars are bought in when none is set, meters.
pub const DEFAULT_STOCK_LENGTH: f64 = 12.0;
/// Most entries of the frozen marks table.
pub const MAX_FROZEN: usize = 5000;
/// Most legs of a frozen shape (bars of more legs are numbered, not frozen).
pub const MAX_MARK_LEGS: usize = 64;
/// Longest leg or radius a frozen shape keeps, meters.
pub const MAX_MARK_LENGTH: f64 = 1000.0;
/// Longest prefix or mark, in characters.
pub const MAX_MARK_TEXT: usize = 40;
/// Shortest and longest stock length, meters.
pub const MIN_STOCK_LENGTH: f64 = 1.0;
pub const MAX_STOCK_LENGTH: f64 = 100.0;

/// How bars are marked: the numbering and the frozen marks table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarMarking {
    /// Text before the number of numbered marks (e.g. "N" for N1, N2…).
    pub prefix: String,
    /// The first number given.
    pub start: u32,
    /// Legs (and radii) of bars of one mark differ by at most this, meters.
    pub length_tolerance: f64,
    /// Length bars are bought in, meters: longer bars need splices.
    pub stock_length: f64,
    /// Marks kept for their shapes, in order: a bar takes the first that
    /// matches it.
    pub frozen: Vec<FrozenMark>,
}

impl Default for BarMarking {
    fn default() -> Self {
        Self {
            prefix: DEFAULT_PREFIX.to_string(),
            start: DEFAULT_START,
            length_tolerance: DEFAULT_LENGTH_TOLERANCE,
            stock_length: DEFAULT_STOCK_LENGTH,
            frozen: Vec::new(),
        }
    }
}

/// A mark kept for one bar shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenMark {
    pub mark: String,
    /// Bar diameter, meters.
    pub diameter: f64,
    /// Steel grade (empty: none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub grade: String,
    pub shape: MarkShape,
}

/// A bar shape in a compact canonical form: open or closed, its legs in
/// order (the orientation `cc_geom::bar_marks` picks among the readings
/// from either end, mirrored and, for closed bars, from any corner).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MarkShape {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    pub legs: Vec<MarkLeg>,
}

/// One leg of a [`MarkShape`]: its length (meters), the radius of an arc
/// leg, the turn along an arc and the bend after the leg (degrees, signed
/// the way the bar first turns), and whether it is part of a hook.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MarkLeg {
    pub length: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub sweep: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub bend: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hook: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl MarkShape {
    /// Whether the shape is usable: one to [`MAX_MARK_LEGS`] legs of finite
    /// lengths above 0 and at most [`MAX_MARK_LENGTH`], radii likewise,
    /// turns within a whole turn either way.
    pub fn is_valid(&self) -> bool {
        let length_ok = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_MARK_LENGTH;
        let angle_ok = |v: f64| v.is_finite() && v.abs() <= 360.0;
        (1..=MAX_MARK_LEGS).contains(&self.legs.len())
            && self
                .legs
                .iter()
                .all(|l| length_ok(l.length) && l.radius.is_none_or(length_ok) && angle_ok(l.sweep) && angle_ok(l.bend))
    }

    /// Length of the bar, meters.
    pub fn length(&self) -> f64 {
        self.legs.iter().map(|l| l.length).sum()
    }
}

impl FrozenMark {
    /// Whether the entry is usable: a mark of 1 to [`MAX_MARK_TEXT`]
    /// characters, a finite diameter above 0, a usable shape.
    pub fn is_valid(&self) -> bool {
        let text_ok = |t: &str| t.chars().count() <= MAX_MARK_TEXT;
        !self.mark.trim().is_empty()
            && text_ok(&self.mark)
            && text_ok(&self.grade)
            && self.diameter.is_finite()
            && self.diameter > 0.0
            && self.diameter <= 1.0
            && self.shape.is_valid()
    }
}

/// Whether `v` is a usable length tolerance.
pub fn tolerance_ok(v: f64) -> bool {
    v.is_finite() && (MIN_LENGTH_TOLERANCE..=MAX_LENGTH_TOLERANCE).contains(&v)
}

/// Whether `v` is a usable stock length.
pub fn stock_length_ok(v: f64) -> bool {
    v.is_finite() && (MIN_STOCK_LENGTH..=MAX_STOCK_LENGTH).contains(&v)
}

impl BarMarking {
    /// Whether nothing differs from the defaults (files leave it out).
    pub fn is_default(&self) -> bool {
        *self == BarMarking::default()
    }

    /// The number of a mark written with the prefix (`N12` → 12), if it is one.
    pub fn number_of(&self, mark: &str) -> Option<u32> {
        let digits = mark.strip_prefix(self.prefix.as_str())?;
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    }

    /// The mark of number `n`.
    pub fn mark_of(&self, n: u32) -> String {
        format!("{}{n}", self.prefix)
    }

    /// Why the values cannot be kept (`None`: they can): the checks
    /// [`Self::repair`] applies, as a message.
    pub fn check(&self) -> Option<String> {
        if self.prefix.chars().count() > MAX_MARK_TEXT {
            return Some(format!("the prefix has at most {MAX_MARK_TEXT} characters"));
        }
        if self.start > MAX_START {
            return Some(format!("start must be at most {MAX_START}"));
        }
        if !tolerance_ok(self.length_tolerance) {
            return Some(format!(
                "length_tolerance must be between {MIN_LENGTH_TOLERANCE} and {MAX_LENGTH_TOLERANCE} m"
            ));
        }
        if !stock_length_ok(self.stock_length) {
            return Some(format!("stock_length must be between {MIN_STOCK_LENGTH} and {MAX_STOCK_LENGTH} m"));
        }
        if self.frozen.len() > MAX_FROZEN {
            return Some(format!("the frozen marks table holds at most {MAX_FROZEN} marks"));
        }
        None
    }

    /// Bring the values into range (a long prefix cut, bad numbers reset to
    /// their defaults, unusable or extra frozen entries dropped); returns
    /// how many changed.
    pub fn repair(&mut self) -> usize {
        let mut n = 0;
        if let Some((end, _)) = self.prefix.char_indices().nth(MAX_MARK_TEXT) {
            self.prefix.truncate(end);
            n += 1;
        }
        if self.start > MAX_START {
            self.start = DEFAULT_START;
            n += 1;
        }
        if !tolerance_ok(self.length_tolerance) {
            self.length_tolerance = DEFAULT_LENGTH_TOLERANCE;
            n += 1;
        }
        if !stock_length_ok(self.stock_length) {
            self.stock_length = DEFAULT_STOCK_LENGTH;
            n += 1;
        }
        let before = self.frozen.len();
        self.frozen.retain(FrozenMark::is_valid);
        self.frozen.truncate(MAX_FROZEN);
        n + before - self.frozen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(mark: &str) -> FrozenMark {
        FrozenMark {
            mark: mark.into(),
            diameter: 0.01,
            grade: "CA-50".into(),
            shape: MarkShape { closed: false, legs: vec![MarkLeg { length: 2.0, ..Default::default() }] },
        }
    }

    #[test]
    fn numbers_read_with_the_prefix() {
        let m = BarMarking::default();
        assert_eq!(m.number_of("N12"), Some(12));
        assert_eq!((m.number_of("N"), m.number_of("P3"), m.number_of("N3a")), (None, None, None));
        assert_eq!(m.mark_of(7), "N7");
    }

    #[test]
    fn repair_bounds_the_settings_and_drops_bad_entries() {
        let mut m = BarMarking::default();
        assert_eq!(m.repair(), 0);
        assert!(m.is_default());
        m.prefix = "x".repeat(MAX_MARK_TEXT + 3);
        m.start = u32::MAX;
        m.length_tolerance = f64::NAN;
        m.stock_length = -1.0;
        let mut bad_leg = entry("N2");
        bad_leg.shape.legs[0].length = f64::INFINITY;
        let mut no_legs = entry("N3");
        no_legs.shape.legs.clear();
        let mut blank = entry(" ");
        blank.diameter = 0.01;
        m.frozen = vec![entry("N1"), bad_leg, no_legs, blank];
        assert!(m.check().is_some());
        assert_eq!(m.repair(), 4 + 3);
        assert_eq!(m.prefix.chars().count(), MAX_MARK_TEXT);
        assert_eq!((m.start, m.length_tolerance, m.stock_length), (1, 0.005, 12.0));
        assert_eq!(m.frozen, vec![entry("N1")]);
        assert_eq!(m.check(), None);
        assert_eq!(m.repair(), 0);
    }

    #[test]
    fn older_files_get_the_defaults() {
        let m: BarMarking = ron::from_str("(prefix: \"P\")").unwrap();
        assert_eq!(m, BarMarking { prefix: "P".into(), ..Default::default() });
        // The compact form leaves out what is at its default.
        let text = ron::to_string(&entry("N1").shape).unwrap();
        assert!(!text.contains("hook") && !text.contains("radius"), "{text}");
    }
}
