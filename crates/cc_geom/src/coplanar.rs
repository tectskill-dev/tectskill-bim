//! Coplanar faces of an imported mesh merged into polygons: the triangles
//! a file splits a flat face into come back as one face, so views draw no
//! diagonals across it and the piece holds fewer faces.
//!
//! Faces grow one neighbor at a time across shared edges (opposite
//! directions, so the winding agrees), keeping each merged face a simple
//! polygon: a neighbor joins only when it shares one run of edges with the
//! face so far and touches it nowhere else, so no hole or pinch can form.
//! A ring of triangles thus stays two faces or more. Vertices are never
//! dropped (a point on a straight run may be a corner of a neighbor).

use glam::DVec3;
use std::collections::{HashMap, HashSet};

/// Farthest a vertex may lie from the plane of the face it joins, in meters.
const PLANE_TOLERANCE: f64 = 1e-5;
/// Cosine of the largest angle between the normals of merged faces (1°).
const NORMAL_COS: f64 = 0.999_847_7;
/// Corners of one merged face.
const MAX_LOOP: usize = 4096;

/// `faces` (indices into `vertices`) with neighbors in one plane merged.
pub fn merge(vertices: &[[f64; 3]], faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    let point = |i: u32| vertices.get(i as usize).map(|v| DVec3::from_array(*v)).filter(|p| p.is_finite());
    // Each face's plane, when it is flat and its corners are distinct.
    let planes: Vec<Option<(DVec3, f64)>> = faces
        .iter()
        .map(|f| {
            let pts: Vec<DVec3> = f.iter().map(|&i| point(i)).collect::<Option<_>>()?;
            let distinct: HashSet<u32> = f.iter().copied().collect();
            if pts.len() < 3 || distinct.len() != f.len() {
                return None;
            }
            let n = newell(&pts).try_normalize()?;
            let d = n.dot(pts[0]);
            pts.iter().all(|p| (n.dot(*p) - d).abs() <= PLANE_TOLERANCE).then_some((n, d))
        })
        .collect();
    // Directed edges to their face; an edge used twice the same way is not shared.
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for (k, f) in faces.iter().enumerate().filter(|(k, _)| planes[*k].is_some()) {
        for i in 0..f.len() {
            let e = (f[i], f[(i + 1) % f.len()]);
            edges.entry(e).and_modify(|v| *v = usize::MAX).or_insert(k);
        }
    }
    let mut used = vec![false; faces.len()];
    let mut out = Vec::with_capacity(faces.len());
    for seed in 0..faces.len() {
        if used[seed] {
            continue;
        }
        used[seed] = true;
        let Some((normal, offset)) = planes[seed] else {
            out.push(faces[seed].clone());
            continue;
        };
        let mut boundary = faces[seed].clone();
        // Grow until no neighbor joins.
        let mut grown = true;
        while grown && boundary.len() < MAX_LOOP {
            grown = false;
            for i in 0..boundary.len() {
                let (a, b) = (boundary[i], boundary[(i + 1) % boundary.len()]);
                let Some(&g) = edges.get(&(b, a)) else { continue };
                if g == usize::MAX || used.get(g) != Some(&false) {
                    continue;
                }
                let Some((n, _)) = planes[g] else { continue };
                let close = faces[g]
                    .iter()
                    .all(|&v| point(v).is_some_and(|p| (normal.dot(p) - offset).abs() <= PLANE_TOLERANCE));
                if n.dot(normal) < NORMAL_COS || !close {
                    continue;
                }
                if let Some(next) = splice(&boundary, &faces[g]) {
                    boundary = next;
                    used[g] = true;
                    grown = true;
                    break;
                }
            }
        }
        out.push(boundary);
    }
    out
}

/// The loop `boundary` with face `g` added, when `g` shares one run of edges
/// with it (each in the opposite direction) and no other vertex: the shared
/// run replaced by the rest of `g`.
fn splice(boundary: &[u32], g: &[u32]) -> Option<Vec<u32>> {
    let (n, m) = (boundary.len(), g.len());
    let on_boundary: HashMap<u32, usize> = boundary.iter().enumerate().map(|(i, v)| (*v, i)).collect();
    let directed: HashSet<(u32, u32)> = (0..n).map(|i| (boundary[i], boundary[(i + 1) % n])).collect();
    let shared: Vec<bool> = (0..m).map(|i| directed.contains(&(g[(i + 1) % m], g[i]))).collect();
    let count = shared.iter().filter(|s| **s).count();
    if count == 0 || count == m {
        return None;
    }
    // The run's first edge: shared, after one that is not.
    let start = (0..m).find(|&i| shared[i] && !shared[(i + m - 1) % m])?;
    if (0..count).any(|k| !shared[(start + k) % m]) {
        return None;
    }
    // g runs from first along the shared edges to last, then back around freely.
    let first = g[start];
    let last = g[(start + count) % m];
    let free: Vec<u32> = (0..m - count).map(|k| g[(start + count + k) % m]).collect();
    // The free path touches the boundary only at its two ends.
    if free.iter().skip(1).any(|v| on_boundary.contains_key(v)) {
        return None;
    }
    // In the boundary the run goes from `last` back to `first`.
    let p = *on_boundary.get(&last)?;
    if boundary[(p + count) % n] != first {
        return None;
    }
    let mut out: Vec<u32> = free;
    out.extend((count..n).map(|k| boundary[(p + k) % n]));
    // `first` closes the free path and opens the rest of the boundary once.
    out.dedup();
    if out.first() == out.last() && out.len() > 1 {
        out.pop();
    }
    let distinct: HashSet<u32> = out.iter().copied().collect();
    (out.len() >= 3 && distinct.len() == out.len()).then_some(out)
}

