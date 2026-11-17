//! Clipboard: cut, copy and paste of elements within the open project, and
//! match type (pick an element, then click others to give them its type).
//!
//! The clipboard holds a snapshot of the copied elements (with the doors and
//! windows of copied walls), so a cut can be pasted after its originals are
//! gone. It lives in the app state and is dropped with the document, since
//! its type and level ids only mean something in that project.

use crate::modify::paint_ghost;
use crate::paint::Xf;
use crate::state::AppState;
use crate::t;
use crate::tools::{Tool, draw_snap, snap_at};
use cc_core::{Element, ElementId, LevelId, PlanTransform, ViewId};
use cc_views::{Drawing2D, Prim};
use egui::{Painter, Response, Ui};
use glam::DVec2;
use std::collections::BTreeSet;
use std::rc::Rc;

/// Copied elements and, while pasting, their preview in the active view.
#[derive(Default)]
pub struct Clipboard {
    elements: Vec<Element>,
    preview: Option<Preview>,
}

/// The clipboard drawn in one plan view on one level, built once per paste.
struct Preview {
    view: ViewId,
    level: LevelId,
    drawing: Rc<Drawing2D>,
    ids: BTreeSet<ElementId>,
    /// Center of the linework: the point that follows the cursor.
    anchor: DVec2,
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// Copy the selection to the clipboard. The system clipboard gets a short
/// note, which also makes Ctrl+V reach the app (egui only reports a paste
/// when the system clipboard holds text).
pub fn copy(ctx: &egui::Context, state: &mut AppState) -> bool {
    let ids: Vec<ElementId> = state.selection.iter().copied().collect();
    if ids.is_empty() {
        state.message = t!("msg-select-first");
        return false;
    }
    let elements = state.project().copy_set(&ids);
    state.message = t!("msg-copied", "count" => elements.len());
    ctx.copy_text(t!("clipboard-note", "count" => elements.len()));
    state.clipboard = Clipboard { elements, preview: None };
    true
}

/// Copy the selection to the clipboard and delete it (pinned elements stay).
pub fn cut(ctx: &egui::Context, state: &mut AppState) {
    if copy(ctx, state) {
        state.delete_selection();
    }
}

/// Ctrl+V: place the clipboard with a click in a plan view; elsewhere paste
/// it where it was copied. What was drawn in a view (detail lines, regions,
/// notes, dimensions…) goes into the active view when it is a 2D view that
/// holds such elements, else back into its own view.
pub fn paste(state: &mut AppState) {
    if state.clipboard.is_empty() {
        state.message = t!("msg-clipboard-empty");
    } else if state.active_level().is_some() {
        state.clipboard.preview = None;
        state.set_tool(Tool::Paste);
        state.message = t!("hint-paste");
    } else {
        paste_aligned(state, false);
    }
}

/// Paste where the elements were copied, on their own levels or (with
/// `current_level`) on the active plan's level.
pub fn paste_aligned(state: &mut AppState, current_level: bool) {
    let level = if current_level {
        let Some(level) = state.active_level() else {
            state.message = t!("msg-plan-only");
            return;
        };
        Some(level)
    } else {
        None
    };
    paste_with(state, DVec2::ZERO, level);
}

/// The view the clipboard's view-owned elements paste into: the active one,
/// when it is a 2D view that holds detailing.
fn target_view(state: &AppState) -> Option<ViewId> {
    let id = state.active_view?;
    let view = state.project().view(id)?;
    let owned = state.clipboard.elements.iter().any(|e| e.owner_view().is_some());
    (owned && cc_views::hosts_cad(&view.kind)).then_some(id)
}

fn paste_with(state: &mut AppState, delta: DVec2, level: Option<LevelId>) {
    let command = cc_engine::commands::modify::ElementsPaste {
        elements: serde_json::to_value(&state.clipboard.elements).unwrap_or_default(),
        delta: Some(delta.to_array()),
        level: level.map(|l| cc_engine::Ref::Id(l.0)),
        view: target_view(state).map(|v| cc_engine::Ref::Id(v.0)),
    };
    let Some(out) = crate::actions::run_command(state, command) else { return };
    let new: Vec<ElementId> = out["created"]
        .as_array()
        .map(|a| a.iter().filter_map(serde_json::Value::as_u64).map(ElementId).collect())
        .unwrap_or_default();
    state.message = if new.is_empty() { t!("msg-paste-nothing") } else { t!("msg-pasted", "count" => new.len()) };
    state.selection = new.into_iter().collect();
}

/// The clipboard as drawn in `view` on `level`: a scratch copy of the
/// project holding only the clipboard (plus the walls of lone doors and
/// windows, which are not drawn).
fn preview(state: &mut AppState, view: ViewId, level: LevelId) -> Option<(Rc<Drawing2D>, BTreeSet<ElementId>, DVec2)> {
    if let Some(p) = &state.clipboard.preview
        && p.view == view
        && p.level == level
    {
        return Some((p.drawing.clone(), p.ids.clone(), p.anchor));
    }
    let target = target_view(state);
    let mut project = state.project().clone();
    let elements = &state.clipboard.elements;
    let copied: BTreeSet<ElementId> = elements.iter().map(|e| e.id).collect();
    let hosts: Vec<Element> = elements
        .iter()
        .filter_map(|e| e.as_hosted())
        .filter(|h| !copied.contains(&h.host))
        .filter_map(|h| project.element(h.host).cloned())
        .collect();
    let pasted: Vec<Element> = elements
        .iter()
        .cloned()
        .map(|mut e| {
            // Detailing shows in the view it will be pasted into.
            if let Some(target) = target {
                e.set_owner_view(target);
            }
            if e.as_hosted().is_none_or(|h| copied.contains(&h.host)) {
                // As the paste does it: tops move up with the base.
                project.move_top_with_base(&mut e, level);
                e.level = level;
            }
            e
        })
        .collect();
    project.elements = pasted.into_iter().chain(hosts).map(|e| (e.id, e)).collect();
    let geom = cc_geom::ModelGeometry::build(&project);
    let drawing = Rc::new(cc_views::generate(&project, &geom, project.view(view)?)?);
    let pts = drawing.prims.iter().flat_map(|p| match p {
        Prim::Line { a, b, element: Some(e), .. } if copied.contains(e) => vec![*a, *b],
        _ => Vec::new(),
    });
    let (lo, hi) =
        pts.fold(None, |acc: Option<(DVec2, DVec2)>, p| Some(acc.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p)))))?;
    let anchor = (lo + hi) * 0.5;
    state.clipboard.preview = Some(Preview { view, level, drawing: drawing.clone(), ids: copied.clone(), anchor });
    Some((drawing, copied, anchor))
}

