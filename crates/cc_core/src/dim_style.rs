//! How a dimension type draws its dimensions: the marks at the ends of the
//! dimension line, the witness lines, the pens and the text around the
//! value. Sizes are paper millimeters, so they read the same at any scale.

use crate::LeaderSymbol;
use crate::detail::{MAX_WEIGHT_MM, MIN_WEIGHT_MM};
use serde::{Deserialize, Serialize};

/// Most paper millimeters any length of the style may take.
pub const MAX_LENGTH_MM: f64 = 50.0;
/// Smallest mark.
pub const MIN_MARK_MM: f64 = 0.1;
/// Most characters of the prefix or the suffix.
pub const MAX_AFFIX: usize = 64;

/// The drawing parameters of a dimension type. Every field has a default,
/// so types saved before them draw as they did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DimensionStyle {
    /// The mark at each end of the dimension line (linear and angular
    /// dimensions; radial ones keep their arrows).
    pub mark: LeaderSymbol,
    /// Length of the mark: the stroke of a tick, the depth of an arrow.
    pub mark_size: f64,
    /// Pen width of the mark; `None` draws ticks heavy and other marks like the lines.
    pub mark_weight: Option<f64>,
    /// Pen width of the dimension and witness lines; `None` is the annotation pen.
    pub line_weight: Option<f64>,
    /// Color of the lines and marks; `None` is the annotation color.
    pub color: Option<[u8; 3]>,
    /// Gap between the measured point and the start of its witness line.
    pub witness_gap: f64,
    /// How far witness lines run past the dimension line.
    pub witness_extension: f64,
    /// How far the dimension line runs past the witness lines.
    pub line_extension: f64,
    /// Distance from the dimension line to the bottom of the text.
    pub text_offset: f64,
    /// Written before the value.
    pub prefix: String,
    /// Written after the value.
    pub suffix: String,
    /// The text hides what lies under it (a white box behind it).
    pub opaque_text: bool,
}

impl Default for DimensionStyle {
    fn default() -> Self {
        DimensionStyle {
            mark: LeaderSymbol::Tick,
            mark_size: 2.6,
            mark_weight: None,
            line_weight: None,
            color: None,
            witness_gap: 1.0,
            witness_extension: 1.5,
            line_extension: 1.5,
            text_offset: 0.8,
            prefix: String::new(),
            suffix: String::new(),
            opaque_text: false,
        }
    }
}

impl DimensionStyle {
    /// The default style of a project of `system`: in imperial projects
    /// round inches on paper (1/8" marks and extensions, 1/16" gaps, the
    /// text 1/32" off the line).
    pub fn of(system: crate::MeasurementSystem) -> Self {
        if system == crate::MeasurementSystem::Metric {
            return Self::default();
        }
        const INCH: f64 = 25.4;
        DimensionStyle {
            mark_size: INCH / 8.0,
            witness_gap: INCH / 16.0,
            witness_extension: INCH / 8.0,
            line_extension: INCH / 8.0,
            text_offset: INCH / 32.0,
            ..Self::default()
        }
    }

    /// `value` written with the prefix and the suffix.
    pub fn label(&self, value: &str) -> String {
        format!("{}{value}{}", self.prefix, self.suffix)
    }

    /// Reset what is out of range; returns the number of repairs.
    pub fn repair(&mut self) -> usize {
        let base = DimensionStyle::default();
        let mut repairs = 0;
        if !(MIN_MARK_MM..=MAX_LENGTH_MM).contains(&self.mark_size) {
            self.mark_size = base.mark_size;
            repairs += 1;
        }
        for (value, fallback) in [
            (&mut self.witness_gap, base.witness_gap),
            (&mut self.witness_extension, base.witness_extension),
            (&mut self.line_extension, base.line_extension),
            (&mut self.text_offset, base.text_offset),
        ] {
            if !(0.0..=MAX_LENGTH_MM).contains(value) {
                *value = fallback;
                repairs += 1;
            }
        }
        for weight in [&mut self.mark_weight, &mut self.line_weight] {
            if weight.is_some_and(|w| !(MIN_WEIGHT_MM..=MAX_WEIGHT_MM).contains(&w)) {
                *weight = None;
                repairs += 1;
            }
        }
        for text in [&mut self.prefix, &mut self.suffix] {
            if text.chars().count() > MAX_AFFIX || text.contains(['\n', '\r']) {
                *text = text.chars().filter(|c| !matches!(c, '\n' | '\r')).take(MAX_AFFIX).collect();
                repairs += 1;
            }
        }
        repairs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_types_get_the_defaults() {
        let style: DimensionStyle = ron::from_str("()").unwrap();
        assert_eq!(style, DimensionStyle::default());
    }

    #[test]
    fn repair_resets_bad_values() {
        let mut s = DimensionStyle {
            mark_size: f64::NAN,
            witness_gap: -1.0,
            line_extension: 1e9,
            line_weight: Some(0.0),
            mark_weight: Some(0.35),
            prefix: "a\nb".into(),
            suffix: "x".repeat(MAX_AFFIX + 5),
            ..DimensionStyle::default()
        };
        assert_eq!(s.repair(), 6);
        assert_eq!(s.mark_size, 2.6);
        assert_eq!((s.witness_gap, s.line_extension), (1.0, 1.5));
        assert_eq!((s.line_weight, s.mark_weight), (None, Some(0.35)));
        assert_eq!(s.prefix, "ab");
        assert_eq!(s.suffix.chars().count(), MAX_AFFIX);
        assert_eq!(s.repair(), 0);
    }

    #[test]
    fn label_wraps_the_value() {
        let s = DimensionStyle { prefix: "~".into(), suffix: " cm".into(), ..DimensionStyle::default() };
        assert_eq!(s.label("120"), "~120 cm");
    }
}
