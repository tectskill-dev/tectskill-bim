//! Rendering a [`RenderScene`]: progressively with a callback that sees
//! each step and may stop the render ([`run`]), or to the end
//! ([`render_image`]), and the image it gives ([`Image`]).

use crate::scene::RenderScene;
use crate::{RenderError, guarded};
use cc_core::ToneMap;
use sort_raytracing::render::{Progressive, Region as TracerRegion};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a step of [`run`] should take once the render has warmed up:
/// the callback sees the image (and may stop it) about this often.
const STEP_TIME: Duration = Duration::from_millis(250);
/// The most samples per pixel one step of [`run`] adds.
const MAX_BATCH: u32 = 256;

/// A rectangle of the image, in pixels from its top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// What the callback of [`run`] says after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Render more samples.
    Continue,
    /// Stop: [`run`] returns the image so far.
    Stop,
}

/// A render in progress, as the callback of [`run`] sees it after a step.
pub struct Progress<'a> {
    render: &'a Progressive<'a>,
    width: u32,
    height: u32,
    elapsed: Duration,
}

impl Progress<'_> {
    /// Samples per pixel rendered so far.
    pub fn samples(&self) -> u32 {
        self.render.samples()
    }

    /// Samples per pixel of the finished render.
    pub fn total_samples(&self) -> u32 {
        self.render.total_samples()
    }

    pub fn is_done(&self) -> bool {
        self.render.is_done()
    }

    /// Time spent rendering so far.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// The part of the image being rendered (all of it unless a region was given).
    pub fn region(&self) -> Region {
        let r = self.render.region();
        Region { x: r.x, y: r.y, width: r.width, height: r.height }
    }

    /// The whole image so far, black outside the region.
    pub fn image(&self) -> Image {
        Image::from_tracer(self.width, self.height, &self.render.image())
    }

    /// The region alone so far, as an image of its size.
    pub fn region_image(&self) -> Image {
        let r = self.region();
        Image::from_tracer(r.width, r.height, &self.render.region_pixels())
    }
}

/// How a [`run`] ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The whole image (black outside the region, if one was given).
    pub image: Image,
    /// Samples per pixel rendered.
    pub samples: u32,
    /// The callback stopped the render before the last sample.
    pub stopped: bool,
}

/// Render `scene` (the `region` of its image only, when given: the rest is
/// never traced and stays black), a few samples per pixel at a time, calling
/// `on_step` after each step until the render is done or the callback says
/// [`Control::Stop`]. The first steps add one sample, so the first picture
/// comes fast; later ones grow so a step takes about a quarter of a second.
/// Blocking: an application runs it on a thread of its own and stops it
/// from the callback (an atomic flag the callback reads). A panic of the
/// path tracer comes back as [`RenderError::Internal`].
pub fn run(
    scene: &RenderScene,
    region: Option<Region>,
    mut on_step: impl FnMut(&Progress<'_>) -> Control,
) -> Result<Outcome, RenderError> {
    let (width, height) = (scene.width(), scene.height());
    let region = match region {
        Some(r) => TracerRegion { x: r.x, y: r.y, width: r.width, height: r.height },
        None => TracerRegion { x: 0, y: 0, width, height },
    };
    let mut render = guarded("starting the render", || {
        Progressive::with_region(&scene.scene, &scene.camera, &scene.tracer, region)
    })?;
    let start = Instant::now();
    let mut batch = 1u32;
    let mut stopped = false;
    loop {
        let step_start = Instant::now();
        let added = guarded("rendering", || render.step_by(batch))?;
        if added == 0 {
            break;
        }
        let progress = Progress { render: &render, width, height, elapsed: start.elapsed() };
        if on_step(&progress) == Control::Stop {
            stopped = !render.is_done();
            break;
        }
        if render.is_done() {
            break;
        }
        // Grow the steps (at most doubling) towards `STEP_TIME`.
        let per_sample = step_start.elapsed().as_secs_f64() / f64::from(added);
        let fit = if per_sample > 0.0 { STEP_TIME.as_secs_f64() / per_sample } else { f64::from(MAX_BATCH) };
        batch = (fit.floor().max(1.0) as u32).clamp(1, batch.saturating_mul(2)).min(MAX_BATCH);
    }
    let image = Image::from_tracer(width, height, &render.image());
    Ok(Outcome { image, samples: render.samples(), stopped })
}

/// Render all of `scene` to the last sample.
pub fn render_image(scene: &RenderScene) -> Result<Image, RenderError> {
    run(scene, None, |_| Control::Continue).map(|o| o.image)
}

/// An image of linear radiance (RGB), rows from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
}

