//! *Structure → Steel Building*: the dialog that runs
//! `steel_building_create` on the active level, and the properties of a
//! selected building (every parameter, through `element_set`, which
//! regenerates its members in the same undo step).

use crate::dialog::Dialog;
use crate::grid_frame_ui::combo;
use crate::member_host_ui::{finished, length, member_rows};
use crate::state::AppState;
use crate::t;
use crate::table::{row, section};
use crate::theme::PALETTE;
use cc_core::steel_building::{self as sb, FrameRoof, Secondary, SteelBuilding};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId, ProfileMaterial, StructuralMaterial};
use cc_engine::Ref;
use cc_engine::commands::steel_building::{SecondaryMembers, SteelBuildingCreate};
use egui::{Context, DragValue, RichText, Ui};

#[derive(Debug, Clone)]
pub struct SteelBuildingDialog {
    pub span: f64,
    pub eave_height: f64,
    pub slope_percent: f64,
    pub roof: FrameRoof,
    pub bays: u32,
    pub bay_spacing: f64,
    pub origin: [f64; 2],
    pub rotation: f64,
    /// `None`: the catalog default.
    pub column_type: Option<ElementTypeId>,
    pub rafter_type: Option<ElementTypeId>,
    pub purlins: bool,
    pub purlin_type: Option<ElementTypeId>,
    pub purlin_spacing: f64,
    pub girts: bool,
    pub girt_type: Option<ElementTypeId>,
    pub girt_spacing: f64,
    pub error: Option<String>,
}

impl Default for SteelBuildingDialog {
    fn default() -> Self {
        Self {
            span: sb::DEFAULT_SPAN,
            eave_height: sb::DEFAULT_EAVE_HEIGHT,
            slope_percent: sb::DEFAULT_SLOPE,
            roof: FrameRoof::Gable,
            bays: sb::DEFAULT_BAYS,
            bay_spacing: sb::DEFAULT_BAY_SPACING,
            origin: [0.0, 0.0],
            rotation: 0.0,
            column_type: None,
            rafter_type: None,
            purlins: true,
            purlin_type: None,
            purlin_spacing: sb::DEFAULT_SECONDARY_SPACING,
            girts: true,
            girt_type: None,
            girt_spacing: sb::DEFAULT_SECONDARY_SPACING,
            error: None,
        }
    }
}

pub fn open(state: &mut AppState) {
    state.dialogs.steel_building = Some(SteelBuildingDialog::default());
}

/// The roof choices.
fn roofs() -> [(FrameRoof, String); 2] {
    [(FrameRoof::Gable, t!("roof-gable")), (FrameRoof::Mono, t!("roof-shed"))]
}

