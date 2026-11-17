//! Boolean operations on closed polygon meshes with binary space
//! partitioning trees (the classic BSP-tree CSG algorithm: each solid's
//! polygons are clipped against the other's tree). Trees live in arenas and
//! are walked with explicit stacks, so deep trees cannot overflow the call
//! stack; operations whose fragments would exceed a budget give up and
//! return `None`, leaving the caller with the uncut solid.

use glam::DVec3;

/// Distance under which a point lies on a plane (meters).
const EPS: f64 = 1e-6;
/// Most polygons an operation may produce or hold in its trees.
const MAX_POLYGONS: usize = 200_000;
/// Most polygons one pass over a tree (building it or clipping by it) may
/// split: far above what real solids need, it bounds the work of
/// degenerate trees.
const MAX_SPLITS: usize = 50_000_000;

#[derive(Debug, Clone, Copy)]
struct Plane {
    normal: DVec3,
    w: f64,
}

impl Plane {
    fn of(verts: &[DVec3]) -> Option<Self> {
        let n = verts.len();
        let normal: DVec3 = (0..n)
            .map(|i| {
                let (a, b) = (verts[i], verts[(i + 1) % n]);
                DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y))
            })
            .sum::<DVec3>()
            .try_normalize()?;
        let center = verts.iter().copied().sum::<DVec3>() / n as f64;
        Some(Self { normal, w: normal.dot(center) })
    }

    fn flip(&mut self) {
        self.normal = -self.normal;
        self.w = -self.w;
    }
}

#[derive(Debug, Clone)]
struct Poly {
    verts: Vec<DVec3>,
    plane: Plane,
}

impl Poly {
    fn new(verts: Vec<DVec3>) -> Option<Self> {
        let plane = Plane::of(&verts)?;
        (verts.len() >= 3).then_some(Self { verts, plane })
    }

    fn flip(&mut self) {
        self.verts.reverse();
        self.plane.flip();
    }
}

const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;

