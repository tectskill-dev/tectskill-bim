//! Building services (MEP): plumbing, electrical, structured cabling and
//! lightning protection systems, the pipe, conduit and cable tray segments
//! that run them, the valves on pipes, and the data that makes a component
//! a plumbing fixture or an electrical, communications or lightning
//! protection device.
//!
//! A system (a water supply, a drain, an electrical circuit, the cabling of
//! a rack, a lightning protection system) is a project setting; pipes,
//! conduits, conductors and components refer to it by [`SystemId`].
//! Defaults follow common Brazilian practice (fixture units and supply
//! weights as in the public tables of the drainage and water supply
//! standards; outlet mounting heights as usual in electrical plans).

use crate::{ElementId, SystemId};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// What a system carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SystemKind {
    #[default]
    ColdWater,
    HotWater,
    /// Sanitary drainage (soil and waste).
    Sanitary,
    /// Drainage vents.
    Vent,
    Rainwater,
    /// An electrical circuit feeding outlets and equipment.
    Power,
    /// An electrical circuit feeding lights.
    Lighting,
    /// Structured cabling: the data and voice outlets cabled to a rack.
    Data,
    /// A lightning protection system: air terminals, conductors and earth electrodes.
    Lightning,
}

/// The families of systems: which elements each takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemClass {
    /// Pipes and plumbing fixtures.
    Plumbing,
    /// Electrical circuits: conduits, cable trays and electrical devices.
    Circuit,
    /// Structured cabling: conduits, cable trays, data outlets and racks.
    Data,
    /// Lightning protection: conductors, air terminals and earth electrodes.
    Lightning,
}

impl SystemClass {
    pub const ALL: [SystemClass; 4] =
        [SystemClass::Plumbing, SystemClass::Circuit, SystemClass::Data, SystemClass::Lightning];
}

impl SystemKind {
    pub const ALL: [SystemKind; 9] = [
        SystemKind::ColdWater,
        SystemKind::HotWater,
        SystemKind::Sanitary,
        SystemKind::Vent,
        SystemKind::Rainwater,
        SystemKind::Power,
        SystemKind::Lighting,
        SystemKind::Data,
        SystemKind::Lightning,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SystemKind::ColdWater => "system-cold-water",
            SystemKind::HotWater => "system-hot-water",
            SystemKind::Sanitary => "system-sanitary",
            SystemKind::Vent => "system-vent",
            SystemKind::Rainwater => "system-rainwater",
            SystemKind::Power => "system-power",
            SystemKind::Lighting => "system-lighting",
            SystemKind::Data => "system-data",
            SystemKind::Lightning => "system-lightning",
        }
    }

    /// Name in commands and files exchanged with other programs (`cold_water`…).
    pub fn name(self) -> &'static str {
        match self {
            SystemKind::ColdWater => "cold_water",
            SystemKind::HotWater => "hot_water",
            SystemKind::Sanitary => "sanitary",
            SystemKind::Vent => "vent",
            SystemKind::Rainwater => "rainwater",
            SystemKind::Power => "power",
            SystemKind::Lighting => "lighting",
            SystemKind::Data => "data",
            SystemKind::Lightning => "lightning",
        }
    }

    /// The family of the system.
    pub fn class(self) -> SystemClass {
        match self {
            SystemKind::ColdWater
            | SystemKind::HotWater
            | SystemKind::Sanitary
            | SystemKind::Vent
            | SystemKind::Rainwater => SystemClass::Plumbing,
            SystemKind::Power | SystemKind::Lighting => SystemClass::Circuit,
            SystemKind::Data => SystemClass::Data,
            SystemKind::Lightning => SystemClass::Lightning,
        }
    }

    /// An electrical circuit (carried by conduits, fed by a board).
    pub fn is_electrical(self) -> bool {
        self.class() == SystemClass::Circuit
    }

    /// Carried by conduits and cable trays: circuits and structured cabling.
    pub fn is_cabled(self) -> bool {
        matches!(self.class(), SystemClass::Circuit | SystemClass::Data)
    }

    /// Gravity drainage: its pipes slope down along the flow.
    pub fn is_drainage(self) -> bool {
        matches!(self, SystemKind::Sanitary | SystemKind::Rainwater)
    }

    /// Color of the system's pipes and conduits in plans and 3D.
    pub fn default_color(self) -> [u8; 3] {
        match self {
            SystemKind::ColdWater => [40, 110, 200],
            SystemKind::HotWater => [210, 50, 40],
            SystemKind::Sanitary => [130, 85, 45],
            SystemKind::Vent => [120, 160, 60],
            SystemKind::Rainwater => [20, 160, 170],
            SystemKind::Power => [220, 130, 20],
            SystemKind::Lighting => [190, 160, 0],
            SystemKind::Data => [30, 140, 90],
            SystemKind::Lightning => [140, 60, 170],
        }
    }
}

