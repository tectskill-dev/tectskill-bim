//! Painted faces on the built solids: finding the face a paint
//! ([`cc_core::paint::FacePaint`]) refers to after a rebuild, and the
//! surface it covers.
//!
//! The rule: among the faces of the element's body (framing and other
//! secondary parts left out) whose normal is within
//! [`cc_core::paint::NORMAL_TOLERANCE_DEG`] of the paint's, the one that
//! passes nearest the paint's point; then, from it, the faces that touch it
//! in the same plane (a wall face split around its openings) and, on a curved
//! surface, the smooth facets that join it, face after face.

use crate::solid::SMOOTH_COS;
use crate::{Face, Look, ModelGeometry, Solid, polygon};
use cc_core::paint::{FacePaint, NORMAL_TOLERANCE_DEG};
use cc_core::{ElementId, Project};
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, BTreeSet};

/// A face of one of an element's solids: the solid's index in the list
/// searched, and the face's index in it.
pub type FaceRef = (usize, usize);

/// Most faces a painted surface grows to (and that are searched for one).
const MAX_SURFACE_FACES: usize = 20_000;
/// Most faces a painted surface may spread over.
const MAX_SPREAD: usize = 4096;
/// Faces whose planes are this close (meters) are one plane.
const PLANE_TOL: f64 = 1e-4;
/// Points this close (meters) touch.
const TOUCH_TOL: f64 = 1e-6;

/// Distance from `p` to the polygon of `face`.
pub fn distance_to_face(face: &Face, p: DVec3) -> f64 {
    let Some(&first) = face.verts.first() else { return f64::INFINITY };
    let n = face.normal;
    let off = (p - first).dot(n);
    let (u, v) = face.basis();
    let flat: Vec<DVec2> = face.verts.iter().map(|q| DVec2::new(q.dot(u), q.dot(v))).collect();
    if n.length_squared() > 0.5 && polygon::contains(&flat, DVec2::new(p.dot(u), p.dot(v))) {
        return off.abs();
    }
    let m = face.verts.len();
    (0..m)
        .filter_map(|i| Some((*face.verts.get(i)?, *face.verts.get((i + 1) % m)?)))
        .map(|(a, b)| segment_distance(a, b, p))
        .fold(f64::INFINITY, f64::min)
}

fn segment_distance(a: DVec3, b: DVec3, p: DVec3) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 1e-24 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

/// The face of `solids` nearest `point` (with a normal within the tolerance
/// of `normal`, when given), and its distance.
pub fn nearest_face(solids: &[&Solid], point: DVec3, normal: Option<DVec3>) -> Option<(FaceRef, f64)> {
    let min_cos = NORMAL_TOLERANCE_DEG.to_radians().cos();
    let mut best: Option<(FaceRef, f64, f64)> = None;
    let mut searched = 0;
    for (si, solid) in solids.iter().enumerate() {
        for (fi, face) in solid.faces.iter().enumerate() {
            searched += 1;
            if searched > MAX_SURFACE_FACES {
                break;
            }
            let cos = normal.map_or(1.0, |n| face.normal.dot(n));
            if cos < min_cos || face.verts.len() < 3 {
                continue;
            }
            let d = distance_to_face(face, point);
            // On a tie (a point on an edge), the face turned most like the paint.
            let better = best.is_none_or(|(_, bd, bc)| d < bd - 1e-9 || (d <= bd + 1e-9 && cos > bc));
            if d.is_finite() && better {
                best = Some(((si, fi), d, cos));
            }
        }
    }
    best.map(|(f, d, _)| (f, d))
}

fn face_of<'a>(solids: &[&'a Solid], (si, fi): FaceRef) -> Option<&'a Face> {
    solids.get(si)?.faces.get(fi)
}

