//! The side panels, Project Browser and Properties: docked on either side of
//! the window, stacked in one column or each in a column of its own. A panel
//! moves by dragging its header onto a side or another panel, or from the
//! header's context menu; the layout is a preference ([`PanelLayout`]).

use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use egui::{Color32, CursorIcon, Id, LayerId, Order, Pos2, Rect, Response, Sense, Stroke, Ui};

/// A side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pane {
    Browser,
    Properties,
}

impl Pane {
    pub const ALL: [Pane; 2] = [Pane::Browser, Pane::Properties];

    fn code(self) -> &'static str {
        match self {
            Pane::Browser => "browser",
            Pane::Properties => "properties",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }

    pub fn title(self) -> String {
        match self {
            Pane::Browser => t!("browser-title"),
            Pane::Properties => t!("props-title"),
        }
    }
}

/// A side of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

/// Where the panels are: the columns on each side, from the window's edge
/// inward, each stacking its panels top to bottom. Every panel is in exactly
/// one column and no column is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelLayout {
    pub left: Vec<Vec<Pane>>,
    pub right: Vec<Vec<Pane>>,
}

/// Both panels stacked on the left, the browser on top.
impl Default for PanelLayout {
    fn default() -> Self {
        Self { left: vec![vec![Pane::Browser, Pane::Properties]], right: Vec::new() }
    }
}

/// Where a moved panel goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A column of its own at the window's edge on that side.
    Edge(Side),
    /// In `other`'s column, above or below it.
    Stack { other: Pane, below: bool },
    /// A column of its own next to `other`'s, to its right or left on screen.
    Beside { other: Pane, right: bool },
}

