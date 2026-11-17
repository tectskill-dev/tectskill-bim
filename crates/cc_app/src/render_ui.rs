//! The render window (*View → Render*): the active 3D view path traced
//! progressively ([`cc_raytracing`]), the picture on the left (fitted; the
//! wheel zooms about the pointer, a drag pans, a double click fits again)
//! and the view's render settings on the right (quality, output,
//! environment), with *Save Image…*, *Region*, *Render*, *Stop* and *Close*.
//!
//! The window is a tool, like a viewer, not a data dialog: it has no Cancel.
//! A setting is written to the view (`view_set`, through
//! [`crate::engine_edit::view`]) only when the user changes one, all of them
//! under one merge key, so whatever is changed in a session of the window is
//! one undo step; Close keeps it. It is modal, so the model cannot change
//! under a running render.
//!
//! The camera is the one the viewport shows when the window opens (its live
//! camera, not the one last written to the model). A camera still fitted to
//! the model, or the default camera of a view never shown, is fitted again
//! for the render's field of view and aspect, as the viewport fits it to its
//! own.
//!
//! Rendering runs on two threads of its own, handed copies of the project,
//! the view's geometry, the view and its temporary visibility: the render
//! thread builds the scene and traces it, sending a copy of the linear image
//! at most four times a second (and the last one); the display thread
//! exposes and tone maps the newest of those (composited over the previous
//! image for a region) and hands the picture to the window, so neither the
//! tracing nor the window waits for that. A stop flag ends the render
//! between steps: *Stop*, closing the window, a new render and dropping the
//! window (the app closing, the document replaced) all set it, and nothing
//! waits for the threads. Each render has its own channels and number, so a
//! late picture of an earlier render never shows in a later one. Exposure and
//! tone mapping only change how the linear image is shown: changing them
//! maps the image again (on a thread) without tracing it again.
//!
//! *Sun Settings…* hides the window and opens the Sun Settings dialog on the
//! view; when that closes (OK or Cancel) the window opens again on the same
//! view and camera, without the picture (the light changed).

use crate::state::AppState;
use crate::t;
use crate::table::{row, row_tip, section};
use crate::theme::PALETTE;
use cc_core::render_settings::{
    MAX_CLAMP, MAX_DEPTH, MAX_EXPOSURE, MAX_FOV_DEG, MAX_PIXELS, MAX_SAMPLES, MAX_SIZE, MAX_STRENGTH, MIN_CLAMP,
    MIN_DEPTH, MIN_EXPOSURE, MIN_FOV_DEG, MIN_SAMPLES, MIN_SIZE, MIN_STRENGTH,
};
use cc_core::{CameraState, Project, RenderSettings, TemporaryVisibility, ToneMap, View, ViewId, ViewKind};
use cc_geom::ModelGeometry;
use cc_raytracing::{Control, Image, Region, RenderError, RenderScene};
use egui::{Color32, ColorImage, Context, DragValue, RichText, Sense, Stroke, Ui, vec2};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Width of the settings column.
const RIGHT: f32 = 300.0;
/// Gap the two columns leave for their divider.
const DIVIDER: f32 = 9.0;
/// Height of the status line under the picture.
const STATUS_H: f32 = 22.0;
/// The picture area's bounds, points.
const MIN_PICTURE: egui::Vec2 = egui::Vec2::new(360.0, 240.0);
const MAX_PICTURE: egui::Vec2 = egui::Vec2::new(880.0, 560.0);
/// Margin between the picture area and the fitted picture.
const MATTE_MARGIN: f32 = 12.0;
/// How often the render thread hands over a picture.
const FRAME_INTERVAL: Duration = Duration::from_millis(250);
/// Smallest region side, pixels.
const MIN_REGION: u32 = 8;
/// Zoom of the picture: bounds, and per point of wheel scroll.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 32.0;
const ZOOM_RATE: f64 = 0.0025;

/// The quality presets (draft, medium, high): samples per pixel and light bounces.
const PRESETS: [(u32, u32); 3] = [(32, 4), (128, 8), (512, 12)];
/// The output sizes offered besides the viewport's.
const SIZES: [(u32, u32); 4] = [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];

/// Exposure (stops) and tone mapping: how linear light is shown.
type Mapping = (f64, ToneMap);

/// What the window keeps of how it was opened (also while Sun Settings,
/// opened from it, are open).
#[derive(Debug, Clone, Copy)]
pub struct Resume {
    view: ViewId,
    /// The camera the viewport showed.
    camera: CameraState,
    /// The camera is fitted to the model: fit it for the render.
    fitted: bool,
    /// The viewport's size in pixels (the *View size*), within the limits.
    view_size: Option<[u32; 2]>,
    /// The merge key of the window's setting changes.
    merge: u64,
}

/// Where a render stands.
#[derive(Debug, Clone, PartialEq)]
enum Phase {
    /// Nothing rendered yet.
    Idle,
    /// The scene is being built.
    Preparing,
    Rendering {
        samples: u32,
        total: u32,
        elapsed: Duration,
    },
    Done {
        samples: u32,
        total: u32,
        elapsed: Duration,
        stopped: bool,
    },
    /// Building or rendering failed (the reason).
    Failed(String),
}

/// The open render window.
pub struct RenderWindow {
    resume: Resume,
    phase: Phase,
    /// The render running.
    run: Option<Run>,
    /// Number of the last render started.
    generation: u64,
    /// The last finished (or stopped) image, in linear light.
    image: Option<Arc<Image>>,
    /// The settings (exposure and tone mapping aside) of that image.
    rendered: Option<RenderSettings>,
    /// How the picture shown maps the image.
    shown: Option<Mapping>,
    /// A picture waiting to be uploaded, and the texture shown.
    pending: Option<ColorImage>,
    texture: Option<egui::TextureHandle>,
    /// The image mapped again on a thread (exposure or tone mapping changed).
    remap: Option<mpsc::Receiver<(Mapping, Option<ColorImage>)>>,
    /// Triangles of the last scene built.
    triangles: Option<usize>,
    /// The picture's zoom (1: fitted) and its offset from the center, points.
    zoom: f32,
    pan: egui::Vec2,
    /// *Region* is on: a drag draws the region instead of panning.
    region_mode: bool,
    /// The region (pixels of the output) and the output size it was drawn for.
    region: Option<(Region, [u32; 2])>,
    /// The region being dragged, in output pixels.
    drag: Option<(egui::Pos2, egui::Pos2)>,
    /// The Size list shows *Custom* (chosen, or a size typed).
    size_custom: bool,
    /// *Sun Settings…* was pressed this frame.
    open_sun: bool,
    /// The phase before the render running, for a Stop before its first picture.
    before: Phase,
}

