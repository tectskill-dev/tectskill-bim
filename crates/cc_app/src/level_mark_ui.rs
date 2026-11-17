//! Level marks in the app: the tool that places them in plans, elevations
//! and sections (through `level_mark_create`), their rows in the properties
//! palette and the rows of their type.

use crate::paint::{PREVIEW, Xf};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::tools::{Snap, draw_snap, label, run_create, snap_at};
use cc_core::level_mark::{LevelMark, MAX_LEVEL};
use cc_core::{ElementId, ElementKind, ElementTypeId, TypeData, View, ViewKind};
use cc_engine::Ref;
use cc_engine::commands::level_mark::LevelMarkCreate;
use cc_views::{Drawing2D, format};
use egui::{Painter, Response, Stroke, Ui};
use glam::DVec2;

/// Whether level marks go in `view`: plans, elevations and sections.
pub fn hosts(view: &View) -> bool {
    matches!(
        view.kind,
        ViewKind::FloorPlan { .. }
            | ViewKind::CeilingPlan { .. }
            | ViewKind::Elevation { .. }
            | ViewKind::Section { .. }
    )
}

/// The point under the cursor: the usual snaps (in elevations and sections
/// to the ends of the view's lines), else in elevations and sections the
/// nearest point of a line within reach, so a click lands on a floor's top.
fn snap(ui: &Ui, state: &mut AppState, view: &View, raw: DVec2, xf: &Xf, drawing: &Drawing2D) -> Snap {
    let shift = ui.input(|i| i.modifiers.shift);
    if view.is_plan() {
        return snap_at(state, view.level(), raw, None, &[], xf.scale, shift);
    }
    let lines = drawing.snap_lines();
    let ends: Vec<DVec2> = lines.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let s = snap_at(state, None, raw, None, &ends, xf.scale, shift);
    if !s.is_free() {
        return s;
    }
    let reach = 6.0 / xf.scale;
    lines
        .iter()
        .map(|(a, b)| cc_geom::polygon::closest_on_segment(*a, *b, raw).0)
        .filter(|q| q.distance(raw) < reach)
        .min_by(|q, r| q.distance(raw).total_cmp(&r.distance(raw)))
        .map_or(s, Snap::nearest)
}

/// The Level Mark tool: the levels a mark would write follow the cursor; a
/// click places one.
#[allow(clippy::too_many_arguments)]
pub fn tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    view: &View,
    type_id: ElementTypeId,
    xf: &Xf,
    drawing: &Drawing2D,
) {
    if !hosts(view) {
        if resp.clicked() {
            state.message = t!("msg-level-mark-view");
        }
        return;
    }
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let s = snap(ui, state, view, raw, xf, drawing);
    draw_snap(painter, xf, s);
    let at = xf.to_screen(s.point);
    painter.circle_stroke(at, 5.0, Stroke::new(1.5, PREVIEW));
    let p = state.project();
    let mark = LevelMark::new(view.id, s.point);
    if let Some(texts) = p.element_type(type_id).and_then(|t| cc_views::level_marks::texts(p, view, &mark, &t.data)) {
        label(painter, at + egui::vec2(10.0, -10.0), texts.join("\n"));
    }
    if resp.clicked() {
        let command = LevelMarkCreate {
            view: Ref::Id(view.id.0),
            position: s.point.to_array(),
            type_ref: Some(Ref::Id(type_id.0)),
            ..Default::default()
        };
        run_create(state, command);
    }
}

/// A level set on the mark or measured: a checkbox to set it and its value.
fn level_row(ui: &mut Ui, label: &str, set: &mut Option<f64>, measured: Option<f64>) -> (bool, Response) {
    row(ui, label, |ui| {
        let mut on = set.is_some();
        let toggled = crate::table::checkbox(ui, &mut on).changed();
        if toggled {
            *set = on.then(|| measured.unwrap_or(0.0));
        }
        let r = match set {
            Some(v) => ui.add(crate::properties::length_drag(v).range(-MAX_LEVEL..=MAX_LEVEL)),
            None => ui.label(measured.map(format::elevation).unwrap_or_default()),
        };
        (toggled, r)
    })
}

