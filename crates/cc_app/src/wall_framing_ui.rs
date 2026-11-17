//! *Structure → Light Steel Frame / Wood Frame*: the dialog that runs
//! `wall_framing_create` on the selected walls (or the active level's), the
//! properties of a selected framing (through `element_set`, which
//! regenerates its members in the same undo step), and the *Framing* row
//! of its members.

use crate::dialog::Dialog;
use crate::grid_frame_ui::combo;
use crate::member_host_ui::{length, member_rows};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::PALETTE;
use cc_core::wall_framing::{self as wf, FramingSystem, WallFraming};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId, StructuralMaterial};
use cc_engine::Ref;
use cc_engine::commands::wall_framing::WallFramingCreate;
use egui::{Context, RichText, Ui};

#[derive(Debug, Clone)]
pub struct WallFramingDialog {
    pub system: FramingSystem,
    pub spacing: f64,
    /// `None`: the catalog default.
    pub stud_type: Option<ElementTypeId>,
    pub track_type: Option<ElementTypeId>,
    pub header_type: Option<ElementTypeId>,
    pub double_top: bool,
    pub blocking: bool,
    pub error: Option<String>,
}

impl WallFramingDialog {
    /// The system's defaults: spacing, top plates, catalog types, no blocking.
    pub fn new(system: FramingSystem) -> Self {
        Self {
            system,
            spacing: system.default_spacing(),
            stud_type: None,
            track_type: None,
            header_type: None,
            double_top: system.default_double_top(),
            blocking: false,
            error: None,
        }
    }
}

pub fn open(state: &mut AppState, system: FramingSystem) {
    state.dialogs.wall_framing = Some(WallFramingDialog::new(system));
}

/// The material of a system's members.
fn material(system: FramingSystem) -> StructuralMaterial {
    match system {
        FramingSystem::LightSteel => StructuralMaterial::Steel,
        FramingSystem::Wood => StructuralMaterial::Timber,
    }
}

fn systems() -> Vec<(FramingSystem, String)> {
    FramingSystem::ALL.into_iter().map(|s| (s, crate::i18n::tr(s.key()))).collect()
}

/// The catalog default, then the project's types of `category` in the system's material.
fn choices(state: &AppState, category: Category, system: FramingSystem) -> Vec<(Option<ElementTypeId>, String)> {
    let mut out = vec![(None, t!("opt-catalog-default"))];
    out.extend(state.project().types_of_material(category, material(system)).map(|t| (Some(t.id), t.name.clone())));
    out
}

/// The walls OK frames: the selected walls, or every wall of the active
/// level when nothing is selected (a selection without walls frames
/// nothing); only straight walls not framed yet.
pub fn walls_to_frame(state: &AppState) -> Vec<ElementId> {
    let p = state.project();
    let selected: Vec<ElementId> =
        state.selection.iter().copied().filter(|id| p.element(*id).is_some_and(|e| e.as_wall().is_some())).collect();
    let candidates = if state.selection.is_empty() {
        let Some(level) = state.active_level() else { return Vec::new() };
        p.elements.values().filter(|e| e.level == level && e.as_wall().is_some()).map(|e| e.id).collect()
    } else {
        selected
    };
    candidates
        .into_iter()
        .filter(|id| p.element(*id).and_then(|e| e.as_wall()).is_some_and(|w| w.arc().is_none()))
        .filter(|id| p.wall_framing_of(*id).is_none())
        .collect()
}

