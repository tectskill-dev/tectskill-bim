//! Renders of small synthetic scenes (a few dozen pixels, a few samples)
//! checked against what they must show, and the example house.

use crate::*;
use cc_core::sun::{SunMode, SunSettings};
use cc_core::{CameraState, Category, ElementId, Project, RenderEnvironment, RenderSettings, ToneMap, View, ViewKind};
use cc_geom::{Face, ModelGeometry, Solid};
use glam::{DVec3, Vec3};

fn view3d(project: &Project) -> View {
    project.views.values().find(|v| matches!(v.kind, ViewKind::ThreeD { .. })).unwrap().clone()
}

fn settings(width: u32, height: u32, samples: u32) -> RenderSettings {
    RenderSettings { width, height, samples, max_depth: 4, ..RenderSettings::default() }
}

/// The camera at `eye` looking at `target`.
fn looking(eye: DVec3, target: DVec3) -> CameraState {
    let d = target - eye;
    let f = d.normalize();
    CameraState {
        target,
        yaw_deg: f.x.atan2(f.y).to_degrees(),
        pitch_deg: (-f.z).asin().to_degrees(),
        distance: d.length(),
    }
}

/// A box of element `id` from `lo` to `hi`, faces outwards.
fn cuboid(id: u64, lo: DVec3, hi: DVec3, material: Option<&str>, color: [u8; 3]) -> Solid {
    let mut s = Solid::new(ElementId(id), Category::Column, color);
    s.material = material.map(str::to_string);
    let c = |x: bool, y: bool, z: bool| {
        DVec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z })
    };
    let (f, t) = (false, true);
    let quads = [
        ([c(f, f, t), c(t, f, t), c(t, t, t), c(f, t, t)], DVec3::Z),
        ([c(f, f, f), c(f, t, f), c(t, t, f), c(t, f, f)], -DVec3::Z),
        ([c(t, f, f), c(t, t, f), c(t, t, t), c(t, f, t)], DVec3::X),
        ([c(f, f, f), c(f, f, t), c(f, t, t), c(f, t, f)], -DVec3::X),
        ([c(f, t, f), c(f, t, t), c(t, t, t), c(t, t, f)], DVec3::Y),
        ([c(f, f, f), c(t, f, f), c(t, f, t), c(f, f, t)], -DVec3::Y),
    ];
    s.faces = quads.into_iter().map(|(v, n)| Face::new(v.to_vec(), n)).collect();
    s
}

fn geometry(solids: Vec<Solid>) -> ModelGeometry {
    ModelGeometry { solids, ..ModelGeometry::default() }
}

fn render(project: &Project, geom: &ModelGeometry, view: &View, camera: &CameraState, s: &RenderSettings) -> Image {
    let scene = RenderScene::build(project, geom, view, camera, s, None).unwrap();
    let image = render_image(&scene).unwrap();
    assert!(image.is_valid());
    assert!(image.pixels.iter().flatten().all(|c| c.is_finite()), "a pixel is not a number");
    image
}

fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn mean(image: &Image, x0: u32, y0: u32, x1: u32, y1: u32) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    let mut n = 0.0f32;
    for y in y0..y1.min(image.height) {
        for x in x0..x1.min(image.width) {
            let p = image.pixels[(y * image.width + x) as usize];
            for k in 0..3 {
                sum[k] += p[k];
            }
            n += 1.0;
        }
    }
    sum.map(|v| v / n.max(1.0))
}

/// Where the 3D viewport puts world point `p` on an image of `w` × `h`
/// pixels: its view-projection (perspective of `fov_deg` vertical, Z up,
/// as `cc_render::view_proj`), then pixels from the top left.
fn viewport_pixel(camera: &CameraState, fov_deg: f64, w: u32, h: u32, p: DVec3) -> (f64, f64) {
    let aspect = w as f32 / h as f32;
    let near = (camera.distance as f32 * 0.01).clamp(0.01, 1.0);
    let proj = glam::camera::rh::proj::directx::perspective((fov_deg as f32).to_radians(), aspect, near, 5000.0);
    let view = glam::camera::rh::view::look_at_mat4(camera.eye().as_vec3(), camera.target.as_vec3(), Vec3::Z);
    let ndc = (proj * view).project_point3(p.as_vec3());
    ((f64::from(ndc.x) + 1.0) * 0.5 * f64::from(w), (1.0 - f64::from(ndc.y)) * 0.5 * f64::from(h))
}

