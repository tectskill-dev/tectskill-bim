//! Callouts in the app: the Callout tool draws a rectangle in a plan,
//! ceiling plan, section or elevation and makes a detail view of that
//! region (`callout_create`); the Select tool picks a callout's mark, moves
//! it and resizes it by its corners (the detail view's crop, `view_set`)
//! and opens its view on a double-click; the view's properties show the
//! callout's parent and open the view.

use crate::paint::{HIGHLIGHT, PREVIEW, Xf};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::tools::Drag;
use cc_core::{CropBox, View, ViewCategory, ViewId};
use cc_engine::Ref;
use cc_engine::commands::callout as cmd;
use cc_views::Drawing2D;
use cc_views::callout::CalloutHandle;
use egui::{Color32, Painter, Pos2, Response, Stroke, Ui};
use glam::DVec2;

/// Screen distance at which a corner is grabbed, points.
const GRIP_PX: f32 = 7.0;

/// Options of the Callout tool.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CalloutOptions {
    /// Scale denominator of the new detail view (`0`: about five times the parent's).
    pub scale: u32,
    /// A drafting view the callout points to instead of a new detail view.
    pub drafting: Option<ViewId>,
}

/// What a drag of a callout mark holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    /// The whole mark, moved.
    Body,
    /// A corner of the rectangle (as [`CropBox::corners`] counts them).
    Corner(usize),
}

/// A point in the active view: snapped in plans, rounded to the zoom elsewhere.
fn point(state: &mut AppState, view: &View, raw: DVec2, anchor: Option<DVec2>, xf: &Xf, painter: &Painter) -> DVec2 {
    if view.is_plan() {
        let s = crate::tools::snap_at(state, view.level(), raw, anchor, &[], xf.scale, false);
        crate::tools::draw_snap(painter, xf, s);
        s.point
    } else {
        let step = crate::tools::round_step(xf.scale);
        (raw / step).round() * step
    }
}

fn rect_outline(painter: &Painter, xf: &Xf, rect: CropBox, color: Color32) {
    let c = rect.corners();
    let stroke = Stroke::new(2.0, color);
    for i in 0..4 {
        painter.line_segment([xf.to_screen(c[i]), xf.to_screen(c[(i + 1) % 4])], stroke);
    }
}

fn mark_outline(painter: &Painter, xf: &Xf, h: &CalloutHandle, color: Color32) {
    rect_outline(painter, xf, h.rect, color);
    painter.circle_stroke(xf.to_screen(h.tag), (h.radius * xf.scale) as f32, Stroke::new(2.0, color));
}

/// The Callout tool: two clicks give opposite corners of the rectangle.
pub fn tool(_ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, view: &View, xf: &Xf) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let anchor = state.interaction.anchor;
    let p = point(state, view, raw, anchor, xf, painter);
    if let Some(a) = anchor {
        rect_outline(painter, xf, CropBox::new(a, p), PREVIEW);
        let size = (p - a).abs();
        crate::tools::label(
            painter,
            xf.to_screen(p),
            format!("{} × {}", cc_views::format::length(size.x), cc_views::format::length(size.y)),
        );
    }
    if !resp.clicked() {
        return;
    }
    let Some(a) = anchor else {
        state.interaction.anchor = Some(p);
        return;
    };
    let rect = CropBox::new(a, p);
    if !rect.is_valid() {
        return;
    }
    let options = state.callout;
    let drafting = options.drafting.filter(|d| state.project().view(*d).is_some_and(|v| v.is_drafting()));
    let command = cmd::CalloutCreate {
        parent: Ref::Id(view.id.0),
        rect: [rect.min.to_array(), rect.max.to_array()],
        scale: (options.scale > 0 && drafting.is_none()).then_some(options.scale.into()),
        name: None,
        drafting: drafting.map(|d| Ref::Id(d.0)),
    };
    state.interaction.anchor = None;
    let Some(out) = crate::actions::run_command(state, command) else { return };
    if let Some(id) = out["id"].as_u64() {
        state.section_selection = Some(ViewId(id));
        state.selection.clear();
        state.message = t!("msg-callout-created", "name" => out["name"].as_str().unwrap_or_default());
    }
    if drafting.is_some() {
        // A drafting view takes one callout.
        state.callout.drafting = None;
    }
}

