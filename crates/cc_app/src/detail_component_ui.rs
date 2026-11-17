//! Detail components in the app: the Detail Component tool (Annotate tab),
//! which places a type in the active 2D view with the rotation, scale and
//! mirror of its options (`detail_component_place`); *Create Detail
//! Component*, which makes a type from the selected detail lines, regions,
//! texts and detail components of the view and puts it in their place
//! (`detail_component_create`); and the rows of a placed one in the
//! properties palette. Pieces are edited with the `component_sheet_piece_*`
//! commands (the command palette).

use crate::paint::{PREVIEW, Xf};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::tools::{draw_snap, run_create, snap_at};
use cc_core::detail_component::{DetailComponent, MAX_SCALE, MIN_SCALE};
use cc_core::{ElementId, ElementKind, ElementTypeId, View};
use cc_engine::Ref;
use cc_engine::commands::detail_component::{DetailComponentCreate, DetailComponentPlace};
use cc_views::{Drawing2D, Prim};
use egui::{DragValue, Painter, Response, Stroke, Ui};

/// Options of the Detail Component tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceOptions {
    /// Degrees counter-clockwise.
    pub rotation: f64,
    pub scale: f64,
    pub mirrored: bool,
}

impl Default for PlaceOptions {
    fn default() -> Self {
        Self { rotation: 0.0, scale: 1.0, mirrored: false }
    }
}

/// The options bar of the tool: rotation, scale and mirror.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let o = &mut state.detail_component;
    ui.label(t!("prop-rotation"));
    ui.add(DragValue::new(&mut o.rotation).speed(1.0).suffix("°"));
    if ui.button(t!("action-turn-90")).on_hover_text(t!("hint-detail-component-turn")).clicked() {
        o.rotation = (o.rotation + 90.0).rem_euclid(360.0);
    }
    ui.label(t!("prop-scale"));
    ui.add(DragValue::new(&mut o.scale).speed(0.01).range(MIN_SCALE..=MAX_SCALE));
    ui.checkbox(&mut o.mirrored, t!("prop-mirrored"));
    ui.separator();
    ui.label(crate::theme::muted(t!("hint-detail-component")));
}

/// The Detail Component tool: the type follows the cursor; a click places it.
pub fn tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    view: &View,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, view.level(), raw, None, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    let o = state.detail_component;
    let mut c = DetailComponent::new(view.id, s.point);
    c.rotation_deg = o.rotation;
    c.scale = o.scale;
    c.mirrored = o.mirrored;
    // The preview: the component's lines where it would land.
    let p = state.project();
    let mut d = Drawing2D::default();
    if let Some(TypeDataRef { name, def }) = type_data(p, type_id) {
        cc_views::detail_component::draw(&mut d, p, name, def, &Default::default(), c.place(), view.scale, None);
    }
    let stroke = Stroke::new(1.0, PREVIEW);
    for prim in d.prims.iter().take(20_000) {
        if let Prim::Line { a, b, .. } = prim {
            painter.line_segment([xf.to_screen(*a), xf.to_screen(*b)], stroke);
        }
    }
    painter.circle_stroke(xf.to_screen(s.point), 4.0, stroke);
    if resp.clicked() {
        let command = DetailComponentPlace {
            type_ref: Ref::Id(type_id.0),
            view: Ref::Id(view.id.0),
            position: s.point.to_array(),
            rotation: Some(o.rotation),
            scale: Some(o.scale),
            mirrored: Some(o.mirrored),
        };
        run_create(state, command);
    }
}

struct TypeDataRef<'a> {
    name: &'a str,
    def: &'a cc_core::ComponentDef,
}

fn type_data(p: &cc_core::Project, id: ElementTypeId) -> Option<TypeDataRef<'_>> {
    match p.element_type(id).map(|t| &t.data) {
        Some(cc_core::TypeData::Component { component, def }) if def.kind == cc_core::ComponentKind::Detail => {
            Some(TypeDataRef { name: component, def })
        }
        _ => None,
    }
}

