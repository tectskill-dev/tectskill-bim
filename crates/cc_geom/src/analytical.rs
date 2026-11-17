//! The analytical model: columns and beams as center lines (members) between
//! shared points (nodes), for structural analysis programs. A beam framed
//! into a column ends on the column's axis, a beam ending on another beam
//! ends on that beam's line, and a member is split wherever another member
//! ends on it, so connected members share their nodes.

use crate::{BeamEnd, ModelGeometry};
use cc_core::{ElementId, ElementKind, Project, TypeData};
use glam::{DVec2, DVec3};

/// Points closer than this are one node (1 cm).
const TOL: f64 = 0.01;

/// How far a beam end may lie from another beam's axis, beyond its half width, and still end on it.
const REACH: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Column,
    Beam,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: usize,
    pub position: DVec3,
    /// The base of a column: where the structure rests on its supports.
    pub support: bool,
}

/// A straight member between two nodes; an element split by connections
/// gives several members.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub id: usize,
    pub element: ElementId,
    pub kind: MemberKind,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalyticalModel {
    pub nodes: Vec<Node>,
    pub members: Vec<Member>,
}

impl AnalyticalModel {
    /// The two end points of a member.
    pub fn line(&self, member: &Member) -> Option<(DVec3, DVec3)> {
        Some((self.nodes.get(member.start)?.position, self.nodes.get(member.end)?.position))
    }
}

/// A member line before nodes are shared.
struct Line {
    element: ElementId,
    kind: MemberKind,
    a: DVec3,
    b: DVec3,
    /// Half width in plan, for beam ends landing on it.
    half: f64,
}

/// Parameter of `p` projected on segment `a`-`b`, and its distance to it.
fn project_on(a: DVec3, b: DVec3, p: DVec3) -> (f64, f64) {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-18 {
        return (0.0, p.distance(a));
    }
    let t = ((p - a).dot(d) / len2).clamp(0.0, 1.0);
    (t, p.distance(a + d * t))
}

/// Parameter of `p` on the plan projection of `a`-`b` and its plan distance.
fn project_plan(a: DVec3, b: DVec3, p: DVec2) -> (f64, f64) {
    let (a2, b2) = (a.truncate(), b.truncate());
    let (q, t) = crate::polygon::closest_on_segment(a2, b2, p);
    (t, q.distance(p))
}

