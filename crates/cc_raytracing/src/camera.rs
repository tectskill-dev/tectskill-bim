//! The render's camera: the 3D view's orbit camera ([`CameraState`]) as a
//! pinhole camera of the path tracer, framing the picture the viewport
//! frames (its perspective has a vertical field of view of 45°, the default
//! of [`cc_core::RenderSettings::fov_deg`]) at the render's aspect.

use crate::RenderError;
use crate::space::{self, Space};
use cc_core::CameraState;
use cc_geom::ModelGeometry;
use glam::{DVec3, Vec3};
use sort_raytracing::camera::Camera;

/// The field of view of the 3D viewport, degrees (vertical).
pub const VIEWPORT_FOV_DEG: f64 = 45.0;

/// The eye and the unit direction the camera looks along, in model
/// coordinates. A camera with no distance (or a negative one) stands at its
/// target, still looking along its yaw and pitch.
pub fn eye_and_forward(camera: &CameraState) -> Result<(DVec3, DVec3), RenderError> {
    let finite = camera.target.is_finite()
        && camera.yaw_deg.is_finite()
        && camera.pitch_deg.is_finite()
        && camera.distance.is_finite();
    if !finite {
        return Err(RenderError::InvalidCamera("a value is not a number".into()));
    }
    let forward = camera.forward().try_normalize().ok_or_else(|| RenderError::InvalidCamera("no direction".into()))?;
    let eye = if camera.distance > 0.0 { camera.eye() } else { camera.target };
    if !eye.is_finite() {
        return Err(RenderError::InvalidCamera("the eye is not a number".into()));
    }
    Ok((eye, forward))
}

/// The path tracer's camera for `camera` with a vertical field of view of
/// `fov_deg` and an image of `width` × `height` pixels, in `space`.
pub(crate) fn tracer_camera(
    camera: &CameraState,
    fov_deg: f64,
    width: u32,
    height: u32,
    space: &Space,
) -> Result<Camera, RenderError> {
    let (eye, forward) = eye_and_forward(camera)?;
    let fov = if fov_deg.is_finite() { fov_deg.clamp(1.0, 179.0) } else { VIEWPORT_FOV_DEG };
    let eye = space.point(eye);
    let target = eye + space::dir(forward);
    if !eye.is_finite() || !target.is_finite() || target == eye {
        return Err(RenderError::InvalidCamera("the eye is too far from the model".into()));
    }
    // Model up (+Z) is the tracer's +Y.
    Ok(Camera::look_at(eye, target, Vec3::Y, fov.to_radians() as f32, width.max(1), height.max(1)))
}

/// `camera` moved back along its direction so the whole of `geom` fits a
/// picture of aspect `aspect` (width over height) with a vertical field of
/// view of `fov_deg`, around the model's center (what the 3D view does to a
/// view whose camera was never moved); the camera itself when there is no
/// geometry.
pub fn fit(camera: &CameraState, geom: &ModelGeometry, fov_deg: f64, aspect: f64) -> CameraState {
    let Some((lo, hi)) = geom.bounds().filter(|(lo, hi)| lo.is_finite() && hi.is_finite()) else {
        return *camera;
    };
    let fov = if fov_deg.is_finite() { fov_deg.clamp(1.0, 179.0) } else { VIEWPORT_FOV_DEG };
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let radius = ((hi - lo).length() * 0.5).max(1.0);
    // The narrower of the vertical and horizontal fields of view limits the fit.
    let half_v = (fov * 0.5).to_radians();
    let half_h = (half_v.tan() * aspect).atan();
    CameraState { target: (lo + hi) * 0.5, distance: radius / half_v.min(half_h).sin() * 1.05, ..*camera }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_cameras_are_refused_or_stand_at_their_target() {
        let space = Space::new(DVec3::ZERO);
        let nan = CameraState { distance: f64::NAN, ..CameraState::default() };
        assert!(matches!(tracer_camera(&nan, 45.0, 32, 32, &space), Err(RenderError::InvalidCamera(_))));
        let inf = CameraState { target: DVec3::new(f64::INFINITY, 0.0, 0.0), ..CameraState::default() };
        assert!(tracer_camera(&inf, 45.0, 32, 32, &space).is_err());
        let zero = CameraState { distance: 0.0, ..CameraState::default() };
        let c = tracer_camera(&zero, 45.0, 32, 32, &space).unwrap();
        assert!(space.model_point(c.eye).distance(zero.target) < 1e-4);
        assert!((space::model_dir(c.forward) - zero.forward()).length() < 1e-5);
        // A field of view that is not a number falls back to the viewport's.
        let c = tracer_camera(&CameraState::default(), f64::NAN, 32, 32, &space).unwrap();
        assert!((c.fov_y - (VIEWPORT_FOV_DEG as f32).to_radians()).abs() < 1e-6);
    }

    #[test]
    fn fitting_frames_the_model() {
        let mut geom = ModelGeometry::default();
        let mut solid = cc_geom::Solid::new(cc_core::ElementId(1), cc_core::Category::Wall, [200; 3]);
        solid.faces.push(cc_geom::Face::new(
            vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 3.0)],
            -DVec3::Y,
        ));
        geom.solids.push(solid);
        let fitted = fit(&CameraState::default(), &geom, 45.0, 16.0 / 9.0);
        assert!(fitted.target.distance(DVec3::new(5.0, 0.0, 1.5)) < 1e-9);
        assert!(fitted.distance > 5.0);
        assert_eq!(fitted.yaw_deg, CameraState::default().yaw_deg);
        assert_eq!(fit(&CameraState::default(), &ModelGeometry::default(), 45.0, 1.0), CameraState::default());
    }
}
