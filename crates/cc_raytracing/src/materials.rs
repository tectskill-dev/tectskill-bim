//! The path tracer's materials, made from the model's.
//!
//! Each face is shaded by a [`Surface`]: what the 3D view's textures style
//! shows of its material (or of the paint on it), after the view's graphics
//! (a color override, halftone, transparency). Its appearance color is the
//! base color, its [`Finish`] sets how rough and shiny it is (table in
//! [`Surface::material`]), metals (material class `Metal`) are metallic, and
//! its texture, decoded once per image ([`Textures`]), replaces the color.
//!
//! Glass is a material of class `Glass`, or one at least
//! [`GLASS_TRANSPARENCY`] transparent: it becomes thin glass (light goes
//! straight through, the pane reflects like a dielectric and shadow rays
//! cross it), so the sun enters rooms through windows. Masses, which the
//! model always draws see-through, and any other transparent material keep
//! their transparency as partial coverage instead.

use cc_core::material::{Finish, MaterialClass, MaterialTexture};
use cc_core::raster::RasterImage;
use glam::Vec3;
use sort_raytracing::material::{AlphaMode, Material, Texture, TextureMap};
use std::sync::Arc;

/// A material at least this transparent (0 to 1) that is not of class glass
/// renders as glass all the same.
pub const GLASS_TRANSPARENCY: f64 = 0.5;
/// How much of a glass material's color tints the light through it: clear
/// panes read as their color in the views, but a pane that absorbed that
/// much would darken a room.
pub const GLASS_TINT: f32 = 0.25;
/// The most textures a render decodes; the materials past them show their color.
pub const MAX_TEXTURES: usize = 64;
/// The longest side of a decoded texture, pixels (larger images are reduced).
pub const MAX_TEXTURE_SIDE: u32 = 2048;
/// The base color of clay renders (sRGB): a white matte of about 0.8 in
/// linear light, lit by the sky and the sun like any other surface.
pub const CLAY_COLOR: [u8; 3] = [231, 231, 231];
/// Coverage steps of a [`Surface`]'s opacity (1000 is opaque).
const OPACITY_STEPS: f64 = 1000.0;

/// How a face is shaded: the recipe of one path tracer material.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Surface {
    /// Base color, sRGB.
    pub color: [u8; 3],
    pub finish: Finish,
    pub metal: bool,
    /// Thin glass.
    pub glass: bool,
    /// How much of each point is there, in thousandths (1000: opaque).
    pub opacity: u16,
    /// The texture replacing the color, an index into [`Textures`].
    pub texture: Option<usize>,
}

/// The view graphics of an element's solids, as the 3D view applies them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Style {
    pub color: Option<[u8; 3]>,
    /// 0 (opaque) to 1.
    pub transparency: Option<f64>,
    /// Faces mixed with white.
    pub halftone: bool,
}

impl Style {
    /// How a resolved graphic override shows on surfaces (as the 3D view
    /// reads it): the surface fill's color (white for a fill of no pattern),
    /// transparency and halftone.
    pub fn of(g: &cc_core::GraphicOverride) -> Self {
        let fill = g.surface_fill;
        let color = match fill.pattern {
            Some(cc_core::Pattern::None) => Some([255, 255, 255]),
            _ => fill.color,
        };
        Self { color, transparency: g.transparency, halftone: g.halftone == Some(true) }
    }
}

/// The decoded textures of a render, one per image.
#[derive(Default)]
pub struct Textures {
    /// Images tried, with their texture (`None`: could not be decoded).
    tried: Vec<(Arc<RasterImage>, Option<usize>)>,
    decoded: Vec<Arc<Texture>>,
}

impl Textures {
    /// The index of `texture`'s image, decoded on first use; `None` when it
    /// cannot be decoded or [`MAX_TEXTURES`] are taken.
    pub fn index(&mut self, texture: &MaterialTexture) -> Option<usize> {
        if let Some((_, index)) =
            self.tried.iter().find(|(image, _)| Arc::ptr_eq(image, &texture.image) || **image == *texture.image)
        {
            return *index;
        }
        let index = (self.decoded.len() < MAX_TEXTURES && texture.image_ok())
            .then(|| decode(&texture.image))
            .flatten()
            .map(|t| {
                self.decoded.push(Arc::new(t));
                self.decoded.len() - 1
            });
        self.tried.push((texture.image.clone(), index));
        index
    }

    /// The decoded texture `index`.
    pub fn get(&self, index: usize) -> Option<&Arc<Texture>> {
        self.decoded.get(index)
    }

    pub fn len(&self) -> usize {
        self.decoded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.decoded.is_empty()
    }
}

/// `image` decoded as an sRGB texture, at most [`MAX_TEXTURE_SIDE`] wide and high.
fn decode(image: &RasterImage) -> Option<Texture> {
    let rgba = cc_views::raster::decode(image, MAX_TEXTURE_SIDE)?;
    let (width, height) = (rgba.width as usize, rgba.height as usize);
    let (texels, rest) = rgba.pixels.as_chunks::<4>();
    // The tracer indexes the texels by row and column: the sizes must agree.
    if width == 0 || height == 0 || !rest.is_empty() || texels.len() != width.checked_mul(height)? {
        return None;
    }
    Some(Texture { width, height, data: texels.to_vec(), srgb: true })
}