/// The catalog default, then the steel types of `category` (cold-formed included).
fn steel_choices(state: &AppState, category: Category) -> Vec<(Option<ElementTypeId>, String)> {
    let p = state.project();
    let mut out = vec![(None, t!("opt-catalog-default"))];
    out.extend(p.types_of_material(category, StructuralMaterial::Steel).map(|t| (Some(t.id), t.name.clone())));
    out
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut d) = state.dialogs.steel_building.clone() else { return };
    let columns = steel_choices(state, Category::Column);
    let beams = steel_choices(state, Category::Beam);
    let roofs = roofs();
    let mut ok = None;
    let response = Dialog::new("steel-building", t!("dlg-steel-building")).width(460.0).show(
        ctx,
        |ui| {
            let label = |ui: &mut Ui, k: String| {
                ui.label(RichText::new(k).color(PALETTE.text_muted));
            };
            egui::Grid::new("steel-building-fields").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                label(ui, t!("prop-span"));
                ui.add(crate::properties::length_drag(&mut d.span).range(sb::SPAN));
                ui.end_row();
                label(ui, t!("prop-eave-height"));
                ui.add(crate::properties::length_drag(&mut d.eave_height).range(sb::EAVE_HEIGHT));
                ui.end_row();
                label(ui, t!("prop-slope"));
                ui.add(DragValue::new(&mut d.slope_percent).range(sb::SLOPE).speed(0.1).suffix(" %"));
                ui.end_row();
                label(ui, t!("prop-roof-shape"));
                combo(ui, "sb-roof", &mut d.roof, &roofs);
                ui.end_row();
                label(ui, t!("prop-bays"));
                ui.add(DragValue::new(&mut d.bays).range(sb::BAYS));
                ui.end_row();
                label(ui, t!("prop-bay-spacing"));
                ui.add(crate::properties::length_drag(&mut d.bay_spacing).range(sb::BAY_SPACING));
                ui.end_row();
                label(ui, t!("fp-origin-x"));
                ui.add(crate::properties::length_drag(&mut d.origin[0]));
                ui.end_row();
                label(ui, t!("fp-origin-y"));
                ui.add(crate::properties::length_drag(&mut d.origin[1]));
                ui.end_row();
                label(ui, t!("prop-rotation"));
                ui.add(DragValue::new(&mut d.rotation).speed(1.0).suffix("°"));
                ui.end_row();
                label(ui, t!("label-column-type"));
                combo(ui, "sb-column", &mut d.column_type, &columns);
                ui.end_row();
                label(ui, t!("label-rafter-type"));
                combo(ui, "sb-rafter", &mut d.rafter_type, &beams);
                ui.end_row();
            });
            for (salt, name, on, type_id, spacing) in [
                ("sb-purlins", t!("prop-purlins"), &mut d.purlins, &mut d.purlin_type, &mut d.purlin_spacing),
                ("sb-girts", t!("prop-girts"), &mut d.girts, &mut d.girt_type, &mut d.girt_spacing),
            ] {
                ui.add_space(4.0);
                ui.checkbox(on, name);
                ui.add_enabled_ui(*on, |ui| {
                    egui::Grid::new(salt).num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                        ui.label(RichText::new(t!("prop-type")).color(PALETTE.text_muted));
                        combo(ui, &format!("{salt}-type"), type_id, &beams);
                        ui.end_row();
                        ui.label(RichText::new(t!("label-spacing")).color(PALETTE.text_muted));
                        ui.add(crate::properties::length_drag(spacing).range(sb::SPACING));
                        ui.end_row();
                    });
                });
            }
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
        let line = |on: bool, type_id: Option<ElementTypeId>, spacing: f64| {
            Some(on.then(|| SecondaryMembers { type_ref: type_id.map(|t| Ref::Id(t.0)), spacing: Some(spacing) }))
        };
        let command = SteelBuildingCreate {
            level: state.active_level().map(|l| Ref::Id(l.0)),
            origin: Some(d.origin),
            rotation: Some(d.rotation),
            span: Some(d.span),
            eave_height: Some(d.eave_height),
            slope_percent: Some(d.slope_percent),
            roof: Some(d.roof.code().into()),
            bays: Some(d.bays),
            bay_spacing: Some(d.bay_spacing),
            column_type: d.column_type.map(|t| Ref::Id(t.0)),
            rafter_type: d.rafter_type.map(|t| Ref::Id(t.0)),
            purlins: line(d.purlins, d.purlin_type, d.purlin_spacing),
            girts: line(d.girts, d.girt_type, d.girt_spacing),
        };
        match state.session.run(command) {
            Ok(out) => {
                let count = out["members"].as_array().map_or(0, Vec::len);
                state.message = t!("msg-steel-building-created", "count" => count);
                if let Some(id) = out["id"].as_u64() {
                    state.selection = [ElementId(id)].into_iter().collect();
                }
                state.dialogs.steel_building = None;
            }
            Err(e) => {
                d.error = Some(e.message);
                state.dialogs.steel_building = Some(d);
            }
        }
        return;
    }
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.steel_building = None;
    } else {
        state.dialogs.steel_building = Some(d);
    }
}

/// Change building `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its members); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut SteelBuilding)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::SteelBuilding(b) = &mut e.kind {
            f(b);
        }
    });
}

