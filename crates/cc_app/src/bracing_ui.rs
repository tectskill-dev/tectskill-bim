//! Bracing in the app: the Bracing command (the bay between two selected
//! columns, else the two-click tool), the tool's options bar and the
//! properties of a selected bracing (every parameter through `element_set`,
//! which regenerates its braces in the same undo step).

use crate::grid_frame_ui::combo;
use crate::member_host_ui::{finished, length, member_rows};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::tools::Tool;
use cc_core::bracing::{self as br, Bracing, BracingPattern};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId, StructuralMaterial};
use cc_engine::commands::bracing::BracingCreate;
use egui::{DragValue, Ui};

fn patterns() -> Vec<(BracingPattern, String)> {
    BracingPattern::ALL.into_iter().map(|p| (p, crate::i18n::tr(p.key()))).collect()
}

/// The command for the bay between two columns, with the tool's options
/// (the storey height comes from the columns).
pub fn between_columns(state: &AppState, columns: Vec<u64>, material: StructuralMaterial) -> BracingCreate {
    let o = &state.bracing;
    BracingCreate {
        columns,
        pattern: Some(o.pattern.code().into()),
        stories: Some(o.stories),
        flip: o.flip,
        alternate: o.alternate,
        material: Some(material.code().into()),
        ..Default::default()
    }
}

/// The Bracing command: with two columns selected it braces their bay,
/// otherwise it starts the tool (two clicks: the bay's column axes).
pub fn start(state: &mut AppState) {
    let material = state.prefs.structure_material;
    if material == StructuralMaterial::Concrete {
        return;
    }
    let columns = crate::actions::selected_columns(state);
    if columns.len() == 2 {
        let command = between_columns(state, columns, material);
        if let Some(out) = crate::actions::run_command(state, command)
            && let Some(id) = out["id"].as_u64()
        {
            state.selection = [ElementId(id)].into_iter().collect();
        }
    } else {
        state.set_tool(Tool::Bracing { material });
        state.message = t!("hint-bracing");
    }
}

/// The flip and alternate boxes of a diagonal; true when one changed.
fn diagonal_boxes(ui: &mut Ui, pattern: BracingPattern, flip: &mut bool, alternate: &mut bool, rows: bool) -> bool {
    if pattern != BracingPattern::Diagonal {
        return false;
    }
    if rows {
        row(ui, &t!("prop-bracing-flip"), |ui| crate::table::checkbox(ui, flip)).changed()
            | row(ui, &t!("prop-bracing-alternate"), |ui| crate::table::checkbox(ui, alternate)).changed()
    } else {
        ui.checkbox(flip, t!("prop-bracing-flip")).changed()
            | ui.checkbox(alternate, t!("prop-bracing-alternate")).changed()
    }
}

/// The options bar of the Bracing tool.
pub fn tool_options(ui: &mut Ui, state: &mut AppState) {
    let o = &mut state.bracing;
    ui.label(t!("prop-truss-pattern"));
    combo(ui, "bracing-pattern", &mut o.pattern, &patterns());
    ui.label(t!("prop-bracing-story-height"));
    ui.add(crate::properties::length_drag(&mut o.height).speed(0.01).range(br::HEIGHT));
    ui.label(t!("prop-bracing-stories"));
    ui.add(DragValue::new(&mut o.stories).range(br::STORIES));
    diagonal_boxes(ui, o.pattern, &mut o.flip, &mut o.alternate, false);
    ui.separator();
    ui.label(crate::theme::muted(t!("hint-bracing")));
}

/// Change bracing `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its braces); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut Bracing)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::Bracing(b) = &mut e.kind {
            f(b);
        }
    });
}

/// The rows of a selected bracing.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, bracing: &Bracing) {
    let mut b = bracing.clone();
    let mut done = false;
    let beams: Vec<(ElementTypeId, String)> =
        state.project().types_of(Category::Beam).map(|x| (x.id, x.name.clone())).collect();
    section(ui, "bracing", t!("action-bracing"), |ui| {
        let pattern = b.pattern;
        row(ui, &t!("prop-truss-pattern"), |ui| {
            combo(ui, &format!("bracing-pattern-{}", id.0), &mut b.pattern, &patterns())
        });
        done |= b.pattern != pattern;
        done |= diagonal_boxes(ui, b.pattern, &mut b.flip, &mut b.alternate, true);
        done |= length(ui, &t!("prop-bracing-story-height"), &mut b.height, br::HEIGHT);
        done |= finished(&row(ui, &t!("prop-bracing-stories"), |ui| {
            ui.add(DragValue::new(&mut b.stories).range(br::STORIES))
        }));
        done |= length(ui, &t!("prop-base-offset"), &mut b.base_offset, br::BASE_OFFSET);
        readonly_row(ui, &t!("prop-length"), cc_views::format::length(bracing.length()));
        if member_rows(ui, bracing) {
            b.detached.clear();
            done = true;
        }
    });
    section(ui, "bracing-types", t!("section-type"), |ui| {
        let before = b.brace_type;
        row(ui, &t!("label-brace-type"), |ui| combo(ui, &format!("bracing-type-{}", id.0), &mut b.brace_type, &beams));
        done |= b.brace_type != before;
    });
    if b != *bracing {
        edit(state, id, "bracing", move |x| *x = b);
    }
    if done {
        state.session.doc.break_merge();
    }
}