/// The box around a face.
fn bounds(f: &Face) -> Option<(DVec3, DVec3)> {
    let first = *f.verts.first()?;
    Some(f.verts.iter().fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
}

/// Whether two faces touch: a corner of one on an edge of the other.
fn touch(a: &Face, b: &Face) -> bool {
    let on_edges = |p: DVec3, f: &Face| {
        let m = f.verts.len();
        (0..m).any(|i| {
            let (Some(&x), Some(&y)) = (f.verts.get(i), f.verts.get((i + 1) % m)) else { return false };
            segment_distance(x, y, p) < TOUCH_TOL
        })
    };
    a.verts.iter().any(|&p| on_edges(p, b)) || b.verts.iter().any(|&p| on_edges(p, a))
}

/// The surface of face `start`: it and, face after face, those touching it
/// in its plane or (from a smooth facet) the smooth facets joining it.
pub fn surface(solids: &[&Solid], start: FaceRef) -> Vec<FaceRef> {
    let Some(first) = face_of(solids, start) else { return Vec::new() };
    let (n0, p0) = (first.normal, first.verts.first().copied().unwrap_or_default());
    let coplanar =
        |f: &Face| f.normal.dot(n0) > 1.0 - 1e-6 && f.verts.iter().all(|p| (*p - p0).dot(n0).abs() < PLANE_TOL);
    // Faces that may join: in the plane, or smooth facets (from a smooth start).
    let candidates: Vec<(FaceRef, (DVec3, DVec3))> = solids
        .iter()
        .enumerate()
        .flat_map(|(si, s)| s.faces.iter().enumerate().map(move |(fi, f)| ((si, fi), f)))
        .filter(|(r, f)| *r != start && (coplanar(f) || (first.smooth && f.smooth)))
        .filter_map(|(r, f)| Some((r, bounds(f)?)))
        .take(MAX_SPREAD)
        .collect();
    let mut taken: BTreeSet<FaceRef> = BTreeSet::from([start]);
    let mut queue = vec![start];
    while let Some(at) = queue.pop() {
        let Some(f) = face_of(solids, at) else { continue };
        let Some((lo, hi)) = bounds(f) else { continue };
        for &(c, (clo, chi)) in &candidates {
            let apart = (clo - hi).max_element() > TOUCH_TOL || (lo - chi).max_element() > TOUCH_TOL;
            if apart || taken.contains(&c) {
                continue;
            }
            let Some(g) = face_of(solids, c) else { continue };
            let joins = (coplanar(g) && coplanar(f)) || (f.smooth && g.smooth && f.normal.dot(g.normal) > SMOOTH_COS);
            if joins && touch(f, g) {
                taken.insert(c);
                queue.push(c);
            }
        }
    }
    taken.into_iter().collect()
}

/// The faces `paint` covers among `solids` (none when no face is turned like it).
pub fn resolve(solids: &[&Solid], paint: &FacePaint) -> Vec<FaceRef> {
    nearest_face(solids, paint.point, Some(paint.normal)).map(|(f, _)| surface(solids, f)).unwrap_or_default()
}

/// The faces of an element's body: its solids that are not secondary parts.
pub fn body_solids(geom: &ModelGeometry, id: ElementId) -> Vec<&Solid> {
    geom.body_of(id).collect()
}

/// Where `ray` meets the body of element `id`: the point and the normal of
/// the face it meets first.
pub fn face_hit(geom: &ModelGeometry, id: ElementId, ray: crate::Ray) -> Option<(DVec3, DVec3)> {
    let mut best: Option<(f64, DVec3)> = None;
    for s in geom.body_of(id) {
        if let Some((t, fi)) = s.raycast_face(ray)
            && best.is_none_or(|(bt, _)| t < bt)
            && let Some(face) = s.faces.get(fi)
        {
            best = Some((t, face.normal));
        }
    }
    best.map(|(t, n)| (ray.origin + ray.dir * t, n))
}

/// Paint the solids of every element with paints (a later paint wins where
/// two cover the same face).
pub(crate) fn apply(project: &Project, solids: &mut [Solid]) {
    let mut of_element: BTreeMap<ElementId, Vec<usize>> = BTreeMap::new();
    for (i, s) in solids.iter().enumerate() {
        if !s.secondary && project.element(s.element).is_some_and(|e| !e.paints.is_empty()) {
            of_element.entry(s.element).or_default().push(i);
        }
    }
    let mut looks: BTreeMap<&str, Option<Look>> = BTreeMap::new();
    for (id, indices) in of_element {
        let Some(e) = project.element(id) else { continue };
        let mut painted: Vec<(usize, usize, Look)> = Vec::new();
        {
            let list: Vec<&Solid> = indices.iter().filter_map(|i| solids.get(*i)).collect();
            for paint in &e.paints {
                let Some(look) =
                    looks.entry(paint.material.as_str()).or_insert_with(|| Look::of(project, &paint.material))
                else {
                    continue;
                };
                for (si, fi) in resolve(&list, paint) {
                    if let Some(&solid) = indices.get(si) {
                        painted.push((solid, fi, look.clone()));
                    }
                }
            }
        }
        for (solid, face, look) in painted {
            if let Some(s) = solids.get_mut(solid) {
                s.painted.insert(face, look);
            }
        }
    }
}

/// Area painted with each material, square meters (the faces each paint covers).
pub fn painted_area(solids: &[&Solid]) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for s in solids {
        for (fi, look) in &s.painted {
            if let (Some(face), Some(m)) = (s.faces.get(*fi), &look.material) {
                *out.entry(m.clone()).or_default() += face.area();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Category;

    /// A 4 x 0.2 x 3 m wall-like box along X, its left face (y = 0.1) split
    /// in two at x = 1 by a second box sharing the plane.
    fn boxes() -> Vec<Solid> {
        let mut a = Solid::new(ElementId(1), Category::Wall, [0; 3]);
        a.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 0.0, 1.0, -0.1, 0.1, 0.0, 3.0);
        a.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 1.0, 4.0, -0.1, 0.1, 0.0, 3.0);
        vec![a]
    }

    #[test]
    fn a_paint_finds_its_face_and_the_plane_it_lies_in() {
        let solids = boxes();
        let list: Vec<&Solid> = solids.iter().collect();
        let paint = FacePaint { point: DVec3::new(0.5, 0.1, 1.0), normal: DVec3::Y, material: "P".into() };
        let faces = resolve(&list, &paint);
        // Both halves of the face at y = 0.1 (touching along x = 1), nothing else.
        assert_eq!(faces.len(), 2, "{faces:?}");
        for (si, fi) in &faces {
            let f = &solids[*si].faces[*fi];
            assert!(f.normal.distance(DVec3::Y) < 1e-9 && f.verts.iter().all(|p| (p.y - 0.1).abs() < 1e-9));
        }
        // The other side is not taken: its normal is opposite.
        let back = FacePaint { normal: -DVec3::Y, ..paint.clone() };
        let faces = resolve(&list, &back);
        assert!(faces.iter().all(|(si, fi)| solids[*si].faces[*fi].normal.y < -0.99));
        // The wall made longer and thicker: the paint stays on its side.
        let mut longer = Solid::new(ElementId(1), Category::Wall, [0; 3]);
        longer.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 0.0, 6.0, -0.15, 0.15, 0.0, 3.0);
        let list = vec![&longer];
        let faces = resolve(&list, &paint);
        assert_eq!(faces.len(), 1);
        assert!((longer.faces[faces[0].1].verts[0].y - 0.15).abs() < 1e-9);
        // A face turned beyond the tolerance takes no paint.
        let turned = FacePaint { normal: DVec3::new(1.0, 1.0, 0.0).normalize(), ..paint };
        assert!(resolve(&list, &turned).is_empty());
    }

    #[test]
    fn painted_solids_carry_the_look() {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let ty = p.default_type(Category::Wall).unwrap();
        let id = cc_core::ElementId(p.alloc_id());
        let material = p.materials[0].name.clone();
        let wall = cc_core::Wall { start: DVec2::ZERO, end: DVec2::new(5.0, 0.0), height: 3.0, ..Default::default() };
        p.elements.insert(
            id,
            cc_core::Element {
                id,
                level,
                type_id: ty,
                mark: String::new(),
                comments: String::new(),
                kind: cc_core::ElementKind::Wall(wall),
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: vec![FacePaint {
                    point: DVec3::new(2.0, 1.0, 1.5),
                    normal: DVec3::Y,
                    material: material.clone(),
                }],
            },
        );
        let geom = ModelGeometry::build(&p);
        let body = body_solids(&geom, id);
        let painted: Vec<(&usize, &Look)> = body.iter().flat_map(|s| s.painted.iter()).collect();
        assert!(!painted.is_empty());
        assert!(painted.iter().all(|(_, l)| l.material.as_deref() == Some(material.as_str())));
        let area = painted_area(&body);
        // The whole left face of a 5 x 3 m wall (give or take the joins at its ends).
        assert!((area[&material] - 15.0).abs() < 1.0, "{area:?}");
    }
}