/// The filmic curve (Narkowicz's fit of ACES), for values of 0 and more.
fn filmic(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// Linear light (0 to 1) as an sRGB channel.
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// `2^stops`, 1 for a value that is not a number.
fn exposure_scale(stops: f64) -> f32 {
    if stops.is_finite() { 2f64.powf(stops.clamp(-60.0, 60.0)) as f32 } else { 1.0 }
}

impl Image {
    fn from_tracer(width: u32, height: u32, pixels: &[glam::Vec3]) -> Self {
        Self { width, height, pixels: pixels.iter().map(|p| p.to_array()).collect() }
    }

    /// Whether the pixels are as many as the size says (and there are some).
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && u64::try_from(self.pixels.len()).is_ok_and(|n| n == u64::from(self.width) * u64::from(self.height))
    }

    /// The pixels multiplied by `2^exposure_stops`, tone mapped and sRGB
    /// encoded, as RGBA bytes (alpha 255), 4 per pixel. Values that are not
    /// numbers come out black.
    pub fn to_rgba8(&self, exposure_stops: f64, tone_map: ToneMap) -> Vec<u8> {
        let scale = exposure_scale(exposure_stops);
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            for c in p {
                let v = c * scale;
                let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
                let v = match tone_map {
                    ToneMap::Filmic => filmic(v),
                    ToneMap::Standard => v.min(1.0),
                };
                out.push((linear_to_srgb(v) * 255.0 + 0.5).clamp(0.0, 255.0) as u8);
            }
            out.push(255);
        }
        out
    }

    /// Write the image to `path`: `.png`, `.jpg` and `.jpeg` exposed, tone
    /// mapped and sRGB encoded (as [`Image::to_rgba8`]); `.exr` and `.hdr`
    /// in linear light, exposed but not tone mapped.
    pub fn save(&self, path: &Path, exposure_stops: f64, tone_map: ToneMap) -> Result<(), RenderError> {
        if !self.is_valid() {
            return Err(RenderError::InvalidImage(format!(
                "{} pixels for {} x {}",
                self.pixels.len(),
                self.width,
                self.height
            )));
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let write = |e: image::ImageError| RenderError::Write(e.to_string());
        let size = || RenderError::InvalidImage("the pixels do not fill the image".into());
        match ext.as_str() {
            "png" | "jpg" | "jpeg" => {
                let rgba = self.to_rgba8(exposure_stops, tone_map);
                let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|[r, g, b, _]| [*r, *g, *b]).collect();
                image::RgbImage::from_raw(self.width, self.height, rgb).ok_or_else(size)?.save(path).map_err(write)
            }
            "exr" | "hdr" => {
                let scale = exposure_scale(exposure_stops);
                let data: Vec<f32> = self
                    .pixels
                    .iter()
                    .flat_map(|p| p.map(|c| if (c * scale).is_finite() { (c * scale).max(0.0) } else { 0.0 }))
                    .collect();
                let img = image::Rgb32FImage::from_raw(self.width, self.height, data).ok_or_else(size)?;
                image::DynamicImage::ImageRgb32F(img).save(path).map_err(write)
            }
            other => Err(RenderError::UnsupportedFormat(other.to_string())),
        }
    }
}
