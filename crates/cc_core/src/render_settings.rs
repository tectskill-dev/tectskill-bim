//! Per-view settings of photorealistic rendering (path tracing).
//!
//! A view that takes them ([`crate::ViewKind::takes_render`]: 3D views)
//! keeps a [`RenderSettings`] in [`crate::View::render`]: the samples per
//! pixel, the light bounces, the output size, the field of view, the
//! exposure and tone mapping, the firefly clamp, the clay mode and the
//! environment (sky, sun and ground). Every field has a serde default, so
//! settings written by older or newer builds load; values out of range are
//! brought back by [`RenderSettings::repair`].

use serde::{Deserialize, Serialize};

/// Fewest samples per pixel.
pub const MIN_SAMPLES: u32 = 1;
/// Most samples per pixel.
pub const MAX_SAMPLES: u32 = 16_384;
/// Samples per pixel of new settings.
pub const DEFAULT_SAMPLES: u32 = 128;

/// Fewest light bounces.
pub const MIN_DEPTH: u32 = 1;
/// Most light bounces.
pub const MAX_DEPTH: u32 = 64;
/// Light bounces of new settings.
pub const DEFAULT_DEPTH: u32 = 8;

/// Smallest output width or height, pixels.
pub const MIN_SIZE: u32 = 16;
/// Largest output width or height, pixels.
pub const MAX_SIZE: u32 = 8192;
/// Most output pixels (7680 x 4320).
pub const MAX_PIXELS: u64 = 33_177_600;
/// Output width of new settings, pixels.
pub const DEFAULT_WIDTH: u32 = 1920;
/// Output height of new settings, pixels.
pub const DEFAULT_HEIGHT: u32 = 1080;

/// Narrowest vertical field of view, degrees.
pub const MIN_FOV_DEG: f64 = 10.0;
/// Widest vertical field of view, degrees.
pub const MAX_FOV_DEG: f64 = 120.0;
/// Vertical field of view of new settings, degrees (the 3D viewport's).
pub const DEFAULT_FOV_DEG: f64 = 45.0;

/// Lowest exposure, stops.
pub const MIN_EXPOSURE: f64 = -10.0;
/// Highest exposure, stops.
pub const MAX_EXPOSURE: f64 = 10.0;
/// Exposure of new settings, stops.
pub const DEFAULT_EXPOSURE: f64 = 0.0;

/// Lowest sample clamp (0 turns it off).
pub const MIN_CLAMP: f64 = 0.0;
/// Highest sample clamp.
pub const MAX_CLAMP: f64 = 1000.0;
/// Sample clamp of new settings.
pub const DEFAULT_CLAMP: f64 = 10.0;

/// Lowest sky or sun strength.
pub const MIN_STRENGTH: f64 = 0.0;
/// Highest sky or sun strength.
pub const MAX_STRENGTH: f64 = 100.0;
/// Sky strength of new settings.
pub const DEFAULT_SKY_STRENGTH: f64 = 1.0;
/// Sun strength of new settings.
pub const DEFAULT_SUN_STRENGTH: f64 = 1.0;
/// Ground color of new settings: a neutral warm gray.
pub const DEFAULT_GROUND_COLOR: [u8; 3] = [120, 115, 105];

/// How the rendered radiance is mapped to display colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ToneMap {
    /// A filmic curve: soft highlights, no hard clipping.
    #[default]
    #[serde(alias = "filmic")]
    Filmic,
    /// Plain clipping to the display range.
    #[serde(alias = "standard")]
    Standard,
}

impl ToneMap {
    pub const ALL: [ToneMap; 2] = [ToneMap::Filmic, ToneMap::Standard];

    /// The name commands use: `filmic` or `standard`.
    pub fn name(self) -> &'static str {
        match self {
            ToneMap::Filmic => "filmic",
            ToneMap::Standard => "standard",
        }
    }
}

/// The light around the model: sky, sun and ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderEnvironment {
    /// Multiplier of the sky's light.
    pub sky_strength: f64,
    /// Multiplier of the sun's light (the view's sun, see [`crate::sun`]).
    pub sun_strength: f64,
    /// Color of the ground, sRGB.
    pub ground_color: [u8; 3],
    /// An endless-looking ground under the model that catches shadows.
    pub ground_plane: bool,
}

