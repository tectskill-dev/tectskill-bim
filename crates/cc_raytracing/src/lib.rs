//! Photorealistic rendering of a 3D view: path tracing on the pure Rust
//! `sort-raytracing` library, with no GPU.
//!
//! [`RenderScene::build`] turns the solids a view shows (its visibility,
//! phase and design options through `cc_views`' graphics resolver, its
//! detail level, painted faces, material textures laid as the 3D view's
//! textures style lays them, smooth normals across curved facets) into the
//! path tracer's triangles and materials ([`materials`]), lights them with a
//! small generated sky, ground and sun ([`environment`]) and places the
//! camera the viewport has ([`camera`]). [`run`] then refines the picture a
//! few samples at a time, handing each step to a callback that may stop it
//! (an application shows it while it converges); [`render_image`] renders it
//! to the end. An [`Image`] holds linear radiance, which
//! [`Image::to_rgba8`] exposes, tone maps and encodes for display and
//! [`Image::save`] writes as PNG or JPEG (or linear as EXR or HDR).
//!
//! The model is Z up; the path tracer is Y up (its sky's poles are on its Y
//! axis), so every point and direction goes through [`space`]: `(x, y, z)`
//! becomes `(x, z, -y)`, a rotation, so windings and normals are kept. The
//! scene is also moved so the model's center is at the origin, which keeps
//! the path tracer's `f32` precise for models far from the project origin.
//!
//! The library may panic on input it does not expect; every call into it is
//! made inside `catch_unwind`, and a panic comes back as
//! [`RenderError::Internal`]. Input is checked before that: non-finite or
//! degenerate triangles are dropped, cameras and settings validated, and
//! scenes past [`MAX_TRIANGLES`] refused.

pub mod camera;
pub mod environment;
pub mod materials;
mod render;
mod scene;
pub mod space;
#[cfg(test)]
mod tests;

pub use render::{Control, Image, Outcome, Progress, Region, render_image, run};
pub use scene::{MAX_TRIANGLES, RenderScene};

/// What went wrong building or rendering a scene, or writing its image.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RenderError {
    /// The view shows more triangles than a render takes.
    #[error("the scene has more than {limit} triangles, the most a render takes")]
    TooManyTriangles { limit: usize },
    /// The camera has values that are not numbers.
    #[error("the camera cannot be used: {0}")]
    InvalidCamera(String),
    /// The image or region asked for cannot be rendered or written.
    #[error("invalid image: {0}")]
    InvalidImage(String),
    /// An image file of a kind that is not written.
    #[error("unsupported image format \"{0}\" (use png, jpg, exr or hdr)")]
    UnsupportedFormat(String),
    /// Writing the image file failed.
    #[error("could not write the image: {0}")]
    Write(String),
    /// The path tracer failed (it panicked); the message says where.
    #[error("the renderer failed: {0}")]
    Internal(String),
}

/// Run `f`, turning a panic into [`RenderError::Internal`].
pub(crate) fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, RenderError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        RenderError::Internal(format!("{what}: {detail}"))
    })
}