/// Where `poly` goes relative to `plane`: coplanar polygons go to the front
/// or back list by facing, spanning ones are split.
fn split(plane: &Plane, poly: Poly, cf: &mut Vec<Poly>, cb: &mut Vec<Poly>, f: &mut Vec<Poly>, b: &mut Vec<Poly>) {
    let mut kind = COPLANAR;
    let types: Vec<u8> = poly
        .verts
        .iter()
        .map(|v| {
            let t = plane.normal.dot(*v) - plane.w;
            let k = if t < -EPS {
                BACK
            } else if t > EPS {
                FRONT
            } else {
                COPLANAR
            };
            kind |= k;
            k
        })
        .collect();
    match kind {
        COPLANAR => {
            if plane.normal.dot(poly.plane.normal) > 0.0 {
                cf.push(poly)
            } else {
                cb.push(poly)
            }
        }
        FRONT => f.push(poly),
        BACK => b.push(poly),
        _ => {
            let n = poly.verts.len();
            let (mut fv, mut bv) = (Vec::new(), Vec::new());
            for i in 0..n {
                let j = (i + 1) % n;
                let (ti, tj) = (types[i], types[j]);
                let (vi, vj) = (poly.verts[i], poly.verts[j]);
                if ti != BACK {
                    fv.push(vi);
                }
                if ti != FRONT {
                    bv.push(vi);
                }
                if (ti | tj) == (FRONT | BACK) {
                    let t = (plane.w - plane.normal.dot(vi)) / plane.normal.dot(vj - vi);
                    let v = vi.lerp(vj, t);
                    fv.push(v);
                    bv.push(v);
                }
            }
            // Pieces keep the plane of the polygon they came from.
            if fv.len() >= 3 {
                f.push(Poly { verts: fv, plane: poly.plane });
            }
            if bv.len() >= 3 {
                b.push(Poly { verts: bv, plane: poly.plane });
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<usize>,
    back: Option<usize>,
    polys: Vec<Poly>,
}

/// A BSP tree in an arena; node 0 is the root.
#[derive(Debug, Clone)]
struct Tree {
    nodes: Vec<Node>,
}

/// The budget ran out.
struct TooBig;

impl Tree {
    fn new(polys: Vec<Poly>) -> Result<Self, TooBig> {
        let mut t = Self { nodes: vec![Node::default()] };
        t.build(polys)?;
        Ok(t)
    }

    fn size(&self) -> usize {
        self.nodes.iter().map(|n| n.polys.len()).sum::<usize>() + self.nodes.len()
    }

    /// Insert polygons, growing the tree from the root.
    fn build(&mut self, polys: Vec<Poly>) -> Result<(), TooBig> {
        // The tree's size, kept up to date as polygons and nodes are added.
        let mut size = self.size();
        let mut work = 0usize;
        let mut stack = vec![(0usize, polys)];
        while let Some((i, polys)) = stack.pop() {
            let Some(first) = polys.first() else { continue };
            work += polys.len();
            if work > MAX_SPLITS {
                return Err(TooBig);
            }
            let plane = *self.nodes[i].plane.get_or_insert(first.plane);
            let (mut cf, mut cb, mut f, mut b) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for p in polys {
                split(&plane, p, &mut cf, &mut cb, &mut f, &mut b);
            }
            size += cf.len() + cb.len();
            self.nodes[i].polys.extend(cf);
            self.nodes[i].polys.extend(cb);
            for (side, list) in [(true, f), (false, b)] {
                if list.is_empty() {
                    continue;
                }
                let child = if side { self.nodes[i].front } else { self.nodes[i].back };
                let child = match child {
                    Some(c) => c,
                    None => {
                        self.nodes.push(Node::default());
                        size += 1;
                        let c = self.nodes.len() - 1;
                        if side {
                            self.nodes[i].front = Some(c);
                        } else {
                            self.nodes[i].back = Some(c);
                        }
                        c
                    }
                };
                stack.push((child, list));
            }
            if size > MAX_POLYGONS {
                return Err(TooBig);
            }
        }
        Ok(())
    }

    /// The parts of `polys` outside this tree's solid.
    fn clip_polygons(&self, polys: Vec<Poly>) -> Result<Vec<Poly>, TooBig> {
        let mut out = Vec::new();
        // Polygons waiting on the stack, kept up to date as it changes.
        let mut pending = polys.len();
        let mut work = 0usize;
        let mut stack = vec![(0usize, polys)];
        while let Some((i, polys)) = stack.pop() {
            pending -= polys.len();
            let node = &self.nodes[i];
            let Some(plane) = node.plane else {
                out.extend(polys);
                continue;
            };
            work += polys.len();
            if work > MAX_SPLITS {
                return Err(TooBig);
            }
            let (mut f, mut b) = (Vec::new(), Vec::new());
            for p in polys {
                let (mut cf, mut cb) = (Vec::new(), Vec::new());
                split(&plane, p, &mut cf, &mut cb, &mut f, &mut b);
                f.extend(cf);
                b.extend(cb);
            }
            match node.front {
                Some(c) => {
                    pending += f.len();
                    stack.push((c, f));
                }
                None => out.extend(f),
            }
            // Behind a leaf is inside the solid: dropped.
            if let Some(c) = node.back {
                pending += b.len();
                stack.push((c, b));
            }
            if out.len() + pending > MAX_POLYGONS {
                return Err(TooBig);
            }
        }
        Ok(out)
    }

    /// Remove the parts of this tree's polygons inside `other`.
    fn clip_to(&mut self, other: &Tree) -> Result<(), TooBig> {
        for node in &mut self.nodes {
            let polys = std::mem::take(&mut node.polys);
            node.polys = other.clip_polygons(polys)?;
        }
        Ok(())
    }

    /// Turn the solid inside out.
    fn invert(&mut self) {
        for node in &mut self.nodes {
            node.polys.iter_mut().for_each(Poly::flip);
            if let Some(p) = &mut node.plane {
                p.flip();
            }
            std::mem::swap(&mut node.front, &mut node.back);
        }
    }

    fn polygons(&self) -> Vec<Poly> {
        self.nodes.iter().flat_map(|n| n.polys.iter().cloned()).collect()
    }
}

/// Polygons fit for splitting: convex faces as they are, others as triangles.
fn polys(mesh: &[Vec<DVec3>]) -> Vec<Poly> {
    let mut out = Vec::with_capacity(mesh.len());
    for face in mesh {
        let Some(p) = Poly::new(face.clone()) else { continue };
        let n = face.len();
        let convex = (0..n).all(|i| {
            let (a, b, c) = (face[i], face[(i + 1) % n], face[(i + 2) % n]);
            (b - a).cross(c - b).dot(p.plane.normal) >= -1e-12
        });
        if convex {
            out.push(p);
            continue;
        }
        let helper = if p.plane.normal.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let u = p.plane.normal.cross(helper).normalize();
        let v = p.plane.normal.cross(u);
        let flat: Vec<glam::DVec2> = face.iter().map(|q| glam::DVec2::new(q.dot(u), q.dot(v))).collect();
        for [a, b, c] in crate::polygon::triangulate(&flat) {
            let (Some(a), Some(b), Some(c)) = (face.get(a), face.get(b), face.get(c)) else { continue };
            // Ear clipping keeps the winding of the outline.
            if let Some(t) = Poly::new(vec![*a, *b, *c]) {
                out.push(t);
            }
        }
    }
    out
}

fn to_mesh(polys: Vec<Poly>) -> Vec<Vec<DVec3>> {
    polys.into_iter().map(|p| p.verts).collect()
}

/// `a` minus `b` (both closed and outward), or `None` past the budget.
pub fn subtract(a: &[Vec<DVec3>], b: &[Vec<DVec3>]) -> Option<Vec<Vec<DVec3>>> {
    let run = || -> Result<Vec<Vec<DVec3>>, TooBig> {
        let mut a = Tree::new(polys(a))?;
        let mut b = Tree::new(polys(b))?;
        a.invert();
        a.clip_to(&b)?;
        b.clip_to(&a)?;
        b.invert();
        b.clip_to(&a)?;
        b.invert();
        a.build(b.polygons())?;
        a.invert();
        Ok(to_mesh(a.polygons()))
    };
    run().ok()
}

/// The common part of `a` and `b`, or `None` past the budget.
pub fn intersect(a: &[Vec<DVec3>], b: &[Vec<DVec3>]) -> Option<Vec<Vec<DVec3>>> {
    let run = || -> Result<Vec<Vec<DVec3>>, TooBig> {
        let mut a = Tree::new(polys(a))?;
        let mut b = Tree::new(polys(b))?;
        a.invert();
        b.clip_to(&a)?;
        b.invert();
        a.clip_to(&b)?;
        b.clip_to(&a)?;
        a.build(b.polygons())?;
        a.invert();
        Ok(to_mesh(a.polygons()))
    };
    run().ok()
}

/// Whether the bounding boxes of two meshes overlap.
pub fn bounds_overlap(a: &[Vec<DVec3>], b: &[Vec<DVec3>]) -> bool {
    let bounds = |m: &[Vec<DVec3>]| {
        m.iter().flatten().fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        })
    };
    let ((alo, ahi), (blo, bhi)) = (bounds(a), bounds(b));
    alo.cmple(bhi + EPS).all() && blo.cmple(ahi + EPS).all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(lo: DVec3, hi: DVec3) -> Vec<Vec<DVec3>> {
        let p = |x: bool, y: bool, z: bool| {
            DVec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z })
        };
        let (f, t) = (false, true);
        vec![
            vec![p(f, f, f), p(f, t, f), p(t, t, f), p(t, f, f)],
            vec![p(f, f, t), p(t, f, t), p(t, t, t), p(f, t, t)],
            vec![p(f, f, f), p(t, f, f), p(t, f, t), p(f, f, t)],
            vec![p(t, f, f), p(t, t, f), p(t, t, t), p(t, f, t)],
            vec![p(t, t, f), p(f, t, f), p(f, t, t), p(t, t, t)],
            vec![p(f, t, f), p(f, f, f), p(f, f, t), p(f, t, t)],
        ]
    }

    fn volume(mesh: &[Vec<DVec3>]) -> f64 {
        mesh.iter().flat_map(|f| (1..f.len() - 1).map(move |i| f[0].dot(f[i].cross(f[i + 1])))).sum::<f64>() / 6.0
    }

    #[test]
    fn a_hole_through_a_wall() {
        let wall = cube(DVec3::ZERO, DVec3::new(4.0, 0.2, 3.0));
        let opening = cube(DVec3::new(1.0, -0.1, 0.0), DVec3::new(2.0, 0.3, 2.0));
        let cut = subtract(&wall, &opening).unwrap();
        assert!((volume(&cut) - (4.0 * 0.2 * 3.0 - 1.0 * 0.2 * 2.0)).abs() < 1e-9, "{}", volume(&cut));
        // A recess that does not go through.
        let niche = cube(DVec3::new(1.0, 0.1, 1.0), DVec3::new(2.0, 0.5, 2.0));
        let cut = subtract(&wall, &niche).unwrap();
        assert!((volume(&cut) - (2.4 - 0.1)).abs() < 1e-9, "{}", volume(&cut));
        let common = intersect(&wall, &niche).unwrap();
        assert!((volume(&common) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn concave_faces_are_split_first() {
        // An L-shaped prism minus a box through its corner.
        let l = [(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)];
        let low: Vec<DVec3> = l.iter().map(|(x, y)| DVec3::new(*x, *y, 0.0)).collect();
        let high: Vec<DVec3> = low.iter().map(|p| *p + DVec3::Z).collect();
        let mut mesh = vec![low.iter().rev().copied().collect::<Vec<_>>(), high.clone()];
        for i in 0..6 {
            let j = (i + 1) % 6;
            mesh.push(vec![low[i], low[j], high[j], high[i]]);
        }
        assert!((volume(&mesh) - 3.0).abs() < 1e-9);
        let cut = subtract(&mesh, &cube(DVec3::new(-1.0, -1.0, -1.0), DVec3::new(0.5, 0.5, 2.0))).unwrap();
        assert!((volume(&cut) - 2.75).abs() < 1e-9, "{}", volume(&cut));
    }

    /// A finely faceted body (a 256-sided cylinder) cut by a box: the trees
    /// grow with many polygons and the cut stays exact for its facets.
    #[test]
    fn many_faced_solids_are_cut() {
        let n = 256;
        let ring = |z: f64| -> Vec<DVec3> {
            (0..n)
                .map(|i| (i as f64 / n as f64 * std::f64::consts::TAU, z))
                .map(|(a, z)| DVec3::new(a.cos(), a.sin(), z))
                .collect()
        };
        let (low, high) = (ring(0.0), ring(1.0));
        let mut mesh = vec![low.iter().rev().copied().collect::<Vec<_>>(), high.clone()];
        for i in 0..n {
            let j = (i + 1) % n;
            mesh.push(vec![low[i], low[j], high[j], high[i]]);
        }
        let whole = volume(&mesh);
        // The half with x > 0 goes.
        let cut = subtract(&mesh, &cube(DVec3::new(0.0, -2.0, -1.0), DVec3::new(2.0, 2.0, 2.0))).unwrap();
        assert!((volume(&cut) - whole / 2.0).abs() < 1e-9, "{} vs {}", volume(&cut), whole / 2.0);
    }
}