/// A level mark in the properties palette: its view, the side of its
/// values, the levels it writes (measured, or set) and whether the raw one shows.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, mark: &LevelMark) {
    let mut m = mark.clone();
    let p = state.project();
    let view = p.view(mark.view);
    let measured = view.and_then(|v| {
        cc_geom::level_mark::levels(p, v, &LevelMark { finished: None, raw: None, show_raw: true, ..mark.clone() })
    });
    let view_name = view.map(|v| v.name.clone()).unwrap_or_default();
    let mut done = false;
    section(ui, "level-mark", t!("section-level-mark"), |ui| {
        readonly_row(ui, &t!("prop-view"), view_name);
        done |= row(ui, &t!("prop-level-mark-flip"), |ui| crate::table::checkbox(ui, &mut m.flip)).changed();
        let (toggled, r) =
            level_row(ui, &t!("prop-level-mark-finished"), &mut m.finished, measured.map(|l| l.finished));
        done |= toggled || r.drag_stopped() || r.lost_focus();
        done |= row(ui, &t!("prop-level-mark-show-raw"), |ui| crate::table::checkbox(ui, &mut m.show_raw)).changed();
        if m.show_raw {
            let (toggled, r) = level_row(ui, &t!("prop-level-mark-raw"), &mut m.raw, measured.and_then(|l| l.raw));
            done |= toggled || r.drag_stopped() || r.lost_focus();
        }
    });
    if m != *mark {
        crate::engine_edit::element(state, id, Some(crate::properties::key((id, "level-mark"))), |e| {
            if let ElementKind::LevelMark(x) = &mut e.kind {
                **x = m;
            }
        });
    }
    if done {
        state.session.doc.break_merge();
    }
}

/// The rows of a level mark type: text size, datum and the suffixes.
pub fn type_rows(ui: &mut Ui, state: &mut AppState, tid: ElementTypeId, data: &TypeData) {
    let TypeData::LevelMark { text_size, datum, finished_suffix, raw_suffix } = data else { return };
    let (mut size, mut base) = (*text_size, *datum);
    let (mut finished, mut raw) = (finished_suffix.clone(), raw_suffix.clone());
    let mut changed = false;
    let mut done = false;
    section(ui, "type-level-mark", t!("section-level-mark"), |ui| {
        let r = row(ui, &t!("prop-text-size"), |ui| {
            ui.add(crate::properties::paper_drag(&mut size).speed(0.1).range(0.5..=50.0))
        });
        changed |= r.changed();
        done |= r.drag_stopped() || r.lost_focus();
        let r = row(ui, &t!("prop-level-mark-datum"), |ui| {
            ui.add(crate::properties::length_drag(&mut base).range(-MAX_LEVEL..=MAX_LEVEL))
        });
        changed |= r.changed();
        done |= r.drag_stopped() || r.lost_focus();
        for (label, text) in
            [(t!("prop-level-mark-finished-suffix"), &mut finished), (t!("prop-level-mark-raw-suffix"), &mut raw)]
        {
            let r = row(ui, &label, |ui| ui.text_edit_singleline(text));
            changed |= r.changed();
            done |= r.lost_focus();
        }
    });
    if changed {
        crate::engine_edit::element_type(state, tid, Some(crate::properties::key((tid, "level-mark-type"))), |t| {
            if let TypeData::LevelMark { text_size, datum, finished_suffix, raw_suffix } = &mut t.data {
                (*text_size, *datum) = (size, base);
                (*finished_suffix, *raw_suffix) = (finished.clone(), raw.clone());
            }
        });
    }
    if done {
        state.session.doc.break_merge();
    }
}
