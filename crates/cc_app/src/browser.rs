//! Project Browser: the tree of views, schedules and sheets, grouped as the
//! project's browser organization says (`cc_core::browser`): by view type
//! alone, or by the criteria of the scheme in use.

use crate::actions;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use crate::tools::Tool;
use cc_core::browser::{GroupKey, Node};
use cc_core::{Category, Project, ViewCategory, ViewId, ViewKind};
use egui::{Rect, Response, Sense, Ui, vec2};

/// A view dragged from the browser (dropped on a sheet, it is placed there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewDrag(pub ViewId);

const ROW_HEIGHT: f32 = 21.0;
const INDENT: f32 = 20.0;

pub fn category_icon(category: ViewCategory) -> Icon {
    match category {
        ViewCategory::FloorPlan => Icon::FloorPlan,
        ViewCategory::CeilingPlan => Icon::CeilingPlan,
        ViewCategory::ThreeD => Icon::View3D,
        ViewCategory::Elevation => Icon::Elevation,
        ViewCategory::Section => Icon::Section,
        ViewCategory::Legend => Icon::Legend,
        ViewCategory::Drafting => Icon::DraftingView,
        ViewCategory::Schedule => Icon::Schedule,
        ViewCategory::Diagram => Icon::Systems,
        ViewCategory::Sheet => Icon::Sheet,
    }
}

/// The browser with its header (the app docks it with `side_panels`).
#[cfg(test)]
pub fn show(ui: &mut Ui, state: &mut AppState) {
    theme::panel_header(ui, &t!("browser-title"), Some(&state.project().info.name));
    body(ui, state);
}

/// The browser's tree, under a header drawn by the caller.
pub fn body(ui: &mut Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().id_salt("browser").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(4.0);
        let tree = state.project().browser_tree();
        nodes(ui, state, &tree, &mut Vec::new(), &mut Vec::new());
        ui.add_space(6.0);
    });
}

/// A group's name and icon.
fn group_look(project: &Project, key: GroupKey) -> (String, Icon) {
    match key {
        GroupKey::ViewType(c) => (tr(c.key()), category_icon(c)),
        GroupKey::Level(l) => (project.level_name(l).to_string(), Icon::Levels),
        GroupKey::Discipline(d) => (tr(d.key()), Icon::Systems),
        GroupKey::Phase(ph) => {
            (project.phases.iter().find(|p| p.id == ph).map(|p| p.name.clone()).unwrap_or_default(), Icon::Phases)
        }
    }
}

/// The rows of `list`, whose groups are `path` deep; `lasts` tells, for each
/// depth above, whether the branch there has ended (for the guide lines).
fn nodes(ui: &mut Ui, state: &mut AppState, list: &[Node], path: &mut Vec<GroupKey>, lasts: &mut Vec<bool>) {
    let n = list.len();
    for (i, node) in list.iter().enumerate() {
        lasts.push(i + 1 == n);
        match node {
            Node::View(id) => view_row(ui, state, *id, lasts),
            Node::Group { key, children } => {
                path.push(*key);
                // A view type at the root keeps the id it had before schemes existed.
                let id = match (path.as_slice(), key) {
                    ([_], GroupKey::ViewType(c)) => ui.make_persistent_id(("browser-open", *c)),
                    _ => ui.make_persistent_id(("browser-open", path.as_slice())),
                };
                let closed = matches!(key, GroupKey::ViewType(ViewCategory::Legend));
                let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, !closed));
                let (name, icon) = group_look(state.project(), *key);
                let label = format!("{name}  ");
                let depth = path.len() - 1;
                let row = tree_row(ui, &lasts[..depth], Some(open), icon, &label, Some(node.view_count()), false);
                // Views are added from the row of their type at the root.
                let added = match key {
                    GroupKey::ViewType(c) if depth == 0 => add_button(ui, state, *c, &row),
                    _ => false,
                };
                if added {
                    open = true;
                } else if row.clicked() {
                    open = !open;
                }
                row.context_menu(|ui| {
                    if ui.button(format!("{}…", t!("action-browser-organization"))).clicked() {
                        crate::browser_org_dialog::open(state);
                        ui.close();
                    }
                });
                ui.data_mut(|d| d.insert_persisted(id, open));
                if open {
                    nodes(ui, state, children, path, lasts);
                }
                path.pop();
            }
        }
        lasts.pop();
    }
}

