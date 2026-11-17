//! Cross-sections of columns and beams: type sections at each detail level,
//! placed column bodies with smooth arc facets, and the legacy section
//! shapes as rectangles and outlines (legends and IFC export).

use crate::Solid;
use cc_core::profiles_project::TypeSection;
use cc_core::{DetailLevel, Project, SectionOutline, SectionShape, TypeData};
use glam::DVec2;

/// The solid parts of a profile in its `width` x `depth` box centered at the
/// origin, as `(min, max)` rectangles that do not overlap.
pub fn shape_rects(shape: SectionShape, width: f64, depth: f64) -> Vec<(DVec2, DVec2)> {
    let (w, d) = (width * 0.5, depth * 0.5);
    let r = |x0: f64, y0: f64, x1: f64, y1: f64| (DVec2::new(x0, y0), DVec2::new(x1, y1));
    match shape.fitted(width, depth) {
        SectionShape::Solid => vec![r(-w, -d, w, d)],
        SectionShape::I { web, flange } => {
            vec![r(-w, -d, w, -d + flange), r(-web * 0.5, -d + flange, web * 0.5, d - flange), r(-w, d - flange, w, d)]
        }
        SectionShape::Channel { web, flange } => {
            vec![r(-w, -d, w, -d + flange), r(-w, -d + flange, -w + web, d - flange), r(-w, d - flange, w, d)]
        }
        SectionShape::Angle { thickness: t } => vec![r(-w, -d, -w + t, d), r(-w + t, -d, w, -d + t)],
        SectionShape::Tube { wall: t } => {
            vec![r(-w, -d, w, -d + t), r(-w, d - t, w, d), r(-w, -d + t, -w + t, d - t), r(w - t, -d + t, w, d - t)]
        }
    }
    .into_iter()
    .filter(|(a, b)| b.x - a.x > 1e-9 && b.y - a.y > 1e-9)
    .collect()
}

/// The outlines of a profile (outer boundary, then holes), counter-clockwise.
pub fn shape_loops(shape: SectionShape, width: f64, depth: f64) -> Vec<Vec<DVec2>> {
    let (w, d) = (width * 0.5, depth * 0.5);
    let p = |pts: &[(f64, f64)]| pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect::<Vec<_>>();
    match shape.fitted(width, depth) {
        SectionShape::Solid => vec![p(&[(-w, -d), (w, -d), (w, d), (-w, d)])],
        SectionShape::I { web, flange: f } => {
            let t = web * 0.5;
            vec![p(&[
                (-w, -d),
                (w, -d),
                (w, -d + f),
                (t, -d + f),
                (t, d - f),
                (w, d - f),
                (w, d),
                (-w, d),
                (-w, d - f),
                (-t, d - f),
                (-t, -d + f),
                (-w, -d + f),
            ])]
        }
        SectionShape::Channel { web, flange: f } => vec![p(&[
            (-w, -d),
            (w, -d),
            (w, -d + f),
            (-w + web, -d + f),
            (-w + web, d - f),
            (w, d - f),
            (w, d),
            (-w, d),
        ])],
        SectionShape::Angle { thickness: t } => {
            vec![p(&[(-w, -d), (w, -d), (w, -d + t), (-w + t, -d + t), (-w + t, d), (-w, d)])]
        }
        SectionShape::Tube { wall: t } => vec![
            p(&[(-w, -d), (w, -d), (w, d), (-w, d)]),
            p(&[(-w + t, -d + t), (-w + t, d - t), (w - t, d - t), (w - t, -d + t)]),
        ],
    }
}

/// Sides of a section whose turn at both ends is below this, degrees: facets
/// of an arc, drawn as one smooth surface.
const SMOOTH_TURN_DEG: f64 = 30.0;

/// A smooth side is at most this many times as long as its shorter neighbor.
const SMOOTH_LENGTH_RATIO: f64 = 2.0;

/// The section of a column or beam type at a detail level: Fine the outline
/// with radii, Medium without (arcs replaced by their chords' corners:
/// rebuild the profile with r/ri/ro = 0), Coarse its bounding rectangle.
pub fn detailed(project: &Project, data: &TypeData, detail: DetailLevel) -> Option<SectionOutline> {
    detailed_section(project, data, detail).map(|s| s.outline)
}

/// [`detailed`] with whether the outline has arcs.
pub fn detailed_section(project: &Project, data: &TypeData, detail: DetailLevel) -> Option<TypeSection> {
    match detail {
        DetailLevel::Fine => project.type_section(data),
        DetailLevel::Medium => project.type_section_without_radii(data),
        DetailLevel::Coarse => {
            let mut s = project.type_section(data)?;
            let (lo, hi) = s.outline.bbox();
            let outer = vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
            s.outline = SectionOutline { outer, holes: Vec::new() };
            s.curved = false;
            Some(s)
        }
    }
}

