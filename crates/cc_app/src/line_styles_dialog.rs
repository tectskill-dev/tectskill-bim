//! The Line Styles dialog (*Annotate → Line Styles*): the built-in line
//! styles, read only, and the project's own, whose name, weight, color and
//! dash pattern are edited in place; *New* adds one and *Delete* removes
//! the selected one (what uses it moves to the thin style). Every change
//! runs a `line_style_*` command; OK keeps them, Cancel undoes them all
//! (`dialog::session`, through [`crate::graphics_dialog::Edits`]).

use crate::graphics_dialog::{Edits, header, key, run};
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::theme::PALETTE;
use cc_core::detail::{DetailStyle, MAX_WEIGHT_MM, MIN_WEIGHT_MM};
use cc_core::{LinePattern, LineStyleId};
use cc_engine::Ref;
use cc_engine::commands::line_style as cmd;
use egui::{Context, RichText, Ui};

/// The open Line Styles dialog.
pub struct LineStylesDialog {
    selected: Option<LineStyleId>,
    /// A name being typed: the style and its text.
    draft: Option<(LineStyleId, String)>,
    edits: Edits,
}

impl LineStylesDialog {
    /// Close the dialog's edits: keep them (OK) or undo them (Cancel).
    pub fn edits_finish(self, state: &mut AppState, keep: bool) {
        self.edits.finish(state, keep);
    }
}

pub fn open(state: &mut AppState) {
    let edits = Edits::begin(state);
    let selected = state.project().line_styles.keys().next().copied();
    state.dialogs.line_styles = Some(LineStylesDialog { selected, draft: None, edits });
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(d) = state.dialogs.line_styles.as_mut() else { return };
    let (mut selected, mut draft) = (d.selected, d.draft.take());
    selected = selected.filter(|id| state.project().line_style(*id).is_some());
    let mut ok = 0;
    let response = crate::dialog::Dialog::new("line-styles", t!("dlg-line-styles")).width(640.0).show(
        ctx,
        |ui| {
            ui.label(RichText::new(t!("ls-hint")).size(11.0).color(PALETTE.text_faint));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| table(ui, state, &mut selected, &mut draft));
            ui.add_space(6.0);
            ui.horizontal(|ui| buttons(ui, state, &mut selected));
        },
        |f| {
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if let Some(d) = state.dialogs.line_styles.as_mut() {
        (d.selected, d.draft) = (selected, draft);
    }
    let close = match response.clicked {
        Some(i) => Some(i == ok),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.line_styles.take()
    {
        d.edits_finish(state, keep);
    }
}

fn pattern_label(p: LinePattern) -> String {
    tr(p.key())
}

fn table(
    ui: &mut Ui,
    state: &mut AppState,
    selected: &mut Option<LineStyleId>,
    draft: &mut Option<(LineStyleId, String)>,
) {
    let styles: Vec<_> = state.project().line_styles.values().cloned().collect();
    let used: Vec<usize> = styles.iter().map(|s| state.project().line_style_users(s.id).len()).collect();
    egui::Grid::new("line-styles-table").num_columns(5).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
        for h in [t!("prop-name"), t!("ls-weight"), t!("mat-color"), t!("mat-pattern"), t!("ls-used-by")] {
            header(ui, h);
        }
        ui.end_row();
        // The built-in styles, read only.
        for s in DetailStyle::ALL {
            ui.label(RichText::new(tr(s.key())).color(PALETTE.text_muted));
            // The number alone in metric projects, with the inch mark in imperial ones.
            let weight = if cc_views::format::imperial() {
                cc_views::format::paper_weight(s.weight_mm())
            } else {
                format!("{:.2}", s.weight_mm())
            };
            ui.label(RichText::new(weight).color(PALETTE.text_muted));
            ui.label(RichText::new(t!("ls-black")).color(PALETTE.text_muted));
            ui.label(RichText::new(pattern_label(s.pattern())).color(PALETTE.text_muted));
            ui.label(RichText::new(t!("ls-built-in")).color(PALETTE.text_faint));
            ui.end_row();
        }
        for (s, n) in styles.iter().zip(used) {
            let id = s.id;
            let style = Ref::Id(id.0);
            ui.horizontal(|ui| {
                if ui.radio(*selected == Some(id), "").clicked() {
                    *selected = Some(id);
                }
                let mut text = match draft {
                    Some((d, t)) if *d == id => t.clone(),
                    _ => s.name.clone(),
                };
                let r = ui.add_sized([170.0, 22.0], egui::TextEdit::singleline(&mut text));
                if r.changed() {
                    *draft = Some((id, text.clone()));
                }
                if r.gained_focus() {
                    *selected = Some(id);
                }
                if r.lost_focus() {
                    if text.trim() != s.name.trim() {
                        let command =
                            cmd::LineStyleSet { style: style.clone(), name: Some(text), ..Default::default() };
                        run(state, None, command);
                    }
                    *draft = None;
                }
            });
            let mut w = s.weight_mm;
            let r = ui.add(crate::properties::weight_drag(&mut w).speed(0.01).range(MIN_WEIGHT_MM..=MAX_WEIGHT_MM));
            if r.changed() {
                let command = cmd::LineStyleSet { style: style.clone(), weight: Some(w), ..Default::default() };
                run(state, Some(key((id, "weight"))), command);
            }
            if r.drag_stopped() || r.lost_focus() {
                state.session.doc.break_merge();
            }
            let mut c = s.color;
            if ui.color_edit_button_srgb(&mut c).changed() {
                let command = cmd::LineStyleSet { style: style.clone(), color: Some(c), ..Default::default() };
                run(state, Some(key((id, "color"))), command);
            }
            let mut p = s.pattern;
            egui::ComboBox::from_id_salt(("line-style-pattern", id.0)).selected_text(pattern_label(p)).show_ui(
                ui,
                |ui| {
                    for option in LinePattern::ALL {
                        ui.selectable_value(&mut p, option, pattern_label(option));
                    }
                },
            );
            if p != s.pattern {
                let pattern = Some(cmd::pattern_code(p).to_string());
                run(state, None, cmd::LineStyleSet { style, pattern, ..Default::default() });
            }
            ui.label(n.to_string());
            ui.end_row();
        }
    });
    if styles.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(t!("ls-none")).color(PALETTE.text_faint));
    }
}

fn buttons(ui: &mut Ui, state: &mut AppState, selected: &mut Option<LineStyleId>) {
    if ui.button(t!("ls-new")).on_hover_text(t!("ls-new-tip")).clicked() {
        let names: Vec<String> = state.project().line_styles.values().map(|s| s.name.to_lowercase()).collect();
        let name = (1..1000)
            .map(|n| t!("ls-default-name", "n" => n))
            .find(|n| !names.contains(&n.to_lowercase()))
            .unwrap_or_default();
        let command = cmd::LineStyleCreate { name, ..Default::default() };
        if let Some(out) = crate::actions::run_command(state, command) {
            *selected = out["id"].as_u64().map(LineStyleId);
        }
    }
    let can_delete = selected.is_some();
    if ui.add_enabled(can_delete, egui::Button::new(t!("ls-delete"))).on_hover_text(t!("ls-delete-tip")).clicked()
        && let Some(id) = selected.take()
    {
        run(state, None, cmd::LineStyleDelete { style: Ref::Id(id.0), replace_with: None });
    }
}