/// An sRGB channel as linear light.
pub fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// An sRGB color as linear light.
pub fn linear(c: [u8; 3]) -> Vec3 {
    Vec3::new(srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]))
}

/// What a face of a material shows, as the 3D view's textures style shows it.
pub struct SurfaceInput<'a> {
    /// The material's name (the solid's own, or the paint's).
    pub material: Option<&'a str>,
    /// The appearance color (when the project has no such material).
    pub appearance: [u8; 3],
    /// The transparency the model gives the face (0 to 1).
    pub transparency: f64,
    /// A mass: kept see-through, never glass.
    pub mass: bool,
}

impl Surface {
    /// The surface of a face with `input`, in an element of view graphics
    /// `style`; `clay` makes every opaque surface [`CLAY_COLOR`] and matte,
    /// and glass clear.
    pub fn of(
        project: &cc_core::Project,
        input: &SurfaceInput,
        style: &Style,
        clay: bool,
        textures: &mut Textures,
    ) -> Self {
        let material = input.material.and_then(|name| project.material(name));
        let class = material.map(|m| m.identity.class);
        let finite = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        let own = finite(input.transparency);
        let glass = !input.mass && (class == Some(MaterialClass::Glass) || own >= GLASS_TRANSPARENCY);
        // Glass lets light through by its transmission; transparency then
        // only comes from the view's graphics.
        let transparency = match (style.transparency, glass) {
            (Some(t), _) => finite(t),
            (None, true) => 0.0,
            (None, false) => own,
        };
        let opacity = ((1.0 - transparency) * OPACITY_STEPS).round().clamp(0.0, OPACITY_STEPS) as u16;
        if clay {
            let color = if glass { [255, 255, 255] } else { CLAY_COLOR };
            return Self { color, finish: Finish::Matte, metal: false, glass, opacity, texture: None };
        }
        let appearance = material.map_or(input.appearance, |m| m.appearance.color);
        let mut color = style.color.unwrap_or(appearance);
        if style.halftone {
            color = color.map(|v| (f32::from(v) + (255.0 - f32::from(v)) * 0.66).round() as u8);
        }
        let textured = style.color.is_none() && !style.halftone && !glass;
        let texture = material.and_then(|m| m.appearance.texture.as_ref()).filter(|_| textured);
        Self {
            color,
            finish: material.map_or(Finish::Matte, |m| m.appearance.finish),
            metal: class == Some(MaterialClass::Metal),
            glass,
            opacity,
            texture: texture.and_then(|t| textures.index(t)),
        }
    }

    /// A matte surface of `color` (the ground).
    pub fn matte(color: [u8; 3]) -> Self {
        Self { color, finish: Finish::Matte, metal: false, glass: false, opacity: 1000, texture: None }
    }

    /// The path tracer's material. Finishes, as roughness / specular
    /// (0.5 is a reflectance of 4 % head on) / clear coat:
    ///
    /// | finish   | dielectric          | metal (metallic 1) |
    /// |----------|---------------------|--------------------|
    /// | matte    | 0.90 / 0.25 / none  | roughness 0.65     |
    /// | satin    | 0.55 / 0.50 / none  | roughness 0.40     |
    /// | glossy   | 0.30 / 0.50 / 0.4   | roughness 0.22     |
    /// | polished | 0.12 / 0.50 / 1.0   | roughness 0.06     |
    ///
    /// Glass is thin, fully transmissive, smooth, of index 1.5, tinted by
    /// [`GLASS_TINT`] of its color. A texture multiplies a white base.
    pub fn material(&self, textures: &Textures) -> Material {
        let base = linear(self.color);
        let mut m = Material { base_color: base, double_sided: true, ..Material::default() };
        if self.glass {
            m.base_color = Vec3::ONE - (Vec3::ONE - base) * GLASS_TINT;
            m.transmission = 1.0;
            m.thin = true;
            m.roughness = 0.0;
            m.specular = 0.5;
            m.ior = 1.5;
        } else if self.metal {
            m.metallic = 1.0;
            m.specular = 0.5;
            m.roughness = match self.finish {
                Finish::Matte => 0.65,
                Finish::Satin => 0.40,
                Finish::Glossy => 0.22,
                Finish::Polished => 0.06,
            };
        } else {
            let (roughness, specular, coat) = match self.finish {
                Finish::Matte => (0.90, 0.25, 0.0),
                Finish::Satin => (0.55, 0.50, 0.0),
                Finish::Glossy => (0.30, 0.50, 0.4),
                Finish::Polished => (0.12, 0.50, 1.0),
            };
            m.roughness = roughness;
            m.specular = specular;
            m.clearcoat = coat;
            m.clearcoat_gloss = 0.9;
        }
        if let Some(texture) = self.texture.and_then(|i| textures.get(i)) {
            m.base_color = Vec3::ONE;
            m.base_color_tex = Some(TextureMap { texture: texture.clone(), transform: None });
        }
        if self.opacity < 1000 {
            m.alpha = f32::from(self.opacity) / OPACITY_STEPS as f32;
            m.alpha_mode = AlphaMode::Blend;
        }
        m
    }
}