fn newell(f: &[DVec3]) -> DVec3 {
    let n = f.len();
    (0..n)
        .map(|i| {
            let (a, b) = (f[i], f[(i + 1) % n]);
            DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signed area of a face along `normal`.
    fn area(vertices: &[[f64; 3]], f: &[u32], normal: DVec3) -> f64 {
        let pts: Vec<DVec3> = f.iter().map(|&i| DVec3::from_array(vertices[i as usize])).collect();
        newell(&pts).dot(normal) / 2.0
    }

    /// An `n` x `n` grid of unit squares in z = 0, each split in two triangles.
    fn grid(n: u32) -> (Vec<[f64; 3]>, Vec<Vec<u32>>) {
        let vertices = (0..=n).flat_map(|y| (0..=n).map(move |x| [x as f64, y as f64, 0.0])).collect();
        let at = |x: u32, y: u32| y * (n + 1) + x;
        let mut faces = Vec::new();
        for y in 0..n {
            for x in 0..n {
                faces.push(vec![at(x, y), at(x + 1, y), at(x + 1, y + 1)]);
                faces.push(vec![at(x, y), at(x + 1, y + 1), at(x, y + 1)]);
            }
        }
        (vertices, faces)
    }

    #[test]
    fn a_split_square_becomes_one_face() {
        let (v, f) = grid(3);
        let merged = merge(&v, f);
        assert_eq!(merged.len(), 1, "{merged:?}");
        // The 12 points around the edge, counter-clockwise like the triangles.
        assert_eq!(merged[0].len(), 12);
        assert!((area(&v, &merged[0], DVec3::Z) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn a_triangulated_cube_has_six_faces() {
        let v: Vec<[f64; 3]> = (0..8).map(|i| [(i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64]).collect();
        let quads = [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]];
        let f: Vec<Vec<u32>> = quads.iter().flat_map(|q| [vec![q[0], q[1], q[2]], vec![q[0], q[2], q[3]]]).collect();
        let merged = merge(&v, f);
        assert_eq!(merged.len(), 6);
        assert!(merged.iter().all(|f| f.len() == 4));
    }

    #[test]
    fn a_ring_keeps_its_hole() {
        // A 3 x 3 grid without its middle square: no single face can hold it.
        let (v, mut f) = grid(3);
        f.drain(8..10);
        let total: f64 = f.iter().map(|f| area(&v, f, DVec3::Z)).sum();
        let merged = merge(&v, f);
        assert!(merged.len() >= 2, "{merged:?}");
        let after: f64 = merged.iter().map(|f| area(&v, f, DVec3::Z)).sum();
        assert!((after - total).abs() < 1e-9 && (total - 8.0).abs() < 1e-9);
        for f in &merged {
            let distinct: HashSet<u32> = f.iter().copied().collect();
            assert_eq!(distinct.len(), f.len());
        }
    }

    #[test]
    fn bent_flipped_and_broken_faces_stay_apart() {
        // Two triangles folded 5 degrees along their shared edge.
        let lift = 5f64.to_radians().tan();
        let v = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, lift]];
        assert_eq!(merge(&v, vec![vec![0, 1, 2], vec![2, 1, 3]]).len(), 2);
        // Flat, but wound against each other: the edge runs the same way in both.
        let flat = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        assert_eq!(merge(&flat, vec![vec![0, 1, 2], vec![1, 2, 3]]).len(), 2);
        // Bad indices, repeated corners and a point at infinity are kept as they are.
        let mut bad = flat.clone();
        bad.push([f64::INFINITY, 0.0, 0.0]);
        let faces = vec![vec![0, 1, 9], vec![0, 0, 1], vec![0, 1, 4], vec![0, 1, 2], vec![2, 1, 3]];
        let merged = merge(&bad, faces);
        assert_eq!(merged.len(), 4, "{merged:?}");
    }
}
