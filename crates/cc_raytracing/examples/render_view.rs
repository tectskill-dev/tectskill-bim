//! Render a 3D view of a project to an image file.
//!
//! `cargo run -p cc_raytracing --release --example render_view -- <file.ccad> <out.png> [view name] [samples] [width height]`
//!
//! The view is the first 3D view unless named; samples and size default to
//! the view's render settings. A camera that was never moved is fitted to
//! the model first, as the 3D view does. `.exr` and `.hdr` outputs hold
//! linear light.

use cc_core::{CameraState, Project, ViewKind};
use cc_geom::ModelGeometry;
use cc_raytracing::{Control, RenderScene, run};
use std::path::Path;

/// The geometry a view shows: its phase and design options, as the
/// engine's session builds it.
fn view_geometry(project: &Project, view: &cc_core::View, full: ModelGeometry) -> ModelGeometry {
    if !view.kind.is_phased() {
        return full;
    }
    let phase = project.view_phase(view).and_then(|ph| project.phase_index(ph)).filter(|i| project.phase_differs(*i));
    let options = project.shown_options(Some(view));
    if phase.is_none() && options == project.shown_options(None) {
        return full;
    }
    ModelGeometry::build_view(project, phase, &options, &full)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("usage: render_view <file.ccad> <out.png> [view name] [samples] [width height]");
        std::process::exit(2);
    };
    let (project, repairs) = cc_core::from_ron_with_repairs(&std::fs::read_to_string(input)?)?;
    if repairs > 0 {
        eprintln!("{repairs} repairs on load");
    }
    let name = args.get(2).filter(|n| !n.is_empty());
    let view = project
        .views
        .values()
        .filter(|v| matches!(v.kind, ViewKind::ThreeD { .. }))
        .find(|v| name.is_none_or(|n| v.name == *n))
        .ok_or("no such 3D view")?;
    let ViewKind::ThreeD { camera, .. } = &view.kind else { return Err("not a 3D view".into()) };

    let mut settings = view.render.clone().unwrap_or_default();
    if let Some(samples) = args.get(3).and_then(|s| s.parse().ok()) {
        settings.samples = samples;
    }
    if let (Some(w), Some(h)) = (args.get(4).and_then(|s| s.parse().ok()), args.get(5).and_then(|s| s.parse().ok())) {
        (settings.width, settings.height) = (w, h);
    }

    let geom = view_geometry(&project, view, ModelGeometry::build(&project));
    let aspect = f64::from(settings.width) / f64::from(settings.height.max(1));
    let camera = if *camera == CameraState::default() {
        cc_raytracing::camera::fit(camera, &geom, settings.fov_deg, aspect)
    } else {
        *camera
    };

    let start = std::time::Instant::now();
    let scene = RenderScene::build(&project, &geom, view, &camera, &settings, None)?;
    eprintln!(
        "\"{}\": {} triangles, {} textures, {}x{}, built in {:.2?}",
        view.name,
        scene.triangles(),
        scene.textures(),
        scene.width(),
        scene.height(),
        start.elapsed()
    );
    let outcome = run(&scene, None, |p| {
        if p.samples().is_power_of_two() || p.is_done() {
            eprintln!("{:5}/{} samples  {:.1?}", p.samples(), p.total_samples(), p.elapsed());
        }
        Control::Continue
    })?;
    outcome.image.save(Path::new(output), scene.exposure(), scene.tone_map())?;
    eprintln!("wrote {output} ({} samples) in {:.2?}", outcome.samples, start.elapsed());
    Ok(())
}