/// The texture coordinates of a point with plane coordinates `uv` (meters,
/// see [`cc_geom::plane_axes`]) for `texture`, as the 3D view lays them: in
/// repeats, rows running down the image so its up is the face's up.
pub fn texture_uv(texture: &MaterialTexture, uv: [f64; 2]) -> [f64; 2] {
    let [x, y] = texture.map(uv);
    [x, -y]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 x 2 PNG of `rgb`.
    pub(crate) fn png(rgb: [u8; 3]) -> RasterImage {
        let mut bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        RasterImage { format: cc_core::raster::RasterFormat::Png, width: 2, height: 2, data: bytes }
    }

    fn input(material: &str) -> SurfaceInput<'_> {
        SurfaceInput { material: Some(material), appearance: [0, 0, 0], transparency: 0.0, mass: false }
    }

    #[test]
    fn glass_masses_and_overrides() {
        let mut p = cc_core::Project::from_template();
        let glass = p.materials.iter().find(|m| m.identity.class == MaterialClass::Glass).unwrap().name.clone();
        let brick = p.materials.first().unwrap().name.clone();
        let mut t = Textures::default();
        let s = Surface::of(&p, &SurfaceInput { transparency: 0.6, ..input(&glass) }, &Style::default(), false, &mut t);
        assert!(s.glass && s.opacity == 1000);
        let m = s.material(&t);
        assert!(m.thin && m.transmission == 1.0 && m.base_color.min_element() > 0.7);
        // A mass of glass stays see-through.
        let mass = SurfaceInput { transparency: 0.6, mass: true, ..input(&glass) };
        let s = Surface::of(&p, &mass, &Style::default(), false, &mut t);
        assert!(!s.glass && s.opacity == 400);
        assert_eq!(s.material(&t).alpha_mode, AlphaMode::Blend);
        // Overrides: color, halftone, transparency.
        let red = Style { color: Some([255, 0, 0]), transparency: Some(0.25), halftone: false };
        let s = Surface::of(&p, &input(&brick), &red, false, &mut t);
        assert_eq!((s.color, s.opacity), ([255, 0, 0], 750));
        let s = Surface::of(&p, &input(&brick), &Style { halftone: true, ..Style::default() }, false, &mut t);
        assert!(s.color.iter().all(|c| *c > 150));
        // Clay: one matte color, glass kept clear.
        let s = Surface::of(&p, &input(&brick), &Style::default(), true, &mut t);
        assert_eq!((s.color, s.finish, s.texture), (CLAY_COLOR, Finish::Matte, None));
        // Metals and finishes.
        let steel = p.materials.iter_mut().find(|m| m.identity.class == MaterialClass::Metal).unwrap();
        steel.appearance.finish = Finish::Polished;
        let steel = steel.name.clone();
        let s = Surface::of(&p, &input(&steel), &Style::default(), false, &mut t);
        let m = s.material(&t);
        assert!(m.metallic == 1.0 && m.roughness < 0.1);
    }

    #[test]
    fn textures_are_decoded_once_and_broken_ones_show_the_color() {
        let mut p = cc_core::Project::from_template();
        let image = Arc::new(png([10, 200, 10]));
        for m in p.materials.iter_mut().take(2) {
            m.appearance.texture =
                Some(MaterialTexture { image: image.clone(), ..MaterialTexture::new(png([0; 3]), 1.0, 1.0) });
        }
        let mut bad = png([1, 2, 3]);
        bad.data.truncate(20);
        p.materials[2].appearance.texture = Some(MaterialTexture::new(bad, 1.0, 1.0));
        let names: Vec<String> = p.materials.iter().take(3).map(|m| m.name.clone()).collect();
        let mut t = Textures::default();
        let surfaces: Vec<Surface> =
            names.iter().map(|n| Surface::of(&p, &input(n), &Style::default(), false, &mut t)).collect();
        assert_eq!(t.len(), 1);
        assert_eq!((surfaces[0].texture, surfaces[1].texture, surfaces[2].texture), (Some(0), Some(0), None));
        let m = surfaces[0].material(&t);
        assert_eq!(m.base_color, Vec3::ONE);
        let texel = m.base_color_tex.unwrap().sample(glam::Vec2::new(0.25, 0.25));
        assert!(texel.y > 0.5 && texel.x < 0.05);
        // A color override hides the texture, as in the 3D view.
        let red = Style { color: Some([255, 0, 0]), ..Style::default() };
        assert_eq!(Surface::of(&p, &input(&names[0]), &red, false, &mut t).texture, None);
    }

    #[test]
    fn texture_coordinates_follow_the_viewport() {
        let t = MaterialTexture::new(png([0; 3]), 0.5, 2.0);
        // Half a repeat across, a quarter up the face: v runs down the image.
        assert_eq!(texture_uv(&t, [0.25, 0.5]), [0.5, -0.25]);
    }
}
