//! The scene of a render: the triangles, materials, lights and camera of a
//! view, owned so a worker thread can render it.

use crate::environment::{self, GROUND_NUDGE, GROUND_RADII, MIN_GROUND_HALF_SIDE};
use crate::materials::{Style, Surface, SurfaceInput, Textures, texture_uv};
use crate::space::{self, Space};
use crate::{RenderError, guarded};
use cc_core::material::MaterialTexture;
use cc_core::{CameraState, Category, ElementId, Project, RenderSettings, TemporaryVisibility, ToneMap, View};
use cc_geom::{ModelGeometry, Solid};
use glam::{DVec3, Vec2, Vec3};
use sort_raytracing::camera::Camera;
use sort_raytracing::render::{Integrator, RenderSettings as TracerSettings};
use sort_raytracing::scene::{Scene, SceneBuilder};
use std::collections::HashMap;

/// The most triangles a render takes (about a gigabyte of scene); a view
/// showing more is refused.
pub const MAX_TRIANGLES: usize = 5_000_000;
/// The most vertices a render takes.
const MAX_VERTICES: usize = 3 * MAX_TRIANGLES;
/// Twice the area below which a triangle is dropped, square meters.
const MIN_DOUBLE_AREA: f64 = 1e-12;
/// How much glossy surfaces are blurred after a rough bounce (the path
/// tracer's default: it tames caustics into soft light).
const FILTER_GLOSSY: f32 = 0.5;

/// A view ready to render: the path tracer's scene (geometry, materials,
/// lights), its camera and how it is sampled, plus the exposure and tone
/// mapping its image is shown with. It owns everything (`Send` and `Sync`),
/// so a worker thread can take it (or an `Arc` of it) and render it with
/// [`crate::run`].
pub struct RenderScene {
    pub(crate) scene: Scene,
    pub(crate) camera: Camera,
    pub(crate) tracer: TracerSettings,
    exposure: f64,
    tone_map: ToneMap,
    triangles: usize,
    textures: usize,
}

/// The triangles of one surface.
#[derive(Default)]
struct Mesh {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
    indices: Vec<[u32; 3]>,
}

/// The meshes of a scene by surface, in the order they were first met (so
/// a scene is built the same way every time).
#[derive(Default)]
struct Meshes {
    list: Vec<(Surface, Mesh)>,
    index: HashMap<Surface, usize>,
    triangles: usize,
    vertices: usize,
}

impl Meshes {
    /// Where the mesh of `surface` is in the list (added when new).
    fn slot(&mut self, surface: &Surface) -> usize {
        match self.index.get(surface) {
            Some(i) => *i,
            None => {
                self.list.push((surface.clone(), Mesh::default()));
                self.index.insert(surface.clone(), self.list.len() - 1);
                self.list.len() - 1
            }
        }
    }
}

/// What a solid's face shows: its own material or its paint.
struct Part<'a> {
    surface: Surface,
    texture: Option<&'a MaterialTexture>,
}