impl PanelLayout {
    pub fn side(&self, side: Side) -> &Vec<Vec<Pane>> {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn side_mut(&mut self, side: Side) -> &mut Vec<Vec<Pane>> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    /// The side, column (from the edge) and row of `pane`.
    pub fn find(&self, pane: Pane) -> Option<(Side, usize, usize)> {
        [Side::Left, Side::Right].into_iter().find_map(|side| {
            self.side(side)
                .iter()
                .enumerate()
                .find_map(|(c, column)| column.iter().position(|p| *p == pane).map(|r| (side, c, r)))
        })
    }

    fn remove(&mut self, pane: Pane) {
        for side in [Side::Left, Side::Right] {
            let columns = self.side_mut(side);
            columns.iter_mut().for_each(|c| c.retain(|p| *p != pane));
            columns.retain(|c| !c.is_empty());
        }
    }

    /// Move `pane` to `target`; false when nothing changed (or the target
    /// names the pane itself).
    pub fn apply(&mut self, pane: Pane, target: Target) -> bool {
        let other = match target {
            Target::Edge(_) => None,
            Target::Stack { other, .. } | Target::Beside { other, .. } => Some(other),
        };
        if other == Some(pane) {
            return false;
        }
        let before = self.clone();
        self.remove(pane);
        match target {
            Target::Edge(side) => self.side_mut(side).insert(0, vec![pane]),
            Target::Stack { other, below } => {
                let Some((side, c, r)) = self.find(other) else {
                    *self = before;
                    return false;
                };
                if let Some(column) = self.side_mut(side).get_mut(c) {
                    column.insert(r + usize::from(below), pane);
                }
            }
            Target::Beside { other, right } => {
                let Some((side, c, _)) = self.find(other) else {
                    *self = before;
                    return false;
                };
                // Columns count from the edge: inward is to the right on the left side.
                let at = if right == (side == Side::Left) { c + 1 } else { c };
                self.side_mut(side).insert(at, vec![pane]);
            }
        }
        *self != before
    }

    /// The stored form: `left=browser+properties;right=`, columns separated by
    /// commas and the panels of a column by `+`.
    pub fn to_text(&self) -> String {
        let side = |columns: &Vec<Vec<Pane>>| {
            columns
                .iter()
                .map(|c| c.iter().map(|p| p.code()).collect::<Vec<_>>().join("+"))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!("left={};right={}", side(&self.left), side(&self.right))
    }

    /// Read [`Self::to_text`]'s form; `None` unless it names every panel once.
    pub fn from_text(text: &str) -> Option<Self> {
        let mut layout = Self { left: Vec::new(), right: Vec::new() };
        let mut seen = Vec::new();
        for part in text.trim().split(';') {
            let (name, columns) = part.split_once('=')?;
            let side = match name.trim() {
                "left" => Side::Left,
                "right" => Side::Right,
                _ => return None,
            };
            for column in columns.split(',').map(str::trim).filter(|c| !c.is_empty()) {
                let mut panes = Vec::new();
                for code in column.split('+') {
                    let pane = Pane::from_code(code.trim())?;
                    if seen.contains(&pane) {
                        return None;
                    }
                    seen.push(pane);
                    panes.push(pane);
                }
                layout.side_mut(side).push(panes);
            }
        }
        (seen.len() == Pane::ALL.len()).then_some(layout)
    }
}

/// Default width of a column and its range.
const COLUMN_WIDTH: f32 = 290.0;
const COLUMN_RANGE: std::ops::RangeInclusive<f32> = 220.0..=520.0;
/// Least height of a stacked panel.
const MIN_PANE_HEIGHT: f32 = 120.0;
/// Width of the strip along each side of the window that docks a panel there.
const EDGE_STRIP: f32 = 36.0;

/// The id under which egui's memory keeps the panel being dragged.
fn drag_id() -> Id {
    Id::new("side-pane-drag")
}

/// Whether `pane` shows now: the component environment shows only its own
/// panel (where Properties is), a profile sketch only Properties.
fn visible(state: &AppState, pane: Pane) -> bool {
    pane == Pane::Properties || (state.component_env.is_none() && !crate::profile_sketch::is_open(state))
}

/// Show the side panels; call it after the top and bottom bars and before
/// the central area.
pub fn show(ui: &mut Ui, state: &mut AppState) {
    let area = ui.available_rect_before_wrap();
    let layout = state.prefs.panels.clone();
    let mut rects: Vec<(Pane, Rect)> = Vec::new();
    for side in [Side::Left, Side::Right] {
        for column in layout.side(side) {
            let panes: Vec<Pane> = column.iter().copied().filter(|p| visible(state, *p)).collect();
            let Some(&first) = column.first() else { continue };
            if panes.is_empty() {
                continue;
            }
            let id = Id::new(("side-column", side, first));
            let panel = match side {
                Side::Left => egui::Panel::left(id),
                Side::Right => egui::Panel::right(id),
            };
            panel
                .frame(egui::Frame::new().fill(PALETTE.panel).stroke(Stroke::new(1.0, PALETTE.border)))
                .resizable(true)
                .default_size(COLUMN_WIDTH)
                .size_range(COLUMN_RANGE)
                .show(ui, |ui| column_ui(ui, state, &panes, &mut rects));
        }
    }
    drag(ui.ctx(), state, area, &rects);
}

/// The panels of one column, each but the last in a resizable strip.
fn column_ui(ui: &mut Ui, state: &mut AppState, panes: &[Pane], rects: &mut Vec<(Pane, Rect)>) {
    let n = panes.len();
    for (i, &pane) in panes.iter().enumerate() {
        if i + 1 == n {
            pane_ui(ui, state, pane, rects);
            continue;
        }
        let h = ui.available_height();
        let below = (n - i - 1) as f32 * MIN_PANE_HEIGHT;
        egui::Panel::top(Id::new(("side-pane", pane)))
            .frame(egui::Frame::new().fill(PALETTE.panel))
            .resizable(true)
            .default_size(h / (n - i) as f32)
            .size_range(MIN_PANE_HEIGHT..=(h - below).max(MIN_PANE_HEIGHT))
            .show(ui, |ui| pane_ui(ui, state, pane, rects));
    }
}

fn pane_ui(ui: &mut Ui, state: &mut AppState, pane: Pane, rects: &mut Vec<(Pane, Rect)>) {
    // The component environment has its own panel, with no header.
    if state.component_env.is_some() {
        crate::component_env::panel(ui, state);
        return;
    }
    rects.push((pane, ui.available_rect_before_wrap()));
    let subtitle = match pane {
        Pane::Browser => Some(state.project().info.name.clone()),
        Pane::Properties => None,
    };
    let header = theme::panel_header(ui, &pane.title(), subtitle.as_deref());
    header_interaction(ui, state, pane, &header);
    match pane {
        Pane::Browser => crate::browser::body(ui, state),
        Pane::Properties => crate::properties::body(ui, state),
    }
}

/// A header starts a drag of its panel and opens the docking menu on a right click.
fn header_interaction(ui: &mut Ui, state: &mut AppState, pane: Pane, header: &Response) {
    let response = ui.interact(header.rect, Id::new(("side-pane-header", pane)), Sense::click_and_drag());
    if response.drag_started() {
        ui.ctx().data_mut(|d| d.insert_temp(drag_id(), pane));
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    response.on_hover_text(t!("panel-drag-hint")).context_menu(|ui| {
        if menu(ui, state, pane) {
            ui.close();
        }
    });
}

/// The docking choices for `pane`; true when one was taken.
pub fn menu(ui: &mut Ui, state: &mut AppState, pane: Pane) -> bool {
    let layout = &state.prefs.panels;
    let mut choices =
        vec![(t!("panel-dock-left"), Target::Edge(Side::Left)), (t!("panel-dock-right"), Target::Edge(Side::Right))];
    for other in Pane::ALL.into_iter().filter(|o| *o != pane) {
        let (Some((side, c, _)), Some((other_side, oc, _))) = (layout.find(pane), layout.find(other)) else { continue };
        if (side, c) == (other_side, oc) {
            // A column of its own, further from the edge than the other's.
            let target = Target::Beside { other, right: other_side == Side::Left };
            choices.push((t!("panel-separate-from", "panel" => other.title()), target));
        } else {
            // The browser goes on top.
            let target = Target::Stack { other, below: pane != Pane::Browser };
            choices.push((t!("panel-stack-with", "panel" => other.title()), target));
        }
    }
    let mut taken = false;
    for (label, target) in choices {
        let mut moved = state.prefs.panels.clone();
        let enabled = moved.apply(pane, target);
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            state.prefs.panels = moved;
            taken = true;
        }
    }
    ui.separator();
    let default = PanelLayout::default();
    if ui.add_enabled(state.prefs.panels != default, egui::Button::new(t!("panel-reset-layout"))).clicked() {
        state.prefs.panels = default;
        taken = true;
    }
    taken
}

/// Where a panel dragged to `pos` would go, and the area that shows it:
/// the strips along the window's sides dock it there, and over another
/// panel the nearest edge picks above, below, left or right of it.
pub fn target_at(pos: Pos2, pane: Pane, area: Rect, rects: &[(Pane, Rect)]) -> Option<(Target, Rect)> {
    // Past the area's sides (the window's border) still docks there.
    if !area.y_range().contains(pos.y) {
        return None;
    }
    let preview_width = COLUMN_WIDTH.min(area.width() / 3.0);
    if pos.x < area.left() + EDGE_STRIP {
        let rect = Rect::from_min_max(area.min, Pos2::new(area.left() + preview_width, area.bottom()));
        return Some((Target::Edge(Side::Left), rect));
    }
    if pos.x > area.right() - EDGE_STRIP {
        let rect = Rect::from_min_max(Pos2::new(area.right() - preview_width, area.top()), area.max);
        return Some((Target::Edge(Side::Right), rect));
    }
    if !area.contains(pos) {
        return None;
    }
    let &(other, r) = rects.iter().find(|(p, r)| *p != pane && r.contains(pos))?;
    if r.width() <= 0.0 || r.height() <= 0.0 {
        return None;
    }
    let u = (pos.x - r.left()) / r.width();
    let v = (pos.y - r.top()) / r.height();
    let edges = [(u, 0), (1.0 - u, 1), (v, 2), (1.0 - v, 3)];
    let (_, edge) = edges.into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
    let c = r.center();
    Some(match edge {
        0 => (Target::Beside { other, right: false }, Rect::from_min_max(r.min, Pos2::new(c.x, r.bottom()))),
        1 => (Target::Beside { other, right: true }, Rect::from_min_max(Pos2::new(c.x, r.top()), r.max)),
        2 => (Target::Stack { other, below: false }, Rect::from_min_max(r.min, Pos2::new(r.right(), c.y))),
        _ => (Target::Stack { other, below: true }, Rect::from_min_max(Pos2::new(r.left(), c.y), r.max)),
    })
}

/// A panel being dragged: the area it would take is shown under the
/// pointer, and releasing the button moves it there (Escape cancels).
fn drag(ctx: &egui::Context, state: &mut AppState, area: Rect, rects: &[(Pane, Rect)]) {
    let Some(pane) = ctx.data(|d| d.get_temp::<Pane>(drag_id())) else { return };
    let (pos, down, escape) =
        ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down(), i.key_pressed(egui::Key::Escape)));
    let target = pos.and_then(|p| target_at(p, pane, area, rects));
    if escape || !down {
        ctx.data_mut(|d| d.remove::<Pane>(drag_id()));
        if !escape && let Some((target, _)) = target {
            state.prefs.panels.apply(pane, target);
        }
        return;
    }
    ctx.set_cursor_icon(CursorIcon::Grabbing);
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("side-pane-drop")));
    if let Some((_, rect)) = target {
        painter.rect_filled(rect, 3.0, PALETTE.accent.gamma_multiply(0.18));
        painter.rect_stroke(rect, 3.0, Stroke::new(2.0, PALETTE.accent), egui::StrokeKind::Inside);
    }
    if let Some(p) = pos {
        let font = egui::FontId::proportional(12.5);
        let galley = painter.layout_no_wrap(pane.title(), font, PALETTE.text);
        let at = p + egui::vec2(14.0, 10.0);
        let back = Rect::from_min_size(at, galley.size()).expand2(egui::vec2(8.0, 4.0));
        painter.rect(back, 3.0, PALETTE.surface, Stroke::new(1.0, PALETTE.border_strong), egui::StrokeKind::Inside);
        painter.galley(at, galley, Color32::PLACEHOLDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Pane::{Browser, Properties};

    #[test]
    fn panels_dock_stack_and_separate_on_either_side() {
        let mut l = PanelLayout::default();
        // Properties to the right edge: one panel on each side.
        assert!(l.apply(Properties, Target::Edge(Side::Right)));
        assert_eq!((l.left.clone(), l.right.clone()), (vec![vec![Browser]], vec![vec![Properties]]));
        // The browser stacked above it: both on the right.
        assert!(l.apply(Browser, Target::Stack { other: Properties, below: false }));
        assert_eq!((l.left.clone(), l.right.clone()), (vec![], vec![vec![Browser, Properties]]));
        // Separated on the right: the browser in a column left of (inward from) Properties.
        assert!(l.apply(Browser, Target::Beside { other: Properties, right: false }));
        assert_eq!(l.right, vec![vec![Properties], vec![Browser]]);
        // And outward of it, at the window's edge.
        assert!(l.apply(Browser, Target::Beside { other: Properties, right: true }));
        assert_eq!(l.right, vec![vec![Browser], vec![Properties]]);
        // Side by side on the left.
        assert!(l.apply(Browser, Target::Edge(Side::Left)));
        assert!(l.apply(Properties, Target::Beside { other: Browser, right: true }));
        assert_eq!((l.left.clone(), l.right.clone()), (vec![vec![Browser], vec![Properties]], vec![]));
        // Nothing changes, or the panel itself is named.
        assert!(!l.apply(Browser, Target::Edge(Side::Left)));
        assert!(!l.apply(Browser, Target::Stack { other: Browser, below: true }));
        // Properties above the browser, stacked again.
        assert!(l.apply(Properties, Target::Stack { other: Browser, below: false }));
        assert_eq!(l.left, vec![vec![Properties, Browser]]);
    }

    #[test]
    fn layouts_are_stored_as_text_and_bad_text_is_refused() {
        let mut l = PanelLayout::default();
        assert_eq!(PanelLayout::from_text(&l.to_text()), Some(l.clone()));
        l.apply(Properties, Target::Edge(Side::Right));
        assert_eq!(l.to_text(), "left=browser;right=properties");
        assert_eq!(PanelLayout::from_text(&l.to_text()), Some(l));
        for bad in ["", "left=browser", "left=browser+browser;right=properties", "up=browser+properties", "left=x"] {
            assert_eq!(PanelLayout::from_text(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_pointer_picks_a_side_or_an_edge_of_another_panel() {
        let area = Rect::from_min_max(Pos2::new(0.0, 100.0), Pos2::new(1200.0, 800.0));
        let browser = Rect::from_min_max(Pos2::new(0.0, 100.0), Pos2::new(290.0, 450.0));
        let props = Rect::from_min_max(Pos2::new(0.0, 450.0), Pos2::new(290.0, 800.0));
        let rects = [(Browser, browser), (Properties, props)];
        let at = |x, y| target_at(Pos2::new(x, y), Properties, area, &rects).map(|(t, _)| t);
        assert_eq!(at(1190.0, 400.0), Some(Target::Edge(Side::Right)));
        assert_eq!(at(10.0, 600.0), Some(Target::Edge(Side::Left)));
        assert_eq!(at(150.0, 110.0), Some(Target::Stack { other: Browser, below: false }));
        assert_eq!(at(280.0, 300.0), Some(Target::Beside { other: Browser, right: true }));
        // Over itself or the views: nowhere.
        assert_eq!(at(150.0, 600.0), None);
        assert_eq!(at(600.0, 400.0), None);
        assert_eq!(at(600.0, 50.0), None);
    }
}