/// Row layout: left padding, then the chevron column, then one indent per depth.
const PAD: f32 = 10.0;
const CHEVRON: f32 = 15.0;

/// Left edge of the icon of a row at `depth`.
fn icon_x(left: f32, depth: usize) -> f32 {
    left + PAD + CHEVRON + depth as f32 * INDENT
}

/// One row of the tree, as deep as `lasts` is long: each entry tells whether
/// the branch at that depth has ended, the last one being the row's own (for
/// the guide lines). `expander` is `Some(open)` for collapsible rows.
fn tree_row(
    ui: &mut Ui,
    lasts: &[bool],
    expander: Option<bool>,
    icon: Icon,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> Response {
    let depth = lasts.len();
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let bg = rect.shrink2(vec2(4.0, 1.0));
    if selected {
        painter.rect_filled(bg, theme::RADIUS as f32, PALETTE.accent_soft);
    } else if response.hovered() {
        painter.rect_filled(bg, theme::RADIUS as f32, PALETTE.hover);
    }
    let cy = rect.center().y;
    let mut x = icon_x(rect.left(), depth);
    if let Some(open) = expander {
        // A nested group's chevron stands between its guide line and its icon.
        let cx = if depth == 0 { x - CHEVRON + 5.0 } else { x - 6.0 };
        let chevron = Rect::from_center_size(egui::pos2(cx, cy), vec2(10.0, 10.0));
        icons::paint(painter, chevron, if open { Icon::ChevronDown } else { Icon::ChevronRight }, PALETTE.text_faint);
    }
    let guide = egui::Stroke::new(1.0, PALETTE.border_strong);
    for (d, last) in lasts.iter().enumerate() {
        let gx = icon_x(rect.left(), d) + 7.0;
        if d + 1 == depth {
            // Guide from the parent's icon down to this row, with a tick into it.
            let bottom = if *last { cy } else { rect.bottom() };
            painter.vline(gx, rect.top()..=bottom, guide);
            if expander.is_none() {
                painter.hline(gx..=x - 3.0, cy, guide);
            }
        } else if !last {
            // A branch above that goes on below this row.
            painter.vline(gx, rect.top()..=rect.bottom(), guide);
        }
    }
    let color = if selected { PALETTE.accent_text } else { PALETTE.text };
    let icon_color = if selected { PALETTE.accent_text } else { PALETTE.text_muted };
    icons::paint(painter, Rect::from_center_size(egui::pos2(x + 7.0, cy), vec2(14.0, 14.0)), icon, icon_color);
    x += 21.0;
    let family = if expander.is_some() { theme::semibold() } else { egui::FontFamily::Proportional };
    let galley = painter.layout_no_wrap(label.to_string(), egui::FontId::new(12.5, family), color);
    let width = galley.size().x;
    painter.galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, color);
    if let Some(n) = count {
        let font = egui::FontId::proportional(11.0);
        painter.text(egui::pos2(x + width, cy), egui::Align2::LEFT_CENTER, n.to_string(), font, PALETTE.text_faint);
    }
    response
}

