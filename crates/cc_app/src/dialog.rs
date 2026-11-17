//! Standard dialog frame shared by every modal window: a header with the
//! title and a close button, a padded body (with an icon for alerts) and a
//! footer with the buttons.
//!
//! Footer conventions: action buttons are right-aligned in the usual desktop
//! order (main action first, Cancel last), auxiliary buttons sit on the left,
//! the main action is highlighted and destructive actions are red. Esc and the
//! close button dismiss the dialog, like Cancel; clicks outside are ignored.

use crate::icons::{self, Icon};
use crate::theme::{self, PALETTE};
use egui::{Color32, Context, RichText, Stroke, Ui, vec2};

/// Minimum size of footer buttons.
pub const BUTTON_SIZE: egui::Vec2 = egui::Vec2::new(88.0, 28.0);
const PAD_X: i8 = 20;

/// Icon shown next to the message of an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Warning,
    Error,
}

impl Kind {
    fn color(self) -> Color32 {
        match self {
            Kind::Warning => PALETTE.warning,
            Kind::Error => PALETTE.danger,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Kind::Warning => "!",
            Kind::Error => "×",
        }
    }
}

/// Visual weight of a footer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Primary,
    Secondary,
    Danger,
}

/// A highlighted button with the standard footer size.
pub fn styled_button(ui: &mut Ui, label: &str, style: Style) -> egui::Response {
    let button = match style {
        Style::Primary => {
            egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(PALETTE.accent).stroke(Stroke::NONE)
        }
        Style::Danger => {
            egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(PALETTE.danger).stroke(Stroke::NONE)
        }
        Style::Secondary => egui::Button::new(label),
    };
    ui.add(button.min_size(BUTTON_SIZE))
}

/// Footer buttons, declared in display order; [`Dialog::show`] reports which was clicked.
#[derive(Default)]
pub struct Footer {
    actions: Vec<(String, Style)>,
    /// Auxiliary buttons and whether each shows as pressed (see [`Footer::toggle`]).
    extras: Vec<(String, bool)>,
    /// Actions shown greyed out (see [`Footer::disable`]).
    disabled: Vec<usize>,
}

impl Footer {
    /// The main action (Enter triggers it when no text field has focus).
    pub fn primary(&mut self, label: impl Into<String>) -> usize {
        self.push(label, Style::Primary)
    }

    pub fn secondary(&mut self, label: impl Into<String>) -> usize {
        self.push(label, Style::Secondary)
    }

    pub fn danger(&mut self, label: impl Into<String>) -> usize {
        self.push(label, Style::Danger)
    }

    /// Grey out the button `index` (as returned by [`Footer::primary`] and
    /// the others, auxiliary buttons too): it cannot be clicked and Enter
    /// does not press it.
    pub fn disable(&mut self, index: usize) {
        self.disabled.push(index);
    }

    /// An auxiliary button on the left side. Its index follows the actions'.
    pub fn extra(&mut self, label: impl Into<String>) -> usize {
        self.toggle(label, false)
    }

    /// An auxiliary button that stays pressed while `on` (a mode of the
    /// dialog the button turns on and off). Its index follows the actions'.
    pub fn toggle(&mut self, label: impl Into<String>, on: bool) -> usize {
        self.extras.push((label.into(), on));
        1000 + self.extras.len() - 1
    }

    fn push(&mut self, label: impl Into<String>, style: Style) -> usize {
        self.actions.push((label.into(), style));
        self.actions.len() - 1
    }

    /// Width the buttons take side by side, with the footer's spacing.
    fn width(&self, ui: &Ui) -> f32 {
        let padding = 2.0 * ui.spacing().button_padding.x;
        let labels = self.extras.iter().map(|(label, _)| label).chain(self.actions.iter().map(|(label, _)| label));
        let (mut total, mut count) = (0.0, 0usize);
        for label in labels {
            let text = egui::WidgetText::from(label.as_str()).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Button,
            );
            total += (text.size().x + padding).max(BUTTON_SIZE.x);
            count += 1;
        }
        total + FOOTER_SPACING * count.saturating_sub(1) as f32
    }
}

/// Gap between footer buttons.
const FOOTER_SPACING: f32 = 8.0;

