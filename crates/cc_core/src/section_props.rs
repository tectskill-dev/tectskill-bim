//! Geometric properties of a section outline (area, centroid, second
//! moments, moduli, radii of gyration, torsion constant, mass per length),
//! computed exactly for the polygon with Green's theorem. SI units.

use crate::structural_profile::SectionOutline;
use glam::DVec2;
use serde::Serialize;

/// Properties of a section. Units: m, m², m⁴, m³ and kg/m.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct SectionProperties {
    /// Area, m².
    pub area: f64,
    /// Centroid in the outline's own coordinates, m.
    pub centroid: [f64; 2],
    /// Second moment about the centroidal x axis, m⁴.
    pub ix: f64,
    /// Second moment about the centroidal y axis, m⁴.
    pub iy: f64,
    /// Product of inertia about the centroidal axes, m⁴.
    pub ixy: f64,
    /// Elastic modulus about x (`ix` over the farthest fiber in y), m³.
    pub wx: f64,
    /// Elastic modulus about y, m³.
    pub wy: f64,
    /// Radius of gyration about x, m.
    pub rx: f64,
    /// Radius of gyration about y, m.
    pub ry: f64,
    /// Torsion constant, m⁴. Approximate: Bredt's 4A²t/s on the mid-wall for
    /// closed shapes, `perimeter * t³ / 6` (`t = 2 * area / perimeter`) for open ones.
    pub j: f64,
    /// Outer perimeter plus the perimeters of the holes, m.
    pub perimeter: f64,
    /// Mass per length, kg/m.
    pub linear_mass: f64,
}

/// Sums of Green's theorem over one loop about the origin:
/// (area, ∫x, ∫y, ∫x², ∫y², ∫xy) as signed by the winding, and the length.
#[derive(Default)]
struct Ring {
    a: f64,
    sx: f64,
    sy: f64,
    ixx: f64,
    iyy: f64,
    ixy: f64,
    len: f64,
}

fn ring(p: &[DVec2]) -> Ring {
    let mut r = Ring::default();
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let c = a.x * b.y - b.x * a.y;
        r.a += c / 2.0;
        r.sx += (a.x + b.x) * c / 6.0;
        r.sy += (a.y + b.y) * c / 6.0;
        r.ixx += (a.y * a.y + a.y * b.y + b.y * b.y) * c / 12.0;
        r.iyy += (a.x * a.x + a.x * b.x + b.x * b.x) * c / 12.0;
        r.ixy += (a.x * b.y + 2.0 * a.x * a.y + 2.0 * b.x * b.y + b.x * a.y) * c / 24.0;
        r.len += (b - a).length();
    }
    r
}