/// A render in progress: its threads stop when it is dropped.
struct Run {
    generation: u64,
    shared: Arc<Shared>,
    rx: mpsc::Receiver<(u64, Message)>,
    /// The settings it renders (exposure and tone mapping aside).
    settings: RenderSettings,
    /// A picture has come.
    pictured: bool,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// What the window and the render threads share.
struct Shared {
    stop: AtomicBool,
    /// The exposure's bits.
    exposure: AtomicU64,
    /// Tone mapping is [`ToneMap::Standard`].
    standard: AtomicBool,
}

impl Shared {
    fn new((exposure, tone): Mapping) -> Self {
        Self {
            stop: AtomicBool::new(false),
            exposure: AtomicU64::new(exposure.to_bits()),
            standard: AtomicBool::new(tone == ToneMap::Standard),
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn mapping(&self) -> Mapping {
        let exposure = f64::from_bits(self.exposure.load(Ordering::Relaxed));
        let tone = if self.standard.load(Ordering::Relaxed) { ToneMap::Standard } else { ToneMap::Filmic };
        (exposure, tone)
    }

    fn set_mapping(&self, (exposure, tone): Mapping) {
        self.exposure.store(exposure.to_bits(), Ordering::Relaxed);
        self.standard.store(tone == ToneMap::Standard, Ordering::Relaxed);
    }
}

/// From the display thread to the window.
enum Message {
    Built {
        triangles: usize,
    },
    Frame {
        picture: ColorImage,
        mapping: Mapping,
        samples: u32,
        total: u32,
        elapsed: Duration,
    },
    Finished {
        image: Option<Image>,
        picture: Option<ColorImage>,
        mapping: Mapping,
        samples: u32,
        total: u32,
        elapsed: Duration,
        stopped: bool,
    },
    /// The reason, or `None` for a thread that died.
    Failed(Option<String>),
}

/// From the render thread to the display thread.
enum Step {
    Built {
        triangles: usize,
    },
    /// The whole image, or the region's alone.
    Frame {
        image: Image,
        region: Option<Region>,
        samples: u32,
        total: u32,
        elapsed: Duration,
    },
    Done {
        samples: u32,
        stopped: bool,
    },
    Failed(Option<String>),
}

/// What the render thread renders.
struct Job {
    project: Project,
    geom: ModelGeometry,
    view: View,
    camera: CameraState,
    fitted: bool,
    settings: RenderSettings,
    temporary: Option<TemporaryVisibility>,
    region: Option<Region>,
}

// ---------------------------------------------------------------- opening

/// Open the window on the active view (when it is a 3D view).
pub fn open(state: &mut AppState) {
    if let Some(view) = state.active_view {
        open_for(state, view);
    }
}

/// A merge key no earlier session of the window used.
fn new_merge_key(view: ViewId) -> u64 {
    static SESSIONS: AtomicU64 = AtomicU64::new(0);
    let n = SESSIONS.fetch_add(1, Ordering::Relaxed);
    crate::graphics_dialog::key(("render-window", view.0, n))
}

/// Open the window on `view`, with the camera its viewport shows now.
pub fn open_for(state: &mut AppState, view: ViewId) {
    if state.dialogs.render.is_some() {
        return;
    }
    let Some(ViewKind::ThreeD { camera, .. }) = state.project().view(view).map(|v| &v.kind) else {
        state.message = t!("why-render-view");
        return;
    };
    let (camera, fitted, view_size) = match state.view3d.get(&view) {
        Some(v3) => (v3.camera, v3.auto_fitted(), v3.pixel_size.map(clamp_size)),
        None => (*camera, *camera == CameraState::default(), None),
    };
    let resume = Resume { view, camera, fitted, view_size, merge: new_merge_key(view) };
    state.dialogs.render = Some(RenderWindow::new(resume));
}

/// A width and height within the limits (the pixels too).
fn clamp_size([w, h]: [u32; 2]) -> [u32; 2] {
    let mut s = RenderSettings { width: w, height: h, ..Default::default() };
    s.repair();
    [s.width, s.height]
}

impl RenderWindow {
    fn new(resume: Resume) -> Self {
        Self {
            resume,
            phase: Phase::Idle,
            run: None,
            generation: 0,
            image: None,
            rendered: None,
            shown: None,
            pending: None,
            texture: None,
            remap: None,
            triangles: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            region_mode: false,
            region: None,
            drag: None,
            size_custom: false,
            open_sun: false,
            before: Phase::Idle,
        }
    }

    fn rendering(&self) -> bool {
        self.run.is_some()
    }
}

/// The view's render settings (the defaults when it has none).
fn current(state: &AppState, view: ViewId) -> Option<RenderSettings> {
    state.project().view(view).map(|v| v.render.clone().unwrap_or_default())
}

/// The settings that need a new render when they change (exposure and tone
/// mapping only change how the image is shown).
fn traced(settings: &RenderSettings) -> RenderSettings {
    RenderSettings { exposure: 0.0, tone_map: ToneMap::Filmic, ..settings.clone() }
}

// ---------------------------------------------------------------- showing

pub fn show(ctx: &Context, state: &mut AppState) {
    // Sun Settings opened from the window closed: open it again.
    if state.dialogs.render.is_none()
        && state.dialogs.sun.is_none()
        && let Some(resume) = state.dialogs.render_resume.take()
        && state.project().view(resume.view).is_some_and(|v| v.kind.takes_render())
    {
        state.dialogs.render = Some(RenderWindow::new(resume));
    }
    let Some(mut w) = state.dialogs.render.take() else { return };
    let Some(settings) = current(state, w.resume.view)
        .filter(|_| state.project().view(w.resume.view).is_some_and(|v| v.kind.takes_render()))
    else {
        return;
    };
    w.poll();
    w.follow_mapping(ctx, (settings.exposure, settings.tone_map));
    w.upload(ctx);
    if w.rendering() {
        // The threads ask for frames as pictures come; this notices a thread that died.
        ctx.request_repaint_after(FRAME_INTERVAL);
    }
    let name = state.project().view(w.resume.view).map(|v| v.name.clone()).unwrap_or_default();
    let (picture_size, body_width) = layout(ctx);
    let rendering = w.rendering();
    let has_image = w.image.is_some();
    let region_on = w.region_mode;
    let mut ids = [usize::MAX; 5];
    let response =
        crate::dialog::Dialog::new("render-window", t!("render-title", "view" => name)).width(body_width).show(
            ctx,
            |ui| body(ui, state, &mut w, &settings, picture_size),
            |f| {
                ids[0] = f.extra(t!("render-save"));
                ids[1] = f.toggle(t!("render-region"), region_on);
                ids[2] = f.primary(t!("action-render"));
                ids[3] = f.secondary(t!("render-stop"));
                ids[4] = f.secondary(t!("action-close"));
                if !has_image || rendering {
                    f.disable(ids[0]);
                }
                if rendering {
                    f.disable(ids[2]);
                } else {
                    f.disable(ids[3]);
                }
            },
        );
    match response.clicked {
        Some(i) if i == ids[0] => save_image(state, &w),
        Some(i) if i == ids[1] => {
            w.region_mode = !w.region_mode;
            if !w.region_mode {
                w.region = None;
                w.drag = None;
            }
        }
        Some(i) if i == ids[2] => w.start(ctx, state),
        Some(i) if i == ids[3] => w.stop(),
        Some(_) => return,
        None if response.dismissed => return,
        None => {}
    }
    if w.open_sun {
        // The window waits for Sun Settings; the render, if any, stops.
        let view = w.resume.view;
        state.dialogs.render_resume = Some(w.resume);
        drop(w);
        crate::sun_ui::open_for(state, view);
        return;
    }
    state.dialogs.render = Some(w);
}

/// The size of the picture area and the width of the dialog's body, for the window's size.
fn layout(ctx: &Context) -> (egui::Vec2, f32) {
    let screen = ctx.content_rect().size();
    let picture = vec2(
        (screen.x - RIGHT - DIVIDER - 100.0).clamp(MIN_PICTURE.x, MAX_PICTURE.x),
        (screen.y - 220.0).clamp(MIN_PICTURE.y, MAX_PICTURE.y),
    );
    (picture.floor(), (picture.x + DIVIDER + RIGHT).floor())
}

fn body(ui: &mut Ui, state: &mut AppState, w: &mut RenderWindow, settings: &RenderSettings, picture: egui::Vec2) {
    crate::dialog::two_columns(ui, [picture.x, RIGHT], |ui, column| {
        if column == 0 {
            picture_area(ui, w, settings, picture);
            status(ui, w, settings);
        } else {
            egui::ScrollArea::vertical()
                .id_salt("render-settings")
                .max_height(picture.y + STATUS_H)
                .auto_shrink([false, true])
                .show(ui, |ui| settings_column(ui, state, w, settings));
        }
    });
}

// ---------------------------------------------------------------- picture

/// `inner` fitted in `outer`, centered, keeping `aspect` (width over height).
fn fit_rect(outer: egui::Rect, aspect: f32) -> egui::Rect {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let size = if outer.width() / outer.height().max(1.0) > aspect {
        vec2(outer.height() * aspect, outer.height())
    } else {
        vec2(outer.width(), outer.width() / aspect)
    };
    egui::Rect::from_center_size(outer.center(), size)
}

/// Output pixels of a point on the picture shown in `shown`, within the output.
fn to_pixels(p: egui::Pos2, shown: egui::Rect, [w, h]: [u32; 2]) -> egui::Pos2 {
    let x = ((p.x - shown.left()) / shown.width().max(1.0)).clamp(0.0, 1.0) * w as f32;
    let y = ((p.y - shown.top()) / shown.height().max(1.0)).clamp(0.0, 1.0) * h as f32;
    egui::pos2(x, y)
}

/// The region between two points in output pixels, when at least
/// [`MIN_REGION`] pixels both ways.
fn region_between(a: egui::Pos2, b: egui::Pos2, [w, h]: [u32; 2]) -> Option<Region> {
    let r = egui::Rect::from_two_pos(a, b);
    let x0 = (r.left().round().max(0.0) as u32).min(w);
    let y0 = (r.top().round().max(0.0) as u32).min(h);
    let x1 = (r.right().round().max(0.0) as u32).min(w);
    let y1 = (r.bottom().round().max(0.0) as u32).min(h);
    let (width, height) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    (width >= MIN_REGION && height >= MIN_REGION).then_some(Region { x: x0, y: y0, width, height })
}

fn picture_area(ui: &mut Ui, w: &mut RenderWindow, settings: &RenderSettings, size: egui::Vec2) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let output = [settings.width, settings.height];
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, PALETTE.matte);
    // The picture shown, else an empty frame of the output's aspect.
    let frame = match &w.texture {
        Some(t) => {
            let [tw, th] = t.size();
            vec2(tw as f32, th as f32)
        }
        None => vec2(settings.width as f32, settings.height as f32),
    };
    let fitted = fit_rect(rect.shrink(MATTE_MARGIN), frame.x / frame.y.max(1.0));
    let center = rect.center() + w.pan;