/// Paste tool: the clipboard follows the cursor (its center on the snapped
/// point) and a click places it on the view's level.
pub fn paste_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    view: ViewId,
    level: LevelId,
    xf: &Xf,
) {
    if state.clipboard.is_empty() {
        state.set_tool(Tool::Select);
        return;
    }
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let Some((drawing, ids, anchor)) = preview(state, view, level) else {
        // Nothing drawable here (e.g. only annotations of another view).
        if resp.clicked() {
            state.set_tool(Tool::Select);
            paste_aligned(state, true);
        }
        return;
    };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, Some(level), raw, None, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    let t = PlanTransform::translate(s.point - anchor);
    paint_ghost(painter, xf, &drawing, &ids, &t);
    if resp.clicked() {
        state.set_tool(Tool::Select);
        paste_with(state, s.point - anchor, Some(level));
    }
}

/// Start match type: with one element selected it is the source, otherwise
/// the first click picks it.
pub fn start_match_type(state: &mut AppState) {
    let source = (state.selection.len() == 1).then(|| state.selection.first().copied()).flatten();
    state.set_tool(Tool::MatchType);
    match source {
        Some(id) => set_source(state, id),
        None => state.message = t!("hint-match-type"),
    }
}

fn set_source(state: &mut AppState, id: ElementId) {
    state.interaction.targets = vec![id];
    let name = state.project().element(id).map(|e| state.project().type_name(e.type_id).to_string());
    state.message = t!("hint-match-type-apply", "type" => name.unwrap_or_default());
}

