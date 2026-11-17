//! The light around the model: a generated sky map with the ground
//! below the horizon, a sun, and an optional ground plane that catches
//! shadows.
//!
//! The sky is an equirectangular map of [`ENV_WIDTH`] × [`ENV_HEIGHT`]
//! pixels in linear light (about 0.35° a pixel: the path tracer reads the
//! nearest pixel, and coarser maps show the sky's gradient as bands), made
//! from the sun's altitude: deep blue at the zenith, paler towards the
//! horizon, warmer there towards a low sun, with a soft glow around the
//! sun; once the sun is below the horizon it fades through a blue-gray dusk
//! to night. Below
//! the horizon the map holds the ground color lit by the sun and the sky (a
//! matte ground's radiance), blended into the sky over a degree or two, so
//! the horizon past the ground plane reads right. The sky map is scaled by
//! [`cc_core::RenderEnvironment::sky_strength`].
//!
//! The sun is a distant light of [`SUN_IRRADIANCE`] at the zenith, dimmed
//! and warmed by the air it crosses (an air mass formula and a per-channel
//! extinction), none below the horizon, scaled by
//! [`cc_core::RenderEnvironment::sun_strength`]. Its disk is not drawn in
//! the sky map: rays bouncing off surfaces would find it and count the sun
//! a second time, as bright isolated pixels; the glow around it is broad
//! and dim enough not to.
//!
//! A view with no sun is lit by [`default_sun`], a sun that follows the
//! view: [`DEFAULT_SUN_ALTITUDE`] high, behind the viewer and to the left
//! (where the 3D view's own light comes from), so the faces seen are lit
//! and the shadows fall to the right.
//!
//! At the default strengths, exposure 0 and the filmic curve, a white wall
//! facing a high sun comes out bright but not clipped, the sky light blue
//! and shaded faces mid gray.

use crate::materials;
use crate::space;
use cc_core::sun::{SunMode, SunSettings, SunState};
use cc_core::{CameraState, Project, RenderEnvironment, View};
use glam::{DVec3, Vec3};
use sort_raytracing::light::{EnvMap, Light, Sky};

/// Width of the sky map, pixels (longitude).
pub const ENV_WIDTH: usize = 1024;
/// Height of the sky map, pixels (latitude, zenith to nadir).
pub const ENV_HEIGHT: usize = 512;
/// Irradiance of the sun at the zenith on a surface facing it, before the
/// air dims it (in the units of the sky map's radiance).
pub const SUN_IRRADIANCE: f64 = 5.0;
/// Altitude of the sun of a view with none, degrees.
pub const DEFAULT_SUN_ALTITUDE: f64 = 45.0;
/// Where the sun of a view with none stands, degrees clockwise from the
/// direction the viewer looks: behind and to the left.
pub const DEFAULT_SUN_AZIMUTH: f64 = 225.0;
/// The ground plane is this far below the ground height, meters, so faces
/// lying at that height (floor slabs) cover it.
pub const GROUND_NUDGE: f64 = 1e-3;
/// Smallest half side of the ground plane, meters.
pub const MIN_GROUND_HALF_SIDE: f64 = 500.0;
/// Half side of the ground plane in model radii (around the model's center).
pub const GROUND_RADII: f64 = 60.0;

/// Light extinction of the air per unit of air mass, red, green, blue: a
/// sun high up is near white, a low one orange.
const EXTINCTION: [f64; 3] = [0.045, 0.075, 0.16];
/// Sky colors (linear) at full daylight: zenith and horizon, and the warm
/// horizon towards a low sun.
const ZENITH: [f64; 3] = [0.06, 0.15, 0.40];
const HORIZON: [f64; 3] = [0.42, 0.52, 0.68];
const HORIZON_WARM: [f64; 3] = [0.80, 0.52, 0.30];
/// Sky colors once the sun has set (they fade further to night).
const DUSK_ZENITH: [f64; 3] = [0.010, 0.014, 0.030];
const DUSK_HORIZON: [f64; 3] = [0.032, 0.034, 0.046];