    // Wheel or pinch zooms about the pointer; two fingers pan.
    if resp.hovered()
        && let Some(pointer) = resp.hover_pos()
    {
        let gesture = crate::gestures::read(ui);
        let factor = gesture.zoom(ZOOM_RATE) as f32;
        if (factor - 1.0).abs() > 1e-6 {
            let zoom = (w.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            // The point under the pointer stays there.
            let rel = (pointer - center) / w.zoom;
            w.pan = pointer - rel * zoom - rect.center();
            w.zoom = zoom;
        }
        w.pan += gesture.pan;
    }
    if resp.double_clicked() {
        w.zoom = 1.0;
        w.pan = egui::Vec2::ZERO;
    }
    let shown = egui::Rect::from_center_size(rect.center() + w.pan, fitted.size() * w.zoom);

    if w.region_mode {
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let at = to_pixels(p, shown, output);
            w.drag = Some((at, at));
        }
        if let (Some((from, _)), Some(p)) = (w.drag, resp.interact_pointer_pos()) {
            w.drag = Some((from, to_pixels(p, shown, output)));
        }
        if resp.drag_stopped()
            && let Some((a, b)) = w.drag.take()
            && let Some(region) = region_between(a, b, output)
        {
            w.region = Some((region, output));
        }
    } else if resp.dragged() {
        w.pan += resp.drag_delta();
    }

    match &w.texture {
        Some(t) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(t.id(), shown, uv, Color32::WHITE);
        }
        None => {
            painter.rect_stroke(shown, 0.0, Stroke::new(1.0, PALETTE.matte_text), egui::StrokeKind::Inside);
            if matches!(w.phase, Phase::Idle | Phase::Failed(_)) {
                let font = egui::FontId::proportional(14.0);
                painter.text(shown.center(), egui::Align2::CENTER_CENTER, t!("render-hint"), font, PALETTE.matte_text);
            }
        }
    }

    // The region: the one drawn, or the one being dragged.
    let px = |p: egui::Pos2| {
        let (ow, oh) = (output[0].max(1) as f32, output[1].max(1) as f32);
        egui::pos2(shown.left() + p.x / ow * shown.width(), shown.top() + p.y / oh * shown.height())
    };
    let drawn = match (w.drag, w.region) {
        (Some((a, b)), _) => Some(egui::Rect::from_two_pos(px(a), px(b))),
        (None, Some((r, at))) if at == output => {
            let min = egui::pos2(r.x as f32, r.y as f32);
            let max = egui::pos2((r.x + r.width) as f32, (r.y + r.height) as f32);
            Some(egui::Rect::from_min_max(px(min), px(max)))
        }
        _ => None,
    };
    if let Some(r) = drawn {
        painter.rect_filled(r, 0.0, PALETTE.accent.gamma_multiply(0.08));
        painter.rect_stroke(r, 0.0, Stroke::new(1.5, PALETTE.accent), egui::StrokeKind::Middle);
    }
    if w.region_mode {
        let font = egui::FontId::proportional(12.0);
        let at = egui::pos2(rect.center().x, rect.top() + 4.0);
        painter.text(at, egui::Align2::CENTER_TOP, t!("render-region-hint"), font, PALETTE.matte_text);
    }
}

