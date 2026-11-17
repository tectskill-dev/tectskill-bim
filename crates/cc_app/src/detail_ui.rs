//! Detail lines in the app: the *Detail Line* tool (Annotate tab) with its
//! line, polyline, arc, circle, ellipse and rectangle modes and the line
//! style in the options bar, the grips of selected detail lines (ends,
//! centers, an arc's middle, axis points), their properties, and their
//! previews in the Split and Offset tools. The tool draws with the sketch
//! mode's curve tools (see [`crate::sketch_mode::draw`]) and makes each
//! curve an element through `detail_line_create` as soon as it is drawn;
//! a polyline goes on from the last end until it closes or Esc.

use crate::paint::{HIGHLIGHT, PREVIEW, Xf};
use crate::sketch_mode::{self, Draw, Session, Target};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::tools::{Drag, Tool, TypedInput, draw_snap, snap_at};
use cc_core::detail::{DetailLine, DetailStyle, LineStyleRef, LineStyles};
use cc_core::sketch::SketchCurve;
use cc_core::{ElementId, ElementKind, ViewId};
use cc_engine::Ref;
use cc_engine::commands::detail::DetailLineCreate;
use cc_engine::commands::sketch::CurveArg;
use cc_views::format;
use egui::{Color32, Painter, Pos2, Response, Stroke, Ui};
use glam::DVec2;

/// Pick distance on screen, points.
const PICK_PX: f64 = 6.0;
/// Curves kept for chaining and snapping while drawing (older ones are elements already).
const MAX_KEPT: usize = 64;

/// How the Detail Line tool draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailMode {
    /// One line from two points.
    #[default]
    Line,
    /// Lines chained end to end.
    Polyline,
    /// An arc through its two ends and a point on it.
    Arc3,
    /// An arc by its center, start and end.
    ArcCenter,
    Circle,
    Ellipse,
    Rectangle,
}

impl DetailMode {
    pub const ALL: [DetailMode; 7] = [
        DetailMode::Line,
        DetailMode::Polyline,
        DetailMode::Arc3,
        DetailMode::ArcCenter,
        DetailMode::Circle,
        DetailMode::Ellipse,
        DetailMode::Rectangle,
    ];

    pub fn key(self) -> &'static str {
        match self {
            DetailMode::Line => "detail-mode-line",
            DetailMode::Polyline => "detail-mode-polyline",
            DetailMode::Arc3 => "detail-mode-arc3",
            DetailMode::ArcCenter => "detail-mode-arc-center",
            DetailMode::Circle => "detail-mode-circle",
            DetailMode::Ellipse => "detail-mode-ellipse",
            DetailMode::Rectangle => "detail-mode-rectangle",
        }
    }

    /// The sketch tool that draws it.
    fn draw(self) -> Draw {
        match self {
            DetailMode::Line | DetailMode::Polyline => Draw::Line,
            DetailMode::Arc3 => Draw::Arc3,
            DetailMode::ArcCenter => Draw::ArcCenter,
            DetailMode::Circle => Draw::Circle,
            DetailMode::Ellipse => Draw::Ellipse,
            DetailMode::Rectangle => Draw::Rect,
        }
    }
}

/// Options of the Detail Line tool (kept between uses, in the options bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DetailOptions {
    pub mode: DetailMode,
    /// A built-in style or one of the project's.
    pub style: LineStyleRef,
}

/// The curve being drawn in a view.
#[derive(Debug, Clone)]
pub struct DetailDraw {
    view: ViewId,
    session: Session,
}

impl DetailDraw {
    fn new(view: ViewId) -> Self {
        Self { view, session: Session::new(Target::Detail, Vec::new()) }
    }
}

/// The Detail Line tool in a 2D view: snaps, previews the curve and makes
/// each finished curve a detail line of the view.
pub fn tool(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, view: &cc_core::View, xf: &Xf) {
    let mut d = match state.detail_draw.take() {
        Some(d) if d.view == view.id => d,
        _ => DetailDraw::new(view.id),
    };
    let opts = state.detail;
    let s = &mut d.session;
    if s.draw != opts.mode.draw() {
        s.picks.clear();
    }
    s.draw = opts.mode.draw();
    s.chain = opts.mode == DetailMode::Polyline;
    if let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) {
        // A new curve starts a new chain: what was drawn before is already in the view.
        if s.picks.is_empty() {
            s.curves.clear();
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let extra = s.snap_points();
        let anchor = s.picks.last().copied();
        let sn = snap_at(state, view.level(), raw, anchor, &extra, xf.scale, shift);
        draw_snap(painter, xf, sn);
        // After the first point, a typed length (Tab: angle) or radius places
        // the next one, and Enter places it as a click would (issue #159).
        if !s.typing() {
            s.typed.clear();
        }
        let enter = !s.typed.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let click = resp.clicked() || enter;
        let had_typed = !s.typed.is_empty();
        let p = s.typed_point(sn.point);
        let before = s.curves.len();
        sketch_mode::draw(click, painter, s, p, xf, PICK_PX / xf.scale);
        if click && had_typed {
            s.typed.clear();
        }
        let new: Vec<SketchCurve> = s.curves.get(before..).map(<[SketchCurve]>::to_vec).unwrap_or_default();
        if s.curves.len() > MAX_KEPT {
            let extra = s.curves.len() - MAX_KEPT;
            s.curves.drain(..extra);
        }
        if !new.is_empty() {
            create(state, view.id, opts.style, &new);
        }
    }
    state.detail_draw = Some(d);
}

