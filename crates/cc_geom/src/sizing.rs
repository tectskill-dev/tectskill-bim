//! Sizing of building services from their networks, with simplified tables
//! of common Brazilian practice (from public sources):
//!
//! - **Sanitary drainage**: each run carries the fixture units of the
//!   fixtures upstream (towards the network's highest ends; the outlet is
//!   its lowest node); the diameter follows the usual table of branches
//!   (40 mm up to 3 units, 50 up to 6, 75 up to 20, 100 up to 160, 150 up
//!   to 620), at least 100 mm downstream of a toilet.
//! - **Cold and hot water**: the flow of a run is 0.3 √ΣP l/s from the
//!   relative weights of the fixtures it feeds (the source is the network's
//!   highest node); the diameter is the smallest PVC pipe keeping the speed
//!   at most 3 m/s.
//! - **Circuits**: the design current is the load over the voltage; the
//!   breaker is the next standard rating; the copper conductor is the
//!   smallest whose capacity (two loaded conductors in conduit, PVC 70 °C)
//!   covers the breaker, at least 1.5 mm² for lighting and 2.5 mm² for
//!   power, and larger until the voltage drop to the farthest device
//!   (along the conduits, else straight) is at most 4 %.

use crate::ModelGeometry;
use crate::mep_net::MepNetwork;
use cc_core::{Category, DeviceKind, ElementId, MepData, Project, SystemId, SystemKind};
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

/// Maximum speed in water pipes, m/s.
pub const MAX_SPEED: f64 = 3.0;
/// Maximum voltage drop of a circuit, percent.
pub const MAX_DROP: f64 = 4.0;
/// Resistivity of copper used for voltage drops, Ω·mm²/m.
const COPPER: f64 = 1.0 / 56.0;

/// PVC water pipes: outside and inside diameters, mm.
const WATER_PIPES: [(f64, f64); 9] = [
    (20.0, 17.0),
    (25.0, 21.6),
    (32.0, 27.8),
    (40.0, 35.2),
    (50.0, 44.0),
    (60.0, 53.4),
    (75.0, 66.6),
    (85.0, 75.6),
    (110.0, 97.8),
];

/// Drain diameters (mm) and the fixture units they take.
const DRAINS: [(f64, f64); 5] = [(40.0, 3.0), (50.0, 6.0), (75.0, 20.0), (100.0, 160.0), (150.0, 620.0)];

/// Standard breaker ratings, A.
const BREAKERS: [f64; 13] = [6.0, 10.0, 16.0, 20.0, 25.0, 32.0, 40.0, 50.0, 63.0, 70.0, 80.0, 100.0, 125.0];

/// Copper conductors (mm²) and their capacity in conduit, A.
const CONDUCTORS: [(f64, f64); 12] = [
    (1.5, 17.5),
    (2.5, 24.0),
    (4.0, 32.0),
    (6.0, 41.0),
    (10.0, 57.0),
    (16.0, 76.0),
    (25.0, 101.0),
    (35.0, 125.0),
    (50.0, 151.0),
    (70.0, 192.0),
    (95.0, 232.0),
    (120.0, 269.0),
];

/// The sizing of one pipe run.
#[derive(Debug, Clone, PartialEq)]
pub struct PipeSizing {
    pub run: ElementId,
    pub system: SystemId,
    pub kind: SystemKind,
    /// Fixture units (drains) or the sum of weights (water) it carries.
    pub load: f64,
    /// Water flow, l/s.
    pub flow: Option<f64>,
    /// Water speed in the current pipe, m/s.
    pub speed: Option<f64>,
    /// Current and required outside diameters, m.
    pub diameter: f64,
    pub required: f64,
}

impl PipeSizing {
    pub fn ok(&self) -> bool {
        self.diameter >= self.required - 1e-6
    }
}

/// The sizing of one circuit.
#[derive(Debug, Clone, PartialEq)]
pub struct CircuitSizing {
    pub system: SystemId,
    /// Installed load, VA.
    pub power: f64,
    pub voltage: f64,
    /// Design current, A.
    pub current: f64,
    /// Breaker rating, A.
    pub breaker: f64,
    /// Conductor section, mm².
    pub conductor: f64,
    /// Length to the farthest device, m (none without a board).
    pub length: Option<f64>,
    /// Voltage drop there, percent.
    pub drop: Option<f64>,
}

impl CircuitSizing {
    pub fn ok(&self) -> bool {
        self.drop.is_none_or(|d| d <= MAX_DROP + 1e-9)
    }
}

/// The required drain diameter (m) for `units` fixture units.
pub fn drain_diameter(units: f64, toilet: bool) -> f64 {
    let mm = DRAINS.iter().find(|(_, max)| units <= *max).map_or(200.0, |d| d.0);
    if toilet { mm.max(100.0) / 1000.0 } else { mm / 1000.0 }
}

/// Flow (l/s) from the sum of relative weights.
pub fn water_flow(weights: f64) -> f64 {
    0.3 * weights.max(0.0).sqrt()
}