/// `mm:ss`, or `h:mm:ss` past an hour.
fn clock(elapsed: Duration) -> String {
    let s = elapsed.as_secs();
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

fn status(ui: &mut Ui, w: &RenderWindow, settings: &RenderSettings) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), STATUS_H),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(STATUS_H);
            let muted = |text: String| RichText::new(text).color(PALETTE.text_muted);
            match &w.phase {
                Phase::Idle => {}
                Phase::Preparing => {
                    ui.spinner();
                    ui.label(muted(t!("render-preparing")));
                }
                Phase::Rendering { samples, total, elapsed } => {
                    ui.spinner();
                    let text = t!("render-status", "done" => *samples, "total" => *total, "time" => clock(*elapsed));
                    ui.label(muted(text));
                }
                Phase::Done { samples, total, elapsed, stopped } => {
                    let (done, total, time) = (*samples, *total, clock(*elapsed));
                    let text = if *stopped {
                        t!("render-stopped", "done" => done, "total" => total, "time" => time)
                    } else {
                        t!("render-status", "done" => done, "total" => total, "time" => time)
                    };
                    ui.label(muted(text));
                    if w.rendered.as_ref().is_some_and(|r| *r != traced(settings)) {
                        ui.label(RichText::new(format!("· {}", t!("render-changed"))).color(PALETTE.text_faint));
                    }
                }
                Phase::Failed(why) => {
                    ui.add(egui::Label::new(RichText::new(why).color(PALETTE.danger)).truncate());
                }
            }
            if let Some(n) = w.triangles {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(t!("render-triangles", "count" => n)).size(11.0).color(PALETTE.text_faint));
                });
            }
        },
    );
}

// ---------------------------------------------------------------- settings

/// Change the view's render settings as `f` changes a copy of them, under
/// the window's merge key (one undo step for the whole session).
fn edit(state: &mut AppState, r: &Resume, f: impl FnOnce(&mut RenderSettings)) -> bool {
    crate::engine_edit::view(state, r.view, Some(r.merge), |v| f(v.render.get_or_insert_with(RenderSettings::default)))
}

/// A checkbox reporting `label` as its name (the row's label is painted).
fn check(ui: &mut Ui, on: &mut bool, label: &str) -> egui::Response {
    let r = crate::table::checkbox(ui, on);
    let (enabled, value, label) = (ui.is_enabled(), *on, label.to_string());
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, value, &label));
    r
}

/// Reads a number typed with either decimal symbol.
fn parse_number(text: &str) -> Option<f64> {
    text.trim().trim_end_matches('°').trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A decimal value written with the project's decimal symbol.
fn decimal(value: &mut f64, decimals: usize) -> DragValue<'_> {
    DragValue::new(value)
        .custom_formatter(move |v, _| cc_views::format::number(v, decimals))
        .custom_parser(parse_number)
}

/// The quality preset `samples` and `bounces` make, if any.
fn preset_of(samples: u32, bounces: u32) -> Option<usize> {
    PRESETS.iter().position(|(s, b)| *s == samples && *b == bounces)
}

/// The name of preset `i` of [`PRESETS`].
fn preset_name(i: usize) -> String {
    match i {
        0 => t!("render-preset-draft"),
        1 => t!("render-preset-medium"),
        _ => t!("render-preset-high"),
    }
}

/// `width` × `height` with the other side made smaller when the pixels pass
/// [`MAX_PIXELS`] (`width_kept`: the width is the side typed).
fn within_pixels(width: u32, height: u32, width_kept: bool) -> (u32, u32) {
    let (w, h) = (width.clamp(MIN_SIZE, MAX_SIZE), height.clamp(MIN_SIZE, MAX_SIZE));
    if u64::from(w) * u64::from(h) <= MAX_PIXELS {
        return (w, h);
    }
    if width_kept {
        let h = u32::try_from(MAX_PIXELS / u64::from(w)).unwrap_or(MIN_SIZE).clamp(MIN_SIZE, MAX_SIZE);
        (w, h)
    } else {
        let w = u32::try_from(MAX_PIXELS / u64::from(h)).unwrap_or(MIN_SIZE).clamp(MIN_SIZE, MAX_SIZE);
        (w, h)
    }
}

fn settings_column(ui: &mut Ui, state: &mut AppState, w: &mut RenderWindow, s: &RenderSettings) {
    let r = w.resume;
    section(ui, "render-quality", t!("render-quality"), |ui| quality_rows(ui, state, &r, s));
    section(ui, "render-output", t!("render-output"), |ui| output_rows(ui, state, w, s));
    section(ui, "render-environment", t!("render-environment"), |ui| environment_rows(ui, state, w, s));
}

fn quality_rows(ui: &mut Ui, state: &mut AppState, r: &Resume, s: &RenderSettings) {
    let preset = preset_of(s.samples, s.max_depth);
    let text = preset.map_or_else(|| t!("render-custom"), preset_name);
    let mut picked = None;
    row(ui, &t!("render-preset"), |ui| {
        egui::ComboBox::from_id_salt("render-preset").selected_text(text).show_ui(ui, |ui| {
            for i in 0..PRESETS.len() {
                if ui.selectable_label(preset == Some(i), preset_name(i)).clicked() {
                    picked = Some(i);
                }
            }
        });
    });
    if let Some((samples, bounces)) = picked.and_then(|i| PRESETS.get(i)) {
        edit(state, r, |x| (x.samples, x.max_depth) = (*samples, *bounces));
    }
    let mut samples = s.samples;
    let tip = t!("render-samples-tip");
    let resp = row_tip(ui, &t!("render-samples"), Some(&tip), |ui| {
        ui.add(DragValue::new(&mut samples).range(MIN_SAMPLES..=MAX_SAMPLES).speed(1.0))
    });
    if resp.changed() && samples != s.samples {
        edit(state, r, |x| x.samples = samples);
    }
    let mut bounces = s.max_depth;
    let tip = t!("render-bounces-tip");
    let resp = row_tip(ui, &t!("render-bounces"), Some(&tip), |ui| {
        ui.add(DragValue::new(&mut bounces).range(MIN_DEPTH..=MAX_DEPTH).speed(0.1))
    });
    if resp.changed() && bounces != s.max_depth {
        edit(state, r, |x| x.max_depth = bounces);
    }
    let mut clamp = s.clamp;
    let off = t!("render-clamp-off");
    let tip = t!("render-clamp-tip");
    let resp = row_tip(ui, &t!("render-clamp"), Some(&tip), |ui| {
        let off_text = off.clone();
        ui.add(
            DragValue::new(&mut clamp)
                .range(MIN_CLAMP..=MAX_CLAMP)
                .speed(0.1)
                .custom_formatter(move |v, _| if v <= 0.0 { off_text.clone() } else { cc_views::format::number(v, 1) })
                .custom_parser(move |text| {
                    if text.trim().eq_ignore_ascii_case(off.trim()) { Some(0.0) } else { parse_number(text) }
                }),
        )
    });
    if resp.changed() && clamp != s.clamp {
        edit(state, r, |x| x.clamp = clamp.clamp(MIN_CLAMP, MAX_CLAMP));
    }
    let label = t!("render-clay");
    let tip = t!("render-clay-tip");
    let mut clay = s.clay;
    if row_tip(ui, &label, Some(&tip), |ui| check(ui, &mut clay, &label)).changed() {
        edit(state, r, |x| x.clay = clay);
    }
}