/// The Callout tool's options: the detail view's scale, or a drafting view to point to.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let drafting: Vec<(ViewId, String)> = state
        .project()
        .views_of(ViewCategory::Drafting)
        .filter(|v| v.callout.is_none())
        .map(|v| (v.id, v.name.clone()))
        .collect();
    let mut o = state.callout;
    if o.drafting.is_some_and(|d| !drafting.iter().any(|(id, _)| *id == d)) {
        o.drafting = None;
    }
    ui.label(t!("callout-shows"));
    let current = o.drafting.and_then(|d| drafting.iter().find(|(id, _)| *id == d)).map(|(_, n)| n.clone());
    egui::ComboBox::from_id_salt("callout-target")
        .selected_text(current.unwrap_or_else(|| t!("callout-new-detail")))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut o.drafting, None, t!("callout-new-detail"));
            for (id, name) in &drafting {
                ui.selectable_value(&mut o.drafting, Some(*id), name);
            }
        });
    if o.drafting.is_none() {
        ui.label(t!("prop-scale"));
        let auto = t!("callout-scale-auto");
        let system = state.project().measurement_system();
        let label = |s: u32| cc_core::scale::label(system, s);
        let text = if o.scale == 0 { auto.clone() } else { label(o.scale) };
        egui::ComboBox::from_id_salt("callout-scale").selected_text(text).show_ui(ui, |ui| {
            ui.selectable_value(&mut o.scale, 0, auto);
            // Detail scales: up to 1:50 (1/4" = 1'-0" in imperial projects).
            for &s in cc_core::scale::choices(system).iter().filter(|s| **s <= 50) {
                ui.selectable_value(&mut o.scale, s, label(s));
            }
        });
    }
    state.callout = o;
    ui.separator();
    ui.label(crate::theme::muted(t!("msg-callout-hint")));
}

/// The selected callout's mark in `drawing`.
fn selected(state: &AppState, drawing: &Drawing2D) -> Option<CalloutHandle> {
    let view = state.section_selection?;
    drawing.callouts.iter().find(|c| c.view == view).copied()
}