/// A view with a sun of `altitude` at `azimuth` (degrees clockwise from north).
fn sunny(project: &Project, azimuth: f64, altitude: f64) -> View {
    let mode = SunMode::Lighting { azimuth, altitude, relative_to_view: false };
    View { sun: Some(SunSettings { mode, ..SunSettings::default() }), ..view3d(project) }
}

#[test]
fn the_scene_can_go_to_another_thread() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<RenderScene>();
}

#[test]
fn the_sky_is_up_and_the_ground_down() {
    let p = Project::from_template();
    let view = view3d(&p);
    let empty = ModelGeometry::default();
    let s = RenderSettings {
        environment: RenderEnvironment { ground_plane: false, ..Default::default() },
        ..settings(16, 16, 4)
    };
    let up = render(&p, &empty, &view, &CameraState { pitch_deg: -60.0, ..CameraState::default() }, &s);
    let down = render(&p, &empty, &view, &CameraState { pitch_deg: 60.0, ..CameraState::default() }, &s);
    let (sky, ground) = (mean(&up, 0, 0, 16, 16), mean(&down, 0, 0, 16, 16));
    assert!(sky[2] > sky[0] * 1.5, "sky {sky:?}");
    assert!(ground[0] > ground[2], "ground {ground:?}");
    // The top of a level view is sky, the bottom ground, with the ground plane too.
    let s = settings(16, 16, 4);
    let level = render(&p, &empty, &view, &CameraState { pitch_deg: 0.0, ..CameraState::default() }, &s);
    let (top, bottom) = (mean(&level, 0, 0, 16, 4), mean(&level, 0, 12, 16, 16));
    assert!(top[2] > top[0] && bottom[0] > bottom[2], "top {top:?} bottom {bottom:?}");
}

#[test]
fn a_box_casts_its_shadow_away_from_the_sun() {
    let p = Project::from_template();
    // The sun in the east, 35° up: the shadow falls to the west.
    let view = sunny(&p, 90.0, 35.0);
    let geom = geometry(vec![cuboid(1, DVec3::new(-0.5, -0.5, 0.0), DVec3::new(0.5, 0.5, 1.0), None, [200; 3])]);
    let camera = looking(DVec3::new(0.0, -0.01, 12.0), DVec3::ZERO);
    let s = settings(40, 40, 8);
    let image = render(&p, &geom, &view, &camera, &s);
    let at = |p: DVec3| {
        let (x, y) = viewport_pixel(&camera, s.fov_deg, s.width, s.height, p);
        mean(&image, x as u32 - 1, y as u32 - 1, x as u32 + 2, y as u32 + 2)
    };
    let (west, east) = (luminance(at(DVec3::new(-1.2, 0.0, 0.0))), luminance(at(DVec3::new(1.2, 0.0, 0.0))));
    assert!(west < east * 0.6, "west {west} east {east}");
}

#[test]
fn the_camera_frames_what_the_viewport_frames() {
    let p = Project::from_template();
    let view = View { sun: None, ..view3d(&p) };
    // A small black box off the center of a view of the sky.
    let center = DVec3::new(1.6, 10.0, 7.4);
    let half = DVec3::splat(0.5);
    let geom = geometry(vec![cuboid(1, center - half, center + half, None, [0; 3])]);
    // Looking up enough that the whole picture is sky.
    let camera = looking(DVec3::new(0.0, 0.0, 2.0), DVec3::new(0.0, 10.0, 7.0));
    let s = RenderSettings {
        environment: RenderEnvironment { ground_plane: false, sun_strength: 0.0, ..Default::default() },
        ..settings(64, 40, 4)
    };
    let image = render(&p, &geom, &view, &camera, &s);
    let (mut sx, mut sy, mut n) = (0.0f64, 0.0f64, 0.0f64);
    for y in 0..image.height {
        for x in 0..image.width {
            if luminance(image.pixels[(y * image.width + x) as usize]) < 0.03 {
                (sx, sy, n) = (sx + f64::from(x) + 0.5, sy + f64::from(y) + 0.5, n + 1.0);
            }
        }
    }
    assert!(n >= 4.0, "the box is not in the picture");
    let (ex, ey) = viewport_pixel(&camera, s.fov_deg, s.width, s.height, center);
    assert!(
        (sx / n - ex).abs() < 1.5 && (sy / n - ey).abs() < 1.5,
        "box at {}, {}; expected {ex}, {ey}",
        sx / n,
        sy / n
    );
    assert!(ex > 32.0 && ey < 20.0, "the box should be right of and above the center");
}