impl RenderScene {
    /// The scene of `view` seen through `camera` (the 3D view's camera: the
    /// one in the view, or the viewport's live one) with `settings` (brought
    /// into range first). `geom` is the geometry the view shows (its phase
    /// and design options, `Session::view_geometry`), and `temporary` what is
    /// temporarily hidden or isolated in it: the elements the view's graphics
    /// hide are left out, its overrides of color, transparency and halftone
    /// shown as in its textures style. Errors for a camera that is not a
    /// number and a scene of more than [`MAX_TRIANGLES`]; an empty view
    /// gives the sky and the ground.
    pub fn build(
        project: &Project,
        geom: &ModelGeometry,
        view: &View,
        camera: &CameraState,
        settings: &RenderSettings,
        temporary: Option<&TemporaryVisibility>,
    ) -> Result<Self, RenderError> {
        let mut settings = settings.clone();
        settings.repair();
        crate::camera::eye_and_forward(camera)?;

        // The solids the view shows, with their element's graphics.
        let mut resolver = cc_views::graphics::Resolver::new(project, geom, view, temporary);
        let mut styles: HashMap<ElementId, Option<Style>> = HashMap::new();
        let shown: Vec<(&Solid, Style)> = geom
            .solids
            .iter()
            .filter(|s| s.detail.shows(view.detail))
            .filter_map(|s| {
                let style =
                    *styles.entry(s.element).or_insert_with(|| resolver.style(s.element).map(|g| Style::of(&g)));
                style.map(|st| (s, st))
            })
            .collect();
        let bounds = shown
            .iter()
            .flat_map(|(s, _)| s.faces.iter().flat_map(|f| f.verts.iter().copied()))
            .filter(|p| p.is_finite())
            .fold(None, |b: Option<(DVec3, DVec3)>, p| Some(b.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p)))));
        let origin = match bounds {
            Some((lo, hi)) => (lo + hi) * 0.5,
            None => camera.target,
        };
        let space = Space::new(origin);
        let radius = bounds.map_or(1.0, |(lo, hi)| ((hi - lo).length() * 0.5).max(1.0));

        let mut textures = Textures::default();
        let mut meshes = Meshes::default();
        for (solid, style) in &shown {
            add_solid(project, solid, style, settings.clay, &space, &mut textures, &mut meshes)?;
        }

        let sun = environment::sun_state(project, view, camera);
        let env = &settings.environment;
        let ground = env.ground_plane.then(|| {
            let z = environment::ground_height(project, view, bounds.map(|(lo, _)| lo.z)) - GROUND_NUDGE;
            let half = (radius * GROUND_RADII).max(MIN_GROUND_HALF_SIDE);
            let c = space.origin;
            // Counter-clockwise seen from above: facing up.
            [(-half, -half), (half, -half), (half, half), (-half, half)]
                .map(|(dx, dy)| space.point(DVec3::new(c.x + dx, c.y + dy, z)))
        });
        // Clay renders are lit like any other: only the surfaces change.
        let lights = environment::lights(&sun, env);
        let (triangles, texture_count) = (meshes.triangles, textures.len());
        let scene = guarded("building the scene", move || {
            let mut builder = SceneBuilder::new();
            for (surface, mesh) in &meshes.list {
                let material = builder.add_material(surface.material(&textures));
                builder.add_mesh(&mesh.positions, Some(&mesh.normals), Some(&mesh.uvs), None, &mesh.indices, material);
            }
            if let Some(quad) = ground {
                let material = builder.add_material(Surface::matte(env.ground_color).material(&textures));
                builder.add_quad(quad, material);
            }
            for light in lights {
                builder.add_light(light);
            }
            builder.build()
        })?;
        let camera = crate::camera::tracer_camera(camera, settings.fov_deg, settings.width, settings.height, &space)?;
        let tracer = TracerSettings {
            samples_per_pixel: settings.samples,
            max_depth: settings.max_depth,
            clamp: (settings.clamp > 0.0).then_some(settings.clamp as f32),
            filter_glossy: FILTER_GLOSSY,
            seed: 0,
            integrator: Integrator::PathTracing,
        };
        Ok(Self {
            scene,
            camera,
            tracer,
            exposure: settings.exposure,
            tone_map: settings.tone_map,
            triangles,
            textures: texture_count,
        })
    }

    /// Width of the image, pixels.
    pub fn width(&self) -> u32 {
        self.camera.width
    }

    /// Height of the image, pixels.
    pub fn height(&self) -> u32 {
        self.camera.height
    }

    /// Samples per pixel of the finished render.
    pub fn samples(&self) -> u32 {
        self.tracer.samples_per_pixel.max(1)
    }

    /// The exposure the image is shown with, stops.
    pub fn exposure(&self) -> f64 {
        self.exposure
    }

    /// The tone mapping the image is shown with.
    pub fn tone_map(&self) -> ToneMap {
        self.tone_map
    }

    /// Triangles of the model in the scene (the ground plane not counted).
    pub fn triangles(&self) -> usize {
        self.triangles
    }

    /// Material textures decoded for the scene.
    pub fn textures(&self) -> usize {
        self.textures
    }
}