impl SectionOutline {
    /// The section's properties for a material of `density` kg/m³ (a
    /// non-finite or negative density counts as 0). A degenerate outline
    /// gives all zeros.
    pub fn properties(&self, density: f64) -> SectionProperties {
        let density = if density.is_finite() && density > 0.0 { density } else { 0.0 };
        // Each loop is taken with a positive area, the outer one added and the holes subtracted.
        let positive = |r: Ring| {
            let k = if r.a < 0.0 { -1.0 } else { 1.0 };
            Ring { a: r.a * k, sx: r.sx * k, sy: r.sy * k, ixx: r.ixx * k, iyy: r.iyy * k, ixy: r.ixy * k, len: r.len }
        };
        let outer = positive(ring(&self.outer));
        let mut t = Ring { len: 0.0, ..positive(ring(&self.outer)) };
        let mut perimeter = outer.len;
        let mut hole_area = 0.0;
        let mut hole_len = 0.0;
        for h in self.holes.iter().map(|h| positive(ring(h))) {
            t.a -= h.a;
            t.sx -= h.sx;
            t.sy -= h.sy;
            t.ixx -= h.ixx;
            t.iyy -= h.iyy;
            t.ixy -= h.ixy;
            perimeter += h.len;
            hole_area += h.a;
            hole_len += h.len;
        }
        let area = t.a;
        if area <= 1e-15 || !area.is_finite() {
            return SectionProperties::default();
        }
        let (cx, cy) = (t.sx / area, t.sy / area);
        let ix = t.ixx - area * cy * cy;
        let iy = t.iyy - area * cx * cx;
        let ixy = t.ixy - area * cx * cy;
        let (mut ymax, mut xmax) = (0.0f64, 0.0f64);
        for p in &self.outer {
            ymax = ymax.max((p.y - cy).abs());
            xmax = xmax.max((p.x - cx).abs());
        }
        let div = |a: f64, b: f64| if b > 1e-15 { a / b } else { 0.0 };
        let j = if perimeter <= 1e-15 {
            0.0
        } else if hole_len > 0.0 {
            // Closed section: mid-wall enclosed area and length, mean thickness.
            let gross = area + hole_area;
            let am = (gross + hole_area) / 2.0;
            let s = (outer.len + hole_len) / 2.0;
            div(4.0 * am * am * (area / s), s)
        } else {
            let te = 2.0 * area / perimeter;
            perimeter * te.powi(3) / 6.0
        };
        SectionProperties {
            area,
            centroid: [cx, cy],
            ix,
            iy,
            ixy,
            wx: div(ix, ymax),
            wy: div(iy, xmax),
            rx: (ix.max(0.0) / area).sqrt(),
            ry: (iy.max(0.0) / area).sqrt(),
            j,
            perimeter,
            linear_mass: area * density,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::structural_profile::ProfileShape;

    #[test]
    fn rectangle_closed_forms() {
        let p = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 }.outline().unwrap().properties(7850.0);
        assert!((p.area - 0.08).abs() < 1e-12);
        assert!((p.ix - 0.2 * 0.4f64.powi(3) / 12.0).abs() < 1e-12);
        assert!((p.iy - 0.4 * 0.2f64.powi(3) / 12.0).abs() < 1e-12);
        assert!((p.wx - 0.2 * 0.4 * 0.4 / 6.0).abs() < 1e-12);
        assert!((p.linear_mass - 0.08 * 7850.0).abs() < 1e-9);
        assert!(p.centroid[0].abs() < 1e-12 && p.centroid[1].abs() < 1e-12);
        assert!((p.perimeter - 1.2).abs() < 1e-12);
    }

    #[test]
    fn circle_and_tube() {
        let p = ProfileShape::Round { d: 0.3 }.outline().unwrap().properties(0.0);
        let exact = std::f64::consts::PI * 0.15f64.powi(4) / 4.0;
        assert!((p.ix - exact).abs() / exact < 0.02);
        let t = ProfileShape::RectTube { h: 0.1, b: 0.1, t: 0.005, ro: 0.0 }.outline().unwrap().properties(0.0);
        let exact = (0.1f64.powi(4) - 0.09f64.powi(4)) / 12.0;
        assert!((t.ix - exact).abs() / exact < 1e-9);
        assert!(t.j > 0.0);
    }

    #[test]
    fn i_section_against_hand_calculation() {
        let (d, bf, tw, tf) = (0.2, 0.1, 0.0056, 0.0085);
        let p = ProfileShape::I { d, bf, tw, tf, r: 0.0 }.outline().unwrap().properties(0.0);
        let hw = d - 2.0 * tf;
        let ix = bf * d.powi(3) / 12.0 - (bf - tw) * hw.powi(3) / 12.0;
        assert!((p.ix - ix).abs() / ix < 1e-9);
    }

    #[test]
    fn angle_centroid_is_off_center() {
        let p = ProfileShape::Angle { h: 0.1, b: 0.1, t: 0.01, r: 0.0 }.outline().unwrap().properties(0.0);
        assert!(p.centroid[0] < 0.0 && p.centroid[1] < 0.0);
        assert!(p.ixy.abs() > 0.0);
    }

    #[test]
    fn degenerate_outline_and_bad_density_are_safe() {
        let empty = crate::structural_profile::SectionOutline::default();
        assert_eq!(empty.properties(7850.0), Default::default());
        let r = ProfileShape::Rect { b: 0.1, h: 0.1, chamfer: 0.0 }.outline().unwrap();
        assert_eq!(r.properties(f64::NAN).linear_mass, 0.0);
    }
}