fn v3(c: [f64; 3]) -> DVec3 {
    DVec3::from_array(c)
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun lighting `view` through `camera`: the view's own sun (a view
/// sun turning with the view follows `camera`), else [`default_sun`].
pub fn sun_state(project: &Project, view: &View, camera: &CameraState) -> SunState {
    let forward = cc_core::sun::camera_forward(camera);
    match &view.sun {
        Some(sun) => sun.state(&project.sun_site(), forward),
        None => default_sun().state(&project.sun_site(), forward),
    }
}

/// The sun of a view that has none: [`DEFAULT_SUN_ALTITUDE`] high, at
/// [`DEFAULT_SUN_AZIMUTH`] from the direction the viewer looks.
pub fn default_sun() -> SunSettings {
    SunSettings {
        mode: SunMode::Lighting {
            azimuth: DEFAULT_SUN_AZIMUTH,
            altitude: DEFAULT_SUN_ALTITUDE,
            relative_to_view: true,
        },
        ..SunSettings::default()
    }
}

/// The unit vector towards the sun, when it is above the horizon and a number.
fn sun_up(sun: &SunState) -> Option<DVec3> {
    let d = sun.direction.try_normalize()?;
    (sun.above_horizon && d.z > 0.0).then_some(d)
}

/// The sine of the sun's altitude (its height), even below the horizon.
fn sun_height(sun: &SunState) -> f64 {
    match sun.direction.try_normalize() {
        Some(d) => d.z,
        None if sun.altitude.is_finite() => sun.altitude.to_radians().sin(),
        None => -1.0,
    }
}

/// The share of sunlight that crosses the air down to the ground, per
/// channel, for a sun of height `sin_alt` (none at or below the horizon).
fn air_transmittance(sin_alt: f64) -> DVec3 {
    if sin_alt <= 0.0 {
        return DVec3::ZERO;
    }
    let alt_deg = sin_alt.clamp(-1.0, 1.0).asin().to_degrees();
    // Kasten and Young's air mass.
    let mass = 1.0 / (sin_alt + 0.50572 * (alt_deg + 6.07995).powf(-1.6364));
    let fade = smoothstep(0.0, 0.03, sin_alt);
    DVec3::new((-EXTINCTION[0] * mass).exp(), (-EXTINCTION[1] * mass).exp(), (-EXTINCTION[2] * mass).exp()) * fade
}

/// The sun's irradiance (linear RGB) at strength 1, on a surface facing it.
pub fn sun_irradiance(sun: &SunState) -> DVec3 {
    match sun_up(sun) {
        Some(d) => air_transmittance(d.z) * SUN_IRRADIANCE,
        None => DVec3::ZERO,
    }
}

/// The sky's radiance (linear RGB) at strength 1 along unit direction `d`
/// (model coordinates, at or above the horizon) under `sun`.
pub fn sky_radiance(d: DVec3, sun: &SunState) -> DVec3 {
    let s = sun.direction.try_normalize().unwrap_or(DVec3::Z);
    let sin_alt = sun_height(sun);
    let h = d.z.clamp(0.0, 1.0);
    // How near the horizon the direction is, 1 on it.
    let low = (1.0 - h).powi(4);
    // Facing the sun across the plan, 0 to 1.
    let facing = match (d.truncate().try_normalize(), s.truncate().try_normalize()) {
        (Some(a), Some(b)) => 0.5 + 0.5 * a.dot(b),
        _ => 0.5,
    };
    // Daylight from 6° below the horizon to 6° above.
    let day = smoothstep(-0.105, 0.105, sin_alt);
    let bright = 0.35 + 0.65 * sin_alt.max(0.0).sqrt();
    let warm = (1.0 - smoothstep(0.0, 0.4, sin_alt)) * day * facing * facing;
    let horizon = v3(HORIZON).lerp(v3(HORIZON_WARM), warm);
    let mut day_sky = v3(ZENITH).lerp(horizon, low) * bright;
    // A glow around the sun, tinted like its light.
    let cos_g = d.dot(s).clamp(-1.0, 1.0);
    let tint = air_transmittance(sin_alt.max(0.02)) / air_transmittance(1.0).max_element().max(1e-6);
    day_sky += tint * (0.8 * ((cos_g - 1.0) * 60.0).exp() + 0.2 * ((cos_g - 1.0) * 6.0).exp()) * bright;
    // Dusk, fading to night by 18° below the horizon, warmer low towards the set sun.
    let night = 0.12 + 0.88 * smoothstep(-0.31, 0.0, sin_alt);
    let mut dusk = v3(DUSK_ZENITH).lerp(v3(DUSK_HORIZON), low) * night;
    dusk += DVec3::new(0.10, 0.05, 0.025) * low * facing.powi(4) * smoothstep(-0.2, 0.0, sin_alt);
    dusk.lerp(day_sky, day)
}

/// The sky map of `sun` and `environment` (in the tracer's frame), with the
/// ground of `environment`'s color below the horizon.
pub fn sky_map(sun: &SunState, environment: &RenderEnvironment) -> EnvMap {
    let strength = |v: f64| if v.is_finite() { v.max(0.0) } else { 1.0 };
    let (sky, sun_strength) = (strength(environment.sky_strength), strength(environment.sun_strength));
    // The direction of each pixel: the tracer's polar angle from its +Y (up)
    // and azimuth from +X towards +Z, back in model coordinates.
    let direction = |x: usize, y: usize| {
        let theta = std::f64::consts::PI * (y as f64 + 0.5) / ENV_HEIGHT as f64;
        let phi = 2.0 * std::f64::consts::PI * (x as f64 + 0.5) / ENV_WIDTH as f64;
        let (st, ct) = theta.sin_cos();
        let (sp, cp) = phi.sin_cos();
        space::model_dir(Vec3::new((st * cp) as f32, ct as f32, (st * sp) as f32)).normalize_or(DVec3::Z)
    };
    let mut skyline = vec![DVec3::ZERO; ENV_WIDTH * ENV_HEIGHT];
    // The sky irradiance on the ground, to light the ground below the horizon.
    let mut sky_irradiance = DVec3::ZERO;
    let d_omega = |y: usize| {
        let theta = std::f64::consts::PI * (y as f64 + 0.5) / ENV_HEIGHT as f64;
        (2.0 * std::f64::consts::PI / ENV_WIDTH as f64) * (std::f64::consts::PI / ENV_HEIGHT as f64) * theta.sin()
    };
    for y in 0..ENV_HEIGHT {
        for x in 0..ENV_WIDTH {
            let d = direction(x, y);
            let l = sky_radiance(DVec3::new(d.x, d.y, d.z.max(0.0)).normalize_or(DVec3::Z), sun) * sky;
            if d.z > 0.0 {
                sky_irradiance += l * d.z * d_omega(y);
            }
            if let Some(px) = skyline.get_mut(y * ENV_WIDTH + x) {
                *px = l;
            }
        }
    }
    let albedo = materials::linear(environment.ground_color).as_dvec3();
    let sun_on_ground = sun_irradiance(sun) * sun_strength * sun_up(sun).map_or(0.0, |d| d.z);
    let ground = albedo * (sun_on_ground + sky_irradiance) / std::f64::consts::PI;
    let pixels = skyline
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let d = direction(i % ENV_WIDTH, i / ENV_WIDTH);
            let t = smoothstep(-0.03, 0.005, d.z);
            let c = ground.lerp(*l, t);
            let c = if c.is_finite() { c.max(DVec3::ZERO) } else { DVec3::ZERO };
            c.as_vec3()
        })
        .collect();
    EnvMap::new(ENV_WIDTH, ENV_HEIGHT, pixels)
}

