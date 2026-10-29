//! Project thumbnails: a small render of the model, stored in the application
//! data folder and shown on the home screen.

use crate::state::AppState;
use cc_core::{Project, ViewKind};
use std::path::{Path, PathBuf};

/// Thumbnail size in pixels (3:2).
pub const SIZE: [u32; 2] = [360, 240];

/// A thumbnail waiting to be rendered (rendering needs the GPU, which only the
/// app loop has).
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// The open document, saved at this path.
    Project(PathBuf),
    /// A bundled sample project.
    Sample(&'static str),
}

fn dir() -> Option<PathBuf> {
    eframe::storage_dir("TectSkill BIM").map(|d| d.join("thumbnails"))
}

/// FNV-1a: stable file names across runs and platforms.
fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

pub fn project_file(path: &Path) -> Option<PathBuf> {
    let key = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    Some(dir()?.join(format!("{:016x}.png", fnv64(key.to_string_lossy().as_bytes()))))
}

pub fn sample_file(name: &str) -> Option<PathBuf> {
    Some(dir()?.join(format!("sample-{name}.png")))
}

/// Render `project` framed whole from its first 3D view's direction.
fn render(project: &Project, rs: &egui_wgpu::RenderState) -> Option<Vec<u8>> {
    let geom = cc_geom::ModelGeometry::build(project);
    geom.bounds()?;
    let mut camera = project
        .views
        .values()
        .find_map(|v| match v.kind {
            ViewKind::ThreeD { camera, .. } => Some(camera),
            _ => None,
        })
        .unwrap_or_default();
    crate::view3d::fit_camera(&mut camera, &geom, SIZE[0] as f32 / SIZE[1] as f32);
    let mut renderer = cc_render::Renderer3D::new(&rs.device);
    renderer.upload(
        &rs.device,
        &cc_render::SceneMesh::build_plain(&cc_render::Tessellation::new(&geom), &Default::default()),
    );
    renderer.render_to_rgba(&rs.device, &rs.queue, SIZE, &camera)
}

fn write_png(path: &Path, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, SIZE[0], SIZE[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// Render pending thumbnails.
pub fn process(state: &mut AppState, rs: &egui_wgpu::RenderState) {
    for job in std::mem::take(&mut state.thumbnail_jobs) {
        let (rgba, file) = match &job {
            Job::Project(path) => (render(state.project(), rs), project_file(path)),
            Job::Sample(name) => {
                let project = crate::home::sample_project(name);
                (project.and_then(|p| render(&p, rs)), sample_file(name))
            }
        };
        let (Some(rgba), Some(file)) = (rgba, file) else { continue };
        if let Err(err) = write_png(&file, &rgba) {
            log::warn!("could not write thumbnail {}: {err}", file.display());
        }
    }
}