/// Speed (m/s) of `flow` l/s in a pipe of `inside` mm.
fn speed(flow: f64, inside: f64) -> f64 {
    let area = std::f64::consts::PI * (inside / 1000.0).powi(2) / 4.0;
    flow / 1000.0 / area
}

/// The smallest PVC water pipe (outside diameter, m) for `flow` l/s.
pub fn water_diameter(flow: f64) -> f64 {
    WATER_PIPES.iter().find(|(_, di)| speed(flow, *di) <= MAX_SPEED).map_or(0.11, |p| p.0 / 1000.0)
}

/// The inside diameter (mm) of a PVC pipe of `outside` m (proportional beyond the table).
fn inside(outside: f64) -> f64 {
    let mm = outside * 1000.0;
    WATER_PIPES.iter().find(|(de, _)| (de - mm).abs() < 0.5).map_or(mm * 0.88, |p| p.1)
}

/// Loads of each edge of a network component: what lies beyond it from `root`.
/// Returns, per edge, (load, whether a toilet lies beyond).
fn subtree_loads(
    adj: &[Vec<(usize, usize)>],
    nodes: &std::collections::BTreeSet<usize>,
    root: usize,
    own: &BTreeMap<usize, (f64, bool)>,
) -> BTreeMap<usize, (f64, bool)> {
    let mut order = Vec::new();
    let mut parent: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(n) = queue.pop_front() {
        order.push(n);
        for &(edge, m) in adj.get(n).into_iter().flatten() {
            if nodes.contains(&m) && seen.insert(m) {
                parent.insert(m, (edge, n));
                queue.push_back(m);
            }
        }
    }
    let mut below: BTreeMap<usize, (f64, bool)> = BTreeMap::new();
    let mut loads = BTreeMap::new();
    for &n in order.iter().rev() {
        let (mut load, mut toilet) = own.get(&n).copied().unwrap_or((0.0, false));
        if let Some(&(l, t)) = below.get(&n) {
            load += l;
            toilet |= t;
        }
        if let Some(&(edge, up)) = parent.get(&n) {
            loads.insert(edge, (load, toilet));
            let entry = below.entry(up).or_insert((0.0, false));
            entry.0 += load;
            entry.1 |= toilet;
        }
    }
    loads
}

/// Connected groups of nodes of the network passing `keep`.
fn components(adj: &[Vec<(usize, usize)>], keep: impl Fn(usize) -> bool) -> Vec<Vec<usize>> {
    let mut seen = vec![false; adj.len()];
    let mut out = Vec::new();
    for start in 0..adj.len() {
        if seen[start] || !keep(start) {
            continue;
        }
        let mut group = Vec::new();
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(n) = stack.pop() {
            group.push(n);
            for &(_, m) in &adj[n] {
                if !seen[m] && keep(m) {
                    seen[m] = true;
                    stack.push(m);
                }
            }
        }
        out.push(group);
    }
    out
}

/// Sizes the drains and water pipes of every plumbing system.
pub fn size_pipes(project: &Project, geom: &ModelGeometry) -> Vec<PipeSizing> {
    let net = &geom.mep;
    let adj = net.adjacency();
    let mut out = Vec::new();
    for (sid, system) in &project.systems {
        let kind = system.kind;
        if !matches!(kind, SystemKind::Sanitary | SystemKind::ColdWater | SystemKind::HotWater) {
            continue;
        }
        // What the fixtures at each node bring.
        let mut own: BTreeMap<usize, (f64, bool)> = BTreeMap::new();
        for &(component, node) in &net.attached {
            if net.nodes.get(node).is_none_or(|n| n.system != Some(*sid)) {
                continue;
            }
            let Some(MepData::Fixture { drain_units, supply_weight, hot_water }) =
                project.element(component).and_then(|e| project.mep_data(e))
            else {
                continue;
            };
            let load = match kind {
                SystemKind::Sanitary => drain_units,
                SystemKind::HotWater if !hot_water => 0.0,
                _ => supply_weight,
            };
            let entry = own.entry(node).or_insert((0.0, false));
            entry.0 += load;
            entry.1 |= drain_units >= 6.0;
        }
        let groups = components(&adj, |n| net.nodes[n].category == Category::Pipe && net.nodes[n].system == Some(*sid));
        let mut run_load: BTreeMap<ElementId, (f64, bool)> = BTreeMap::new();
        for nodes in groups {
            // Drains flow to their lowest node; water comes from the highest.
            // Among nodes at the same height (to the millimeter), a free end
            // without fixtures is the outlet or the source.
            let rank = |n: &usize| {
                let z = (net.nodes[*n].point.z * 1000.0).round();
                let free = !own.contains_key(n) && adj[*n].len() == 1;
                (if kind == SystemKind::Sanitary { -z } else { z }, free)
            };
            let root = nodes.iter().copied().max_by(|a, b| {
                let (ra, rb) = (rank(a), rank(b));
                ra.0.total_cmp(&rb.0).then(ra.1.cmp(&rb.1))
            });
            let Some(root) = root else { continue };
            let nodes: std::collections::BTreeSet<usize> = nodes.into_iter().collect();
            for (edge, (load, toilet)) in subtree_loads(&adj, &nodes, root, &own) {
                let Some(e) = net.edges.get(edge) else { continue };
                let entry = run_load.entry(e.run).or_insert((0.0, false));
                if load > entry.0 {
                    entry.0 = load;
                }
                entry.1 |= toilet;
            }
        }
        for (run, (load, toilet)) in run_load {
            let Some(g) = geom.segments.get(&run) else { continue };
            let (flow, required, speed_now) = match kind {
                SystemKind::Sanitary => (None, drain_diameter(load, toilet), None),
                _ => {
                    let q = water_flow(load);
                    (Some(q), water_diameter(q), Some(speed(q, inside(g.diameter))))
                }
            };
            out.push(PipeSizing {
                run,
                system: *sid,
                kind,
                load,
                flow,
                speed: speed_now,
                diameter: g.diameter,
                required,
            });
        }
    }
    out
}