/// Solid parts of a column (outline + holes) placed in plan.
pub fn column_body(section: &SectionOutline, position: DVec2, rotation_deg: f64) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
    let rot = if rotation_deg.is_finite() { DVec2::from_angle(rotation_deg.to_radians()) } else { DVec2::X };
    let at = |ring: &[DVec2]| ring.iter().map(|p| position + rot.rotate(*p)).collect::<Vec<_>>();
    (at(&section.outer), section.holes.iter().map(|h| at(h)).collect())
}

/// Whether each side of a closed ring (from point `i` to `i + 1`) is a facet
/// of an arc: the ring turns by less than [`SMOOTH_TURN_DEG`] at both ends,
/// and the side is at most [`SMOOTH_LENGTH_RATIO`] times as long as the
/// shorter of its neighbors (so a long flat side between two fillets stays
/// flat).
pub fn smooth_sides(ring: &[DVec2]) -> Vec<bool> {
    let n = ring.len();
    if n < 3 {
        return vec![false; n];
    }
    let side = |i: usize| ring[(i + 1) % n] - ring[i % n];
    let dir = |i: usize| side(i).normalize_or_zero();
    let len = |i: usize| side(i).length();
    let gentle =
        |a: DVec2, b: DVec2| a != DVec2::ZERO && b != DVec2::ZERO && a.dot(b) > SMOOTH_TURN_DEG.to_radians().cos();
    (0..n)
        .map(|i| {
            let (prev, next) = (i + n - 1, i + 1);
            gentle(dir(prev), dir(i))
                && gentle(dir(i), dir(next))
                && len(i) <= SMOOTH_LENGTH_RATIO * len(prev).min(len(next))
        })
        .collect()
}