/// Make `curves` detail lines of `view` (one undo step); they are selected.
pub(crate) fn create(state: &mut AppState, view: ViewId, style: LineStyleRef, curves: &[SketchCurve]) {
    // A project style that was deleted meanwhile falls back to the default.
    let style = if style.exists(&state.project().line_styles) { style } else { LineStyleRef::default() };
    let command = DetailLineCreate {
        view: Ref::Id(view.0),
        curves: curves.iter().map(CurveArg::of).collect(),
        style: Some(style.code(&state.project().line_styles)),
    };
    if let Some(out) = crate::actions::run_command(state, command) {
        let ids = out["created"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).map(ElementId).collect());
        state.selection = ids.unwrap_or_default();
    }
}

/// The Detail Line tool takes typed values now: it has the first point of
/// a line, arc, circle, ellipse or rectangle.
pub fn typing(state: &AppState) -> bool {
    state.tool == Tool::DetailLine && state.detail_draw.as_ref().is_some_and(|d| d.session.typing())
}

/// Where typed values go: the detail line being drawn (see [`typing`]),
/// else the other tools' lengths.
pub fn typed_target(state: &mut AppState) -> &mut TypedInput {
    let detail = typing(state);
    match (detail, state.detail_draw.as_mut()) {
        (true, Some(d)) => &mut d.session.typed,
        _ => &mut state.typed,
    }
}

/// Esc while a curve is being drawn: drop the typed value, else its picks.
/// Returns whether it did.
pub fn escape(state: &mut AppState) -> bool {
    match state.detail_draw.as_mut() {
        Some(d) if !d.session.typed.is_empty() || d.session.typed.angle_active => {
            d.session.typed.clear();
            true
        }
        Some(d) if !d.session.picks.is_empty() => {
            d.session.picks.clear();
            true
        }
        _ => false,
    }
}

/// Whether a modify tool may run outside a plan: the elements it moves
/// are all owned by a view (detail lines, texts, dimensions), whose
/// coordinates are the view's own. Tools that pick their element take only
/// detail lines there (see [`crate::modify`]).
pub fn view_owned(state: &AppState, tool: &Tool) -> bool {
    match tool {
        Tool::TrimCorner | Tool::TrimExtend | Tool::Split | Tool::Offset => true,
        _ => {
            let targets = &state.interaction.targets;
            !targets.is_empty()
                && targets.iter().all(|id| state.project().element(*id).is_some_and(|e| e.owner_view().is_some()))
        }
    }
}

/// Whether a detail line passes within `r` of `p` (a quick bounds check for snapping).
pub fn near(line: &DetailLine, p: DVec2, r: f64) -> bool {
    let pts = line.curve.points();
    let Some(first) = pts.first() else { return false };
    let (lo, hi) = pts.iter().fold((*first, *first), |(lo, hi), q| (lo.min(*q), hi.max(*q)));
    p.x >= lo.x - r && p.y >= lo.y - r && p.x <= hi.x + r && p.y <= hi.y + r
}

/// The curve of detail line `id`.
fn curve_of(state: &AppState, id: ElementId) -> Option<SketchCurve> {
    match state.project().element(id).map(|e| &e.kind) {
        Some(ElementKind::DetailLine(l)) => Some(l.curve),
        _ => None,
    }
}

fn paint_curve(painter: &Painter, xf: &Xf, c: &SketchCurve, stroke: Stroke) {
    let pts: Vec<Pos2> = c.points().iter().map(|p| xf.to_screen(*p)).collect();
    painter.add(egui::Shape::line(pts, stroke));
}

