//! The networks formed by pipes, conduits and cable trays: where runs meet
//! (their ends within a centimeter, or an end on another run), the fittings
//! there, the valves on pipes (each a node on its pipe's run) and the
//! fixtures and devices attached to the networks' nodes.
//!
//! Pipes connect only to pipes of the same system; conduits connect to any
//! conduit (they meet in boxes), cable trays to any cable tray. Fittings
//! are derived like wall joins: an elbow where two runs turn, a coupling
//! (or a reducer, between diameters or tray widths) where they continue
//! straight, a tee where three branches meet and a cross from four.

use crate::{SegmentGeom, ValveGeom};
use cc_core::{Category, ElementId, ElementKind, LevelId, MepData, Project, SystemId};
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, HashMap};

/// Ends closer than this meet (1 cm).
pub const TOL: f64 = 0.01;
/// A fixture or device is attached to a node within this distance in plan.
pub const ATTACH: f64 = 0.05;
/// Beyond this many node-run pairs, ends on the middle of runs are not looked for.
const MAX_PAIRS: usize = 4_000_000;
/// Two runs continue straight when they deviate less than this (degrees).
const STRAIGHT_DEG: f64 = 5.0;

/// What a fitting is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FittingKind {
    /// Two runs turning by `angle` degrees.
    Elbow {
        angle: f64,
    },
    /// Two runs of one diameter continuing straight.
    Coupling,
    /// Two runs of different diameters continuing straight.
    Reducer,
    Tee,
    Cross,
}

impl FittingKind {
    pub fn key(&self) -> &'static str {
        match self {
            FittingKind::Elbow { .. } => "fitting-elbow",
            FittingKind::Coupling => "fitting-coupling",
            FittingKind::Reducer => "fitting-reducer",
            FittingKind::Tee => "fitting-tee",
            FittingKind::Cross => "fitting-cross",
        }
    }

    /// The angle of an elbow rounded to the usual 45° or 90° (other angles to the degree).
    pub fn nominal_angle(&self) -> Option<f64> {
        match *self {
            FittingKind::Elbow { angle } => Some(match angle {
                a if (a - 45.0).abs() < 7.5 => 45.0,
                a if (a - 90.0).abs() < 7.5 => 90.0,
                a => a.round(),
            }),
            _ => None,
        }
    }
}

/// A fitting at a node.
#[derive(Debug, Clone, PartialEq)]
pub struct Fitting {
    pub kind: FittingKind,
    pub category: Category,
    pub node: usize,
    pub point: DVec3,
    pub level: LevelId,
    pub system: Option<SystemId>,
    /// Runs meeting there (one per end; a run passing through counts once).
    pub runs: Vec<ElementId>,
    /// Unit direction and diameter of each branch leaving the node.
    pub branches: Vec<(DVec3, f64)>,
}

impl Fitting {
    /// Diameters of the branches, largest first, without repeats.
    pub fn diameters(&self) -> Vec<f64> {
        let mut d: Vec<f64> = self.branches.iter().map(|b| b.1).collect();
        d.sort_by(|a, b| b.total_cmp(a));
        d.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
        d
    }

    pub fn diameter(&self) -> f64 {
        self.branches.iter().map(|b| b.1).fold(0.0, f64::max)
    }
}

/// A point where runs end or meet.
#[derive(Debug, Clone, PartialEq)]
pub struct NetNode {
    pub point: DVec3,
    pub category: Category,
    pub system: Option<SystemId>,
    pub level: LevelId,
}

/// A piece of a run between two consecutive nodes along it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetEdge {
    pub run: ElementId,
    pub a: usize,
    pub b: usize,
    pub length: f64,
}

/// The pipe, conduit and cable tray networks of a project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MepNetwork {
    pub nodes: Vec<NetNode>,
    pub edges: Vec<NetEdge>,
    pub fittings: Vec<Fitting>,
    /// Valves, each on the node of its pipe where it sits.
    pub valves: Vec<(ElementId, usize)>,
    /// Fixtures (on pipe nodes), devices (on conduit nodes) and air terminals
    /// and earth electrodes (on lightning conductor nodes) attached to the networks.
    pub attached: Vec<(ElementId, usize)>,
}

