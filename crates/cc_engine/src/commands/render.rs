//! Photorealistic rendering of 3D views (path tracing, `cc_raytracing`).

use super::views::apply_render;
use crate::error::{EngineError, ErrorCode, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::{Session, guard};
use crate::t;
use cc_core::{CameraState, ViewKind};
use cc_raytracing::{Control, Region, RenderError, RenderScene};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Instant;

/// Render a 3D view photorealistically (path tracing on the processor, no
/// GPU) and write the image: `.png`, `.jpg`/`.jpeg` exposed, tone mapped and
/// sRGB encoded, `.exr` and `.hdr` in linear light. The view's render
/// settings are used (the defaults when it has none), patched by `settings`
/// for this render only; its camera is the one kept in the view, fitted to
/// the model when it was never moved, as the 3D view does. The view is
/// rendered as printed, without temporary hiding. Changes nothing in the
/// model. Returns the path, the image size, the samples per pixel, the
/// seconds spent and the triangles rendered.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewRender {
    /// 3D view id or name.
    pub view: Ref,
    /// Path of the image to write (.png, .jpg, .jpeg, .exr or .hdr).
    pub path: PathBuf,
    /// Render settings for this render only, not kept in the view: the
    /// fields to change over the view's settings (or the defaults when it
    /// has none), shaped and bounded like `render` in `view_set`.
    #[serde(default)]
    pub settings: Option<Value>,
    /// Render only this rectangle of the image, `[x, y, width, height]` in
    /// pixels from its top left corner (inside the image); the rest stays
    /// black.
    #[serde(default)]
    pub region: Option<[u32; 4]>,
}

/// A render failure as an engine error.
fn render_error(e: RenderError) -> EngineError {
    let code = match e {
        RenderError::UnsupportedFormat(_)
        | RenderError::InvalidImage(_)
        | RenderError::InvalidCamera(_)
        | RenderError::TooManyTriangles { .. } => ErrorCode::InvalidArguments,
        RenderError::Write(_) => ErrorCode::Io,
        RenderError::Internal(_) => ErrorCode::Internal,
    };
    EngineError::new(code, e.to_string())
}

/// The image formats written.
const FORMATS: [&str; 5] = ["png", "jpg", "jpeg", "exr", "hdr"];

impl Command for ViewRender {
    const NAME: &'static str = "view_render";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::view(s.project(), &self.view)?;
        let view = s.project().view(id).ok_or_else(|| EngineError::not_found("view"))?;
        let ViewKind::ThreeD { camera, .. } = &view.kind else {
            return Err(EngineError::invalid(t!("msg-render-view")));
        };
        if !view.kind.takes_render() {
            return Err(EngineError::invalid(t!("msg-render-view")));
        }
        let camera = *camera;
        let settings = match &self.settings {
            Some(patch) => apply_render(view.render.as_ref(), patch)?,
            None => view.render.clone().unwrap_or_default(),
        };
        // Refused before any work, as the image would be.
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !FORMATS.contains(&ext.as_str()) {
            return Err(render_error(RenderError::UnsupportedFormat(ext)));
        }
        let region = match self.region {
            Some([x, y, width, height]) => {
                let inside = width > 0
                    && height > 0
                    && u64::from(x) + u64::from(width) <= u64::from(settings.width)
                    && u64::from(y) + u64::from(height) <= u64::from(settings.height);
                if !inside {
                    return Err(EngineError::invalid(format!(
                        "region [{x}, {y}, {width}, {height}] is not inside the {} x {} image",
                        settings.width, settings.height
                    )));
                }
                Some(Region { x, y, width, height })
            }
            None => None,
        };

        s.view_geometry(id);
        let project = s.project();
        let geom = s.geom_for(id);
        let view = project.view(id).ok_or_else(|| EngineError::not_found("view"))?;
        let camera = if camera == CameraState::default() {
            let aspect = f64::from(settings.width) / f64::from(settings.height.max(1));
            cc_raytracing::camera::fit(&camera, geom, settings.fov_deg, aspect)
        } else {
            camera
        };

        let start = Instant::now();
        let scene =
            guard(|| RenderScene::build(project, geom, view, &camera, &settings, None))?.map_err(render_error)?;
        let outcome = guard(|| cc_raytracing::run(&scene, region, |_| Control::Continue))?.map_err(render_error)?;
        guard(|| outcome.image.save(&self.path, scene.exposure(), scene.tone_map()))?.map_err(render_error)?;
        Ok(json!({
            "path": self.path,
            "width": scene.width(),
            "height": scene.height(),
            "samples": outcome.samples,
            "seconds": start.elapsed().as_secs_f64(),
            "triangles": scene.triangles(),
        }))
    }
}