/// The lights of a render under `sun`: the sun (when it is above the horizon
/// and has strength) and the sky.
pub fn lights(sun: &SunState, environment: &RenderEnvironment) -> Vec<Light> {
    let mut out = Vec::new();
    let strength = if environment.sun_strength.is_finite() { environment.sun_strength.max(0.0) } else { 1.0 };
    let irradiance = sun_irradiance(sun) * strength;
    if let Some(d) = sun_up(sun)
        && irradiance.max_element() > 0.0
    {
        out.push(Light::Distant { dir: space::dir(-d).normalize_or(Vec3::NEG_Y), intensity: irradiance.as_vec3() });
    }
    out.push(Light::Sky(Sky { intensity: Vec3::ONE, env: Some(sky_map(sun, environment)), rotation: 0.0 }));
    out
}

/// The height of the ground plane: the view's sun's ground level, else the
/// lowest level, else the bottom of the model (`bottom`), else 0.
pub fn ground_height(project: &Project, view: &View, bottom: Option<f64>) -> f64 {
    let sun_ground = view.sun.as_ref().and_then(|s| s.ground).and_then(|l| project.level(l)).map(|l| l.elevation);
    let lowest = project.levels.iter().map(|l| l.elevation).filter(|z| z.is_finite()).min_by(f64::total_cmp);
    sun_ground.filter(|z| z.is_finite()).or(lowest).or(bottom.filter(|z| z.is_finite())).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sun_at(altitude: f64, azimuth_from_x: f64) -> SunState {
        let (a, z) = (altitude.to_radians(), azimuth_from_x.to_radians());
        SunState {
            azimuth: 0.0,
            altitude,
            direction: DVec3::new(a.cos() * z.cos(), a.cos() * z.sin(), a.sin()),
            frame: None,
            above_horizon: altitude > 0.0,
        }
    }

    #[test]
    fn the_sky_is_blue_above_and_the_ground_below() {
        let env = RenderEnvironment::default();
        let sun = sun_at(50.0, 30.0);
        let map = sky_map(&sun, &env);
        let at = |d: DVec3| map.evaluate(space::dir(d.normalize()));
        let zenith = at(DVec3::Z);
        assert!(zenith.z > zenith.x * 2.0, "{zenith}");
        let ground = at(-DVec3::Z);
        // The ground's warm gray, lit.
        assert!(ground.x > ground.z && ground.x > 0.05, "{ground}");
        // Paler at the horizon than at the zenith.
        let horizon = at(DVec3::new(-1.0, 0.0, 0.05));
        assert!(horizon.x > zenith.x * 2.0, "{horizon} {zenith}");
        // Brighter towards the sun.
        assert!(intensity(at(sun.direction)) > intensity(at(-sun.direction.truncate().extend(0.6))));
    }

    fn intensity(c: Vec3) -> f32 {
        0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
    }

    #[test]
    fn the_sun_weakens_and_warms_as_it_sets() {
        let noon = sun_irradiance(&sun_at(70.0, 0.0));
        let low = sun_irradiance(&sun_at(5.0, 0.0));
        assert!(noon.max_element() > 4.0 && noon.x / noon.z < 1.3, "{noon}");
        assert!(low.max_element() < noon.max_element() && low.x / low.z > 2.0, "{low}");
        assert_eq!(sun_irradiance(&sun_at(-5.0, 0.0)), DVec3::ZERO);
        let env = RenderEnvironment::default();
        assert_eq!(lights(&sun_at(-5.0, 0.0), &env).len(), 1);
        assert_eq!(lights(&sun_at(40.0, 0.0), &env).len(), 2);
        let off = RenderEnvironment { sun_strength: 0.0, ..env.clone() };
        assert_eq!(lights(&sun_at(40.0, 0.0), &off).len(), 1);
        // At night the sky is dark.
        let night = sky_radiance(DVec3::Z, &sun_at(-30.0, 0.0));
        assert!(night.max_element() < 0.02, "{night}");
        // A sun that is not a number lights nothing and breaks nothing.
        let broken = SunState { direction: DVec3::NAN, altitude: f64::NAN, ..sun_at(40.0, 0.0) };
        assert_eq!(sun_irradiance(&broken), DVec3::ZERO);
        assert!(sky_radiance(DVec3::Z, &broken).is_finite());
    }

    #[test]
    fn the_default_sun_stands_behind_the_viewer_to_the_left() {
        let p = Project::from_template();
        let view = p.views.values().find(|v| matches!(v.kind, cc_core::ViewKind::ThreeD { .. })).unwrap().clone();
        let view = View { sun: None, ..view };
        let camera = CameraState { yaw_deg: 0.0, ..CameraState::default() };
        let sun = sun_state(&p, &view, &camera);
        // Looking north (+Y): the sun is south-west, 45° up.
        assert!((sun.altitude - DEFAULT_SUN_ALTITUDE).abs() < 1e-6);
        assert!(sun.direction.x < 0.0 && sun.direction.y < 0.0 && sun.direction.z > 0.6, "{}", sun.direction);
    }
}