impl Default for RenderEnvironment {
    fn default() -> Self {
        Self {
            sky_strength: DEFAULT_SKY_STRENGTH,
            sun_strength: DEFAULT_SUN_STRENGTH,
            ground_color: DEFAULT_GROUND_COLOR,
            ground_plane: true,
        }
    }
}

impl RenderEnvironment {
    /// Every value finite and in range.
    pub fn is_valid(&self) -> bool {
        in_range(self.sky_strength, MIN_STRENGTH, MAX_STRENGTH)
            && in_range(self.sun_strength, MIN_STRENGTH, MAX_STRENGTH)
    }

    /// Bring values back in range (non-finite ones to their defaults);
    /// whether anything changed.
    pub fn repair(&mut self) -> bool {
        let mut changed = repair_f64(&mut self.sky_strength, MIN_STRENGTH, MAX_STRENGTH, DEFAULT_SKY_STRENGTH);
        changed |= repair_f64(&mut self.sun_strength, MIN_STRENGTH, MAX_STRENGTH, DEFAULT_SUN_STRENGTH);
        changed
    }
}

/// How a view is rendered photorealistically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    /// Samples per pixel.
    pub samples: u32,
    /// Light bounces.
    pub max_depth: u32,
    /// Output width, pixels.
    pub width: u32,
    /// Output height, pixels.
    pub height: u32,
    /// Vertical field of view, degrees.
    pub fov_deg: f64,
    /// Exposure, stops (EV).
    pub exposure: f64,
    /// How radiance maps to display colors.
    pub tone_map: ToneMap,
    /// Largest radiance of a sample (removes fireflies); 0 turns it off.
    pub clamp: f64,
    /// Render every opaque surface in one white matte (glass stays clear
    /// glass), lit by the same sky, sun and ground as a normal render.
    pub clay: bool,
    /// Sky, sun and ground.
    pub environment: RenderEnvironment,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            samples: DEFAULT_SAMPLES,
            max_depth: DEFAULT_DEPTH,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            fov_deg: DEFAULT_FOV_DEG,
            exposure: DEFAULT_EXPOSURE,
            tone_map: ToneMap::Filmic,
            clamp: DEFAULT_CLAMP,
            clay: false,
            environment: RenderEnvironment::default(),
        }
    }
}

fn in_range(v: f64, min: f64, max: f64) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}

fn repair_f64(v: &mut f64, min: f64, max: f64, default: f64) -> bool {
    let fixed = if v.is_finite() { v.clamp(min, max) } else { default };
    let changed = fixed.to_bits() != v.to_bits();
    *v = fixed;
    changed
}

fn repair_u32(v: &mut u32, min: u32, max: u32) -> bool {
    let fixed = (*v).clamp(min, max);
    let changed = fixed != *v;
    *v = fixed;
    changed
}

impl RenderSettings {
    /// Every value finite and in range, and the output not over [`MAX_PIXELS`].
    pub fn is_valid(&self) -> bool {
        (MIN_SAMPLES..=MAX_SAMPLES).contains(&self.samples)
            && (MIN_DEPTH..=MAX_DEPTH).contains(&self.max_depth)
            && (MIN_SIZE..=MAX_SIZE).contains(&self.width)
            && (MIN_SIZE..=MAX_SIZE).contains(&self.height)
            && self.pixels() <= MAX_PIXELS
            && in_range(self.fov_deg, MIN_FOV_DEG, MAX_FOV_DEG)
            && in_range(self.exposure, MIN_EXPOSURE, MAX_EXPOSURE)
            && in_range(self.clamp, MIN_CLAMP, MAX_CLAMP)
            && self.environment.is_valid()
    }

