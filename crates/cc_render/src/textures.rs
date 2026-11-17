//! Material textures for the 3D view: each material's image decoded once,
//! resized to one square layer of a texture array (with its mipmaps), so
//! the GPU memory they take is bounded ([`LAYER_SIZE`], [`MAX_LAYERS`]).
//! A material whose image cannot be decoded, or past the last layer, shows
//! its appearance color instead.

use cc_core::material::MaterialTexture;
use cc_core::raster::{RasterFormat, RasterImage};
use std::sync::Arc;

/// Side of a layer, pixels.
pub const LAYER_SIZE: u32 = 512;
/// Most textures a view holds at once.
pub const MAX_LAYERS: usize = 16;

/// One decoded texture: its source image (to tell when it changed) and its
/// mip levels, RGBA (sRGB), the first `LAYER_SIZE` square.
#[derive(Debug, Clone)]
struct Layer {
    source: Arc<RasterImage>,
    mips: Vec<Vec<u8>>,
}

/// The textures of a project's materials, ready to upload.
#[derive(Debug, Clone, Default)]
pub struct TextureSet {
    layers: Vec<Layer>,
    /// Per material name: its layer and how its texture is laid out.
    materials: std::collections::HashMap<String, (u32, MaterialTexture)>,
    /// Images that could not be decoded (not tried again).
    failed: Vec<Arc<RasterImage>>,
    /// Changes whenever the layers do (the renderer uploads them again).
    generation: u64,
}

fn same(a: &Arc<RasterImage>, b: &Arc<RasterImage>) -> bool {
    Arc::ptr_eq(a, b) || **a == **b
}

impl TextureSet {
    /// Follow the materials of `project`: images already decoded are kept,
    /// new ones decoded, those no longer used dropped.
    pub fn update(&mut self, project: &cc_core::Project) {
        let mut layers: Vec<Layer> = Vec::new();
        let mut materials = std::collections::HashMap::new();
        let mut changed = false;
        for m in &project.materials {
            let Some(texture) = m.appearance.texture.as_ref().filter(|t| t.image_ok()) else { continue };
            let index = match layers.iter().position(|l| same(&l.source, &texture.image)) {
                Some(i) => i,
                None => {
                    if layers.len() >= MAX_LAYERS || self.failed.iter().any(|f| same(f, &texture.image)) {
                        continue;
                    }
                    let kept = self.layers.iter().position(|l| same(&l.source, &texture.image));
                    let layer = match kept {
                        Some(i) => self.layers.get(i).cloned(),
                        None => {
                            changed = true;
                            let decoded =
                                decode(&texture.image).map(|mips| Layer { source: texture.image.clone(), mips });
                            if decoded.is_none() {
                                self.failed.push(texture.image.clone());
                            }
                            decoded
                        }
                    };
                    let Some(layer) = layer else { continue };
                    layers.push(layer);
                    layers.len() - 1
                }
            };
            materials.insert(m.name.clone(), (index as u32, texture.clone()));
        }
        changed |= layers.len() != self.layers.len()
            || layers.iter().zip(&self.layers).any(|(a, b)| !Arc::ptr_eq(&a.source, &b.source));
        self.failed.retain(|f| {
            project.materials.iter().any(|m| m.appearance.texture.as_ref().is_some_and(|t| same(&t.image, f)))
        });
        self.layers = layers;
        self.materials = materials;
        if changed {
            self.generation = self.generation.wrapping_add(1);
        }
    }

    /// The layer of material `name`'s texture and its layout.
    pub fn of(&self, name: &str) -> Option<(u32, &MaterialTexture)> {
        self.materials.get(name).map(|(i, t)| (*i, t))
    }

    /// How many layers there are.
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Mip level `level` of each layer, in layer order.
    pub(crate) fn level(&self, level: usize) -> impl Iterator<Item = &[u8]> {
        self.layers.iter().map(move |l| l.mips.get(level).map_or(&[][..], Vec::as_slice))
    }
}

/// Mip levels of a layer: [`LAYER_SIZE`] down to 1 pixel.
pub(crate) fn mip_count() -> u32 {
    LAYER_SIZE.ilog2() + 1
}

fn image_format(format: RasterFormat) -> image::ImageFormat {
    match format {
        RasterFormat::Png => image::ImageFormat::Png,
        RasterFormat::Jpeg => image::ImageFormat::Jpeg,
        RasterFormat::Gif => image::ImageFormat::Gif,
        RasterFormat::Bmp => image::ImageFormat::Bmp,
        RasterFormat::Webp => image::ImageFormat::WebP,
    }
}

/// Decode `image` (bounded by its declared size) and make the mip levels of a layer.
fn decode(image: &RasterImage) -> Option<Vec<Vec<u8>>> {
    if !image.is_valid() {
        return None;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&image.data), image_format(image.format));
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(image.width.max(1));
    limits.max_image_height = Some(image.height.max(1));
    limits.max_alloc = Some(u64::from(image.width) * u64::from(image.height) * 8 + (1 << 20));
    reader.limits(limits);
    let decoded = reader.decode().ok()?.into_rgba8();
    let filter = image::imageops::FilterType::Triangle;
    let mut level = image::imageops::resize(&decoded, LAYER_SIZE, LAYER_SIZE, filter);
    let mut mips = Vec::new();
    loop {
        let side = level.width();
        mips.push(level.as_raw().clone());
        if side <= 1 {
            break;
        }
        level = image::imageops::resize(&level, side / 2, side / 2, filter);
    }
    Some(mips)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 x 2 PNG, red.
    fn png() -> RasterImage {
        let mut bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 10, 10, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        RasterImage { format: RasterFormat::Png, width: 2, height: 2, data: bytes }
    }

    #[test]
    fn textures_are_decoded_once_and_bounded() {
        let mut p = cc_core::Project::from_template();
        let image = Arc::new(png());
        for m in p.materials.iter_mut().take(2) {
            m.appearance.texture =
                Some(MaterialTexture { image: image.clone(), ..MaterialTexture::new(png(), 1.0, 1.0) });
        }
        let mut set = TextureSet::default();
        set.update(&p);
        // Two materials, one image: one layer.
        assert_eq!(set.len(), 1);
        let g = set.generation();
        assert_eq!(set.level(0).next().map(<[u8]>::len), Some((LAYER_SIZE * LAYER_SIZE * 4) as usize));
        assert_eq!(set.level(mip_count() as usize - 1).next().map(<[u8]>::len), Some(4));
        assert!(set.of(&p.materials[1].name).is_some());
        // Nothing changed: nothing to upload again.
        set.update(&p);
        assert_eq!(set.generation(), g);
        // A broken image falls back to the color.
        let mut bad = png();
        bad.data.truncate(20);
        p.materials[1].appearance.texture = Some(MaterialTexture::new(bad, 1.0, 1.0));
        set.update(&p);
        assert!(set.of(&p.materials[1].name).is_none() && set.of(&p.materials[0].name).is_some());
        // More textures than layers: the rest show their color.
        for (i, m) in p.materials.iter_mut().enumerate() {
            let mut img = png();
            img.data.extend_from_slice(&(i as u32).to_le_bytes());
            m.appearance.texture = Some(MaterialTexture::new(img, 1.0, 1.0));
        }
        set.update(&p);
        assert_eq!(set.len(), MAX_LAYERS.min(p.materials.len()));
    }
}