/// The beam type purlins and girts get when turned on in the palette: the
/// first cold-formed one, else the rafters'.
fn secondary_default(state: &AppState, rafter: ElementTypeId) -> ElementTypeId {
    let p = state.project();
    p.types_of(Category::Beam)
        .find(|t| p.type_profile_def(&t.data).is_some_and(|d| d.material == ProfileMaterial::ColdFormed))
        .map_or(rafter, |t| t.id)
}

/// The rows of a selected steel building.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, building: &SteelBuilding) {
    let mut b = building.clone();
    let mut done = false;
    let all = |c: Category| -> Vec<(ElementTypeId, String)> {
        state.project().types_of(c).map(|t| (t.id, t.name.clone())).collect()
    };
    let (columns, beams) = (all(Category::Column), all(Category::Beam));
    let fallback = secondary_default(state, b.rafter_type);
    let roofs = roofs();
    section(ui, "steel-building", t!("section-steel-building"), |ui| {
        done |= length(ui, &t!("prop-span"), &mut b.span, sb::SPAN);
        done |= length(ui, &t!("prop-eave-height"), &mut b.eave_height, sb::EAVE_HEIGHT);
        done |= finished(&row(ui, &t!("prop-slope"), |ui| {
            ui.add(DragValue::new(&mut b.slope_percent).range(sb::SLOPE).speed(0.1).suffix(" %"))
        }));
        let roof = b.roof;
        row(ui, &t!("prop-roof-shape"), |ui| combo(ui, &format!("sb-roof-{}", id.0), &mut b.roof, &roofs));
        done |= b.roof != roof;
        done |= finished(&row(ui, &t!("prop-bays"), |ui| ui.add(DragValue::new(&mut b.bays).range(sb::BAYS))));
        done |= length(ui, &t!("prop-bay-spacing"), &mut b.bay_spacing, sb::BAY_SPACING);
        done |= length(ui, &t!("fp-origin-x"), &mut b.origin.x, -1e6..=1e6);
        done |= length(ui, &t!("fp-origin-y"), &mut b.origin.y, -1e6..=1e6);
        done |= finished(&row(ui, &t!("prop-rotation"), |ui| {
            ui.add(DragValue::new(&mut b.rotation_deg).speed(1.0).suffix("°"))
        }));
        if member_rows(ui, building) {
            b.detached.clear();
            done = true;
        }
    });
    section(ui, "steel-building-types", t!("section-type"), |ui| {
        let before = (b.column_type, b.rafter_type);
        row(ui, &t!("label-column-type"), |ui| combo(ui, &format!("sb-column-{}", id.0), &mut b.column_type, &columns));
        row(ui, &t!("label-rafter-type"), |ui| combo(ui, &format!("sb-rafter-{}", id.0), &mut b.rafter_type, &beams));
        done |= (b.column_type, b.rafter_type) != before;
    });
    section(ui, "steel-building-secondary", t!("section-secondary-members"), |ui| {
        for (salt, name, slot) in
            [("purlins", t!("prop-purlins"), &mut b.purlins), ("girts", t!("prop-girts"), &mut b.girts)]
        {
            let mut on = slot.is_some();
            if row(ui, &name, |ui| crate::table::checkbox(ui, &mut on)).changed() {
                *slot = on.then_some(Secondary { type_id: fallback, spacing: sb::DEFAULT_SECONDARY_SPACING });
                done = true;
            }
            if let Some(s) = slot {
                let type_id = s.type_id;
                row(ui, &t!("prop-type"), |ui| combo(ui, &format!("sb-{salt}-type-{}", id.0), &mut s.type_id, &beams));
                done |= s.type_id != type_id;
                done |= length(ui, &t!("label-spacing"), &mut s.spacing, sb::SPACING);
            }
        }
    });
    if b != *building {
        edit(state, id, "steel-building", move |x| *x = b);
    }
    if done {
        state.session.doc.break_merge();
    }
}