/// Add a column body from `base` to `top` to `s`; the sides of a `curved`
/// section that are facets of its arcs are smooth.
pub fn add_column_body(s: &mut Solid, outer: &[DVec2], holes: &[Vec<DVec2>], base: f64, top: f64, curved: bool) {
    let before = s.faces.len();
    s.add_prism_with_holes(outer, holes, |_| base, |_| top);
    if !curved {
        return;
    }
    // Sides come last: the outline counter-clockwise, then the holes clockwise.
    let mut rings = vec![crate::polygon::ccw(outer)];
    rings.extend(holes.iter().filter(|h| h.len() >= 3).map(|h| {
        let mut h = crate::polygon::ccw(h);
        h.reverse();
        h
    }));
    let flags: Vec<bool> = rings.iter().flat_map(|r| smooth_sides(r)).collect();
    let added = s.faces.len() - before;
    if added < flags.len() {
        return;
    }
    let first = s.faces.len() - flags.len();
    for (face, smooth) in s.faces[first..].iter_mut().zip(flags) {
        face.smooth = smooth;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::polygon::signed_area;
    use cc_core::ColumnSection;

    fn rect_area(rects: &[(DVec2, DVec2)]) -> f64 {
        rects.iter().map(|(a, b)| (b.x - a.x) * (b.y - a.y)).sum()
    }

    fn loops_area(loops: &[Vec<DVec2>]) -> f64 {
        loops.iter().map(|l| signed_area(l)).sum()
    }

    #[test]
    fn parts_and_outlines_have_the_profile_area() {
        let (w, d) = (0.2, 0.3);
        let cases = [
            (SectionShape::Solid, w * d),
            (SectionShape::I { web: 0.01, flange: 0.02 }, 2.0 * w * 0.02 + 0.01 * (d - 0.04)),
            (SectionShape::Channel { web: 0.01, flange: 0.02 }, 2.0 * w * 0.02 + 0.01 * (d - 0.04)),
            (SectionShape::Angle { thickness: 0.02 }, 0.02 * d + (w - 0.02) * 0.02),
            (SectionShape::Tube { wall: 0.01 }, w * d - (w - 0.02) * (d - 0.02)),
        ];
        for (shape, expected) in cases {
            assert!((rect_area(&shape_rects(shape, w, d)) - expected).abs() < 1e-12, "{shape:?} parts");
            assert!((loops_area(&shape_loops(shape, w, d)) - expected).abs() < 1e-12, "{shape:?} outline");
        }
    }

    #[test]
    fn thicknesses_that_do_not_fit_are_clamped() {
        // A web wider than the box fills it; flanges thicker than half the depth meet.
        let rects = shape_rects(SectionShape::I { web: 1.0, flange: 1.0 }, 0.2, 0.3);
        assert!((rect_area(&rects) - 0.2 * 0.3).abs() < 1e-12);
    }

    /// A project with an IPE 200 profile and a column type on it, turned
    /// `rotation_deg`; returns the type's data.
    fn ipe_column(rotation_deg: f64) -> (Project, TypeData) {
        use cc_core::structural_profile::{ProfileDef, ProfileMaterial, ProfileShape};
        let mut p = Project::from_template();
        let pid = cc_core::ProfileId(p.alloc_id());
        let shape = ProfileShape::I { d: 0.2, bf: 0.1, tw: 0.0056, tf: 0.0085, r: 0.012 };
        let def = ProfileDef {
            id: pid,
            uid: cc_core::Uid::generate(),
            name: "IPE 200".into(),
            material: ProfileMaterial::Steel,
            shape,
            catalog: None,
            description: String::new(),
        };
        p.profiles.insert(pid, def);
        let data = TypeData::Column {
            section: ColumnSection::Rectangular { width: 0.1, depth: 0.2 },
            material: String::new(),
            profile: Some(pid),
            profile_rotation_deg: rotation_deg,
            profile_mirror: false,
        };
        (p, data)
    }

    #[test]
    fn detail_levels_round_square_or_box_a_section() {
        let (p, data) = ipe_column(0.0);
        let fine = detailed(&p, &data, DetailLevel::Fine).unwrap();
        let medium = detailed(&p, &data, DetailLevel::Medium).unwrap();
        let coarse = detailed(&p, &data, DetailLevel::Coarse).unwrap();
        assert!(fine.outer.len() > 12);
        assert_eq!(medium.outer.len(), 12, "the I without root radii");
        assert_eq!(coarse.outer.len(), 4);
        assert!(coarse.holes.is_empty());
        // All three share the bounding box; medium's area has no fillets.
        assert_eq!(fine.bbox(), coarse.bbox());
        let a = |o: &SectionOutline| signed_area(&o.outer);
        assert!((a(&medium) - (2.0 * 0.1 * 0.0085 + (0.2 - 0.017) * 0.0056)).abs() < 1e-12);
        assert!((a(&coarse) - 0.1 * 0.2).abs() < 1e-12);
        // The type's rotation turns every level.
        let (p, data) = ipe_column(90.0);
        let (lo, hi) = detailed(&p, &data, DetailLevel::Coarse).unwrap().bbox();
        assert!(((hi - lo) - DVec2::new(0.2, 0.1)).abs().max_element() < 1e-9);
        assert_eq!(detailed(&p, &data, DetailLevel::Medium).unwrap().outer.len(), 12);
    }

    #[test]
    fn column_bodies_are_placed_and_smooth_only_on_arcs() {
        let (p, data) = ipe_column(0.0);
        let section = p.type_section(&data).unwrap();
        assert!(section.curved);
        let (outer, holes) = column_body(&section.outline, DVec2::new(5.0, 1.0), 90.0);
        assert!(holes.is_empty());
        assert!((signed_area(&outer) - signed_area(&section.outline.outer)).abs() < 1e-12);
        let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Column, [0; 3]);
        add_column_body(&mut s, &outer, &holes, 0.0, 3.0, true);
        assert!(s.faces.iter().any(|f| f.smooth), "fillet facets are smooth");
        assert!(s.faces.iter().filter(|f| f.normal.z.abs() > 0.5).all(|f| !f.smooth), "caps are not");
        // Without radii: a plain I has no smooth faces.
        let square = p.type_section_without_radii(&data).unwrap();
        assert!(!square.curved);
        let (outer, holes) = column_body(&square.outline, DVec2::ZERO, 0.0);
        let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Column, [0; 3]);
        add_column_body(&mut s, &outer, &holes, 0.0, 3.0, square.curved);
        assert!(s.faces.iter().all(|f| !f.smooth));
        assert!((s.volume() - 3.0 * signed_area(&outer)).abs() < 1e-9);
        // Sharp corners are never smooth, gentle turns are.
        let rect = [DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
        assert_eq!(smooth_sides(&rect), vec![false; 4]);
        let circle: Vec<DVec2> = (0..24).map(|i| DVec2::from_angle(i as f64 / 24.0 * std::f64::consts::TAU)).collect();
        assert!(smooth_sides(&circle).iter().all(|s| *s));
        // The long flat sides between an IPE's fillets (its web) stay flat.
        let ring = &section.outline.outer;
        let flags = smooth_sides(ring);
        let longest = (0..ring.len())
            .max_by(|&a, &b| {
                let l = |i: usize| ring[i].distance(ring[(i + 1) % ring.len()]);
                l(a).total_cmp(&l(b))
            })
            .unwrap();
        assert!(!flags[longest], "the web side is flat");
        assert!(flags.iter().any(|s| *s), "the fillet facets are smooth");
        // A round tube is smooth all round, outside and in.
        use cc_core::structural_profile::ProfileShape;
        let tube = ProfileShape::RoundTube { d: 0.2, t: 0.01 }.outline().unwrap();
        assert!(smooth_sides(&tube.outer).iter().all(|s| *s));
        assert!(tube.holes.iter().all(|h| smooth_sides(h).iter().all(|s| *s)));
    }
}