/// Add the faces of `solid` to `meshes`, each in its material or its paint.
fn add_solid(
    project: &Project,
    solid: &Solid,
    style: &Style,
    clay: bool,
    space: &Space,
    textures: &mut Textures,
    meshes: &mut Meshes,
) -> Result<(), RenderError> {
    let mass = solid.category == Category::Mass;
    let mut part = |material: Option<&str>, appearance: [u8; 3], transparency: f64| {
        let input = SurfaceInput { material, appearance, transparency, mass };
        let surface = Surface::of(project, &input, style, clay, textures);
        let texture =
            surface.texture.and(material.and_then(|n| project.material(n))).and_then(|m| m.appearance.texture.as_ref());
        Part { surface, texture }
    };
    let own = part(solid.material.as_deref(), solid.appearance, solid.transparency);
    let painted: HashMap<usize, Part> = solid
        .painted
        .iter()
        .map(|(fi, look)| (*fi, part(look.material.as_deref(), look.appearance, look.transparency)))
        .collect();
    let normals = solid.vertex_normals();
    let shifts = solid.facet_shifts();
    for (fi, face) in solid.faces.iter().enumerate() {
        if face.verts.len() < 3 || !face.verts.iter().all(|p| p.is_finite()) {
            continue;
        }
        let Part { surface, texture } = painted.get(&fi).unwrap_or(&own);
        let (u, v) = cc_geom::plane_axes(face.normal);
        let [du, dv] = shifts.get(fi).copied().unwrap_or_default();
        let face_normals = normals.get(fi);
        meshes.vertices += face.verts.len();
        if meshes.vertices > MAX_VERTICES {
            return Err(RenderError::TooManyTriangles { limit: MAX_TRIANGLES });
        }
        let slot = meshes.slot(surface);
        let Some((_, mesh)) = meshes.list.get_mut(slot) else { continue };
        let start = mesh.positions.len();
        // Whole repeats are taken off per face so the coordinates stay precise as f32.
        let mut shift: Option<[f64; 2]> = None;
        for (k, p) in face.verts.iter().enumerate() {
            let n = face_normals.and_then(|ns| ns.get(k)).copied().filter(|n| n.is_finite()).unwrap_or(face.normal);
            let uv = match texture {
                Some(t) => {
                    let [x, y] = texture_uv(t, [p.dot(u) + du, p.dot(v) + dv]);
                    let s = *shift.get_or_insert([x.floor(), y.floor()]);
                    let uv = Vec2::new((x - s[0]) as f32, (y - s[1]) as f32);
                    if uv.is_finite() { uv } else { Vec2::ZERO }
                }
                None => Vec2::ZERO,
            };
            mesh.positions.push(space.point(*p));
            mesh.normals.push(if n.is_finite() { space::dir(n) } else { Vec3::ZERO });
            mesh.uvs.push(uv);
        }
        let mut triangles = 0;
        for [a, b, c] in face.triangulate() {
            let (Some(pa), Some(pb), Some(pc)) = (face.verts.get(a), face.verts.get(b), face.verts.get(c)) else {
                continue;
            };
            if (*pb - *pa).cross(*pc - *pa).length_squared() <= MIN_DOUBLE_AREA * MIN_DOUBLE_AREA {
                continue;
            }
            let index = |i: usize| u32::try_from(start + i).ok();
            let (Some(ia), Some(ib), Some(ic)) = (index(a), index(b), index(c)) else { continue };
            mesh.indices.push([ia, ib, ic]);
            triangles += 1;
        }
        meshes.triangles += triangles;
        if meshes.triangles > MAX_TRIANGLES {
            return Err(RenderError::TooManyTriangles { limit: MAX_TRIANGLES });
        }
    }
    Ok(())
}
