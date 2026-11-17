//! The path tracer's frame. The model is Z up, in meters, in `f64`; the
//! path tracer is Y up (its sky map has its poles on Y) and works in `f32`.
//! A model point `p` becomes `p - origin` turned by `(x, y, z) → (x, z, -y)`:
//! a rotation of -90° about X, so the handedness, triangle windings and
//! normals are kept, model up (+Z) is the tracer's up (+Y) and model north
//! (+Y) is the tracer's -Z.

use glam::{DVec3, Vec3};

/// Model coordinates to the path tracer's, around a chosen origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Space {
    /// The model point that is the tracer's origin.
    pub origin: DVec3,
}

impl Space {
    pub fn new(origin: DVec3) -> Self {
        let origin = if origin.is_finite() { origin } else { DVec3::ZERO };
        Self { origin }
    }

    /// A model point in the tracer's frame.
    pub fn point(&self, p: DVec3) -> Vec3 {
        dir(p - self.origin)
    }

    /// A tracer point back in model coordinates.
    pub fn model_point(&self, p: Vec3) -> DVec3 {
        self.origin + model_dir(p)
    }
}

/// A model direction (or any vector) in the tracer's frame.
pub fn dir(d: DVec3) -> Vec3 {
    Vec3::new(d.x as f32, d.z as f32, -d.y as f32)
}

/// A tracer direction back in model coordinates.
pub fn model_dir(d: Vec3) -> DVec3 {
    DVec3::new(f64::from(d.x), -f64::from(d.z), f64::from(d.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_is_up_and_the_turn_keeps_handedness() {
        assert_eq!(dir(DVec3::Z), Vec3::Y);
        assert_eq!(dir(DVec3::Y), -Vec3::Z);
        assert_eq!(dir(DVec3::X), Vec3::X);
        // x × y = z in both frames.
        assert_eq!(dir(DVec3::X).cross(dir(DVec3::Y)), dir(DVec3::Z));
        let s = Space::new(DVec3::new(100.0, -20.0, 3.0));
        let p = DVec3::new(101.5, -18.0, 7.25);
        assert!(s.model_point(s.point(p)).distance(p) < 1e-6);
        assert_eq!(s.point(s.origin), Vec3::ZERO);
        assert_eq!(Space::new(DVec3::NAN).origin, DVec3::ZERO);
    }
}
