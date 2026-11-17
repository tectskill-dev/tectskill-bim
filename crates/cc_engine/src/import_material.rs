//! Materials of imported models (SketchUp, OBJ, glTF): name, color,
//! opacity and texture. A texture's picture becomes the material's
//! appearance texture, repeating every so many meters: the model's own
//! texture coordinates cannot be kept (textures are laid on each face's
//! plane), so the repeat comes from them — the meters one unit of the
//! coordinates spans on the faces, see [`UvScale`]. The material's color is
//! its color times the picture's average, so it looks right where textures
//! are not drawn.

use cc_core::material::{MAX_TEXTURE_SIZE, MaterialTexture};
use cc_core::raster::RasterImage;
use glam::{DVec2, DVec3};

/// Smallest repeat a texture gets, meters.
const MIN_REPEAT: f64 = 0.001;
/// Side the picture is reduced to for its average color.
const AVERAGE_SIDE: u32 = 64;

/// A material a model uses.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedMaterial {
    pub name: String,
    /// sRGB.
    pub color: [u8; 3],
    /// 1 opaque, 0 invisible.
    pub opacity: f64,
    pub texture: Option<MaterialTexture>,
}

impl ImportedMaterial {
    /// A plain, opaque material.
    pub fn plain(name: impl Into<String>, color: [u8; 3]) -> Self {
        Self { name: name.into(), color, opacity: 1.0, texture: None }
    }

    /// The project material.
    pub fn to_material(&self) -> cc_core::Material {
        let mut m = cc_core::Material::new(self.name.clone(), self.color);
        let opacity = if self.opacity.is_finite() { self.opacity.clamp(0.0, 1.0) } else { 1.0 };
        // Not quite invisible: a face nobody can see is no use.
        m.graphics.transparency = (1.0 - opacity).min(0.9);
        m.appearance.texture = self.texture.clone();
        m
    }
}

/// An image file usable as a texture (a picture format this reads, within
/// the texture limits).
pub fn texture_image(bytes: Vec<u8>) -> Option<RasterImage> {
    let image = cc_views::raster::probe(bytes)?;
    MaterialTexture::new(image.clone(), 1.0, 1.0).image_ok().then_some(image)
}

/// sRGB byte to linear 0..1.
pub fn to_linear(c: u8) -> f64 {
    let s = f64::from(c) / 255.0;
    if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// Linear 0..1 to an sRGB byte (200 when not a number).
pub fn to_srgb(v: f64) -> u8 {
    if !v.is_finite() {
        return 200;
    }
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// The average color of a picture (linear, weighted by alpha), or `None`
/// when it cannot be decoded or is wholly clear.
pub fn average_color(image: &RasterImage) -> Option<[f64; 3]> {
    let rgba = cc_views::raster::decode(image, AVERAGE_SIDE)?;
    let (mut sum, mut weight) = ([0.0; 3], 0.0);
    for px in rgba.pixels.as_chunks::<4>().0 {
        let a = f64::from(px[3]) / 255.0;
        for k in 0..3 {
            sum[k] += to_linear(px[k]) * a;
        }
        weight += a;
    }
    (weight > 0.0).then(|| sum.map(|s| s / weight))
}

/// A material with color `factor` (linear) times the average of `image`,
/// which becomes its texture repeating every `repeat` meters (1 m when the
/// model gives no texture coordinates).
pub fn textured(
    name: String,
    factor: [f64; 3],
    opacity: f64,
    image: Option<RasterImage>,
    repeat: Option<(f64, f64)>,
) -> ImportedMaterial {
    let average = image.as_ref().and_then(average_color).unwrap_or([1.0; 3]);
    let color = [0, 1, 2].map(|k| to_srgb(factor[k] * average[k]));
    let (width, height) = repeat.unwrap_or((1.0, 1.0));
    let texture = image.map(|i| MaterialTexture::new(i, width, height));
    ImportedMaterial { name, color, opacity, texture }
}

/// The meters a unit of texture coordinates spans on the faces, along u and
/// v, averaged over triangles by their area.
#[derive(Debug, Clone, Copy, Default)]
pub struct UvScale {
    u: f64,
    v: f64,
    area: f64,
}

impl UvScale {
    /// Add a triangle with its corners' texture coordinates.
    pub fn add(&mut self, p: [DVec3; 3], uv: [DVec2; 3]) {
        let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
        let (d1, d2) = (uv[1] - uv[0], uv[2] - uv[0]);
        let det = d1.perp_dot(d2);
        let area = e1.cross(e2).length() / 2.0;
        if det.abs() < 1e-12 || area.is_nan() || area <= 0.0 || !det.is_finite() {
            return;
        }
        // The change of position along u and along v.
        let du = (e1 * d2.y - e2 * d1.y) / det;
        let dv = (e2 * d1.x - e1 * d2.x) / det;
        let (lu, lv) = (du.length(), dv.length());
        if lu.is_finite() && lv.is_finite() {
            self.u += lu * area;
            self.v += lv * area;
            self.area += area;
        }
    }

    /// The repeat (width, height) in meters, when any triangle had coordinates.
    pub fn repeat(&self) -> Option<(f64, f64)> {
        let clamp = |x: f64| x.clamp(MIN_REPEAT, MAX_TEXTURE_SIZE);
        (self.area > 0.0).then(|| (clamp(self.u / self.area), clamp(self.v / self.area)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_round_trips() {
        for c in [0u8, 1, 10, 100, 128, 200, 255] {
            assert_eq!(to_srgb(to_linear(c)), c);
        }
        assert_eq!(to_srgb(f64::NAN), 200);
    }

    #[test]
    fn uv_scale_is_meters_per_repeat() {
        let mut s = UvScale::default();
        // A 2 x 4 m rectangle in a wall (x, z), the texture repeated twice across and once up.
        let p = [DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 4.0)];
        let uv = [DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0)];
        s.add(p, uv);
        let (w, h) = s.repeat().unwrap();
        assert!((w - 1.0).abs() < 1e-12 && (h - 4.0).abs() < 1e-12, "{w} {h}");
        // Degenerate coordinates add nothing.
        let mut empty = UvScale::default();
        empty.add(p, [DVec2::ZERO; 3]);
        assert_eq!(empty.repeat(), None);
    }
}
