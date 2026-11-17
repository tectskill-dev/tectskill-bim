//! Drawing scales as people read them: `1:50` in metric projects, the
//! customary `1/4" = 1'-0"` in imperial ones. A view keeps its scale as the
//! ratio `N` of 1:`N` (`View::scale`); this only names and reads it.

use crate::MeasurementSystem;

/// The customary imperial scales: ratio and label (architectural, then engineering).
pub const IMPERIAL: [(u32, &str); 19] = [
    (1, "12\" = 1'-0\""),
    (2, "6\" = 1'-0\""),
    (4, "3\" = 1'-0\""),
    (8, "1 1/2\" = 1'-0\""),
    (12, "1\" = 1'-0\""),
    (16, "3/4\" = 1'-0\""),
    (24, "1/2\" = 1'-0\""),
    (32, "3/8\" = 1'-0\""),
    (48, "1/4\" = 1'-0\""),
    (64, "3/16\" = 1'-0\""),
    (96, "1/8\" = 1'-0\""),
    (128, "3/32\" = 1'-0\""),
    (192, "1/16\" = 1'-0\""),
    (120, "1\" = 10'-0\""),
    (240, "1\" = 20'-0\""),
    (360, "1\" = 30'-0\""),
    (480, "1\" = 40'-0\""),
    (600, "1\" = 50'-0\""),
    (1200, "1\" = 100'-0\""),
];

/// The scales a metric picker offers.
const METRIC_CHOICES: [u32; 13] = [1, 2, 5, 10, 20, 25, 50, 75, 100, 200, 250, 500, 1000];

/// The scales an imperial picker offers, in ascending ratio.
const IMPERIAL_CHOICES: [u32; 19] = [1, 2, 4, 8, 12, 16, 24, 32, 48, 64, 96, 120, 128, 192, 240, 360, 480, 600, 1200];

/// The scale 1:`n` as a project of `system` writes it: the customary label
/// of an imperial scale in the table, else `1:n`.
pub fn label(system: MeasurementSystem, n: u32) -> String {
    if system == MeasurementSystem::Imperial
        && let Some((_, l)) = IMPERIAL.iter().find(|(r, _)| *r == n)
    {
        return (*l).to_string();
    }
    format!("1:{n}")
}

/// The scales a scale picker offers in a project of `system`, ascending.
pub fn choices(system: MeasurementSystem) -> &'static [u32] {
    match system {
        MeasurementSystem::Metric => &METRIC_CHOICES,
        MeasurementSystem::Imperial => &IMPERIAL_CHOICES,
    }
}

/// Reads a scale typed as `1:48`, `48` or a label of the imperial table
/// (`1/4" = 1'-0"`, spaces optional, `1'` for `1'-0"`). Zero is refused.
pub fn parse(text: &str) -> Option<u32> {
    let t = text.trim();
    if t.len() > 64 {
        return None;
    }
    let ratio = if let Some(rest) = t.strip_prefix("1:") {
        rest.trim().parse::<u32>().ok()
    } else if let Ok(n) = t.parse::<u32>() {
        Some(n)
    } else {
        let key = normalize(t);
        IMPERIAL.iter().find(|(_, l)| normalize(l) == key).map(|(r, _)| *r)
    };
    ratio.filter(|n| *n > 0)
}

/// A label without spaces and with `'-0"` read as `'`.
fn normalize(s: &str) -> String {
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    compact.replace("'-0\"", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(label(MeasurementSystem::Imperial, 48), "1/4\" = 1'-0\"");
        assert_eq!(label(MeasurementSystem::Imperial, 8), "1 1/2\" = 1'-0\"");
        assert_eq!(label(MeasurementSystem::Imperial, 1200), "1\" = 100'-0\"");
        assert_eq!(label(MeasurementSystem::Imperial, 50), "1:50", "odd ratios stay ratios");
        assert_eq!(label(MeasurementSystem::Imperial, 0), "1:0");
        assert_eq!(label(MeasurementSystem::Metric, 48), "1:48");
        assert_eq!(label(MeasurementSystem::Metric, 50), "1:50");
    }

    #[test]
    fn choice_lists() {
        assert_eq!(choices(MeasurementSystem::Metric), &METRIC_CHOICES);
        let imperial = choices(MeasurementSystem::Imperial);
        assert!(imperial.windows(2).all(|w| w[0] < w[1]), "ascending");
        assert_eq!(imperial.len(), IMPERIAL.len());
        assert!(IMPERIAL.iter().all(|(r, _)| imperial.contains(r)));
    }

    #[test]
    fn parsing() {
        assert_eq!(parse("1:48"), Some(48));
        assert_eq!(parse(" 1 : 50 "), None, "a space after 1 is not a ratio");
        assert_eq!(parse("1: 50"), Some(50));
        assert_eq!(parse("48"), Some(48));
        assert_eq!(parse("1/4\" = 1'-0\""), Some(48));
        assert_eq!(parse("1/4\"=1'-0\""), Some(48));
        assert_eq!(parse("1/4\" = 1'"), Some(48));
        assert_eq!(parse("1 1/2\" = 1'-0\""), Some(8));
        assert_eq!(parse("1\" = 20'"), Some(240));
        assert_eq!(parse("12\"=1'"), Some(1));
        for (r, l) in IMPERIAL {
            assert_eq!(parse(l), Some(r), "{l}");
        }
        assert_eq!(parse("0"), None);
        assert_eq!(parse("1:0"), None);
        assert_eq!(parse("1/5\" = 1'-0\""), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("abc"), None);
    }
}