/// Shortest distances along the conduits from `from` to every node.
fn distances(net: &MepNetwork, adj: &[Vec<(usize, usize)>], from: usize) -> Vec<f64> {
    #[derive(PartialEq)]
    struct Item(f64, usize);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Item {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other.0.total_cmp(&self.0)
        }
    }
    let mut dist = vec![f64::INFINITY; adj.len()];
    let mut heap = BinaryHeap::new();
    if let Some(d) = dist.get_mut(from) {
        *d = 0.0;
        heap.push(Item(0.0, from));
    }
    while let Some(Item(d, n)) = heap.pop() {
        if d > dist[n] {
            continue;
        }
        for &(edge, m) in &adj[n] {
            let len = net.edges.get(edge).map_or(0.0, |e| e.length);
            if d + len < dist[m] {
                dist[m] = d + len;
                heap.push(Item(d + len, m));
            }
        }
    }
    dist
}

/// Sizes every electrical circuit.
pub fn size_circuits(project: &Project, geom: &ModelGeometry) -> Vec<CircuitSizing> {
    let net = &geom.mep;
    let adj = net.adjacency();
    let node_of = |id: ElementId| net.attached.iter().find(|(c, _)| *c == id).map(|(_, n)| *n);
    let position = |id: ElementId| {
        let e = project.element(id)?;
        let cc_core::ElementKind::Component(c) = &e.kind else { return None };
        Some(c.position.extend(project.level_elevation(e.level) + c.base_offset))
    };
    let mut out = Vec::new();
    for (sid, system) in project.systems.iter().filter(|(_, s)| s.kind.is_electrical()) {
        let devices: Vec<(ElementId, f64)> = project
            .system_members(*sid)
            .filter_map(|e| match project.mep_data(e) {
                Some(MepData::Device { device, power, .. }) if device != DeviceKind::Panel => Some((e.id, power)),
                _ => None,
            })
            .collect();
        let power: f64 = devices.iter().map(|d| d.1).sum();
        let voltage = if system.voltage > 0.0 { system.voltage } else { 127.0 };
        let current = power / voltage;
        let breaker = BREAKERS.iter().copied().find(|b| *b >= current).unwrap_or(125.0);
        let minimum = if system.kind == SystemKind::Lighting { 1.5 } else { 2.5 };
        let length = system.panel.map(|panel| {
            let along = node_of(panel).map(|n| distances(net, &adj, n));
            devices
                .iter()
                .map(|(d, _)| {
                    let by_conduit = along.as_ref().zip(node_of(*d)).map(|(dist, n)| dist[n]).filter(|l| l.is_finite());
                    by_conduit.unwrap_or_else(|| match (position(panel), position(*d)) {
                        (Some(a), Some(b)) => (a - b).abs().element_sum(),
                        _ => 0.0,
                    })
                })
                .fold(0.0, f64::max)
        });
        let drop_with = |s: f64| length.map(|l| 200.0 * COPPER * l * current / (s * voltage));
        let mut conductor =
            CONDUCTORS.iter().find(|(s, capacity)| *s >= minimum && *capacity >= breaker).map_or(120.0, |c| c.0);
        while drop_with(conductor).is_some_and(|d| d > MAX_DROP)
            && let Some(&(next, _)) = CONDUCTORS.iter().find(|(s, _)| *s > conductor)
        {
            conductor = next;
        }
        out.push(CircuitSizing {
            system: *sid,
            power,
            voltage,
            current,
            breaker,
            conductor,
            length,
            drop: drop_with(conductor),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        assert_eq!(drain_diameter(1.0, false), 0.04);
        assert_eq!(drain_diameter(6.0, true), 0.1);
        assert_eq!(drain_diameter(25.0, false), 0.1);
        // ΣP = 1: 0.3 l/s, 1.3 m/s in 17 mm.
        assert!((water_flow(1.0) - 0.3).abs() < 1e-12);
        assert_eq!(water_diameter(0.3), 0.02);
        // 1.2 l/s needs 27.8 mm inside (2.0 m/s); 21.6 mm would run 3.3 m/s.
        assert_eq!(water_diameter(1.2), 0.032);
    }
}
