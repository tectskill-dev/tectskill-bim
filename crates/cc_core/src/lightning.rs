//! Lightning protection: the conductors of a lightning protection system
//! (on the roof, down the walls and in the ground) and the protection
//! classes the coverage check uses.
//!
//! Air terminals and earth electrodes are components (devices of the
//! [`DeviceKind::AirTerminal`](crate::DeviceKind::AirTerminal) and
//! [`DeviceKind::EarthElectrode`](crate::DeviceKind::EarthElectrode) kinds);
//! conductors are runs like pipes ([`MepSegment`]), of a conductor type
//! that gives their diameter and material.

use crate::MepSegment;
use serde::{Deserialize, Serialize};

/// What a lightning protection conductor does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ConductorRole {
    /// Air termination: a conductor on the roof that intercepts strikes.
    #[default]
    Air,
    /// Down conductor: from the roof down the walls to the earthing.
    Down,
    /// Earthing: a buried conductor (an earth ring) or a bonding conductor.
    Earth,
}

impl ConductorRole {
    pub const ALL: [ConductorRole; 3] = [ConductorRole::Air, ConductorRole::Down, ConductorRole::Earth];

    pub fn key(self) -> &'static str {
        match self {
            ConductorRole::Air => "conductor-air",
            ConductorRole::Down => "conductor-down",
            ConductorRole::Earth => "conductor-earth",
        }
    }

    /// Name in commands (`air`, `down`, `earth`).
    pub fn name(self) -> &'static str {
        match self {
            ConductorRole::Air => "air",
            ConductorRole::Down => "down",
            ConductorRole::Earth => "earth",
        }
    }

    /// The usual role of `run`: vertical runs are down conductors, runs
    /// below the level are in the ground, the others on the roof.
    pub fn of_run(run: &MepSegment) -> Self {
        if run.is_vertical() {
            ConductorRole::Down
        } else if run.start_offset.max(run.end_offset) < 0.0 {
            ConductorRole::Earth
        } else {
            ConductorRole::Air
        }
    }
}

/// A conductor of a lightning protection system: a straight run (on a
/// roof, down a wall or in the ground) with its role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightningConductor {
    pub run: MepSegment,
    #[serde(default)]
    pub role: ConductorRole,
}

/// Depth of an earth ring and of the top of earth electrodes below the
/// level, meters (the usual minimum burial depth).
pub const EARTH_DEPTH: f64 = 0.5;
/// Usual distance of an earth ring from the building's walls, meters.
pub const EARTH_RING_DISTANCE: f64 = 1.0;
/// Smallest and largest conductor diameter, meters (values outside are cut in files).
pub const MIN_CONDUCTOR_DIAMETER: f64 = 0.002;
pub const MAX_CONDUCTOR_DIAMETER: f64 = 0.05;

/// Class of a lightning protection system (I is the most demanding). It
/// sets the rolling sphere radius and the mesh width of the air termination,
/// as in the public tables of the lightning protection standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LpsClass {
    I,
    II,
    #[default]
    III,
    IV,
}

impl LpsClass {
    pub const ALL: [LpsClass; 4] = [LpsClass::I, LpsClass::II, LpsClass::III, LpsClass::IV];

    /// Radius of the rolling sphere, meters.
    pub fn sphere_radius(self) -> f64 {
        match self {
            LpsClass::I => 20.0,
            LpsClass::II => 30.0,
            LpsClass::III => 45.0,
            LpsClass::IV => 60.0,
        }
    }

    /// Width of the air termination mesh, meters.
    pub fn mesh_width(self) -> f64 {
        match self {
            LpsClass::I => 5.0,
            LpsClass::II => 10.0,
            LpsClass::III => 15.0,
            LpsClass::IV => 20.0,
        }
    }

    /// Name in commands (`I` to `IV`).
    pub fn name(self) -> &'static str {
        match self {
            LpsClass::I => "I",
            LpsClass::II => "II",
            LpsClass::III => "III",
            LpsClass::IV => "IV",
        }
    }

    /// The class named `I` to `IV` (or `1` to `4`), in any case.
    pub fn parse(name: &str) -> Option<Self> {
        let wanted = name.trim().to_ascii_uppercase();
        LpsClass::ALL
            .into_iter()
            .enumerate()
            .find(|(i, c)| c.name() == wanted || (i + 1).to_string() == wanted)
            .map(|x| x.1)
    }
}

/// Horizontal reach of the protected volume of an air termination point at
/// `tip` above a reference plane, at `height` above that plane, for a
/// rolling sphere of `radius`: the protected volume of a single rod (the
/// sphere resting on the plane and touching the rod). `None` when the
/// point is above what the rod protects.
pub fn protected_reach(tip: f64, height: f64, radius: f64) -> Option<f64> {
    if !(tip.is_finite() && height.is_finite() && radius > 0.0) {
        return None;
    }
    let r = |h: f64| {
        let h = h.clamp(0.0, radius);
        (h * (2.0 * radius - h)).max(0.0).sqrt()
    };
    let top = tip.min(radius);
    let height = height.max(0.0);
    (height <= top).then(|| r(top) - r(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;

    #[test]
    fn roles_follow_the_run() {
        let roof = MepSegment::new(DVec2::ZERO, DVec2::new(5.0, 0.0), 6.0);
        assert_eq!(ConductorRole::of_run(&roof), ConductorRole::Air);
        let mut down = MepSegment::new(DVec2::ZERO, DVec2::ZERO, 6.0);
        down.end_offset = -0.5;
        assert_eq!(ConductorRole::of_run(&down), ConductorRole::Down);
        let ring = MepSegment::new(DVec2::ZERO, DVec2::new(5.0, 0.0), -0.5);
        assert_eq!(ConductorRole::of_run(&ring), ConductorRole::Earth);
    }

    #[test]
    fn a_rod_protects_a_circle_on_its_plane() {
        // A 2 m rod and a 45 m sphere: sqrt(2 * (90 - 2)) on the plane it stands on.
        let reach = protected_reach(2.0, 0.0, 45.0).unwrap();
        assert!((reach - (2.0f64 * 88.0).sqrt()).abs() < 1e-12);
        // Less at mid-height, nothing above the tip.
        assert!(protected_reach(2.0, 1.0, 45.0).unwrap() < reach);
        assert!(protected_reach(2.0, 2.5, 45.0).is_none());
        // Rods taller than the sphere protect a circle of its radius.
        assert!((protected_reach(80.0, 0.0, 45.0).unwrap() - 45.0).abs() < 1e-12);
        assert_eq!(LpsClass::parse("iii"), Some(LpsClass::III));
        assert_eq!(LpsClass::parse("1"), Some(LpsClass::I));
        assert_eq!(LpsClass::parse("V"), None);
    }
}