fn output_rows(ui: &mut Ui, state: &mut AppState, w: &mut RenderWindow, s: &RenderSettings) {
    let r = w.resume;
    let size = [s.width, s.height];
    let fixed = SIZES.iter().position(|(a, b)| [*a, *b] == size);
    let viewport = r.view_size.filter(|v| *v == size && fixed.is_none());
    let text = if w.size_custom {
        t!("render-custom")
    } else if let Some(i) = fixed.and_then(|i| SIZES.get(i)) {
        format!("{} × {}", i.0, i.1)
    } else if let Some([a, b]) = viewport {
        t!("render-size-view", "width" => a, "height" => b)
    } else {
        t!("render-custom")
    };
    // Picked: a size, or `None` for Custom.
    let mut picked: Option<Option<[u32; 2]>> = None;
    row(ui, &t!("render-size"), |ui| {
        egui::ComboBox::from_id_salt("render-size").selected_text(text).show_ui(ui, |ui| {
            if let Some([a, b]) = r.view_size {
                let on = !w.size_custom && viewport.is_some();
                if ui.selectable_label(on, t!("render-size-view", "width" => a, "height" => b)).clicked() {
                    picked = Some(Some([a, b]));
                }
            }
            for (i, (a, b)) in SIZES.iter().enumerate() {
                if ui.selectable_label(!w.size_custom && fixed == Some(i), format!("{a} × {b}")).clicked() {
                    picked = Some(Some([*a, *b]));
                }
            }
            let custom = w.size_custom || (fixed.is_none() && viewport.is_none());
            if ui.selectable_label(custom, t!("render-custom")).clicked() {
                picked = Some(None);
            }
        });
    });
    match picked {
        Some(Some([a, b])) => {
            w.size_custom = false;
            let (a, b) = within_pixels(a, b, true);
            edit(state, &r, |x| (x.width, x.height) = (a, b));
        }
        Some(None) => w.size_custom = true,
        None => {}
    }
    let (mut width, mut height) = (s.width, s.height);
    let (cw, ch) = row(ui, &t!("render-dimensions"), |ui| {
        ui.spacing_mut().interact_size.x = ((ui.available_width() - 24.0) / 2.0).max(40.0);
        let cw = ui.add(DragValue::new(&mut width).range(MIN_SIZE..=MAX_SIZE).speed(1.0)).changed();
        ui.label(RichText::new("×").color(PALETTE.text_faint));
        let ch = ui.add(DragValue::new(&mut height).range(MIN_SIZE..=MAX_SIZE).speed(1.0)).changed();
        (cw, ch)
    });
    if (cw || ch) && (width, height) != (s.width, s.height) {
        w.size_custom = true;
        let (a, b) = within_pixels(width, height, cw);
        edit(state, &r, |x| (x.width, x.height) = (a, b));
    }
    let mut fov = s.fov_deg;
    let resp = row(ui, &t!("render-fov"), |ui| {
        ui.add(
            DragValue::new(&mut fov)
                .range(MIN_FOV_DEG..=MAX_FOV_DEG)
                .speed(0.2)
                .custom_formatter(|v, _| format!("{}°", cc_views::format::number(v, 1)))
                .custom_parser(parse_number),
        )
    });
    if resp.changed() && fov != s.fov_deg {
        edit(state, &r, |x| x.fov_deg = fov.clamp(MIN_FOV_DEG, MAX_FOV_DEG));
    }
    let mut exposure = s.exposure;
    let tip = t!("render-exposure-tip");
    let resp = row_tip(ui, &t!("render-exposure"), Some(&tip), |ui| {
        ui.add(decimal(&mut exposure, 1).range(MIN_EXPOSURE..=MAX_EXPOSURE).speed(0.02))
    });
    if resp.changed() {
        // In steps of a tenth of a stop.
        let stops = ((exposure * 10.0).round() / 10.0).clamp(MIN_EXPOSURE, MAX_EXPOSURE);
        if stops != s.exposure {
            edit(state, &r, |x| x.exposure = stops);
        }
    }
    let tone_label = |t: ToneMap| match t {
        ToneMap::Filmic => t!("render-tone-filmic"),
        ToneMap::Standard => t!("render-tone-standard"),
    };
    let mut tone = s.tone_map;
    row(ui, &t!("render-tone-map"), |ui| {
        egui::ComboBox::from_id_salt("render-tone-map").selected_text(tone_label(tone)).show_ui(ui, |ui| {
            for t in ToneMap::ALL {
                ui.selectable_value(&mut tone, t, tone_label(t));
            }
        });
    });
    if tone != s.tone_map {
        edit(state, &r, |x| x.tone_map = tone);
    }
}

fn environment_rows(ui: &mut Ui, state: &mut AppState, w: &mut RenderWindow, s: &RenderSettings) {
    let r = w.resume;
    let env = &s.environment;
    let mut sky = env.sky_strength;
    let resp = row(ui, &t!("render-sky-strength"), |ui| {
        ui.add(decimal(&mut sky, 2).range(MIN_STRENGTH..=MAX_STRENGTH).speed(0.01))
    });
    if resp.changed() && sky != env.sky_strength {
        edit(state, &r, |x| x.environment.sky_strength = sky.clamp(MIN_STRENGTH, MAX_STRENGTH));
    }
    let mut sun = env.sun_strength;
    let resp = row(ui, &t!("render-sun-strength"), |ui| {
        ui.add(decimal(&mut sun, 2).range(MIN_STRENGTH..=MAX_STRENGTH).speed(0.01))
    });
    if resp.changed() && sun != env.sun_strength {
        edit(state, &r, |x| x.environment.sun_strength = sun.clamp(MIN_STRENGTH, MAX_STRENGTH));
    }
    let mut color = env.ground_color;
    if row(ui, &t!("render-ground-color"), |ui| ui.color_edit_button_srgb(&mut color)).changed()
        && color != env.ground_color
    {
        edit(state, &r, |x| x.environment.ground_color = color);
    }
    let label = t!("render-ground-plane");
    let mut ground = env.ground_plane;
    if row(ui, &label, |ui| check(ui, &mut ground, &label)).changed() {
        edit(state, &r, |x| x.environment.ground_plane = ground);
    }
    crate::table::button_row(ui, |ui| {
        if ui.button(t!("render-sun-settings")).clicked() {
            w.open_sun = true;
        }
    });
}