/// The command OK runs for `walls`.
pub fn command(d: &WallFramingDialog, walls: &[ElementId]) -> WallFramingCreate {
    let type_ref = |t: Option<ElementTypeId>| t.map(|t| Ref::Id(t.0));
    WallFramingCreate {
        walls: walls.iter().map(|w| w.0).collect(),
        system: Some(d.system.code().into()),
        spacing: Some(d.spacing),
        stud_type: type_ref(d.stud_type),
        track_type: type_ref(d.track_type),
        header_type: type_ref(d.header_type),
        double_top: Some(d.double_top),
        blocking: Some(d.blocking),
    }
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut d) = state.dialogs.wall_framing.clone() else { return };
    let studs = choices(state, Category::Column, d.system);
    let beams = choices(state, Category::Beam, d.system);
    let systems = systems();
    let mut ok = None;
    let response = Dialog::new("wall-framing", t!("dlg-wall-framing")).width(440.0).show(
        ctx,
        |ui| {
            let label = |ui: &mut Ui, k: String| {
                ui.label(RichText::new(k).color(PALETTE.text_muted));
            };
            egui::Grid::new("wall-framing-fields").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                label(ui, t!("prop-framing-system"));
                let system = d.system;
                combo(ui, "wf-system", &mut d.system, &systems);
                if d.system != system {
                    // Another system: its spacing, top plates and catalog types.
                    let fresh = WallFramingDialog::new(d.system);
                    (d.spacing, d.double_top) = (fresh.spacing, fresh.double_top);
                    (d.stud_type, d.track_type, d.header_type) = (None, None, None);
                }
                ui.end_row();
                label(ui, t!("prop-stud-spacing"));
                ui.add(crate::properties::length_drag(&mut d.spacing).range(wf::SPACING));
                ui.end_row();
                label(ui, t!("label-stud-type"));
                combo(ui, "wf-stud", &mut d.stud_type, &studs);
                ui.end_row();
                label(ui, t!("label-track-type"));
                combo(ui, "wf-track", &mut d.track_type, &beams);
                ui.end_row();
                label(ui, t!("label-header-type"));
                combo(ui, "wf-header", &mut d.header_type, &beams);
                ui.end_row();
            });
            ui.add_space(4.0);
            if d.system == FramingSystem::Wood {
                ui.checkbox(&mut d.double_top, t!("prop-double-top"));
            }
            ui.checkbox(&mut d.blocking, t!("prop-blocking"));
            if let Some(e) = &d.error {
                ui.add_space(6.0);
                ui.label(RichText::new(e).color(PALETTE.danger));
            }
        },
        |f| {
            ok = Some(f.primary(t!("action-ok")));
            f.secondary(t!("action-cancel"));
        },
    );
    if response.clicked.is_some() && response.clicked == ok {
        let walls = walls_to_frame(state);
        if walls.is_empty() {
            d.error = Some(t!("msg-no-walls-to-frame"));
            state.dialogs.wall_framing = Some(d);
            return;
        }
        match state.session.run(command(&d, &walls)) {
            Ok(out) => {
                let framings = out["framings"].as_array().cloned().unwrap_or_default();
                let count: usize = framings.iter().map(|f| f["members"].as_array().map_or(0, Vec::len)).sum();
                state.message = t!("msg-walls-framed", "walls" => framings.len(), "count" => count);
                state.selection = framings.iter().filter_map(|f| f["id"].as_u64()).map(ElementId).collect();
                state.dialogs.wall_framing = None;
            }
            Err(e) => {
                d.error = Some(e.message);
                state.dialogs.wall_framing = Some(d);
            }
        }
        return;
    }
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.wall_framing = None;
    } else {
        state.dialogs.wall_framing = Some(d);
    }
}

/// Change framing `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its members); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut WallFraming)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::WallFraming(x) = &mut e.kind {
            f(x);
        }
    });
}

/// The rows of a selected framing.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, framing: &WallFraming) {
    let mut x = framing.clone();
    let mut done = false;
    let all = |c: Category| -> Vec<(ElementTypeId, String)> {
        state.project().types_of(c).map(|t| (t.id, t.name.clone())).collect()
    };
    let (columns, beams) = (all(Category::Column), all(Category::Beam));
    section(ui, "wall-framing", t!("section-wall-framing"), |ui| {
        readonly_row(ui, &t!("prop-framing-system"), crate::i18n::tr(framing.system.key()));
        done |= length(ui, &t!("prop-stud-spacing"), &mut x.spacing, wf::SPACING);
        if x.system == FramingSystem::Wood
            && row(ui, &t!("prop-double-top"), |ui| crate::table::checkbox(ui, &mut x.double_top)).changed()
        {
            done = true;
        }
        if row(ui, &t!("prop-blocking"), |ui| crate::table::checkbox(ui, &mut x.blocking)).changed() {
            done = true;
        }
        if member_rows(ui, framing) {
            x.detached.clear();
            done = true;
        }
    });
    section(ui, "wall-framing-types", t!("section-type"), |ui| {
        let before = (x.stud_type, x.track_type, x.header_type);
        row(ui, &t!("label-stud-type"), |ui| combo(ui, &format!("wf-stud-{}", id.0), &mut x.stud_type, &columns));
        row(ui, &t!("label-track-type"), |ui| combo(ui, &format!("wf-track-{}", id.0), &mut x.track_type, &beams));
        row(ui, &t!("label-header-type"), |ui| combo(ui, &format!("wf-header-{}", id.0), &mut x.header_type, &beams));
        done |= (x.stud_type, x.track_type, x.header_type) != before;
    });
    if x != *framing {
        edit(state, id, "wall-framing", move |f| *f = x);
    }
    if done {
        state.session.doc.break_merge();
    }
}

/// The *Framing* row of a member of a wall framing: its button selects the framing.
pub fn member_row(ui: &mut Ui, state: &mut AppState, member: ElementId) {
    let p = state.project();
    let Some(host) = p
        .member_host_of(member)
        .filter(|h| matches!(p.element(*h).map(|e| &e.kind), Some(ElementKind::WallFraming(_))))
    else {
        return;
    };
    let mut select = false;
    section(ui, "member-framing", t!("section-wall-framing"), |ui| {
        select = row(ui, &t!("prop-wall-framing"), |ui| ui.button(t!("action-select-framing"))).clicked();
    });
    if select {
        state.selection = [host].into_iter().collect();
    }
}