/// The row of view `id`, as deep as `lasts` is long.
fn view_row(ui: &mut Ui, state: &mut AppState, id: ViewId, lasts: &[bool]) {
    let Some((label, category)) = state.project().view(id).map(|v| (cc_core::browser::label(v), v.category())) else {
        return;
    };
    if let Some((rid, text)) = state.rename.as_mut().filter(|(rid, _)| *rid == id) {
        let rid = *rid;
        let finished = ui
            .horizontal(|ui| {
                ui.add_space(PAD + CHEVRON + lasts.len() as f32 * INDENT + 21.0);
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 8.0));
                // Focus once when renaming starts: Enter, Esc or a click
                // elsewhere then ends it (asking every frame kept the
                // field focused for good, and every other field locked).
                let started = egui::Id::new(("rename-focused", rid.0));
                if !ui.data(|d| d.get_temp::<bool>(started).unwrap_or(false)) {
                    ui.data_mut(|d| d.insert_temp(started, true));
                    r.request_focus();
                }
                let done = r.lost_focus();
                if done {
                    ui.data_mut(|d| d.remove::<bool>(started));
                }
                done.then(|| !ui.input(|i| i.key_pressed(egui::Key::Escape)))
            })
            .inner;
        if let Some(accept) = finished
            && let Some((_, text)) = state.rename.take()
            && accept
        {
            actions::rename_view(state, rid, text);
        }
        return;
    }
    let active = state.active_view == Some(id);
    let r = tree_row(ui, lasts, None, category_icon(category), &label, None, active);
    // Views that go on sheets can be dragged onto one.
    let r = if state.project().view(id).is_some_and(|v| v.placeable()) {
        let r = r.interact(Sense::drag());
        r.dnd_set_drag_payload(ViewDrag(id));
        if r.dragged() {
            drag_label(ui, &label);
        }
        r
    } else {
        r
    };
    if r.clicked() {
        state.open_view(id);
    }
    r.context_menu(|ui| {
        if ui.button(t!("browser-open")).clicked() {
            state.open_view(id);
            ui.close();
        }
        if ui.button(t!("browser-rename")).clicked() {
            let name = state.project().view(id).map(|v| v.name.clone()).unwrap_or_default();
            state.rename = Some((id, name));
            ui.data_mut(|d| d.remove::<bool>(egui::Id::new(("rename-focused", id.0))));
            ui.close();
        }
        if ui.button(t!("browser-duplicate")).clicked() {
            actions::duplicate_view(state, id, false);
            ui.close();
        }
        // Views that hold detailing can be copied with it.
        let detailed = state.project().view(id).is_some_and(|v| cc_views::hosts_cad(&v.kind));
        if detailed && ui.button(t!("browser-duplicate-detailing")).clicked() {
            actions::duplicate_view(state, id, true);
            ui.close();
        }
        if crate::title_block_env::sheet_title_block_of(state, id).is_some()
            && ui.button(t!("action-edit-title-block")).clicked()
        {
            crate::title_block_env::edit_sheet(state, id);
            ui.close();
        }
        if let Some(sheet) = state
            .active_view
            .filter(|s| matches!(state.project().view(*s).map(|v| &v.kind), Some(ViewKind::Sheet { .. })) && *s != id)
        {
            let placeable = state.project().view(id).is_some_and(|v| v.placeable());
            if placeable && ui.button(t!("browser-place-on-sheet")).clicked() {
                actions::place_on_sheet(state, sheet, id);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(t!("browser-delete")).clicked() {
            actions::delete_view(state, id);
            ui.close();
        }
    });
}

/// The name of a view being dragged, next to the pointer.
fn drag_label(ui: &Ui, label: &str) {
    let Some(pos) = ui.ctx().pointer_interact_pos() else { return };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("browser-view-drag"));
    let painter = ui.ctx().layer_painter(layer);
    let galley = painter.layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), PALETTE.text);
    let rect = Rect::from_min_size(pos + vec2(14.0, 10.0), galley.size() + vec2(12.0, 6.0));
    painter.rect_filled(rect, theme::RADIUS as f32, PALETTE.panel);
    painter.rect_stroke(
        rect,
        theme::RADIUS as f32,
        egui::Stroke::new(1.0, PALETTE.border_strong),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(6.0, 3.0), galley, PALETTE.text);
}