fn volts() -> f64 {
    127.0
}

/// A plumbing system or an electrical circuit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MepSystem {
    pub name: String,
    /// Short code shown in plan tags (e.g. a system abbreviation or a circuit number).
    #[serde(default)]
    pub code: String,
    pub kind: SystemKind,
    /// Display color; `None` uses the kind's color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    /// Circuits: the distribution board (a panel component) feeding it;
    /// structured cabling: the rack its outlets are cabled to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<ElementId>,
    /// Circuits: nominal voltage, volts.
    #[serde(default = "volts")]
    pub voltage: f64,
}

impl MepSystem {
    pub fn new(name: impl Into<String>, code: impl Into<String>, kind: SystemKind) -> Self {
        Self { name: name.into(), code: code.into(), kind, color: None, panel: None, voltage: volts() }
    }

    pub fn color(&self) -> [u8; 3] {
        self.color.unwrap_or_else(|| self.kind.default_color())
    }

    /// The code, or the name when it has none.
    pub fn label(&self) -> &str {
        if self.code.is_empty() { &self.name } else { &self.code }
    }
}

/// A straight pipe or conduit run between two points, each at its own
/// elevation above the level: horizontal, sloped, or vertical (a riser,
/// when both ends share the plan position).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MepSegment {
    pub start: DVec2,
    pub end: DVec2,
    /// Elevation of the center line above the level at the start.
    pub start_offset: f64,
    /// Elevation of the center line above the level at the end.
    pub end_offset: f64,
    /// Systems it belongs to: one for a pipe, the circuits a conduit carries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<SystemId>,
}

impl MepSegment {
    pub fn new(start: DVec2, end: DVec2, offset: f64) -> Self {
        Self { start, end, start_offset: offset, end_offset: offset, systems: Vec::new() }
    }

    /// Length in plan.
    pub fn plan_length(&self) -> f64 {
        self.start.distance(self.end)
    }

    /// Rise from the start to the end.
    pub fn rise(&self) -> f64 {
        self.end_offset - self.start_offset
    }

    /// True length along the run.
    pub fn length(&self) -> f64 {
        self.plan_length().hypot(self.rise())
    }

    /// A vertical run (less than 1 mm long in plan).
    pub fn is_vertical(&self) -> bool {
        self.plan_length() < 1e-3
    }

    /// Fall per unit of plan length (positive when the end is lower); 0 for risers.
    pub fn slope(&self) -> f64 {
        let run = self.plan_length();
        if run < 1e-3 { 0.0 } else { -self.rise() / run }
    }

    /// The end points in the world, on a level at `level_z`.
    pub fn points(&self, level_z: f64) -> (DVec3, DVec3) {
        (self.start.extend(level_z + self.start_offset), self.end.extend(level_z + self.end_offset))
    }

    /// The first system, for pipes.
    pub fn system(&self) -> Option<SystemId> {
        self.systems.first().copied()
    }
}

/// How a cable tray is built; it picks the 3D shape and the plan drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TrayStyle {
    /// Two side rails joined by rungs.
    #[default]
    Ladder,
    /// A channel with a solid bottom.
    SolidBottom,
    /// A channel with a perforated bottom.
    Perforated,
}

impl TrayStyle {
    pub const ALL: [TrayStyle; 3] = [TrayStyle::Ladder, TrayStyle::SolidBottom, TrayStyle::Perforated];

    pub fn key(self) -> &'static str {
        match self {
            TrayStyle::Ladder => "tray-ladder",
            TrayStyle::SolidBottom => "tray-solid-bottom",
            TrayStyle::Perforated => "tray-perforated",
        }
    }

    /// Name in commands (`ladder`, `solid_bottom`, `perforated`).
    pub fn name(self) -> &'static str {
        match self {
            TrayStyle::Ladder => "ladder",
            TrayStyle::SolidBottom => "solid_bottom",
            TrayStyle::Perforated => "perforated",
        }
    }
}

/// Distance between the rungs of a ladder tray, meters.
pub const TRAY_RUNG_SPACING: f64 = 0.3;
/// Widest and tallest cable tray section, meters (larger values in files are cut down).
pub const MAX_TRAY_WIDTH: f64 = 2.0;
pub const MAX_TRAY_HEIGHT: f64 = 1.0;