/// Where a dialog stands while it is open: moved by its header, and as wide
/// as its body turned out to be. Forgotten when the dialog closes, so it
/// opens centered again.
#[derive(Clone, Copy, Default)]
struct Placed {
    /// Offset from the center of the window.
    offset: egui::Vec2,
    /// Width the body took in the last frame.
    body_width: f32,
    /// Size of the whole dialog in the last frame.
    size: egui::Vec2,
    /// The header and its close button in the last frame.
    header: Option<(egui::Rect, egui::Rect)>,
    /// The header is being dragged.
    moving: bool,
    /// Frames in a row the body has been wider than the dialog.
    wide_frames: u32,
    /// Frame the dialog was last shown in.
    frame: u64,
}

impl Placed {
    fn load(ctx: &Context, id: egui::Id) -> Self {
        let now = ctx.cumulative_frame_nr();
        let kept = ctx.data(|d| d.get_temp::<Placed>(id)).filter(|p| now.saturating_sub(p.frame) <= 1);
        Placed { frame: now, ..kept.unwrap_or_default() }
    }

    fn store(self, ctx: &Context, id: egui::Id) {
        ctx.data_mut(|d| d.insert_temp(id, self));
    }

    /// Move with the pointer while the header is dragged: from a press on
    /// the header (not on its close button, nor on a popup or another
    /// dialog over it) until the button is let go. The pointer is read
    /// directly because the modal itself takes the drags of its whole area.
    fn follow(&mut self, ctx: &Context, layer: egui::LayerId) {
        let (pressed, down, origin, delta) = ctx.input(|i| {
            (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.delta())
        });
        if !down {
            self.moving = false;
        } else if pressed && let (Some(origin), Some((header, close))) = (origin, self.header) {
            self.moving = header.contains(origin) && !close.contains(origin) && ctx.layer_id_at(origin) == Some(layer);
        }
        if self.moving {
            self.drag(ctx, delta);
        }
    }

    /// Move by `delta`, keeping the dialog inside the window (egui holds it
    /// there anyway, and the offset must not run ahead of what is shown).
    fn drag(&mut self, ctx: &Context, delta: egui::Vec2) {
        let screen = ctx.content_rect();
        let centered = screen.center() - self.size / 2.0;
        let min = screen.min;
        let max = (screen.max - self.size).max(min);
        let wanted = centered + self.offset + delta;
        let pos = egui::pos2(wanted.x.clamp(min.x, max.x), wanted.y.clamp(min.y, max.y));
        self.offset = pos - centered;
    }
}

/// In tests, a body wider than its dialog is an error: every UI test that
/// opens a dialog checks its layout.
#[cfg(test)]
pub mod overflow {
    use std::cell::Cell;

    thread_local! {
        static ALLOWED: Cell<bool> = const { Cell::new(false) };
    }

    /// Let bodies overflow in this test thread (tests of the overflow itself).
    pub fn allow() {
        ALLOWED.with(|a| a.set(true));
    }

    pub(super) fn check(title: &str, by: f32, frames: u32) {
        assert!(frames < 3 || ALLOWED.with(Cell::get), "the body of dialog \"{title}\" is {by} points too wide");
    }
}

pub struct DialogResponse {
    /// Index returned by the [`Footer`] method of the clicked button.
    pub clicked: Option<usize>,
    /// Closed with Esc or the close button.
    pub dismissed: bool,
}

pub struct Dialog {
    id: egui::Id,
    title: String,
    width: f32,
    kind: Option<Kind>,
}