/// "+" button at the right of a category row. Returns true when it was used.
fn add_button(ui: &mut Ui, state: &mut AppState, category: ViewCategory, row: &Response) -> bool {
    if category == ViewCategory::Elevation {
        return false;
    }
    let rect = Rect::from_center_size(egui::pos2(row.rect.right() - 18.0, row.rect.center().y), vec2(20.0, 20.0));
    let r = ui.interact(rect, row.id.with("add"), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t!("browser-new")));
    let r = r.on_hover_text(t!("browser-new"));
    if r.hovered() {
        ui.painter().rect_filled(rect, theme::RADIUS as f32, PALETTE.pressed);
    }
    if row.hovered() || r.hovered() || r.has_focus() {
        icons::paint(ui.painter(), rect.shrink(4.0), Icon::Plus, PALETTE.text_muted);
    }
    let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
    let mut used = false;
    match category {
        ViewCategory::FloorPlan | ViewCategory::CeilingPlan => {
            egui::Popup::menu(&r).show(|ui| {
                for (id, name) in levels {
                    if ui.button(name).clicked() {
                        actions::new_plan(state, id, category == ViewCategory::CeilingPlan);
                        used = true;
                    }
                }
            });
        }
        ViewCategory::Schedule => {
            egui::Popup::menu(&r).show(|ui| {
                for c in Category::SCHEDULABLE {
                    if ui.button(actions::t_category(c)).clicked() {
                        actions::new_schedule(state, c);
                        used = true;
                    }
                }
            });
        }
        _ if r.clicked() => {
            match category {
                ViewCategory::ThreeD => {
                    actions::new_3d(state);
                }
                ViewCategory::Section => {
                    state.set_tool(Tool::Section);
                    state.message = t!("msg-section-hint");
                }
                ViewCategory::Legend => {
                    actions::new_legend(state);
                }
                ViewCategory::Drafting => {
                    actions::new_drafting_view(state);
                }
                ViewCategory::Sheet => {
                    actions::new_sheet(state);
                }
                _ => {}
            }
            used = true;
        }
        _ => {}
    }
    used || r.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_engine::Session;
    use serde_json::json;

    /// The views of `category` with their labels, in the browser's order.
    fn ordered(project: &Project, category: ViewCategory) -> Vec<(ViewId, String)> {
        let tree = project.browser_tree();
        let views = tree.iter().find_map(|n| match n {
            Node::Group { key: GroupKey::ViewType(c), children } if *c == category => Some(children),
            _ => None,
        });
        views
            .into_iter()
            .flatten()
            .filter_map(|n| match n {
                Node::View(id) => project.view(*id).map(|v| (*id, cc_core::browser::label(v))),
                Node::Group { .. } => None,
            })
            .collect()
    }

    fn names(s: &Session, category: ViewCategory) -> Vec<String> {
        ordered(s.project(), category).into_iter().map(|(_, n)| n).collect()
    }

    /// Plans are listed by their level's elevation, whatever their names,
    /// and follow a level moved below another.
    #[test]
    fn plans_are_ordered_by_level_elevation() {
        let mut s = Session::default();
        for (name, elevation) in [("A top", 90.0), ("Z bottom", -30.0)] {
            s.execute("level_create", json!({ "name": name, "elevation": elevation })).unwrap();
        }
        for category in [ViewCategory::FloorPlan, ViewCategory::CeilingPlan] {
            let p = s.project();
            let elevations: Vec<f64> = ordered(p, category)
                .iter()
                .map(|(id, _)| p.view(*id).and_then(|v| v.level()).map_or(f64::NAN, |l| p.level_elevation(l)))
                .collect();
            assert!(
                elevations.len() >= 3 && elevations.windows(2).all(|w| w[0] <= w[1]),
                "{category:?}: {elevations:?}"
            );
            let names = names(&s, category);
            assert_eq!(names.first().map(String::as_str), Some("Z bottom"));
            assert_eq!(names.last().map(String::as_str), Some("A top"));
        }
        s.execute("level_set", json!({ "level": "A top", "elevation": -100.0 })).unwrap();
        assert_eq!(names(&s, ViewCategory::FloorPlan).first().map(String::as_str), Some("A top"));
    }
}