/// The grips of the selected detail lines of the active view: dragging one
/// reshapes the line (`element_set`). Returns whether it took the pointer.
pub fn grips(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, xf: &Xf) -> bool {
    if let Some(Drag::DetailGrip { id, which }) = state.interaction.drag.clone() {
        let Some(curve) = curve_of(state, id) else {
            state.interaction.drag = None;
            return true;
        };
        let raw = resp.interact_pointer_pos().or(resp.hover_pos()).map(|p| xf.to_world(p));
        let new = raw.map(|raw| {
            let shift = ui.input(|i| i.modifiers.shift);
            let level = state.active_level();
            let s = snap_at(state, level, raw, None, &[], xf.scale, shift);
            draw_snap(painter, xf, s);
            sketch_mode::moved(&curve, which, s.point)
        });
        if let Some(new) = new.filter(SketchCurve::is_valid) {
            paint_curve(painter, xf, &new, Stroke::new(1.5, PREVIEW));
        }
        if resp.drag_stopped() {
            state.interaction.drag = None;
            if let Some(new) = new.filter(|c| c.is_valid() && *c != curve) {
                crate::engine_edit::element(state, id, None, |e| {
                    if let ElementKind::DetailLine(l) = &mut e.kind {
                        l.curve = new;
                    }
                });
            }
        }
        return true;
    }
    if state.interaction.drag.is_some() {
        return false;
    }
    let view = state.active_view;
    let handles: Vec<(ElementId, u8, DVec2)> = state
        .selection
        .iter()
        .take(50)
        .filter(|id| !state.project().is_pinned(**id))
        .filter_map(|id| match state.project().element(*id).map(|e| &e.kind) {
            Some(ElementKind::DetailLine(l)) if Some(l.view) == view => Some((*id, l.curve)),
            _ => None,
        })
        .flat_map(|(id, c)| sketch_mode::curve_grips(&c).into_iter().map(move |(w, p)| (id, w, p)))
        .collect();
    if handles.is_empty() {
        return false;
    }
    for (_, which, at) in &handles {
        let c = xf.to_screen(*at);
        if *which == 2 || *which >= 4 {
            painter.circle(c, 4.5, Color32::WHITE, Stroke::new(1.5, HIGHLIGHT));
        } else {
            let r = egui::Rect::from_center_size(c, egui::vec2(9.0, 9.0));
            painter.rect_filled(r, 1.5, Color32::WHITE);
            painter.rect_stroke(r, 1.5, Stroke::new(1.5, HIGHLIGHT), egui::StrokeKind::Middle);
        }
    }
    let at = |p: Pos2| handles.iter().find(|h| xf.to_screen(h.2).distance(p) <= 7.0).copied();
    if resp.hover_pos().and_then(at).is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some((id, which, _)) = ui.input(|i| i.pointer.press_origin()).and_then(at)
    {
        state.interaction.drag = Some(Drag::DetailGrip { id, which });
        return true;
    }
    false
}

/// The point of a curve nearest `p` (through its chords).
fn nearest_on(c: &SketchCurve, p: DVec2) -> Option<DVec2> {
    c.points()
        .windows(2)
        .map(|w| cc_geom::polygon::closest_on_segment(w[0], w[1], p).0)
        .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
}

/// The Split tool over a detail line: a tick where it splits; a click
/// splits it. Returns whether `id` is a detail line.
pub fn split_pick(painter: &Painter, xf: &Xf, state: &mut AppState, id: ElementId, raw: DVec2, click: bool) -> bool {
    let Some(c) = curve_of(state, id) else { return false };
    let Some(p) = nearest_on(&c, raw) else { return true };
    painter.circle_stroke(xf.to_screen(p), 5.0, Stroke::new(2.0, PREVIEW));
    if click {
        let command = cc_engine::commands::modify::ElementSplit { id: id.0, point: p.to_array() };
        crate::actions::run_command(state, command);
    }
    true
}

/// The Offset tool over a detail line: the copy on the cursor's side at the
/// distance of the options bar; a click makes it. Returns whether `id` is a
/// detail line.
pub fn offset_pick(
    painter: &Painter,
    xf: &Xf,
    state: &mut AppState,
    id: ElementId,
    raw: DVec2,
    distance: f64,
    click: bool,
) -> bool {
    use cc_core::sketch_edit::{offset, offset_left};
    let Some(c) = curve_of(state, id) else { return false };
    let d = distance.abs();
    let Some(moved) = offset(&c, d, raw) else {
        if click {
            state.message = t!("sketch-no-offset");
        }
        return true;
    };
    let pts: Vec<Pos2> = moved.points().iter().map(|p| xf.to_screen(*p)).collect();
    for w in pts.windows(2) {
        painter.extend(egui::Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.5, PREVIEW), 8.0, 4.0));
    }
    if click {
        // The engine offsets to the left of the curve for a positive distance.
        let signed = if offset_left(&c, d) == Some(moved) { d } else { -d };
        let command = cc_engine::commands::modify::ElementOffset { id: id.0, distance: signed };
        crate::actions::run_command(state, command);
    }
    true
}