// ---------------------------------------------------------------- rendering

impl RenderWindow {
    /// Start a render of the view as it is now (stopping one running).
    fn start(&mut self, ctx: &Context, state: &mut AppState) {
        self.run = None;
        let view_id = self.resume.view;
        let Some(view) = state.project().view(view_id).cloned() else { return };
        let mut settings = view.render.clone().unwrap_or_default();
        settings.repair();
        let output = [settings.width, settings.height];
        let region = self.region.filter(|(_, at)| *at == output).map(|(r, _)| r);
        // A region is laid over the last image when it has the same size.
        let base = region.and(self.image.clone()).filter(|i| [i.width, i.height] == output && i.is_valid());
        let job = Job {
            geom: state.session.view_geometry(view_id).clone(),
            project: state.project().clone(),
            temporary: state.session.temporary(view_id).cloned(),
            camera: self.resume.camera,
            fitted: self.resume.fitted,
            settings: settings.clone(),
            region,
            view,
        };
        self.generation += 1;
        let generation = self.generation;
        let shared = Arc::new(Shared::new((settings.exposure, settings.tone_map)));
        let (steps_tx, steps_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let max_side = ctx.input(|i| i.max_texture_side).max(1);
        let display = Display { output, base, max_side, generation, ctx: ctx.clone() };
        let (shared_r, shared_d) = (shared.clone(), shared.clone());
        let tracer = std::thread::Builder::new().name("render".into()).spawn(move || trace(job, &shared_r, &steps_tx));
        let shower = std::thread::Builder::new()
            .name("render-display".into())
            .spawn(move || display.run(&steps_rx, &shared_d, &tx));
        self.before = std::mem::replace(&mut self.phase, Phase::Preparing);
        if tracer.is_err() || shower.is_err() {
            shared.stop.store(true, Ordering::Relaxed);
            self.phase = Phase::Failed(t!("render-failed", "error" => t!("render-thread-failed")));
            return;
        }
        self.run = Some(Run { generation, shared, rx, settings: traced(&settings), pictured: false });
    }

    /// Stop the render: what is rendered stays (a render with no picture yet
    /// is dropped, and the window shows what it showed before).
    fn stop(&mut self) {
        match &self.run {
            Some(run) if run.pictured => run.shared.stop.store(true, Ordering::Relaxed),
            Some(_) => {
                self.run = None;
                self.phase = std::mem::replace(&mut self.before, Phase::Idle);
            }
            None => {}
        }
    }

    /// Take the messages of the render running.
    fn poll(&mut self) {
        let mut messages = Vec::new();
        let mut gone = false;
        if let Some(run) = &self.run {
            loop {
                match run.rx.try_recv() {
                    Ok(m) => messages.push(m),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        gone = true;
                        break;
                    }
                }
            }
        }
        for (generation, message) in messages {
            self.apply(generation, message);
        }
        if gone && self.run.is_some() {
            self.run = None;
            self.phase = Phase::Failed(t!("render-failed", "error" => t!("render-thread-failed")));
        }
    }

    fn apply(&mut self, generation: u64, message: Message) {
        // Only the render running speaks (each has its own channel; this is a second guard).
        let Some(run) = self.run.as_mut().filter(|r| r.generation == generation) else { return };
        match message {
            Message::Built { triangles } => self.triangles = Some(triangles),
            Message::Frame { picture, mapping, samples, total, elapsed } => {
                run.pictured = true;
                self.pending = Some(picture);
                self.shown = Some(mapping);
                self.phase = Phase::Rendering { samples, total, elapsed };
            }
            Message::Finished { image, picture, mapping, samples, total, elapsed, stopped } => {
                let settings = run.settings.clone();
                self.run = None;
                match image {
                    Some(image) => {
                        self.image = Some(Arc::new(image));
                        self.rendered = Some(settings);
                        if let Some(p) = picture {
                            self.pending = Some(p);
                        }
                        self.shown = Some(mapping);
                        self.phase = Phase::Done { samples, total, elapsed, stopped };
                    }
                    None => self.phase = std::mem::replace(&mut self.before, Phase::Idle),
                }
            }
            Message::Failed(why) => {
                self.run = None;
                let why = why.unwrap_or_else(|| t!("render-thread-failed"));
                self.phase = Phase::Failed(t!("render-failed", "error" => why));
            }
        }
    }

    /// Show the image with `wanted` exposure and tone mapping: the render
    /// running takes it for its next picture; a finished image is mapped
    /// again on a thread.
    fn follow_mapping(&mut self, ctx: &Context, wanted: Mapping) {
        if let Some(run) = &self.run {
            run.shared.set_mapping(wanted);
            return;
        }
        if let Some(rx) = &self.remap {
            match rx.try_recv() {
                Ok((mapping, picture)) => {
                    if let Some(p) = picture {
                        self.pending = Some(p);
                    }
                    self.shown = Some(mapping);
                    self.remap = None;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.remap = None;
                    // Not mapped: do not try again for this mapping.
                    self.shown = Some(wanted);
                }
            }
        }
        let Some(image) = self.image.clone() else { return };
        if self.shown == Some(wanted) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let max_side = ctx.input(|i| i.max_texture_side).max(1);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("render-remap".into()).spawn(move || {
            let picture = std::panic::catch_unwind(|| picture_of(&image, wanted, max_side)).ok().flatten();
            let _ = tx.send((wanted, picture));
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.remap = Some(rx),
            Err(_) => self.shown = Some(wanted),
        }
    }

    /// Upload the newest picture to the window's texture.
    fn upload(&mut self, ctx: &Context) {
        let Some(picture) = self.pending.take() else { return };
        let options =
            egui::TextureOptions { magnification: egui::TextureFilter::Nearest, ..egui::TextureOptions::LINEAR };
        match &mut self.texture {
            Some(t) => t.set(picture, options),
            None => self.texture = Some(ctx.load_texture("render-window", picture, options)),
        }
    }
}

/// `image` exposed and tone mapped as a picture no side of which passes
/// `max_side` (a larger image is shown shrunk); `None` for an image whose
/// pixels do not fill it.
fn picture_of(image: &Image, (exposure, tone): Mapping, max_side: usize) -> Option<ColorImage> {
    if !image.is_valid() {
        return None;
    }
    rgba_picture(image.width, image.height, &image.to_rgba8(exposure, tone), max_side)
}

/// RGBA bytes of a `width` × `height` image as a picture no side of which
/// passes `max_side` (every k-th pixel of a larger one).
fn rgba_picture(width: u32, height: u32, rgba: &[u8], max_side: usize) -> Option<ColorImage> {
    let (w, h) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    if w == 0 || h == 0 || rgba.len() != w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    let k = w.max(h).div_ceil(max_side.max(1)).max(1);
    let (sw, sh) = (w.div_ceil(k), h.div_ceil(k));
    let mut pixels = Vec::with_capacity(sw * sh);
    for y in 0..sh {
        for x in 0..sw {
            let i = ((y * k) * w + x * k) * 4;
            let p = rgba.get(i..i + 3)?;
            pixels.push(Color32::from_rgb(p[0], p[1], p[2]));
        }
    }
    Some(ColorImage::new([sw, sh], pixels))
}

/// The render thread: build the scene and trace it, handing each picture
/// (and how it ended) to the display thread.
fn trace(job: Job, shared: &Shared, tx: &mpsc::Sender<Step>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| traced_run(job, shared, tx)));
    let last = match result {
        Ok(Ok((samples, stopped))) => Step::Done { samples, stopped },
        Ok(Err(e)) => Step::Failed(Some(e.to_string())),
        Err(_) => Step::Failed(None),
    };
    let _ = tx.send(last);
}