/// A box of `material` filling the view, lit by a high sun.
fn wall_of(p: &Project, material: &str, clay: bool) -> [f32; 3] {
    wall_in_sun(p, material, clay, 180.0)
}

/// The face of a box of `material` filling the view, with a high sun at `azimuth`.
fn wall_in_sun(p: &Project, material: &str, clay: bool, azimuth: f64) -> [f32; 3] {
    let view = sunny(p, azimuth, 60.0);
    let geom = geometry(vec![cuboid(1, DVec3::new(-5.0, 0.0, 0.0), DVec3::new(5.0, 1.0, 6.0), Some(material), [9; 3])]);
    let camera = looking(DVec3::new(0.0, -4.0, 3.0), DVec3::new(0.0, 0.0, 3.0));
    let image = render(p, &geom, &view, &camera, &RenderSettings { clay, ..settings(16, 16, 4) });
    mean(&image, 0, 0, 16, 16)
}

#[test]
fn materials_show_their_appearance_and_clay_is_gray() {
    let mut p = Project::from_template();
    let name = p.materials[0].name.clone();
    p.materials[0].appearance.color = [200, 30, 30];
    let red = wall_of(&p, &name, false);
    assert!(red[0] > red[1] * 2.0 && red[0] > red[2] * 2.0, "{red:?}");
    let clay = wall_of(&p, &name, true);
    assert!(neutral(clay) && clay[0] > 0.2, "{clay:?}");
}

/// Whether a color is a gray: no channel more than 15 % off their mean (the
/// sky tints faces in the shade a little blue).
fn neutral(c: [f32; 3]) -> bool {
    let m = (c[0] + c[1] + c[2]) / 3.0;
    m > 0.0 && c.iter().all(|v| (v - m).abs() < 0.15 * m)
}

#[test]
fn clay_is_lit_by_the_sun() {
    let mut p = Project::from_template();
    let name = p.materials[0].name.clone();
    p.materials[0].appearance.color = [200, 30, 30];
    // The same face with the sun in front of it and behind the box: the
    // sun and the shadow show, which ambient occlusion alone would not.
    let (a, b) = (wall_in_sun(&p, &name, true, 180.0), wall_in_sun(&p, &name, true, 0.0));
    let (lit, shaded) = if luminance(a) > luminance(b) { (a, b) } else { (b, a) };
    assert!(luminance(lit) > luminance(shaded) * 1.5, "lit {lit:?}, shaded {shaded:?}");
    // The material's red is gone: the sunlit face is gray, the shaded one
    // takes the blue of the sky.
    assert!(neutral(lit) && shaded[0] <= shaded[1] && shaded[1] <= shaded[2], "lit {lit:?}, shaded {shaded:?}");
}

#[test]
fn sunlight_goes_through_glass() {
    let p = Project::from_template();
    let glass = p.materials.iter().find(|m| m.identity.class == cc_core::MaterialClass::Glass).unwrap().name.clone();
    let concrete =
        p.materials.iter().find(|m| m.identity.class == cc_core::MaterialClass::Concrete).unwrap().name.clone();
    let view = sunny(&p, 180.0, 70.0);
    let camera = looking(DVec3::new(0.0, -7.0, 1.0), DVec3::new(0.0, 0.0, 0.0));
    let s = settings(32, 24, 8);
    // The floor under a roof of each material, seen from the side below it.
    let under = |material: &str| {
        let roof = cuboid(1, DVec3::new(-3.0, -3.0, 2.0), DVec3::new(3.0, 3.0, 2.01), Some(material), [9; 3]);
        let image = render(&p, &geometry(vec![roof]), &view, &camera, &s);
        let (x, y) = viewport_pixel(&camera, s.fov_deg, s.width, s.height, DVec3::new(0.0, 0.0, 0.0));
        luminance(mean(&image, x as u32 - 3, y as u32 - 1, x as u32 + 3, y as u32 + 2))
    };
    let (lit, shaded) = (under(&glass), under(&concrete));
    assert!(lit > shaded * 2.0, "under glass {lit}, under concrete {shaded}");
}