/// The view and the selected elements a detail component can be made of:
/// detail lines, regions, text notes and detail components, all of the
/// active view (one that holds drawings).
pub fn selection(state: &AppState) -> Option<(cc_core::ViewId, Vec<ElementId>)> {
    let view = crate::cad_ui::cad_view(state)?;
    let p = state.project();
    let ids: Vec<ElementId> = state.selection.iter().copied().collect();
    let fits = |id: &ElementId| {
        p.element(*id).is_some_and(|e| {
            e.owner_view() == Some(view)
                && matches!(
                    e.kind,
                    ElementKind::DetailLine(_)
                        | ElementKind::FilledRegion(_)
                        | ElementKind::MaskingRegion(_)
                        | ElementKind::Text(_)
                        | ElementKind::DetailComponent(_)
                )
        })
    };
    (!ids.is_empty() && ids.iter().all(fits)).then_some((view, ids))
}

/// A component name not yet taken: `Detail 1`, `Detail 2`…
fn free_name(p: &cc_core::Project) -> String {
    let base = t!("tpl-detail-component");
    let taken = |n: &str| {
        p.types
            .values()
            .any(|t| t.name == n || matches!(&t.data, cc_core::TypeData::Component { component, .. } if component == n))
    };
    (1..).map(|i| format!("{base} {i}")).find(|n| !taken(n)).unwrap_or(base)
}

/// *Create Detail Component*: the selection becomes a new detail component
/// type (named `Detail 1`, `Detail 2`…, renamed in its type's properties),
/// placed where the elements were; the new element is selected.
pub fn create_from_selection(state: &mut AppState) {
    let Some((view, ids)) = selection(state) else {
        state.message = t!("why-select-detail");
        return;
    };
    let command = DetailComponentCreate {
        name: free_name(state.project()),
        pieces: Vec::new(),
        view: Some(Ref::Id(view.0)),
        elements: ids.iter().map(|i| i.0).collect(),
        origin: None,
        replace: Some(true),
        description: None,
    };
    if let Some(out) = crate::actions::run_command(state, command)
        && let Some(id) = out["placed"].as_u64()
    {
        state.selection = std::collections::BTreeSet::from([ElementId(id)]);
        state.message = t!("msg-detail-component-created", "name" => out["name"].as_str().unwrap_or_default());
    }
}

/// A placed detail component in the properties palette: its component and
/// view, and its rotation, scale and mirror.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, c: &DetailComponent) {
    let p = state.project();
    let component = p
        .element(id)
        .and_then(|e| cc_views::detail_component::def_of(p, e))
        .map(|(n, _)| n.to_string())
        .unwrap_or_default();
    let view = p.view(c.view).map(|v| v.name.clone()).unwrap_or_default();
    let mut new = c.clone();
    let mut done = false;
    section(ui, "detail-component", t!("section-detail-component"), |ui| {
        readonly_row(ui, &t!("prop-component"), component);
        readonly_row(ui, &t!("prop-view"), view);
        let r =
            row(ui, &t!("prop-rotation"), |ui| ui.add(DragValue::new(&mut new.rotation_deg).speed(1.0).suffix("°")));
        done |= r.drag_stopped() || r.lost_focus();
        let r = row(ui, &t!("prop-scale"), |ui| {
            ui.add(DragValue::new(&mut new.scale).speed(0.01).range(MIN_SCALE..=MAX_SCALE))
        });
        done |= r.drag_stopped() || r.lost_focus();
        done |= row(ui, &t!("prop-mirrored"), |ui| crate::table::checkbox(ui, &mut new.mirrored)).changed();
    });
    new.rotation_deg = new.rotation_deg.rem_euclid(360.0);
    if new != *c {
        crate::engine_edit::element(state, id, Some(crate::properties::key((id, "detail-component"))), |e| {
            if let ElementKind::DetailComponent(x) = &mut e.kind {
                **x = new;
            }
        });
    }
    if done {
        state.session.doc.break_merge();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_names_skip_taken_ones() {
        let mut state = AppState::new(cc_core::Document::new(crate::actions::new_project()));
        let first = free_name(state.project());
        let view = state.project().views.keys().next().copied().unwrap();
        let out = crate::actions::run_command(
            &mut state,
            DetailComponentCreate {
                name: first.clone(),
                pieces: vec![
                    serde_json::from_value(serde_json::json!({
                        "kind": "stroke", "points": [{"u": 0, "v": 0}, {"u": 1, "v": 0}]
                    }))
                    .unwrap(),
                ],
                view: None,
                elements: vec![],
                origin: None,
                replace: None,
                description: None,
            },
        );
        assert!(out.is_some());
        assert_ne!(free_name(state.project()), first);
        let _ = view;
    }
}