fn traced_run(job: Job, shared: &Shared, tx: &mpsc::Sender<Step>) -> Result<(u32, bool), RenderError> {
    let Job { project, geom, view, camera, fitted, settings, temporary, region } = job;
    // View filters may compare formatted values: the project's units, as on the window's thread.
    cc_views::format::set_units(&project.units);
    let camera =
        if fitted { cc_raytracing::camera::fit(&camera, &geom, settings.fov_deg, settings.aspect()) } else { camera };
    let scene = RenderScene::build(&project, &geom, &view, &camera, &settings, temporary.as_ref())?;
    drop((project, geom));
    let _ = tx.send(Step::Built { triangles: scene.triangles() });
    if shared.stopped() {
        return Ok((0, true));
    }
    let mut last: Option<Instant> = None;
    let outcome = cc_raytracing::run(&scene, region, |p| {
        let stop = shared.stopped();
        if stop || p.is_done() || last.is_none_or(|t| t.elapsed() >= FRAME_INTERVAL) {
            last = Some(Instant::now());
            let (image, region) = match region {
                Some(_) => (p.region_image(), Some(p.region())),
                None => (p.image(), None),
            };
            let frame =
                Step::Frame { image, region, samples: p.samples(), total: p.total_samples(), elapsed: p.elapsed() };
            if tx.send(frame).is_err() {
                return Control::Stop;
            }
        }
        if stop { Control::Stop } else { Control::Continue }
    })?;
    Ok((outcome.samples, outcome.stopped))
}

/// The display thread's part: the output's size, the image a region is laid
/// over, the largest texture side, the render's number and the window's context.
struct Display {
    output: [u32; 2],
    base: Option<Arc<Image>>,
    max_side: usize,
    generation: u64,
    ctx: Context,
}

/// The image being shown (linear) and its bytes as last mapped.
struct Canvas {
    image: Image,
    rgba: Vec<u8>,
    mapping: Mapping,
}

impl Display {
    /// Map the newest picture of the render thread for the window, until
    /// the render ends or the window goes away.
    fn run(self, rx: &mpsc::Receiver<Step>, shared: &Shared, tx: &mpsc::Sender<(u64, Message)>) {
        let send = |m: Message| {
            let sent = tx.send((self.generation, m)).is_ok();
            if sent {
                self.ctx.request_repaint();
            }
            sent
        };
        let mut canvas: Option<Canvas> = None;
        let (mut total, mut elapsed) = (0, Duration::ZERO);
        loop {
            let Ok(first) = rx.recv() else {
                send(Message::Failed(None));
                return;
            };
            let mut steps = vec![first];
            steps.extend(rx.try_iter());
            // Only the newest picture is worth mapping.
            let newest = steps.iter().rposition(|s| matches!(s, Step::Frame { .. }));
            for (i, step) in steps.into_iter().enumerate() {
                match step {
                    Step::Frame { .. } if Some(i) != newest => {}
                    Step::Frame { image, region, samples, total: t, elapsed: e } => {
                        (total, elapsed) = (t, e);
                        let mapping = shared.mapping();
                        let Some(picture) = self.paint(&mut canvas, image, region, mapping) else { continue };
                        if !send(Message::Frame { picture, mapping, samples, total, elapsed }) {
                            return;
                        }
                    }
                    Step::Built { triangles } => {
                        if !send(Message::Built { triangles }) {
                            return;
                        }
                    }
                    Step::Done { samples, stopped } => {
                        let mapping = shared.mapping();
                        let (image, picture) = match canvas.take() {
                            Some(mut c) => {
                                if c.mapping != mapping {
                                    c.rgba = c.image.to_rgba8(mapping.0, mapping.1);
                                }
                                let picture = rgba_picture(c.image.width, c.image.height, &c.rgba, self.max_side);
                                (Some(c.image), picture)
                            }
                            None => (None, None),
                        };
                        send(Message::Finished { image, picture, mapping, samples, total, elapsed, stopped });
                        return;
                    }
                    Step::Failed(why) => {
                        send(Message::Failed(why));
                        return;
                    }
                }
            }
        }
    }

    /// Lay `image` (the whole picture, or the region's) on the canvas and map it.
    fn paint(
        &self,
        canvas: &mut Option<Canvas>,
        image: Image,
        region: Option<Region>,
        mapping: Mapping,
    ) -> Option<ColorImage> {
        let [w, h] = self.output;
        match region {
            None => {
                let rgba = image.to_rgba8(mapping.0, mapping.1);
                let picture = rgba_picture(image.width, image.height, &rgba, self.max_side);
                *canvas = Some(Canvas { image, rgba, mapping });
                picture
            }
            Some(r) => {
                let c = canvas.get_or_insert_with(|| {
                    let image = self
                        .base
                        .as_deref()
                        .filter(|b| [b.width, b.height] == self.output && b.is_valid())
                        .cloned()
                        .unwrap_or_else(|| Image {
                            width: w,
                            height: h,
                            pixels: vec![[0.0; 3]; (w as usize).saturating_mul(h as usize)],
                        });
                    let rgba = image.to_rgba8(mapping.0, mapping.1);
                    Canvas { image, rgba, mapping }
                });
                if c.mapping != mapping {
                    c.rgba = c.image.to_rgba8(mapping.0, mapping.1);
                    c.mapping = mapping;
                }
                if image.is_valid() && image.width == r.width && image.height == r.height {
                    let bytes = image.to_rgba8(mapping.0, mapping.1);
                    let (cw, rw) = (c.image.width as usize, r.width as usize);
                    for row in 0..r.height as usize {
                        let y = r.y as usize + row;
                        let (to, from) = (y * cw + r.x as usize, row * rw);
                        if let (Some(dst), Some(src)) =
                            (c.image.pixels.get_mut(to..to + rw), image.pixels.get(from..from + rw))
                        {
                            dst.copy_from_slice(src);
                        }
                        if let (Some(dst), Some(src)) =
                            (c.rgba.get_mut(to * 4..(to + rw) * 4), bytes.get(from * 4..(from + rw) * 4))
                        {
                            dst.copy_from_slice(src);
                        }
                    }
                }
                rgba_picture(c.image.width, c.image.height, &c.rgba, self.max_side)
            }
        }
    }
}