impl Dialog {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>) -> Self {
        Self { id: egui::Id::new(("dialog", id)), title: title.into(), width: 420.0, kind: None }
    }

    /// Width of the body content.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Show an alert icon next to the body.
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn show(self, ctx: &Context, body: impl FnOnce(&mut Ui), footer: impl FnOnce(&mut Footer)) -> DialogResponse {
        // A single-line text field gives up the focus in the frame Enter is
        // pressed in, so the focus is read before the body: an Enter that
        // confirms a field (a search, a name) must not also press OK.
        let focus_before = ctx.memory(|m| m.focused().is_some());
        let mut buttons = Footer::default();
        footer(&mut buttons);
        let frame = egui::Frame::popup(&ctx.style_of(egui::Theme::Light))
            .inner_margin(egui::Margin::ZERO)
            .corner_radius(8)
            .fill(PALETTE.panel);
        let mut clicked = None;
        let mut close_clicked = false;
        let mut placed = Placed::load(ctx, self.id);
        placed.follow(ctx, egui::LayerId::new(egui::Order::Foreground, self.id));
        let area = egui::Modal::default_area(self.id).anchor(egui::Align2::CENTER_CENTER, placed.offset);
        let modal = egui::Modal::new(self.id).area(area).frame(frame).show(ctx, |ui| {
            // The footer buttons always fit: a dialog too narrow for them widens.
            let body_width = self.width.max(buttons.width(ui).ceil() + 1.0);
            // A body that outgrew its width last frame takes the header and
            // the footer with it. The body itself is laid out at its own
            // width every frame, so the dialog narrows again when it can.
            let content_width = body_width.max(placed.body_width);
            ui.set_width(content_width + 2.0 * PAD_X as f32);
            ui.spacing_mut().item_spacing.y = 0.0;

            // Header; dragging it moves the dialog (see `Placed::follow`).
            let mut close_rect = egui::Rect::NOTHING;
            let header = egui::Frame::new()
                .inner_margin(egui::Margin { left: PAD_X, right: 12, top: 14, bottom: 6 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Not selectable: a drag on the title moves the dialog.
                        let title = RichText::new(&self.title).family(theme::semibold()).size(15.0).color(PALETTE.text);
                        ui.add(egui::Label::new(title).selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (rect, r) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
                            close_rect = rect;
                            if r.hovered() {
                                ui.painter().rect_filled(rect, theme::RADIUS as f32, PALETTE.hover);
                            }
                            let color = if r.hovered() { PALETTE.text } else { PALETTE.text_muted };
                            icons::paint(ui.painter(), rect.shrink(8.0), Icon::Close, color);
                            r.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::t!("action-close"))
                            });
                            close_clicked = r.on_hover_text(crate::t!("action-close")).clicked();
                        });
                    });
                });
            placed.header = Some((header.response.rect, close_rect));

            // Body.
            let shown = egui::Frame::new()
                .inner_margin(egui::Margin { left: PAD_X, right: PAD_X, top: 8, bottom: 18 })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    match self.kind {
                        Some(kind) => {
                            ui.horizontal_top(|ui| {
                                alert_icon(ui, kind);
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.set_width(body_width - 48.0);
                                    body(ui);
                                });
                            });
                        }
                        None => {
                            ui.set_width(body_width);
                            body(ui);
                        }
                    }
                });
            let measured = shown.response.rect.width() - 2.0 * PAD_X as f32;
            // A scroll area finding out it needs its bar overflows for one frame.
            placed.wide_frames = if measured > body_width + 1.0 { placed.wide_frames + 1 } else { 0 };
            #[cfg(test)]
            overflow::check(&self.title, measured - body_width, placed.wide_frames);
            // Lay the header and footer out again at once, so a dialog never
            // shows with them narrower than its body.
            if (content_width - body_width.max(measured)).abs() > 0.5 {
                ctx.request_discard("dialog width");
            }
            placed.body_width = measured;

            // Footer.
            let response = egui::Frame::new()
                .fill(PALETTE.surface)
                .inner_margin(egui::Margin { left: PAD_X, right: PAD_X, top: 12, bottom: 12 })
                .show(ui, |ui| {
                    ui.set_width(content_width);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = FOOTER_SPACING;
                        for (i, (label, on)) in buttons.extras.iter().enumerate() {
                            let enabled = !buttons.disabled.contains(&(1000 + i));
                            let r = ui
                                .add_enabled_ui(enabled, |ui| {
                                    if *on {
                                        ui.add(egui::Button::new(label.as_str()).selected(true).min_size(BUTTON_SIZE))
                                    } else {
                                        styled_button(ui, label, Style::Secondary)
                                    }
                                })
                                .inner;
                            if r.clicked() {
                                clicked = Some(1000 + i);
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for (i, (label, style)) in buttons.actions.iter().enumerate().rev() {
                                let enabled = !buttons.disabled.contains(&i);
                                if ui.add_enabled_ui(enabled, |ui| styled_button(ui, label, *style)).inner.clicked() {
                                    clicked = Some(i);
                                }
                            }
                        });
                    });
                });
            let r = response.response.rect;
            ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, PALETTE.border));
            placed.size = ui.min_rect().size();
        });
        placed.store(ctx, self.id);

        // Keyboard shortcuts act on the topmost dialog only (one may open over another).
        let top = modal.is_top_modal;
        let no_text_focus = !focus_before && ctx.memory(|m| m.focused().is_none());
        if top && clicked.is_none() && no_text_focus && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            clicked = buttons
                .actions
                .iter()
                .enumerate()
                .position(|(i, (_, s))| *s == Style::Primary && !buttons.disabled.contains(&i));
        }
        let escape = top && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        DialogResponse { clicked, dismissed: close_clicked || escape }
    }
}