    /// Output pixels.
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Output width over height.
    pub fn aspect(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height.max(1))
    }

    /// Bring values back in range (non-finite ones to their defaults) and
    /// scale an output over [`MAX_PIXELS`] down, keeping its aspect;
    /// whether anything changed.
    pub fn repair(&mut self) -> bool {
        let mut changed = repair_u32(&mut self.samples, MIN_SAMPLES, MAX_SAMPLES);
        changed |= repair_u32(&mut self.max_depth, MIN_DEPTH, MAX_DEPTH);
        changed |= repair_u32(&mut self.width, MIN_SIZE, MAX_SIZE);
        changed |= repair_u32(&mut self.height, MIN_SIZE, MAX_SIZE);
        if self.pixels() > MAX_PIXELS {
            let k = (MAX_PIXELS as f64 / self.pixels() as f64).sqrt();
            // Floor so the product stays under the limit; the minimum keeps both sides valid.
            let scale = |v: u32| ((f64::from(v) * k).floor() as u32).clamp(MIN_SIZE, MAX_SIZE);
            self.width = scale(self.width);
            self.height = scale(self.height);
            while self.pixels() > MAX_PIXELS && (self.width > MIN_SIZE || self.height > MIN_SIZE) {
                if self.width >= self.height {
                    self.width -= 1;
                } else {
                    self.height -= 1;
                }
            }
            changed = true;
        }
        changed |= repair_f64(&mut self.fov_deg, MIN_FOV_DEG, MAX_FOV_DEG, DEFAULT_FOV_DEG);
        changed |= repair_f64(&mut self.exposure, MIN_EXPOSURE, MAX_EXPOSURE, DEFAULT_EXPOSURE);
        changed |= repair_f64(&mut self.clamp, MIN_CLAMP, MAX_CLAMP, DEFAULT_CLAMP);
        changed |= self.environment.repair();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let s = RenderSettings::default();
        assert!(s.is_valid());
        assert!((s.aspect() - 16.0 / 9.0).abs() < 1e-12);
        assert!(!s.clone().repair());
    }

    #[test]
    fn repair_fixes_bad_values() {
        let mut s = RenderSettings {
            samples: 0,
            max_depth: 1000,
            fov_deg: f64::NAN,
            exposure: -50.0,
            clamp: f64::INFINITY,
            environment: RenderEnvironment { sky_strength: -1.0, sun_strength: f64::NAN, ..Default::default() },
            ..Default::default()
        };
        assert!(!s.is_valid());
        assert!(s.repair());
        assert!(s.is_valid());
        assert_eq!(s.samples, MIN_SAMPLES);
        assert_eq!(s.max_depth, MAX_DEPTH);
        assert_eq!(s.fov_deg, DEFAULT_FOV_DEG);
        assert_eq!(s.exposure, MIN_EXPOSURE);
        assert_eq!(s.clamp, DEFAULT_CLAMP);
        assert_eq!(s.environment.sky_strength, MIN_STRENGTH);
        assert_eq!(s.environment.sun_strength, DEFAULT_SUN_STRENGTH);
    }

    #[test]
    fn repair_scales_oversized_output_down() {
        let mut s = RenderSettings { width: 8192, height: 8192, ..Default::default() };
        assert!(!s.is_valid());
        assert!(s.repair());
        assert!(s.is_valid());
        assert!(s.pixels() <= MAX_PIXELS);
        assert_eq!(s.width, s.height);

        let mut wide = RenderSettings { width: 8192, height: 4608, ..Default::default() };
        assert!(wide.repair());
        assert!(wide.is_valid());
        assert!((wide.aspect() - 16.0 / 9.0).abs() < 0.01);

        let mut tiny = RenderSettings { width: 0, height: 100_000, ..Default::default() };
        assert!(tiny.repair());
        assert!(tiny.is_valid());
    }

    #[test]
    fn empty_input_reads_as_defaults() {
        let from_ron: RenderSettings = ron::from_str("()").unwrap();
        assert_eq!(from_ron, RenderSettings::default());
        let partial: RenderSettings = ron::from_str("(samples: 32, environment: ())").unwrap();
        assert_eq!(partial, RenderSettings { samples: 32, ..Default::default() });
    }

    #[test]
    fn round_trip() {
        let s = RenderSettings {
            samples: 512,
            tone_map: ToneMap::Standard,
            clay: true,
            environment: RenderEnvironment { ground_plane: false, ground_color: [1, 2, 3], ..Default::default() },
            ..Default::default()
        };
        let ron = ron::to_string(&s).unwrap();
        assert_eq!(ron::from_str::<RenderSettings>(&ron).unwrap(), s);
    }
}