// ---------------------------------------------------------------- saving

/// Save the last image (*Save Image…*): PNG and JPEG exposed and tone
/// mapped as shown, OpenEXR and Radiance HDR in linear light.
fn save_image(state: &mut AppState, w: &RenderWindow) {
    let Some(image) = w.image.clone() else { return };
    let Some(settings) = current(state, w.resume.view) else { return };
    let name = state.project().view(w.resume.view).map(|v| v.name.clone()).unwrap_or_default();
    let dialog = rfd::FileDialog::new()
        .add_filter(t!("file-filter-png"), &["png"])
        .add_filter(t!("file-filter-jpeg"), &["jpg", "jpeg"])
        .add_filter(t!("file-filter-exr"), &["exr"])
        .add_filter(t!("file-filter-hdr"), &["hdr"])
        .set_file_name(format!("{name}.png"));
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("png");
    }
    let (exposure, tone) = (settings.exposure, settings.tone_map);
    let saved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| image.save(&path, exposure, tone)));
    state.message = match saved {
        Ok(Ok(())) => t!("msg-exported", "path" => path.display()),
        Ok(Err(e)) => t!("msg-export-failed", "error" => e.to_string()),
        Err(_) => t!("msg-export-failed", "error" => t!("render-thread-failed")),
    };
}

// ---------------------------------------------------------------- tests

/// Wait for the render running in the open window to end (tests).
#[cfg(test)]
pub fn wait(state: &mut AppState) {
    let deadline = Instant::now() + Duration::from_secs(600);
    while let Some(w) = state.dialogs.render.as_mut() {
        let Some(run) = &w.run else { return };
        match run.rx.recv_timeout(Duration::from_millis(50)) {
            Ok((generation, message)) => w.apply(generation, message),
            Err(mpsc::RecvTimeoutError::Timeout) => assert!(Instant::now() < deadline, "the render never ended"),
            Err(mpsc::RecvTimeoutError::Disconnected) => w.poll(),
        }
    }
}

#[cfg(test)]
impl RenderWindow {
    pub fn phase_done(&self) -> Option<(u32, bool)> {
        match self.phase {
            Phase::Done { samples, stopped, .. } => Some((samples, stopped)),
            _ => None,
        }
    }

    pub fn image(&self) -> Option<&Image> {
        self.image.as_deref()
    }

    pub fn is_rendering(&self) -> bool {
        self.rendering()
    }

    pub fn set_region(&mut self, region: Region, output: [u32; 2]) {
        self.region_mode = true;
        self.region = Some((region, output));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_and_presets() {
        assert_eq!(clock(Duration::from_secs(12)), "00:12");
        assert_eq!(clock(Duration::from_secs(3 * 60 + 7)), "03:07");
        assert_eq!(clock(Duration::from_secs(3600 + 61)), "1:01:01");
        assert_eq!(preset_of(128, 8), Some(1));
        assert_eq!(preset_of(100, 8), None);
        for (samples, bounces) in PRESETS {
            assert!((MIN_SAMPLES..=MAX_SAMPLES).contains(&samples));
            assert!((MIN_DEPTH..=MAX_DEPTH).contains(&bounces));
        }
        assert_eq!(parse_number(" 2,5 "), Some(2.5));
        assert_eq!(parse_number("45°"), Some(45.0));
        assert_eq!(parse_number("x"), None);
    }

    #[test]
    fn sizes_stay_within_the_limits() {
        assert_eq!(within_pixels(1920, 1080, true), (1920, 1080));
        let (w, h) = within_pixels(8192, 8192, true);
        assert_eq!(w, 8192);
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS);
        let (w, h) = within_pixels(8192, 8192, false);
        assert_eq!(h, 8192);
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS);
        assert_eq!(within_pixels(1, 99_999, true), (MIN_SIZE, MAX_SIZE));
        assert_eq!(clamp_size([0, 5000]), [MIN_SIZE, 5000]);
        for (w, h) in SIZES {
            assert_eq!(within_pixels(w, h, true), (w, h));
        }
    }

    #[test]
    fn regions_are_clamped_and_not_too_small() {
        let out = [100, 50];
        let r = region_between(egui::pos2(90.0, -5.0), egui::pos2(-3.0, 30.0), out).unwrap();
        assert_eq!((r.x, r.y, r.width, r.height), (0, 0, 90, 30));
        assert!(region_between(egui::pos2(10.0, 10.0), egui::pos2(15.0, 40.0), out).is_none());
        let shown = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(200.0, 100.0));
        assert_eq!(to_pixels(egui::pos2(110.0, 60.0), shown, out), egui::pos2(50.0, 25.0));
        assert_eq!(to_pixels(egui::pos2(-50.0, 500.0), shown, out), egui::pos2(0.0, 50.0));
    }

    #[test]
    fn pictures_fit_the_texture_limit() {
        let rgba = vec![200u8; 10 * 4 * 4];
        let p = rgba_picture(10, 4, &rgba, 4).unwrap();
        assert_eq!(p.size, [4, 2]);
        assert_eq!(p.pixels[0], Color32::from_rgb(200, 200, 200));
        assert_eq!(rgba_picture(10, 4, &rgba, 64).unwrap().size, [10, 4]);
        // Bytes that do not fill the size, and empty images, give no picture.
        assert!(rgba_picture(10, 5, &rgba, 64).is_none());
        assert!(rgba_picture(0, 4, &[], 64).is_none());
        let bad = Image { width: 3, height: 3, pixels: vec![[0.0; 3]; 4] };
        assert!(picture_of(&bad, (0.0, ToneMap::Filmic), 64).is_none());
    }

    #[test]
    fn fitting_keeps_the_aspect() {
        let outer = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(400.0, 200.0));
        let r = fit_rect(outer, 1.0);
        assert_eq!(r.size(), vec2(200.0, 200.0));
        assert_eq!(r.center(), outer.center());
        assert_eq!(fit_rect(outer, 4.0).size(), vec2(400.0, 100.0));
        assert_eq!(fit_rect(outer, f32::NAN).size(), vec2(200.0, 200.0));
    }
}