/// What a valve does; it picks the plan symbol and the IFC predefined type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ValveKind {
    /// Opens or shuts the flow with a sliding gate.
    #[default]
    Gate,
    /// Opens or shuts the flow with a quarter turn.
    Ball,
    /// Lets the flow pass one way only.
    Check,
    /// Regulates the flow.
    Globe,
}

impl ValveKind {
    pub const ALL: [ValveKind; 4] = [ValveKind::Gate, ValveKind::Ball, ValveKind::Check, ValveKind::Globe];

    pub fn key(self) -> &'static str {
        match self {
            ValveKind::Gate => "valve-gate",
            ValveKind::Ball => "valve-ball",
            ValveKind::Check => "valve-check",
            ValveKind::Globe => "valve-globe",
        }
    }

    /// Name in commands (`gate`, `ball`, `check`, `globe`).
    pub fn name(self) -> &'static str {
        match self {
            ValveKind::Gate => "gate",
            ValveKind::Ball => "ball",
            ValveKind::Check => "check",
            ValveKind::Globe => "globe",
        }
    }

    /// Whether it has a stem and handwheel (check valves work by themselves).
    pub fn has_handle(self) -> bool {
        !matches!(self, ValveKind::Check)
    }
}

/// A valve on a pipe: an inline accessory `at` meters along its host's run
/// (true length from the start), of the pipe's diameter. It moves, is
/// copied and is deleted with its pipe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Valve {
    pub host: ElementId,
    /// Distance from the start of the host pipe along its run, meters
    /// (clamped to the run where it is read).
    pub at: f64,
}

impl Valve {
    /// Its position along `run` as a fraction (0 at the start, 1 at the end).
    pub fn fraction(&self, run: &MepSegment) -> f64 {
        let length = run.length();
        if length < 1e-9 || !self.at.is_finite() { 0.0 } else { (self.at / length).clamp(0.0, 1.0) }
    }

    /// Its center in the world, on a host run whose level is at `level_z`.
    pub fn point(&self, run: &MepSegment, level_z: f64) -> DVec3 {
        let (a, b) = run.points(level_z);
        a.lerp(b, self.fraction(run))
    }
}

/// What an electrical device is; it picks the plan symbol and the IFC class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DeviceKind {
    /// General-purpose outlet.
    #[default]
    Outlet,
    /// Outlet dedicated to one appliance.
    DedicatedOutlet,
    Switch,
    /// Light point on the ceiling.
    CeilingLight,
    /// Light point on a wall.
    WallLight,
    /// Distribution board.
    Panel,
    /// Data or voice outlet of the structured cabling (one or more jacks).
    DataOutlet,
    /// Communications rack (with its patch panels) the data outlets are cabled to.
    Rack,
    /// Air terminal (a lightning rod) of a lightning protection system.
    AirTerminal,
    /// Earth electrode (a driven rod) of a lightning protection system.
    EarthElectrode,
}

impl DeviceKind {
    pub const ALL: [DeviceKind; 10] = [
        DeviceKind::Outlet,
        DeviceKind::DedicatedOutlet,
        DeviceKind::Switch,
        DeviceKind::CeilingLight,
        DeviceKind::WallLight,
        DeviceKind::Panel,
        DeviceKind::DataOutlet,
        DeviceKind::Rack,
        DeviceKind::AirTerminal,
        DeviceKind::EarthElectrode,
    ];