/// Two top-down columns side by side (a list and its details), with a divider
/// as tall as the taller column; `column` draws column 0 (left) and 1 (right).
/// `ui.separator()` in a row takes the available height instead, and in a
/// modal that grows with the modal every frame until the dialog runs off the
/// window.
pub fn two_columns(ui: &mut Ui, widths: [f32; 2], mut column: impl FnMut(&mut Ui, usize)) {
    let layout = egui::Layout::top_down(egui::Align::Min);
    let height = ui.ctx().content_rect().height();
    ui.horizontal_top(|ui| {
        let mut side = |ui: &mut Ui, i: usize| {
            // The last column takes what is left, so the columns never widen the dialog.
            let width = if i == 1 { widths[i].min(ui.available_width()) } else { widths[i] };
            ui.allocate_ui_with_layout(vec2(width, height), layout, |ui| {
                ui.set_width(width);
                column(ui, i);
            })
            .response
            .rect
        };
        let l = side(ui, 0);
        let (divider, _) = ui.allocate_exact_size(vec2(9.0, 0.0), egui::Sense::hover());
        let r = side(ui, 1);
        let y = l.top().min(r.top())..=l.bottom().max(r.bottom());
        ui.painter().vline(divider.center().x, y, Stroke::new(1.0, PALETTE.border));
    });
}

/// Round colored badge with the alert glyph.
fn alert_icon(ui: &mut Ui, kind: Kind) {
    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 15.0, kind.color());
    let font = egui::FontId::new(17.0, theme::semibold());
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, kind.glyph(), font, Color32::WHITE);
}

/// Edits made while a data dialog is open can be cancelled as a whole: the
/// dialog remembers the undo depth when it opens and undoes back to it.
pub mod session {
    use crate::state::AppState;

    /// Mark the start of the dialog's edits (no-op if already marked).
    pub fn begin(state: &mut AppState) {
        if state.dialogs.edit_start.is_none() {
            state.session.doc.break_merge();
            state.dialogs.edit_start = Some(state.session.doc.undo_depth());
        }
    }

    /// Keep the edits.
    pub fn commit(state: &mut AppState) {
        state.session.doc.break_merge();
        state.dialogs.edit_start = None;
    }

    /// Undo every edit made since [`begin`].
    pub fn cancel(state: &mut AppState) {
        if let Some(start) = state.dialogs.edit_start.take() {
            state.session.undo_to(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::session;
    use crate::state::AppState;
    use cc_core::{Document, Project};

    #[test]
    fn cancel_undoes_the_dialog_edits_and_ok_keeps_them() {
        let mut state = AppState::new(Document::new(Project::from_template()));
        state.session.doc.edit("before", |e| e.info_mut().name = "A".into());

        session::begin(&mut state);
        state.session.doc.edit("in dialog", |e| e.info_mut().name = "B".into());
        state.session.doc.edit("in dialog", |e| e.info_mut().number = "42".into());
        session::cancel(&mut state);
        assert_eq!(state.project().info.name, "A");
        assert_ne!(state.project().info.number, "42");

        session::begin(&mut state);
        state.session.doc.edit("in dialog", |e| e.info_mut().name = "C".into());
        session::commit(&mut state);
        assert_eq!(state.project().info.name, "C");
        assert!(state.dialogs.edit_start.is_none());
    }
}