/// The options bar of the Detail Line tool: the mode and the line style.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let o = &mut state.detail;
    for mode in DetailMode::ALL {
        ui.selectable_value(&mut o.mode, mode, crate::i18n::tr(mode.key()));
    }
    ui.separator();
    ui.label(t!("label-line-style"));
    let mut style = Some(o.style);
    if style_picker(ui, "detail-style", &state.session.doc.project.line_styles, &mut style, false)
        && let Some(s) = style
    {
        state.detail.style = s;
    }
    let o = &state.detail;
    let hint = match o.mode {
        DetailMode::Polyline => "hint-detail-polyline",
        DetailMode::Arc3 => "hint-detail-arc3",
        DetailMode::ArcCenter => "hint-detail-arc-center",
        DetailMode::Ellipse => "hint-detail-ellipse",
        _ => "hint-detail-line",
    };
    ui.label(crate::theme::muted(crate::i18n::tr(hint)));
}

/// The name of a line style as shown: a built-in style's translated name,
/// a project style's own name; `None` (no line) when allowed.
pub fn style_label(style: Option<LineStyleRef>, styles: &LineStyles) -> String {
    match style {
        None => t!("line-style-none"),
        Some(LineStyleRef::BuiltIn(s)) => crate::i18n::tr(s.key()),
        Some(LineStyleRef::Project(id)) => {
            styles.get(&id).map(|s| s.name.clone()).unwrap_or_else(|| crate::i18n::tr(DetailStyle::default().key()))
        }
    }
}

/// A combo box of the built-in line styles and the project's (and "none"
/// with `allow_none`); returns whether it changed.
pub fn style_picker(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    styles: &LineStyles,
    style: &mut Option<LineStyleRef>,
    allow_none: bool,
) -> bool {
    let before = *style;
    egui::ComboBox::from_id_salt(id).selected_text(style_label(*style, styles)).show_ui(ui, |ui| {
        if allow_none {
            ui.selectable_value(style, None, t!("line-style-none"));
        }
        for s in DetailStyle::ALL {
            ui.selectable_value(style, Some(LineStyleRef::BuiltIn(s)), crate::i18n::tr(s.key()));
        }
        if !styles.is_empty() {
            ui.separator();
        }
        for s in styles.values() {
            ui.selectable_value(style, Some(LineStyleRef::Project(s.id)), s.name.as_str());
        }
    });
    *style != before
}

/// The properties of a selected detail line: its style, view and size.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, line: &DetailLine) {
    let line = *line;
    section(ui, "detail-line", t!("section-graphics"), |ui| {
        let mut style = Some(line.style);
        let styles = state.project().line_styles.clone();
        let changed = row(ui, &t!("label-line-style"), |ui| {
            style_picker(ui, ("detail-line-style", id.0), &styles, &mut style, false)
        });
        if changed && let Some(style) = style {
            crate::engine_edit::element(state, id, None, |e| {
                if let ElementKind::DetailLine(l) = &mut e.kind {
                    l.style = style;
                }
            });
        }
        let view = state.project().view(line.view).map(|v| v.name.clone()).unwrap_or_default();
        readonly_row(ui, &t!("prop-view"), view);
    });
    section(ui, "dimensions", t!("section-dimensions"), |ui| {
        let kind = match line.curve {
            SketchCurve::Line { .. } => "detail-mode-line",
            SketchCurve::Arc { .. } => "detail-mode-arc3",
            SketchCurve::Circle { .. } => "detail-mode-circle",
            SketchCurve::Ellipse { .. } => "detail-mode-ellipse",
        };
        readonly_row(ui, &t!("prop-shape"), crate::i18n::tr(kind));
        let length: f64 = line.curve.points().windows(2).map(|w| w[0].distance(w[1])).sum();
        readonly_row(ui, &t!("prop-length"), format::length(length));
        match line.curve {
            SketchCurve::Arc { a, b, bulge } => {
                if let Some(arc) = cc_core::arc::Arc::from_bulge(a, b, bulge) {
                    readonly_row(ui, &t!("prop-radius"), format::length(arc.radius));
                }
            }
            SketchCurve::Circle { radius, .. } => readonly_row(ui, &t!("prop-radius"), format::length(radius)),
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_map_to_sketch_tools() {
        assert_eq!(DetailMode::Polyline.draw(), Draw::Line);
        assert_eq!(DetailMode::Rectangle.draw(), Draw::Rect);
        let line = DetailLine {
            view: ViewId(1),
            curve: SketchCurve::Line { a: DVec2::ZERO, b: DVec2::new(2.0, 0.0) },
            style: LineStyleRef::default(),
        };
        assert!(near(&line, DVec2::new(1.0, 0.05), 0.1));
        assert!(!near(&line, DVec2::new(1.0, 1.0), 0.1));
    }
}
