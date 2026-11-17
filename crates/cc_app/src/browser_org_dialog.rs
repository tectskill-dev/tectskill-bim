//! The Browser Organization dialog (*View → Browser Organization*, also in
//! the menu of the project browser's groups): the schemes that group the
//! browser's views. The marked scheme is the one in use, and the browser
//! follows it at once; *Default* lists the views by type alone. A project
//! scheme has a name and the criteria it nests, outermost first. Every
//! change runs a `browser_scheme_*` command; OK keeps them, Cancel undoes
//! them all (`dialog::session`, through [`crate::graphics_dialog::Edits`]).

use crate::graphics_dialog::{Edits, run};
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::theme::PALETTE;
use cc_core::BrowserSchemeId;
use cc_core::browser::{BrowserScheme, GroupBy, MAX_CRITERIA, MAX_SCHEMES};
use cc_engine::Ref;
use cc_engine::commands::browser as cmd;
use egui::{Context, RichText, Ui};

/// The open Browser Organization dialog.
pub struct BrowserOrgDialog {
    /// A name being typed: the scheme and its text.
    draft: Option<(BrowserSchemeId, String)>,
    edits: Edits,
}

impl BrowserOrgDialog {
    /// Close the dialog's edits: keep them (OK) or undo them (Cancel).
    pub fn edits_finish(self, state: &mut AppState, keep: bool) {
        self.edits.finish(state, keep);
    }
}

pub fn open(state: &mut AppState) {
    let edits = Edits::begin(state);
    state.dialogs.browser_org = Some(BrowserOrgDialog { draft: None, edits });
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(d) = state.dialogs.browser_org.as_mut() else { return };
    let mut draft = d.draft.take();
    let mut ok = 0;
    let response =
        crate::dialog::Dialog::new("browser-organization", t!("action-browser-organization")).width(520.0).show(
            ctx,
            |ui| {
                ui.label(RichText::new(t!("browser-org-hint")).size(11.0).color(PALETTE.text_faint));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| schemes(ui, state, &mut draft));
                ui.add_space(6.0);
                ui.horizontal(|ui| buttons(ui, state));
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                criteria(ui, state);
            },
            |f| {
                ok = f.primary(t!("action-ok"));
                f.secondary(t!("action-cancel"));
            },
        );
    if let Some(d) = state.dialogs.browser_org.as_mut() {
        d.draft = draft;
    }
    let close = match response.clicked {
        Some(i) => Some(i == ok),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.browser_org.take()
    {
        d.edits_finish(state, keep);
    }
}

/// The names of `criteria`, outermost first.
fn summary(criteria: &[GroupBy]) -> String {
    criteria.iter().map(|g| tr(g.key())).collect::<Vec<_>>().join(" › ")
}

/// The list of schemes: the default, then the project's, each with a mark
/// that puts it in use and, for the project's, its name to edit.
fn schemes(ui: &mut Ui, state: &mut AppState, draft: &mut Option<(BrowserSchemeId, String)>) {
    let list: Vec<BrowserScheme> = state.project().browser.schemes.clone();
    let in_use = state.project().browser.in_use().map(|s| s.id);
    egui::Grid::new("browser-schemes").num_columns(2).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
        if ui.radio(in_use.is_none(), t!("browser-org-default")).clicked() && in_use.is_some() {
            run(state, None, cmd::BrowserSchemeUse { scheme: None });
        }
        ui.label(RichText::new(summary(&cc_core::browser::DEFAULT_GROUPING)).color(PALETTE.text_muted));
        ui.end_row();
        for s in &list {
            let id = s.id;
            ui.horizontal(|ui| {
                let mut pick = ui.radio(in_use == Some(id), "").clicked();
                let mut text = match draft {
                    Some((d, t)) if *d == id => t.clone(),
                    _ => s.name.clone(),
                };
                let r = ui.add_sized([200.0, 22.0], egui::TextEdit::singleline(&mut text));
                if r.changed() {
                    *draft = Some((id, text.clone()));
                }
                pick |= r.gained_focus();
                if r.lost_focus() {
                    if text.trim() != s.name.trim() {
                        let command =
                            cmd::BrowserSchemeSet { scheme: Ref::Id(id.0), name: Some(text), ..Default::default() };
                        run(state, None, command);
                    }
                    *draft = None;
                }
                if pick && in_use != Some(id) {
                    run(state, None, cmd::BrowserSchemeUse { scheme: Some(Ref::Id(id.0)) });
                }
            });
            ui.label(RichText::new(summary(&s.group_by)).color(PALETTE.text_muted));
            ui.end_row();
        }
    });
}

fn buttons(ui: &mut Ui, state: &mut AppState) {
    let count = state.project().browser.schemes.len();
    if ui.add_enabled(count < MAX_SCHEMES, egui::Button::new(t!("action-new"))).clicked() {
        let names: Vec<String> = state.project().browser.schemes.iter().map(|s| s.name.to_lowercase()).collect();
        let name = (1..1000)
            .map(|n| t!("browser-org-default-name", "n" => n))
            .find(|n| !names.contains(&n.to_lowercase()))
            .unwrap_or_default();
        run(state, None, cmd::BrowserSchemeCreate { name, ..Default::default() });
    }
    let in_use = state.project().browser.in_use().map(|s| s.id);
    if ui.add_enabled(in_use.is_some(), egui::Button::new(t!("action-delete"))).clicked()
        && let Some(id) = in_use
    {
        run(state, None, cmd::BrowserSchemeDelete { scheme: Ref::Id(id.0) });
    }
}

/// The criteria of the scheme in use: a list per depth, each offering what
/// the depths above have not taken; "None" ends the nesting there.
fn criteria(ui: &mut Ui, state: &mut AppState) {
    let Some(scheme) = state.project().browser.in_use().cloned() else {
        ui.label(RichText::new(t!("browser-org-default-text")).color(PALETTE.text_muted));
        return;
    };
    let none = t!("browser-org-none");
    let mut chosen = scheme.group_by.clone();
    egui::Grid::new("browser-criteria").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        // One list more than the criteria chosen, to add the next.
        for depth in 0..(scheme.group_by.len() + 1).min(MAX_CRITERIA).min(GroupBy::ALL.len()) {
            let label = if depth == 0 { t!("browser-org-group-by") } else { t!("browser-org-then-by") };
            ui.label(RichText::new(label).color(PALETTE.text_muted));
            let current = scheme.group_by.get(depth).copied();
            let mut pick = current;
            let text = current.map_or_else(|| none.clone(), |g| tr(g.key()));
            egui::ComboBox::from_id_salt(("browser-criterion", depth)).selected_text(text).width(200.0).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(&mut pick, None, none.as_str());
                    let taken = scheme.group_by.get(..depth).unwrap_or_default();
                    for g in GroupBy::ALL.into_iter().filter(|g| !taken.contains(g)) {
                        ui.selectable_value(&mut pick, Some(g), tr(g.key()));
                    }
                },
            );
            if pick != current {
                chosen.truncate(depth);
                if let Some(g) = pick {
                    chosen.push(g);
                    // What followed stays, less the criterion now chosen above it.
                    chosen.extend(scheme.group_by.iter().skip(depth + 1).filter(|x| **x != g));
                }
            }
            ui.end_row();
        }
    });
    if chosen != scheme.group_by {
        let group_by = Some(chosen.iter().map(|g| g.name().to_string()).collect());
        run(state, None, cmd::BrowserSchemeSet { scheme: Ref::Id(scheme.id.0), group_by, ..Default::default() });
    }
}