#[test]
fn hidden_elements_are_left_out() {
    let p = cc_core::from_ron(include_str!("../../../examples/casa_simples.ccad")).unwrap();
    let geom = ModelGeometry::build(&p);
    let view = view3d(&p);
    let camera = CameraState::default();
    let s = settings(16, 16, 1);
    let all = RenderScene::build(&p, &geom, &view, &camera, &s, None).unwrap().triangles();
    let walls: usize = geom
        .solids
        .iter()
        .filter(|x| x.category == Category::Wall)
        .map(|x| x.faces.iter().map(|f| f.triangulate().len()).sum::<usize>())
        .sum();
    assert!(walls > 0 && all > walls);
    // Temporarily hidden walls.
    let hide = cc_core::TemporaryVisibility {
        isolate: false,
        elements: Default::default(),
        categories: [Category::Wall].into(),
    };
    let no_walls = RenderScene::build(&p, &geom, &view, &camera, &s, Some(&hide)).unwrap().triangles();
    assert_eq!(no_walls, all - walls);
    // An element hidden in the view.
    let wall = geom.solids.iter().find(|x| x.category == Category::Wall).unwrap().element;
    let mut hidden = view.clone();
    hidden.graphics.hidden.insert(wall);
    let fewer = RenderScene::build(&p, &geom, &hidden, &camera, &s, None).unwrap().triangles();
    assert!(fewer < all && fewer > no_walls);
}

#[test]
fn a_stopped_render_returns_at_once_and_regions_leave_the_rest_black() {
    let p = Project::from_template();
    let view = view3d(&p);
    let s = settings(24, 16, 64);
    let scene = RenderScene::build(&p, &ModelGeometry::default(), &view, &CameraState::default(), &s, None).unwrap();
    let mut steps = 0;
    let outcome = run(&scene, None, |progress| {
        steps += 1;
        assert_eq!(progress.samples(), 1);
        assert_eq!(progress.total_samples(), 64);
        Control::Stop
    })
    .unwrap();
    assert_eq!((steps, outcome.samples, outcome.stopped), (1, 1, true));
    // A region: the rest is never traced.
    let region = Region { x: 8, y: 4, width: 6, height: 5 };
    let outcome = run(&scene, Some(region), |p| {
        assert_eq!(p.region(), region);
        assert_eq!(p.region_image().pixels.len(), 30);
        Control::Continue
    })
    .unwrap();
    assert!(!outcome.stopped && outcome.samples == 64);
    for y in 0..16 {
        for x in 0..24 {
            let px = outcome.image.pixels[(y * 24 + x) as usize];
            let inside = (8..14).contains(&x) && (4..9).contains(&y);
            assert_eq!(px == [0.0; 3], !inside, "pixel {x}, {y}: {px:?}");
        }
    }
    // A region outside the image renders nothing.
    let outside = run(&scene, Some(Region { x: 100, y: 100, width: 5, height: 5 }), |_| Control::Continue).unwrap();
    assert!(outside.image.pixels.iter().all(|p| *p == [0.0; 3]));
}

#[test]
fn broken_geometry_and_cameras_do_not_panic() {
    let p = Project::from_template();
    let view = view3d(&p);
    let s = settings(16, 12, 2);
    let mut broken = cuboid(1, DVec3::ZERO, DVec3::ONE, None, [200; 3]);
    // A face with a point that is not a number, a degenerate one, one of two points, an empty one.
    broken.faces.push(Face::new(vec![DVec3::ZERO, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X], DVec3::Z));
    broken.faces.push(Face::new(vec![DVec3::ZERO, DVec3::X, DVec3::X * 2.0], DVec3::Z));
    broken.faces.push(Face::new(vec![DVec3::ZERO, DVec3::X], DVec3::Z));
    broken.faces.push(Face::new(vec![], DVec3::NAN));
    let mut infinite = cuboid(2, DVec3::ZERO, DVec3::ONE, None, [200; 3]);
    infinite.faces[0].verts[0] = DVec3::INFINITY;
    let geom = geometry(vec![broken, infinite]);
    let scene = RenderScene::build(&p, &geom, &view, &CameraState::default(), &s, None).unwrap();
    // The six faces of the box and five of the other: two triangles each.
    assert_eq!(scene.triangles(), 22);
    render(&p, &geom, &view, &CameraState::default(), &s);
    // An empty model is the sky and the ground.
    let empty = render(&p, &ModelGeometry::default(), &view, &CameraState::default(), &s);
    assert!(empty.pixels.iter().any(|c| luminance(*c) > 0.01));
    // Cameras: not a number is refused, no distance stands at the target.
    for bad in [
        CameraState { distance: f64::NAN, ..CameraState::default() },
        CameraState { yaw_deg: f64::INFINITY, ..CameraState::default() },
        CameraState { target: DVec3::NAN, ..CameraState::default() },
    ] {
        let r = RenderScene::build(&p, &geom, &view, &bad, &s, None);
        assert!(matches!(r, Err(RenderError::InvalidCamera(_))));
    }
    let zero = CameraState { distance: 0.0, ..CameraState::default() };
    render(&p, &geom, &view, &zero, &s);
    // Settings out of range are brought back.
    let wild = RenderSettings { width: 0, height: 0, samples: 0, fov_deg: f64::NAN, ..s.clone() };
    let scene = RenderScene::build(&p, &geom, &view, &CameraState::default(), &wild, None).unwrap();
    assert!(scene.width() >= cc_core::render_settings::MIN_SIZE && scene.samples() >= 1);
}

