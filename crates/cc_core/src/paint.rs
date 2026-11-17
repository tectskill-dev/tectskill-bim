//! Painted faces: a material laid on one surface of an element, over the
//! material its type gives it (a painted wall face, a tiled floor top).
//!
//! The geometry is derived and rebuilt after every edit, so a paint does not
//! hold a face index. It keeps what identifies the face from outside: a
//! point on it and its outward normal, in model coordinates. After a rebuild
//! the paint goes on the element's face (of its body, framing left out)
//! whose normal is within [`NORMAL_TOLERANCE_DEG`] of the stored one and
//! that passes nearest the stored point, and from there on the faces of the
//! element in the same plane (a wall face split by its openings) and, for a
//! curved surface, the smooth facets joined to it (see `cc_geom::paint`).
//! Moving, rotating or mirroring the element in plan moves the point and
//! turns the normal with it; other edits (a longer wall, a thicker layer)
//! keep the paint on the face that is still nearest. A face turned beyond
//! the tolerance leaves the paint unshown until it is painted again.

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// Most paints an element keeps.
pub const MAX_PAINTS: usize = 256;
/// How far a face may have turned and still take its paint, degrees.
pub const NORMAL_TOLERANCE_DEG: f64 = 20.0;

/// A material painted on one surface of an element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacePaint {
    /// A point on the painted face, meters.
    pub point: DVec3,
    /// The face's outward unit normal when it was painted.
    pub normal: DVec3,
    /// The material's name.
    pub material: String,
}

impl FacePaint {
    /// Whether the numbers are usable (finite, a normal of unit length).
    pub fn is_valid(&self) -> bool {
        self.point.is_finite()
            && self.point.abs().max_element() < 1e9
            && self.normal.is_finite()
            && (self.normal.length() - 1.0).abs() < 1e-6
    }

    /// Move and turn with the element in plan.
    pub fn transform(&mut self, t: &crate::PlanTransform) {
        let p = t.apply(self.point.truncate());
        self.point = p.extend(self.point.z);
        let n = t.apply_vector(self.normal.truncate());
        self.normal = n.extend(self.normal.z).normalize_or(DVec3::Z);
    }

    pub fn translate(&mut self, delta: DVec2) {
        self.point += delta.extend(0.0);
    }
}

/// Drop paints with bad numbers, of materials that are gone, and past the
/// limit; returns how many went. Normals are made unit length first.
pub fn repair(paints: &mut Vec<FacePaint>, material_exists: &dyn Fn(&str) -> bool) -> usize {
    let before = paints.len();
    for p in paints.iter_mut() {
        if p.normal.is_finite() && p.normal.length() > 1e-9 {
            p.normal = p.normal.normalize();
        }
    }
    paints.retain(|p| p.is_valid() && material_exists(&p.material));
    paints.truncate(MAX_PAINTS);
    before - paints.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_drops_bad_paints_and_missing_materials() {
        let ok =
            FacePaint { point: DVec3::new(1.0, 2.0, 0.5), normal: DVec3::new(0.0, 2.0, 0.0), material: "A".into() };
        let mut paints = vec![
            ok.clone(),
            FacePaint { material: "Gone".into(), ..ok.clone() },
            FacePaint { point: DVec3::new(f64::NAN, 0.0, 0.0), ..ok.clone() },
            FacePaint { normal: DVec3::ZERO, ..ok.clone() },
        ];
        assert_eq!(repair(&mut paints, &|m| m == "A"), 3);
        assert_eq!(paints.len(), 1);
        assert_eq!(paints[0].normal, DVec3::Y);
        let mut many = vec![paints[0].clone(); MAX_PAINTS + 3];
        assert_eq!(repair(&mut many, &|_| true), 3);
    }

    #[test]
    fn paints_turn_with_their_element() {
        let mut p = FacePaint { point: DVec3::new(1.0, 0.0, 2.0), normal: DVec3::X, material: "A".into() };
        let t = crate::PlanTransform::rotate_about(DVec2::ZERO, std::f64::consts::FRAC_PI_2);
        p.transform(&t);
        assert!(p.point.distance(DVec3::new(0.0, 1.0, 2.0)) < 1e-9);
        assert!(p.normal.distance(DVec3::Y) < 1e-9);
        p.translate(DVec2::new(1.0, 1.0));
        assert!(p.point.distance(DVec3::new(1.0, 2.0, 2.0)) < 1e-9);
    }
}