/// The runs that serve a family of systems: pipes, conduits or lightning conductors.
fn category_of(class: cc_core::SystemClass) -> Category {
    match class {
        cc_core::SystemClass::Plumbing => Category::Pipe,
        cc_core::SystemClass::Circuit | cc_core::SystemClass::Data => Category::Conduit,
        cc_core::SystemClass::Lightning => Category::LightningConductor,
    }
}

/// Which runs may connect: pipes of one system, all conduits, all cable
/// trays, all lightning conductors.
fn group(e: &cc_core::Element) -> Option<(Category, Option<SystemId>)> {
    match &e.kind {
        ElementKind::Pipe(s) => Some((Category::Pipe, s.system())),
        ElementKind::Conduit(_) => Some((Category::Conduit, None)),
        ElementKind::CableTray(_) => Some((Category::CableTray, None)),
        ElementKind::LightningConductor(_) => Some((Category::LightningConductor, None)),
        _ => None,
    }
}

/// A grid cell of one group's nodes.
type GridKey = (Category, Option<SystemId>, (i64, i64, i64));

fn cell(p: DVec3) -> (i64, i64, i64) {
    let q = |v: f64| (v / TOL).floor() as i64;
    (q(p.x), q(p.y), q(p.z))
}

impl MepNetwork {
    pub fn build(
        project: &Project,
        segments: &BTreeMap<ElementId, SegmentGeom>,
        valves: &BTreeMap<ElementId, ValveGeom>,
    ) -> Self {
        let mut net = MepNetwork::default();
        // Nodes per group, found through a grid of TOL cells.
        let mut grid: HashMap<GridKey, Vec<usize>> = HashMap::new();
        // Each run's nodes with their parameter along it.
        let mut along: BTreeMap<ElementId, Vec<(f64, usize)>> = BTreeMap::new();
        let mut runs = Vec::new();
        // Nodes placed by a riser only: a horizontal run meeting them gives its level
        // (the floor where the fitting is drawn).
        let mut by_riser: Vec<bool> = Vec::new();
        for g in segments.values() {
            let Some(e) = project.element(g.element) else { continue };
            let Some((category, system)) = group(e) else { continue };
            runs.push((g, category, system, e.level));
            let vertical = e.as_segment().is_some_and(cc_core::MepSegment::is_vertical);
            for (t, p) in [(0.0, g.a), (1.0, g.b)] {
                let (cx, cy, cz) = cell(p);
                let mut found = None;
                'search: for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            let key = (category, system, (cx + dx, cy + dy, cz + dz));
                            if let Some(ids) = grid.get(&key)
                                && let Some(&n) = ids.iter().find(|&&n| net.nodes[n].point.distance(p) <= TOL)
                            {
                                found = Some(n);
                                break 'search;
                            }
                        }
                    }
                }
                let n = found.unwrap_or_else(|| {
                    net.nodes.push(NetNode { point: p, category, system, level: e.level });
                    by_riser.push(vertical);
                    let n = net.nodes.len() - 1;
                    grid.entry((category, system, (cx, cy, cz))).or_default().push(n);
                    n
                });
                if !vertical && let Some(flag) = by_riser.get_mut(n).filter(|f| **f) {
                    *flag = false;
                    net.nodes[n].level = e.level;
                }
                along.entry(g.element).or_default().push((t, n));
            }
        }
        // Ends on the middle of another run of the group.
        if net.nodes.len().saturating_mul(runs.len()) <= MAX_PAIRS {
            for (n, node) in net.nodes.iter().enumerate() {
                for (g, category, system, _) in &runs {
                    if (*category, *system) != (node.category, node.system) {
                        continue;
                    }
                    let list = along.entry(g.element).or_default();
                    if list.iter().any(|(_, m)| *m == n) {
                        continue;
                    }
                    let d = g.b - g.a;
                    let len2 = d.length_squared();
                    if len2 < 1e-12 {
                        continue;
                    }
                    let t = (node.point - g.a).dot(d) / len2;
                    let length = len2.sqrt();
                    if t * length > TOL && (1.0 - t) * length > TOL && (g.a + d * t).distance(node.point) <= TOL {
                        list.push((t, n));
                    }
                }
            }
        }
        // Valves: a node on their pipe's run where they sit (a node already there within TOL).
        let mut run_of: std::collections::HashMap<ElementId, usize> = std::collections::HashMap::new();
        for (i, r) in runs.iter().enumerate() {
            run_of.entry(r.0.element).or_insert(i);
        }
        for v in valves.values() {
            let Some((g, category, system, level)) = run_of.get(&v.host).and_then(|i| runs.get(*i)) else { continue };
            let d = g.b - g.a;
            let len2 = d.length_squared();
            if len2 < 1e-12 {
                continue;
            }
            let t = ((v.point - g.a).dot(d) / len2).clamp(0.0, 1.0);
            let list = along.entry(g.element).or_default();
            let n = match list.iter().find(|(_, m)| net.nodes.get(*m).is_some_and(|x| x.point.distance(v.point) <= TOL))
            {
                Some(&(_, m)) => m,
                None => {
                    net.nodes.push(NetNode { point: v.point, category: *category, system: *system, level: *level });
                    let n = net.nodes.len() - 1;
                    list.push((t, n));
                    n
                }
            };
            net.valves.push((v.element, n));
        }
        // Edges along each run, between consecutive nodes.
        let mut branches: Vec<Vec<(ElementId, DVec3, f64)>> = vec![Vec::new(); net.nodes.len()];
        for (g, ..) in &runs {
            let Some(list) = along.get_mut(&g.element) else { continue };
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            list.dedup_by_key(|x| x.1);
            let Some(dir) = (g.b - g.a).try_normalize() else { continue };
            for w in list.windows(2) {
                let ((ta, a), (tb, b)) = (w[0], w[1]);
                net.edges.push(NetEdge { run: g.element, a, b, length: (tb - ta) * g.length() });
                branches[a].push((g.element, dir, g.diameter));
                branches[b].push((g.element, -dir, g.diameter));
            }
        }
        for (n, b) in branches.into_iter().enumerate() {
            let node = &net.nodes[n];
            let kind = match b.len() {
                0 | 1 => continue,
                2 => {
                    let deviation = 180.0 - b[0].1.angle_between(b[1].1).to_degrees();
                    if deviation >= STRAIGHT_DEG {
                        FittingKind::Elbow { angle: deviation }
                    } else if (b[0].2 - b[1].2).abs() > 1e-4 {
                        FittingKind::Reducer
                    } else if b[0].0 == b[1].0 {
                        // A node on the middle of a run with nothing else there.
                        continue;
                    } else {
                        FittingKind::Coupling
                    }
                }
                3 => FittingKind::Tee,
                _ => FittingKind::Cross,
            };
            let mut runs: Vec<ElementId> = b.iter().map(|x| x.0).collect();
            runs.dedup();
            net.fittings.push(Fitting {
                kind,
                category: node.category,
                node: n,
                point: node.point,
                level: node.level,
                system: node.system,
                runs,
                branches: b.iter().map(|x| (x.1, x.2)).collect(),
            });
        }
        // Fixtures on pipe nodes, devices on conduit nodes: each connector on
        // the nearest node of a system of its kind (in plan, then in height);
        // components without connectors by their insertion point, the
        // nearest in plan, then in height.
        let kind_of = |n: &NetNode| n.system.and_then(|s| project.system(s)).map(|s| s.kind);
        for e in project.elements.values() {
            let ElementKind::Component(c) = &e.kind else { continue };
            let connectors = crate::component_connectors(project, e);
            if !connectors.is_empty() {
                for k in connectors {
                    let category = category_of(k.system.class());
                    let near = net
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| n.category == category && kind_of(n).is_none_or(|kind| kind == k.system))
                        .filter(|(_, n)| DVec2::new(n.point.x, n.point.y).distance(k.point.truncate()) <= ATTACH)
                        .min_by(|a, b| (a.1.point.z - k.point.z).abs().total_cmp(&(b.1.point.z - k.point.z).abs()));
                    if let Some((n, _)) = near
                        && !net.attached.contains(&(e.id, n))
                    {
                        net.attached.push((e.id, n));
                    }
                }
                continue;
            }
            let category = match project.mep_data(e) {
                Some(MepData::Fixture { .. }) => Category::Pipe,
                Some(MepData::Device { device, .. }) => category_of(device.class()),
                None => continue,
            };
            let z = project.level_elevation(e.level) + c.base_offset;
            let near = |n: &NetNode| {
                (n.category == category && DVec2::new(n.point.x, n.point.y).distance(c.position) <= ATTACH)
                    .then(|| (n.point.z - z).abs())
            };
            // A fixture joins each of its systems' networks (drain, supply…) once.
            let mut best: BTreeMap<Option<SystemId>, (f64, usize)> = BTreeMap::new();
            for (n, node) in net.nodes.iter().enumerate() {
                if let Some(dz) = near(node) {
                    let entry = best.entry(node.system).or_insert((dz, n));
                    if dz < entry.0 {
                        *entry = (dz, n);
                    }
                }
            }
            if category != Category::Pipe {
                if let Some((_, n)) = best.values().min_by(|a, b| a.0.total_cmp(&b.0)) {
                    net.attached.push((e.id, *n));
                }
            } else {
                net.attached.extend(best.values().map(|(_, n)| (e.id, *n)));
            }
        }
        net
    }

    /// Edges at each node: (edge index, the node at its other end).
    pub fn adjacency(&self) -> Vec<Vec<(usize, usize)>> {
        let mut adj = vec![Vec::new(); self.nodes.len()];
        for (i, e) in self.edges.iter().enumerate() {
            if let Some(a) = adj.get_mut(e.a) {
                a.push((i, e.b));
            }
            if let Some(b) = adj.get_mut(e.b) {
                b.push((i, e.a));
            }
        }
        adj
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Document, Element, MepSegment};

    fn add(doc: &mut Document, kind: ElementKind, category: Category) -> ElementId {
        add_on(doc, 0, kind, category)
    }

    fn add_on(doc: &mut Document, floor: usize, kind: ElementKind, category: Category) -> ElementId {
        doc.edit("t", |e| {
            let level = e.project().levels[floor].id;
            let type_id = e.project().default_type(category).unwrap();
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind,
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: Vec::new(),
            });
            id
        })
    }

    fn run(a: [f64; 2], b: [f64; 2], z: f64) -> MepSegment {
        MepSegment::new(DVec2::from_array(a), DVec2::from_array(b), z)
    }

    #[test]
    fn elbows_tees_and_couplings_where_runs_meet() {
        let mut doc = Document::new(Project::from_template());
        // An L, a branch ending on the middle of the long leg, and a straight continuation.
        add(&mut doc, ElementKind::Pipe(run([0.0, 0.0], [4.0, 0.0], 0.5)), Category::Pipe);
        add(&mut doc, ElementKind::Pipe(run([4.0, 0.0], [4.0, 3.0], 0.5)), Category::Pipe);
        add(&mut doc, ElementKind::Pipe(run([2.0, 0.0], [2.0, -2.0], 0.5)), Category::Pipe);
        add(&mut doc, ElementKind::Pipe(run([4.0, 3.0], [4.0, 5.0], 0.5)), Category::Pipe);
        // A conduit apart: no fitting with the pipes.
        add(&mut doc, ElementKind::Conduit(run([0.0, 0.0], [0.0, 2.0], 0.5)), Category::Conduit);
        let geom = crate::ModelGeometry::build(&doc.project);
        let kinds: Vec<FittingKind> = geom.mep.fittings.iter().map(|f| f.kind).collect();
        assert_eq!(kinds.len(), 3, "{kinds:?}");
        assert!(kinds.iter().any(|k| matches!(k, FittingKind::Elbow { angle } if (angle - 90.0).abs() < 1e-6)));
        assert!(kinds.contains(&FittingKind::Tee));
        assert!(kinds.contains(&FittingKind::Coupling));
        // The long leg is split by the tee: 5 edges in all, plus the conduit.
        assert_eq!(geom.mep.edges.len(), 6);
    }

    #[test]
    fn pipes_of_different_systems_do_not_connect() {
        let mut doc = Document::new(Project::from_template());
        let systems: Vec<SystemId> = doc.project.systems.keys().copied().collect();
        let mut a = run([0.0, 0.0], [2.0, 0.0], 0.5);
        a.systems = vec![systems[0]];
        let mut b = run([2.0, 0.0], [2.0, 2.0], 0.5);
        b.systems = vec![systems[1]];
        add(&mut doc, ElementKind::Pipe(a), Category::Pipe);
        add(&mut doc, ElementKind::Pipe(b), Category::Pipe);
        let geom = crate::ModelGeometry::build(&doc.project);
        assert!(geom.mep.fittings.is_empty());
    }

    #[test]
    fn risers_connect_runs_of_two_floors() {
        let mut doc = Document::new(Project::from_template());
        let (ground, first) = (doc.project.levels[0].id, doc.project.levels[1].id);
        // A riser drawn on the ground floor from 0.5 m up to 0.5 m above the first floor (3 m).
        let mut riser = run([2.0, 0.0], [2.0, 0.0], 0.5);
        riser.end_offset = 3.5;
        add(&mut doc, ElementKind::Pipe(run([0.0, 0.0], [2.0, 0.0], 0.5)), Category::Pipe);
        add(&mut doc, ElementKind::Pipe(riser), Category::Pipe);
        add_on(&mut doc, 1, ElementKind::Pipe(run([2.0, 0.0], [2.0, 3.0], 0.5)), Category::Pipe);
        // A branch upstairs ending on the middle of the riser, from a wall at 1.5 m below the first floor.
        add_on(&mut doc, 1, ElementKind::Pipe(run([4.0, 0.0], [2.0, 0.0], -1.5)), Category::Pipe);
        let geom = crate::ModelGeometry::build(&doc.project);
        let mut found: Vec<(LevelId, FittingKind)> = geom.mep.fittings.iter().map(|f| (f.level, f.kind)).collect();
        found.sort_by_key(|(l, k)| (l.0, k.key()));
        // Elbows at both ends of the riser, each on the floor of its horizontal run, and a tee on it.
        let elbow = |l| (l, FittingKind::Elbow { angle: 90.0 });
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.contains(&elbow(ground)) && found.contains(&elbow(first)), "{found:?}");
        assert!(found.contains(&(first, FittingKind::Tee)), "{found:?}");
    }

    fn add_type(doc: &mut Document, data: cc_core::TypeData) {
        doc.edit("t", |e| {
            let id = cc_core::ElementTypeId(e.alloc_id());
            e.add_type(cc_core::ElementType {
                id,
                name: "t".into(),
                data,
                info: Default::default(),
                uid: Default::default(),
            })
        });
    }

    #[test]
    fn cable_trays_join_trays_only() {
        let mut doc = Document::new(Project::from_template());
        let tray = cc_core::TypeData::CableTray {
            width: 0.3,
            height: 0.1,
            style: cc_core::TrayStyle::SolidBottom,
            material: String::new(),
        };
        add_type(&mut doc, tray);
        add(&mut doc, ElementKind::CableTray(run([0.0, 0.0], [4.0, 0.0], 2.5)), Category::CableTray);
        add(&mut doc, ElementKind::CableTray(run([4.0, 0.0], [4.0, 3.0], 2.5)), Category::CableTray);
        // A conduit ending at the corner: it meets no tray.
        add(&mut doc, ElementKind::Conduit(run([4.0, 0.0], [6.0, 0.0], 2.5)), Category::Conduit);
        let geom = crate::ModelGeometry::build(&doc.project);
        assert_eq!(geom.mep.fittings.len(), 1);
        let f = &geom.mep.fittings[0];
        assert_eq!(f.category, Category::CableTray);
        assert!(matches!(f.kind, FittingKind::Elbow { angle } if (angle - 90.0).abs() < 1e-6));
        assert!((f.diameter() - 0.3).abs() < 1e-12);
    }

    #[test]
    fn a_valve_is_a_node_on_its_pipe() {
        let mut doc = Document::new(Project::from_template());
        add_type(&mut doc, cc_core::TypeData::Valve { kind: cc_core::ValveKind::Ball, material: String::new() });
        let pipe = add(&mut doc, ElementKind::Pipe(run([0.0, 0.0], [4.0, 0.0], 0.5)), Category::Pipe);
        let valve = add(&mut doc, ElementKind::Valve(cc_core::Valve { host: pipe, at: 1.0 }), Category::Valve);
        let geom = crate::ModelGeometry::build(&doc.project);
        assert!(geom.mep.fittings.is_empty());
        assert_eq!(geom.mep.edges.len(), 2);
        let &(id, node) = geom.mep.valves.first().unwrap();
        assert_eq!(id, valve);
        assert!(geom.mep.nodes[node].point.distance(DVec3::new(1.0, 0.0, 0.5)) < 1e-9);
        let lengths: Vec<f64> = geom.mep.edges.iter().map(|e| e.length).collect();
        assert!((lengths[0] - 1.0).abs() < 1e-9 && (lengths[1] - 3.0).abs() < 1e-9, "{lengths:?}");
        assert_eq!(geom.solids_of(valve).count(), 1);
        // Deleting the pipe deletes its valve.
        doc.edit("t", |e| e.remove_element(pipe));
        assert!(doc.project.element(valve).is_none());
    }
}