#[test]
fn tone_mapping_exposes_and_encodes() {
    let image = Image { width: 2, height: 1, pixels: vec![[0.18, 0.18, 0.18], [f32::NAN, 4.0, -1.0]] };
    let rgba = image.to_rgba8(0.0, ToneMap::Filmic);
    assert_eq!(rgba.len(), 8);
    assert_eq!((rgba[3], rgba[7]), (255, 255));
    assert_eq!((rgba[4], rgba[6]), (0, 0));
    let brighter = image.to_rgba8(1.0, ToneMap::Filmic);
    assert!(brighter[0] > rgba[0]);
    // Standard clips; the filmic curve rolls off.
    let standard = image.to_rgba8(0.0, ToneMap::Standard);
    assert_eq!(standard[5], 255);
    assert!(rgba[5] < 255 && rgba[5] > 200);
    let half = Image { width: 1, height: 1, pixels: vec![[0.5, 0.5, 0.5]] };
    assert_eq!(half.to_rgba8(0.0, ToneMap::Standard)[0], 188);
}

#[test]
fn images_are_written_by_extension() {
    let dir = std::env::temp_dir().join(format!("cc_raytracing_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = Image { width: 3, height: 2, pixels: vec![[0.2, 0.4, 0.8]; 6] };
    for name in ["out.png", "out.jpg", "out.exr", "out.hdr"] {
        image.save(&dir.join(name), 0.0, ToneMap::Filmic).unwrap();
        assert!(std::fs::metadata(dir.join(name)).unwrap().len() > 0, "{name}");
    }
    let png = image::open(dir.join("out.png")).unwrap();
    assert_eq!((png.width(), png.height()), (3, 2));
    let exr = image::open(dir.join("out.exr")).unwrap().into_rgb32f();
    assert!((exr.get_pixel(0, 0)[2] - 0.8).abs() < 1e-3);
    assert!(matches!(image.save(&dir.join("out.tga"), 0.0, ToneMap::Filmic), Err(RenderError::UnsupportedFormat(_))));
    let short = Image { width: 3, height: 2, pixels: vec![[0.0; 3]; 5] };
    assert!(matches!(short.save(&dir.join("short.png"), 0.0, ToneMap::Filmic), Err(RenderError::InvalidImage(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_example_house_renders() {
    let p = cc_core::from_ron(include_str!("../../../examples/casa_simples.ccad")).unwrap();
    let geom = ModelGeometry::build(&p);
    let view = view3d(&p);
    let ViewKind::ThreeD { camera, .. } = view.kind else { panic!("not a 3D view") };
    let s = settings(48, 27, 4);
    let camera = camera::fit(&camera, &geom, s.fov_deg, 48.0 / 27.0);
    let scene = RenderScene::build(&p, &geom, &view, &camera, &s, None).unwrap();
    assert!(scene.triangles() > 1000, "{}", scene.triangles());
    let image = render(&p, &geom, &view, &camera, &s);
    let lum: Vec<f32> = image.pixels.iter().map(|c| luminance(*c)).collect();
    let (lo, hi) = lum.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    assert!(hi > lo * 3.0 && hi - lo > 0.2, "a flat image: {lo} to {hi}");
    // The roof's tiles in the middle of the picture, the gray ground around it.
    let middle = mean(&image, 22, 11, 26, 15);
    assert!(middle[0] > middle[1] * 1.5, "{middle:?}");
    let corner = mean(&image, 0, 0, 4, 4);
    assert!(corner[0] < corner[1] * 1.3, "{corner:?}");
}