/// Callout marks under the Select tool. Returns `true` when they took the
/// pointer this frame (the element selection is left alone).
pub fn select(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, xf: &Xf, drawing: &Drawing2D) -> bool {
    if drawing.callouts.is_empty() && !matches!(state.interaction.drag, Some(Drag::Callout { .. })) {
        return false;
    }
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let tol = 5.0 / xf.scale;
    let idle = state.interaction.drag.is_none();
    let current = selected(state, drawing);

    // The selected mark and its corners.
    let mut grips: Vec<(Pos2, Drag)> = Vec::new();
    if let Some(h) = &current {
        mark_outline(painter, xf, h, HIGHLIGHT);
        if idle {
            for (i, c) in h.rect.corners().into_iter().enumerate() {
                let p = xf.to_screen(c);
                let r = egui::Rect::from_center_size(p, egui::vec2(9.0, 9.0));
                painter.rect_filled(r, 1.5, Color32::WHITE);
                painter.rect_stroke(r, 1.5, Stroke::new(1.5, HIGHLIGHT), egui::StrokeKind::Middle);
                grips.push((p, Drag::Callout { view: h.view, part: Part::Corner(i), start: c }));
            }
        }
    }
    let grip_at = |p: Pos2| grips.iter().find(|(at, _)| at.distance(p) <= GRIP_PX).map(|(_, d)| d.clone());

    // Pre-selection highlight.
    if idle
        && let Some(h) = raw.and_then(|p| drawing.callout_at(p, tol))
        && Some(h.view) != state.section_selection
    {
        mark_outline(painter, xf, h, HIGHLIGHT.gamma_multiply(0.7));
    }
    if idle && resp.hover_pos().and_then(grip_at).is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // A double-click opens the callout's view (also when it follows a
    // click so closely that it counts as a triple one).
    if (resp.double_clicked() || resp.triple_clicked())
        && let Some(h) = raw.and_then(|p| drawing.callout_at(p, tol))
    {
        let view = h.view;
        state.interaction.drag = None;
        state.open_view(view);
        return true;
    }

    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
        let w = xf.to_world(origin);
        if let Some(drag) = grip_at(origin) {
            state.interaction.drag = Some(drag);
            return true;
        }
        if let Some(h) = drawing.callout_at(w, tol) {
            state.section_selection = Some(h.view);
            state.selection.clear();
            state.interaction.drag = Some(Drag::Callout { view: h.view, part: Part::Body, start: w });
            return true;
        }
        return false;
    }

    if let Some(Drag::Callout { view, part, start }) = state.interaction.drag.clone() {
        let Some(h) = drawing.callouts.iter().find(|c| c.view == view).copied() else {
            state.interaction.drag = None;
            return true;
        };
        let mut moved = None;
        if let Some(raw) = raw.or(state.cursor) {
            let step = crate::tools::round_step(xf.scale);
            let rect = match part {
                Part::Body => {
                    let d = ((raw - start) / step).round() * step;
                    crate::tools::label(
                        painter,
                        xf.to_screen(raw),
                        format!("{} {}", t!("label-move"), cc_views::format::length(d.length())),
                    );
                    h.rect.translated(d)
                }
                Part::Corner(i) => {
                    let fixed = h.rect.corners()[(i + 2) % 4];
                    CropBox::new(fixed, (raw / step).round() * step)
                }
            };
            rect_outline(painter, xf, rect, PREVIEW);
            moved = (rect.is_valid() && rect != h.rect).then_some(rect);
        }
        if resp.drag_stopped() {
            state.interaction.drag = None;
            if let Some(rect) = moved {
                crate::engine_edit::view(state, view, None, |x| {
                    x.set_callout_rect(rect);
                });
            }
        }
        return true;
    }

    if resp.clicked()
        && let Some(h) = raw.and_then(|p| drawing.callout_at(p, tol))
    {
        state.section_selection = Some(h.view);
        state.selection.clear();
        return true;
    }
    false
}

/// The callout and crop rows of a view's properties: the view its callout
/// is drawn in, opening the view, removing the callout or the crop.
pub fn view_rows(ui: &mut Ui, state: &mut AppState, id: ViewId) {
    let Some(view) = state.project().view(id).cloned() else { return };
    if view.callout.is_none() && view.crop.is_none() {
        return;
    }
    let parent = state.project().callout_parent(id).map(|p| p.name.clone());
    section(ui, "callout", t!("section-callout"), |ui| {
        if let Some(parent) = parent {
            readonly_row(ui, &t!("prop-callout-parent"), parent);
        }
        if let Some(rect) = view.callout_rect().or(view.crop) {
            let size = rect.size();
            let text = format!("{} × {}", cc_views::format::length(size.x), cc_views::format::length(size.y));
            readonly_row(ui, &t!("prop-crop"), text);
        }
        if state.active_view != Some(id) && row(ui, "", |ui| ui.button(t!("callout-open")).clicked()) {
            state.open_view(id);
        }
        if view.callout.is_some() {
            if row(ui, "", |ui| ui.button(t!("callout-remove")).clicked()) {
                crate::actions::run_command(state, cmd::CalloutRemove { view: Ref::Id(id.0) });
            }
        } else if row(ui, "", |ui| ui.button(t!("crop-clear")).clicked()) {
            let command =
                cc_engine::commands::views::ViewSet { view: Ref::Id(id.0), crop: Some(None), ..Default::default() };
            crate::actions::run_command(state, command);
        }
    });
}