/// Match type tool: the first click picks the source, later clicks give its
/// type to elements of the same category (a click on empty space picks again).
pub fn match_type_tool(resp: &Response, painter: &Painter, state: &mut AppState, xf: &Xf, drawing: &Drawing2D) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let hovered = drawing.hit_test(raw, 6.0 / xf.scale);
    let source = state.interaction.targets.first().copied().filter(|id| state.project().element(*id).is_some());
    let mut lit: BTreeSet<ElementId> = source.into_iter().collect();
    lit.extend(hovered);
    paint_ghost(painter, xf, drawing, &lit, &PlanTransform::IDENTITY);
    if !resp.clicked() {
        return;
    }
    let (Some(source), Some(target)) = (source, hovered) else {
        match hovered {
            Some(id) => set_source(state, id),
            None => {
                state.interaction.targets.clear();
                state.message = t!("hint-match-type");
            }
        }
        return;
    };
    let project = state.project();
    let (Some(from), Some(to)) = (project.element(source), project.element(target)) else { return };
    if from.category() != to.category() {
        state.message = t!("msg-match-type-category");
        return;
    }
    let type_id = from.type_id;
    if to.type_id != type_id {
        let command =
            cc_engine::commands::edit::ElementsSetType { ids: vec![target.0], type_ref: cc_engine::Ref::Id(type_id.0) };
        crate::actions::run_command(state, command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AppState {
        AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()))
    }

    fn a_wall(state: &AppState) -> ElementId {
        state.project().elements.values().find_map(|e| e.as_hosted().map(|h| h.host)).unwrap()
    }

    #[test]
    fn cut_and_paste_restore_a_wall_with_its_openings() {
        let mut state = sample();
        let wall = a_wall(&state);
        let before = state.project().elements.len();
        state.selection = [wall].into_iter().collect();
        cut(&egui::Context::default(), &mut state);
        assert!(state.project().element(wall).is_none());
        assert!(state.project().elements.len() < before);
        paste_aligned(&mut state, false);
        assert_eq!(state.project().elements.len(), before);
        // The pasted copy is selected and can be pasted again.
        assert!(!state.selection.is_empty());
        paste_aligned(&mut state, false);
        assert!(state.project().elements.len() > before);
    }

    #[test]
    fn paste_without_a_copy_explains() {
        let mut state = sample();
        let before = state.project().elements.len();
        paste(&mut state);
        assert_eq!(state.message, t!("msg-clipboard-empty"));
        assert_eq!(state.project().elements.len(), before);
        assert_eq!(state.tool, Tool::Select);
    }

    #[test]
    fn the_preview_centers_the_copy() {
        let mut state = sample();
        let wall = a_wall(&state);
        state.selection = [wall].into_iter().collect();
        copy(&egui::Context::default(), &mut state);
        let plan = state
            .project()
            .views
            .values()
            .find(|v| matches!(v.kind, cc_core::ViewKind::FloorPlan { .. }))
            .map(|v| (v.id, v.level()));
        let (view, Some(level)) = plan.unwrap() else { panic!("plan without level") };
        let (_, ids, anchor) = preview(&mut state, view, level).unwrap();
        assert!(ids.contains(&wall));
        let w = state.project().element(wall).and_then(|e| e.as_wall()).unwrap();
        // The anchor lies near the wall's middle (its faces surround the axis).
        assert!(anchor.distance((w.start + w.end) * 0.5) < 0.5, "{anchor}");
    }

    #[test]
    fn detailing_pastes_into_the_active_view() {
        let mut state = sample();
        let plan = state.project().views.values().find(|v| v.is_plan()).map(|v| v.id).unwrap();
        let out = state
            .session
            .execute(
                "filled_region_create",
                serde_json::json!({"view": plan.0, "curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [3, 2]}]}),
            )
            .unwrap();
        let region = ElementId(out["id"].as_u64().unwrap());
        let other = state.session.execute("view_duplicate", serde_json::json!({"view": plan.0})).unwrap();
        let other = ViewId(other["id"].as_u64().unwrap());
        state.selection = [region].into_iter().collect();
        copy(&egui::Context::default(), &mut state);

        // Shown in the preview of the plan it goes to, then pasted there.
        state.active_view = Some(other);
        let level = state.active_level().unwrap();
        let (_, ids, _) = preview(&mut state, other, level).expect("the region previews in the other plan");
        assert!(ids.contains(&region));
        paste_aligned(&mut state, false);
        let pasted: Vec<ElementId> = state.selection.iter().copied().collect();
        assert_eq!(pasted.len(), 1);
        assert_eq!(state.project().element(pasted[0]).and_then(|e| e.owner_view()), Some(other));

        // From a 3D view it goes back into its own plan.
        let three_d = state.project().views.values().find(|v| v.category() == cc_core::ViewCategory::ThreeD);
        state.active_view = three_d.map(|v| v.id);
        assert!(state.active_view.is_some(), "the sample has a 3D view");
        state.selection = [region].into_iter().collect();
        copy(&egui::Context::default(), &mut state);
        paste_aligned(&mut state, false);
        let pasted = state.selection.first().copied().unwrap();
        assert_eq!(state.project().element(pasted).and_then(|e| e.owner_view()), Some(plan));
    }
}