    pub fn key(self) -> &'static str {
        match self {
            DeviceKind::Outlet => "device-outlet",
            DeviceKind::DedicatedOutlet => "device-dedicated-outlet",
            DeviceKind::Switch => "device-switch",
            DeviceKind::CeilingLight => "device-ceiling-light",
            DeviceKind::WallLight => "device-wall-light",
            DeviceKind::Panel => "device-panel",
            DeviceKind::DataOutlet => "device-data-outlet",
            DeviceKind::Rack => "device-rack",
            DeviceKind::AirTerminal => "device-air-terminal",
            DeviceKind::EarthElectrode => "device-earth-electrode",
        }
    }

    /// Name in commands (`outlet`, `data_outlet`, `air_terminal`…).
    pub fn name(self) -> &'static str {
        match self {
            DeviceKind::Outlet => "outlet",
            DeviceKind::DedicatedOutlet => "dedicated_outlet",
            DeviceKind::Switch => "switch",
            DeviceKind::CeilingLight => "ceiling_light",
            DeviceKind::WallLight => "wall_light",
            DeviceKind::Panel => "panel",
            DeviceKind::DataOutlet => "data_outlet",
            DeviceKind::Rack => "rack",
            DeviceKind::AirTerminal => "air_terminal",
            DeviceKind::EarthElectrode => "earth_electrode",
        }
    }

    /// A light point (on a lighting circuit).
    pub fn is_light(self) -> bool {
        matches!(self, DeviceKind::CeilingLight | DeviceKind::WallLight)
    }

    /// The family of systems it belongs to.
    pub fn class(self) -> SystemClass {
        match self {
            DeviceKind::Outlet
            | DeviceKind::DedicatedOutlet
            | DeviceKind::Switch
            | DeviceKind::CeilingLight
            | DeviceKind::WallLight
            | DeviceKind::Panel => SystemClass::Circuit,
            DeviceKind::DataOutlet | DeviceKind::Rack => SystemClass::Data,
            DeviceKind::AirTerminal | DeviceKind::EarthElectrode => SystemClass::Lightning,
        }
    }

    /// What feeds a system of its family: a distribution board or a rack.
    pub fn is_board(self) -> bool {
        matches!(self, DeviceKind::Panel | DeviceKind::Rack)
    }

    /// The kind of system that serves it (`None` for boards, racks and switches).
    pub fn circuit_kind(self) -> Option<SystemKind> {
        match self {
            DeviceKind::Outlet | DeviceKind::DedicatedOutlet => Some(SystemKind::Power),
            DeviceKind::CeilingLight | DeviceKind::WallLight => Some(SystemKind::Lighting),
            DeviceKind::DataOutlet => Some(SystemKind::Data),
            DeviceKind::AirTerminal | DeviceKind::EarthElectrode => Some(SystemKind::Lightning),
            DeviceKind::Switch | DeviceKind::Panel | DeviceKind::Rack => None,
        }
    }
}

fn one() -> u32 {
    1
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

/// Most jacks on one data outlet (larger values in files are cut down).
pub const MAX_PORTS: u32 = 48;

/// Building-services data of a component definition.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MepData {
    /// A plumbing fixture: its drainage fixture units, its relative weight
    /// on the water supply and whether it takes hot water.
    Fixture { drain_units: f64, supply_weight: f64, hot_water: bool },
    /// An electrical device with its apparent power in volt-amperes; a data
    /// outlet with its number of jacks (`ports`), each cabled to its rack.
    Device {
        device: DeviceKind,
        power: f64,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        ports: u32,
    },
}

impl MepData {
    /// An electrical device of `power` VA with one port.
    pub fn device_of(device: DeviceKind, power: f64) -> Self {
        MepData::Device { device, power, ports: 1 }
    }

    pub fn device(&self) -> Option<DeviceKind> {
        match *self {
            MepData::Device { device, .. } => Some(device),
            MepData::Fixture { .. } => None,
        }
    }

    /// Apparent power of a device, VA (0 for fixtures).
    pub fn power(&self) -> f64 {
        match *self {
            MepData::Device { power, .. } => power,
            MepData::Fixture { .. } => 0.0,
        }
    }

    /// Cables a data outlet takes back to its rack: one per jack (0 for
    /// anything else).
    pub fn cables(&self) -> u32 {
        match *self {
            MepData::Device { device: DeviceKind::DataOutlet, ports, .. } => ports,
            _ => 0,
        }
    }
}

/// Allowance added to each horizontal cable for the slack left at the rack
/// and at the outlet, meters.
pub const CABLE_SLACK: f64 = 3.0;
/// Longest horizontal cable (permanent link) of structured cabling, meters,
/// as in the usual cabling standards.
pub const MAX_LINK_LENGTH: f64 = 90.0;

/// The estimated route of a horizontal cable from an outlet at `outlet` to
/// a rack at `rack` (world points): up from the outlet to `ceiling` (the
/// world height of the outlet's ceiling, where the pathways run), across in
/// plan along the two axes, and down (or up) to the rack, plus
/// [`CABLE_SLACK`].
pub fn cable_length(outlet: DVec3, rack: DVec3, ceiling: f64) -> f64 {
    let across = (outlet.x - rack.x).abs() + (outlet.y - rack.y).abs();
    let vertical = (ceiling - outlet.z).max(0.0) + (ceiling - rack.z).abs();
    let total = across + vertical + CABLE_SLACK;
    if total.is_finite() { total } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_measures_slope_and_length() {
        let mut s = MepSegment::new(DVec2::ZERO, DVec2::new(4.0, 0.0), -0.3);
        s.end_offset = -0.34;
        assert!((s.slope() - 0.01).abs() < 1e-12);
        assert!((s.length() - 4.0f64.hypot(0.04)).abs() < 1e-12);
        let riser = MepSegment { end: DVec2::ZERO, end_offset: 2.8, ..s };
        assert!(riser.is_vertical());
        assert_eq!(riser.slope(), 0.0);
        assert!((riser.length() - 3.1).abs() < 1e-12);
    }
}
