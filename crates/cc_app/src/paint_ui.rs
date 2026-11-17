//! The Paint and Remove Paint tools: click a face of an element in a 3D view
//! to paint it with the material chosen in the options bar (`paint`), or to
//! take its paint off (`paint_remove`).

use crate::state::AppState;
use crate::t;
use crate::theme;
use crate::tools::Tool;
use cc_core::{ElementId, ViewId};
use cc_engine::Ref;
use cc_engine::commands::paint as cmd;
use egui::Ui;

/// Start painting (or removing paint): the material defaults to the last
/// one used, else the project's first.
pub fn start(state: &mut AppState, remove: bool) {
    let p = state.project();
    if p.material(&state.paint_material).is_none() {
        state.paint_material = p.materials.first().map(|m| m.name.clone()).unwrap_or_default();
    }
    state.set_tool(Tool::Paint { remove });
    state.message = if remove { t!("hint-remove-paint") } else { t!("hint-paint") };
}

/// The options bar: the material to paint with, and what to do.
pub fn options_bar(ui: &mut Ui, state: &mut AppState, remove: bool) {
    if !remove {
        ui.label(t!("prop-material"));
        let mut material = state.paint_material.clone();
        if crate::dialogs::material_combo(ui, state, "paint-material", &mut material) {
            state.paint_material = material;
        }
    }
    ui.label(theme::muted(if remove { t!("hint-remove-paint") } else { t!("hint-paint") }));
}

/// A click in 3D view `view` with the tool: `picked` is the element under
/// the pointer, `ray` the pointer's ray.
pub fn pick(state: &mut AppState, view: ViewId, remove: bool, picked: Option<ElementId>, ray: cc_geom::Ray) {
    let hit = picked.and_then(|id| Some((id, cc_geom::paint::face_hit(state.session.geom_for(view), id, ray)?)));
    let Some((id, (point, normal))) = hit else {
        state.message = if remove { t!("hint-remove-paint") } else { t!("hint-paint") };
        return;
    };
    let (point, normal) = (Some(point.to_array()), Some(normal.to_array()));
    let done = if remove {
        crate::actions::run_command(state, cmd::PaintRemove { element: id.0, point, normal }).is_some()
    } else {
        let Some(point) = point else { return };
        let material = Ref::Name(state.paint_material.clone());
        crate::actions::run_command(state, cmd::Paint { element: id.0, point, normal, material }).is_some()
    };
    if done {
        state.message = if remove { t!("msg-paint-removed") } else { t!("msg-painted") };
    }
}
