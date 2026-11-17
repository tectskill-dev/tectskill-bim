//! Conceptual masses: the body of a mass (an extrusion, or a loft between
//! two outlines) and its mass floors, the sections at the levels it lists.

use cc_core::mass::{Mass, MassFloor, area, perimeter};
use cc_core::{Element, Project, SketchPlane};
use glam::{DVec2, DVec3};

/// A mass's body and floors.
#[derive(Debug, Clone, PartialEq)]
pub struct MassGeom {
    /// Faces of the closed body.
    pub faces: Vec<Vec<DVec3>>,
    /// Elevations of the base and the top.
    pub z_range: (f64, f64),
    pub volume: f64,
    /// Mass floors from the lowest up: one per listed level the mass spans
    /// (at or above its base, below its top).
    pub floors: Vec<MassFloor>,
}

impl MassGeom {
    /// Total area of the mass floors (the gross floor area).
    pub fn floor_area(&self) -> f64 {
        self.floors.iter().map(|f| f.area).sum()
    }
}

/// The body and floors of mass `m` (element `e`), or `None` when it is degenerate.
pub fn mass_geom(project: &Project, e: &Element, m: &Mass) -> Option<MassGeom> {
    if !m.is_valid() {
        return None;
    }
    let base = project.level_elevation(e.level) + m.base_offset;
    let top = base + m.height;
    let bottom = m.section(0.0);
    let faces = match &m.top {
        None => {
            let mut s = crate::Solid::new(e.id, e.category(), [0, 0, 0]);
            s.add_prism(&bottom, |_| base, |_| top);
            s.faces.into_iter().map(|f| f.verts).collect()
        }
        Some(_) => cc_core::sweep::blend(SketchPlane::Plan, &bottom, &m.section(1.0), base, m.height).ok()?,
    };
    if faces.is_empty() {
        return None;
    }
    let volume = cc_core::sweep::volume(&faces).abs();
    let mut levels: Vec<_> = m
        .floors
        .iter()
        .filter_map(|id| project.level(*id))
        .filter(|l| l.elevation >= base - 1e-9 && l.elevation < top - 1e-9)
        .collect();
    levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    levels.dedup_by_key(|l| l.id);
    let floors = levels
        .into_iter()
        .map(|l| {
            let outline: Vec<DVec2> = m.section((l.elevation - base) / m.height);
            MassFloor { level: l.id, area: area(&outline), perimeter: perimeter(&outline), outline }
        })
        .collect();
    Some(MassGeom { faces, z_range: (base, top), volume, floors })
}

/// The face of mass `m` (built as `g`) at `p`, a point on its body: the top
/// or bottom, or the side over the nearest edge of an extrusion's outline
/// (a loft's sides are not pushed).
pub fn face_at(m: &Mass, g: &MassGeom, p: DVec3) -> Option<cc_core::mass::MassFace> {
    use cc_core::mass::MassFace;
    let eps = 1e-4 * (1.0 + g.z_range.1.abs());
    if (p.z - g.z_range.1).abs() < eps {
        return Some(MassFace::Top);
    }
    if (p.z - g.z_range.0).abs() < eps {
        return Some(MassFace::Bottom);
    }
    if m.top.is_some() {
        return None;
    }
    let n = m.outline.len();
    let q = p.truncate();
    (0..n)
        .filter_map(|i| {
            let (a, b) = (*m.outline.get(i)?, *m.outline.get((i + 1) % n)?);
            Some((crate::polygon::closest_on_segment(a, b, q).0.distance(q), i))
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, i)| MassFace::Side(i))
}

/// The outline of a face of `m` in the world, and its outward normal.
pub fn face_outline(m: &Mass, g: &MassGeom, face: cc_core::mass::MassFace) -> Option<(Vec<DVec3>, DVec3)> {
    use cc_core::mass::MassFace;
    let (base, top) = g.z_range;
    match face {
        MassFace::Top => Some((m.section(1.0).iter().map(|p| p.extend(top)).collect(), DVec3::Z)),
        MassFace::Bottom => Some((m.section(0.0).iter().map(|p| p.extend(base)).collect(), -DVec3::Z)),
        MassFace::Side(i) => {
            let n = m.outline.len();
            let (a, b) = (*m.outline.get(i)?, *m.outline.get((i + 1) % n)?);
            let u = (b - a).try_normalize()?;
            let ccw = crate::polygon::signed_area(&m.outline) > 0.0;
            let out = if ccw { DVec2::new(u.y, -u.x) } else { DVec2::new(-u.y, u.x) };
            Some((vec![a.extend(base), b.extend(base), b.extend(top), a.extend(top)], out.extend(0.0)))
        }
    }
}