/// Build the analytical model of the structural columns and the beams.
pub fn analytical_model(project: &Project, geom: &ModelGeometry) -> AnalyticalModel {
    let mut lines: Vec<Line> = Vec::new();
    let mut column_axes: Vec<(ElementId, DVec2)> = Vec::new();
    for e in project.elements.values() {
        let ElementKind::Column(c) = &e.kind else { continue };
        if !c.structural || !matches!(project.element_type(e.type_id).map(|t| &t.data), Some(TypeData::Column { .. })) {
            continue;
        }
        let (base, top) = project.column_span(e, c);
        let (a, b) = (c.position.extend(base), c.position.extend(top));
        if a.is_finite() && b.is_finite() && top - base > TOL {
            lines.push(Line { element: e.id, kind: MemberKind::Column, a, b, half: 0.0 });
            column_axes.push((e.id, c.position));
        }
    }
    let first_beam = lines.len();
    for g in geom.beams.values() {
        // The centroid line, at mid depth.
        let at = |u: f64| {
            let (z0, z1) = g.z_at(u);
            g.world(u, 0.0).extend((z0 + z1) * 0.5)
        };
        let end_point = |end: BeamEnd, u: f64| match end {
            // Onto the column's axis, at the beam's height there.
            BeamEnd::Column(c) => match column_axes.iter().find(|(id, _)| *id == c) {
                Some((_, axis)) => {
                    let uc = g.locate(*axis);
                    let (z0, z1) = g.z_at(uc);
                    axis.extend((z0 + z1) * 0.5)
                }
                None => at(u),
            },
            _ => at(u),
        };
        let (a, b) = (end_point(g.start, 0.0), end_point(g.end, g.length));
        if a.is_finite() && b.is_finite() && a.distance(b) > TOL {
            lines.push(Line { element: g.element, kind: MemberKind::Beam, a, b, half: g.half });
        }
    }
    // Beam ends that stop on another beam end on its line.
    for i in first_beam..lines.len() {
        for end in [false, true] {
            let p = if end { lines[i].b } else { lines[i].a };
            // Beams beside it in plan, at a height where the depths meet (a beam
            // far above or below stays apart); the nearest in space wins, so
            // members in one vertical plane (a truss) find their own chord.
            let host = (first_beam..lines.len())
                .filter(|&j| j != i)
                .filter_map(|j| {
                    let (t, d) = project_plan(lines[j].a, lines[j].b, p.truncate());
                    let q = lines[j].a + (lines[j].b - lines[j].a) * t;
                    let near = d <= lines[j].half + REACH
                        && (q.z - p.z).abs() <= lines[i].half.max(lines[j].half) * 4.0 + REACH;
                    near.then_some((q, q.distance(p)))
                })
                .min_by(|x, y| x.1.total_cmp(&y.1));
            if let Some((q, _)) = host {
                if end {
                    lines[i].b = q;
                } else {
                    lines[i].a = q;
                }
            }
        }
    }

    // Shared nodes.
    let mut nodes: Vec<Node> = Vec::new();
    let mut node_at = |p: DVec3, support: bool| -> usize {
        if let Some(n) = nodes.iter_mut().find(|n| n.position.distance(p) < TOL) {
            n.support |= support;
            return n.id;
        }
        let id = nodes.len();
        nodes.push(Node { id, position: p, support });
        id
    };
    let ends: Vec<(usize, usize)> =
        lines.iter().map(|l| (node_at(l.a, l.kind == MemberKind::Column), node_at(l.b, false))).collect();

    // Split each line at the nodes lying on it.
    let mut members = Vec::new();
    for (l, &(s, e)) in lines.iter().zip(&ends) {
        let mut on: Vec<(f64, usize)> = nodes
            .iter()
            .filter(|n| n.id != s && n.id != e)
            .filter_map(|n| {
                let (t, d) = project_on(l.a, l.b, n.position);
                (d < TOL && t > 1e-9 && t < 1.0 - 1e-9).then_some((t, n.id))
            })
            .collect();
        on.sort_by(|x, y| x.0.total_cmp(&y.0));
        let chain: Vec<usize> = std::iter::once(s).chain(on.into_iter().map(|(_, n)| n)).chain([e]).collect();
        for w in chain.windows(2) {
            if w[0] != w[1] {
                members.push(Member { id: members.len(), element: l.element, kind: l.kind, start: w[0], end: w[1] });
            }
        }
    }
    AnalyticalModel { nodes, members }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Beam, Category, Column, Document, Element, WallTop};

    fn frame() -> (Project, Vec<ElementId>) {
        // Two columns 5 m apart and a beam between them, and a beam from the
        // middle of that beam going north.
        let mut doc = Document::new(Project::from_template());
        let mut ids = Vec::new();
        doc.edit("frame", |e| {
            let level = e.project().levels[0].id;
            let mut add = |e: &mut cc_core::EditCtx, category, kind| {
                let type_id = e.project().default_type(category).unwrap();
                let id = ElementId(e.alloc_id());
                let (mark, comments, uid) = (String::new(), String::new(), Default::default());
                e.add_element(Element {
                    id,
                    level,
                    type_id,
                    mark,
                    comments,
                    kind,
                    uid,
                    phase_created: Default::default(),
                    phase_demolished: None,
                    design_option: None,
                    paints: Vec::new(),
                });
                ids.push(id);
            };
            for x in [0.0, 5.0] {
                let column = Column {
                    position: DVec2::new(x, 0.0),
                    rotation_deg: 0.0,
                    base_offset: 0.0,
                    height: 3.0,
                    top: WallTop::Unconnected,
                    structural: true,
                };
                add(e, Category::Column, ElementKind::Column(column));
            }
            add(e, Category::Beam, ElementKind::Beam(Beam::new(DVec2::ZERO, DVec2::new(5.0, 0.0), 3.0)));
            add(e, Category::Beam, ElementKind::Beam(Beam::new(DVec2::new(2.5, 0.0), DVec2::new(2.5, 4.0), 3.0)));
        });
        (doc.project, ids)
    }

    #[test]
    fn members_share_nodes_at_their_connections() {
        let (project, ids) = frame();
        let geom = ModelGeometry::build(&project);
        let m = analytical_model(&project, &geom);
        let of = |id: ElementId| m.members.iter().filter(|x| x.element == id).collect::<Vec<_>>();
        // The 0.40 m beams' axes are at 2.80 m: the columns split there.
        assert_eq!(of(ids[0]).len(), 2);
        assert_eq!(of(ids[1]).len(), 2);
        // The main beam runs from column axis to column axis, split by the secondary beam.
        let main = of(ids[2]);
        assert_eq!(main.len(), 2);
        let (a, _) = m.line(main[0]).unwrap();
        let (_, b) = m.line(main[1]).unwrap();
        assert!(a.distance(DVec3::new(0.0, 0.0, 2.8)) < 1e-9 && b.distance(DVec3::new(5.0, 0.0, 2.8)) < 1e-9);
        // The secondary beam starts on the main beam's line.
        let (start, _) = m.line(of(ids[3])[0]).unwrap();
        assert!(start.distance(DVec3::new(2.5, 0.0, 2.8)) < 1e-9, "{start}");
        // Column bases (the supports) and tops, two beam-column joints, the tee and the free end.
        assert_eq!(m.nodes.len(), 8);
        assert_eq!(m.nodes.iter().filter(|n| n.support).count(), 2);
        assert!(m.nodes.iter().any(|n| n.position.distance(DVec3::new(0.0, 0.0, 3.0)) < 1e-9));
    }
}
