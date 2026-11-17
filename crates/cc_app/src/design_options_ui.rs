//! Design options in the application: the Design Options dialog (sets,
//! their options and primary option, accepting the primary), the menu of
//! the option being edited, and the option rows of view, template and
//! element properties. Every change goes through the engine's commands; OK
//! keeps the dialog's changes, Cancel undoes them.

use crate::graphics_dialog::{Edits, header, run};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::PALETTE;
use cc_core::{Element, OptionId, OptionSetId, Project, ViewId};
use cc_engine::Ref;
use cc_engine::commands::design_options as cmd;
use egui::{Context, RichText, Ui};
use std::collections::BTreeMap;

/// A text being typed, sent when the field loses focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Draft {
    Set(OptionSetId),
    Option(OptionId),
}

/// The open Design Options dialog.
pub struct OptionsDialog {
    drafts: BTreeMap<Draft, String>,
    edits: Edits,
}

pub fn open(state: &mut AppState) {
    let edits = Edits::begin(state);
    state.dialogs.design_options = Some(OptionsDialog { drafts: BTreeMap::new(), edits });
}

/// A text field holding a draft while focused; returns the text to commit
/// when it loses focus with a change.
fn draft_field(
    ui: &mut Ui,
    drafts: &mut BTreeMap<Draft, String>,
    which: Draft,
    current: &str,
    width: f32,
) -> Option<String> {
    let mut text = drafts.get(&which).cloned().unwrap_or_else(|| current.to_string());
    let r = ui.add_sized([width, 22.0], egui::TextEdit::singleline(&mut text));
    if r.changed() {
        drafts.insert(which, text);
    }
    if r.lost_focus() {
        return drafts.remove(&which).filter(|t| t.trim() != current.trim());
    }
    None
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(d) = state.dialogs.design_options.as_mut() else { return };
    let mut drafts = std::mem::take(&mut d.drafts);
    let mut ok = 0;
    let response = crate::dialog::Dialog::new("design-options", t!("dlg-design-options")).width(720.0).show(
        ctx,
        |ui| {
            ui.label(RichText::new(t!("opt-hint")).size(11.0).color(PALETTE.text_faint));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| sets(ui, state, &mut drafts));
        },
        |f| {
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if let Some(d) = state.dialogs.design_options.as_mut() {
        d.drafts = drafts;
    }
    let close = match response.clicked {
        Some(i) => Some(i == ok),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.design_options.take()
    {
        d.edits.finish(state, keep);
    }
}

/// Every set: its name, options (name, primary, elements) and actions.
fn sets(ui: &mut Ui, state: &mut AppState, drafts: &mut BTreeMap<Draft, String>) {
    let sets = state.project().option_sets.clone();
    if sets.is_empty() {
        ui.label(RichText::new(t!("opt-no-sets")).color(PALETTE.text_muted));
    }
    for set in &sets {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("opt-set")).color(PALETTE.text_muted));
            if let Some(name) = draft_field(ui, drafts, Draft::Set(set.id), &set.name, 220.0) {
                run(state, None, cmd::OptionSetRename { set: Ref::Id(set.id.0), name });
            }
            if ui.button(t!("opt-add-option")).clicked() {
                run(state, None, cmd::OptionCreate { set: Ref::Id(set.id.0), name: None, primary: false });
            }
            if ui.button(t!("opt-accept-primary")).on_hover_text(t!("opt-accept-primary-tip")).clicked() {
                run(state, None, cmd::OptionAcceptPrimary { set: Ref::Id(set.id.0) });
            }
            if crate::dialog::styled_button(ui, &t!("opt-delete-set"), crate::dialog::Style::Danger)
                .on_hover_text(t!("opt-delete-set-tip"))
                .clicked()
            {
                run(state, None, cmd::OptionSetDelete { set: Ref::Id(set.id.0) });
            }
        });
        let primary = set.primary_option();
        let counts: Vec<usize> = set.options.iter().map(|o| state.project().option_elements(o.id).count()).collect();
        egui::Grid::new(("option-set", set.id.0)).num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            header(ui, t!("prop-name"));
            header(ui, t!("opt-primary"));
            header(ui, t!("opt-elements"));
            ui.label("");
            ui.end_row();
            for (o, count) in set.options.iter().zip(counts) {
                if let Some(name) = draft_field(ui, drafts, Draft::Option(o.id), &o.name, 220.0) {
                    let command =
                        cmd::OptionSetProps { option: Ref::Id(o.id.0), name: Some(name), ..Default::default() };
                    run(state, None, command);
                }
                let is_primary = primary == Some(o.id);
                if ui.radio(is_primary, "").clicked() && !is_primary {
                    let command =
                        cmd::OptionSetProps { option: Ref::Id(o.id.0), primary: Some(true), ..Default::default() };
                    run(state, None, command);
                }
                ui.label(count.to_string());
                if ui
                    .add_enabled(!is_primary, egui::Button::new("×").small())
                    .on_hover_text(t!("opt-delete-option"))
                    .clicked()
                {
                    run(state, None, cmd::OptionDelete { option: Ref::Id(o.id.0) });
                }
                ui.end_row();
            }
        });
    }
    ui.add_space(8.0);
    if ui.button(t!("opt-new-set")).clicked() {
        let names: Vec<String> = sets.iter().map(|s| s.name.to_lowercase()).collect();
        let name = (1..1000)
            .map(|n| t!("opt-default-set-name", "n" => n))
            .find(|n| !names.contains(&n.to_lowercase()))
            .unwrap_or_default();
        let options = vec![t!("option-default-name", "n" => 1), t!("option-default-name", "n" => 2)];
        run(state, None, cmd::OptionSetCreate { name, options });
    }
}

// ---------------------------------------------------------------- active option

/// The name of the option being edited, or of the main model.
pub fn active_label(state: &AppState) -> String {
    let p = state.project();
    p.active_option().map(|o| p.option_label(o)).unwrap_or_else(|| t!("opt-main-model"))
}

/// The menu of the option being edited: the main model, then every option.
pub fn active_menu(ui: &mut Ui, state: &mut AppState) {
    let p = state.project();
    let current = p.active_option();
    let mut choices: Vec<(Option<OptionId>, String)> = vec![(None, t!("opt-main-model"))];
    choices.extend(p.option_sets.iter().flat_map(|s| s.options.iter().map(|o| (Some(o.id), p.option_label(o.id)))));
    for (option, label) in choices {
        if ui.selectable_label(current == option, label).clicked() {
            state.ribbon_peek = false;
            crate::actions::run_command(state, cmd::OptionActiveSet { option: option.map(|o| Ref::Id(o.0)) });
            ui.close();
        }
    }
}

// ---------------------------------------------------------------- properties

/// The label of a view's choice for `set`: an option, or automatic (and
/// what it shows now).
fn choice_label(p: &Project, set: OptionSetId, choice: Option<OptionId>, shown: Option<OptionId>) -> String {
    let name =
        |o: Option<OptionId>| o.and_then(|o| p.option_set(set)?.option(o)).map(|o| o.name.clone()).unwrap_or_default();
    match choice {
        Some(_) => name(choice),
        None => t!("opt-automatic", "option" => name(shown)),
    }
}

/// Combo boxes choosing an option per set; returns the changed choices
/// (`None`: automatic).
fn choice_rows(
    ui: &mut Ui,
    p: &Project,
    salt: (&str, u64),
    choices: &BTreeMap<OptionSetId, OptionId>,
    shown: &std::collections::BTreeSet<OptionId>,
    locked: bool,
) -> Vec<(OptionSetId, Option<OptionId>)> {
    let mut changed = Vec::new();
    for set in &p.option_sets {
        let current = choices.get(&set.id).copied();
        let showing = set.options.iter().find(|o| shown.contains(&o.id)).map(|o| o.id);
        let label = choice_label(p, set.id, current, showing);
        if locked {
            readonly_row(ui, &set.name, label);
            continue;
        }
        let mut chosen = current;
        row(ui, &set.name, |ui| {
            egui::ComboBox::from_id_salt((salt.0, salt.1, set.id.0)).selected_text(label).show_ui(ui, |ui| {
                let automatic = t!("opt-automatic", "option" => choice_label(p, set.id, set.primary_option(), None));
                ui.selectable_value(&mut chosen, None, automatic);
                for o in &set.options {
                    ui.selectable_value(&mut chosen, Some(o.id), &o.name);
                }
            });
        });
        if chosen != current {
            changed.push((set.id, chosen));
        }
    }
    changed
}

/// The Design Options section of a view's properties (plans, elevations,
/// sections, 3D views and schedules): the option shown of each set,
/// read-only when the view's template controls it.
pub fn view_section(ui: &mut Ui, state: &mut AppState, view: ViewId) {
    let p = state.project();
    let Some(v) = p.view(view).filter(|v| v.kind.is_phased()) else { return };
    if p.option_sets.is_empty() {
        return;
    }
    let locked = v.template.and_then(|t| p.template(t)).is_some_and(|t| t.include.design_options);
    let (choices, shown) = (v.design_options.clone(), p.shown_options(Some(v)));
    let mut changed = Vec::new();
    section(ui, "view-design-options", t!("section-design-options"), |ui| {
        changed = choice_rows(ui, state.project(), ("view-option", view.0), &choices, &shown, locked);
    });
    for (set, option) in changed {
        let command =
            cmd::ViewOptionsSet { view: Ref::Id(view.0), set: Ref::Id(set.0), option: option.map(|o| Ref::Id(o.0)) };
        crate::actions::run_command(state, command);
    }
}

/// A view template's choices (in the template manager's grid).
pub fn template_rows(ui: &mut Ui, state: &mut AppState, template: cc_core::TemplateId) {
    let p = state.project();
    let Some(t) = p.template(template) else { return };
    if p.option_sets.is_empty() {
        ui.label(RichText::new(t!("opt-no-sets")).color(PALETTE.text_muted));
        return;
    }
    let mut choices = t.design_options.clone();
    let shown = p.primary_options();
    let changed = ui.vertical(|ui| choice_rows(ui, p, ("template-option", template.0), &choices, &shown, false)).inner;
    if changed.is_empty() {
        return;
    }
    for (set, option) in changed {
        match option {
            Some(o) => choices.insert(set, o),
            None => choices.remove(&set),
        };
    }
    let list =
        choices.iter().map(|(s, o)| cmd::OptionChoice { set: Ref::Id(s.0), option: Some(Ref::Id(o.0)) }).collect();
    let command = cc_engine::commands::graphics::TemplateSet {
        template: Ref::Id(template.0),
        design_options: Some(list),
        ..Default::default()
    };
    run(state, None, command);
}

/// The design option an element belongs to (hosted elements show their
/// host's, read-only).
pub fn element_rows(ui: &mut Ui, state: &mut AppState, el: &Element) {
    let p = state.project();
    if !el.category().is_phased() || p.option_sets.is_empty() {
        return;
    }
    let hosted = cc_core::phase::phase_host(el).is_some_and(|h| p.element(h).is_some());
    let current = el.design_option;
    let label = |o: Option<OptionId>| o.map(|o| p.option_label(o)).unwrap_or_else(|| t!("opt-main-model"));
    let current_label = label(current);
    let choices: Vec<(Option<OptionId>, String)> = std::iter::once((None, t!("opt-main-model")))
        .chain(p.option_sets.iter().flat_map(|s| s.options.iter().map(|o| (Some(o.id), label(Some(o.id))))))
        .collect();
    let id = el.id;
    let mut chosen = current;
    section(ui, "design-option", t!("section-design-options"), |ui| {
        if hosted {
            readonly_row(ui, &t!("prop-design-option"), format!("{current_label} ({})", t!("opt-follows-host")));
            return;
        }
        row(ui, &t!("prop-design-option"), |ui| {
            egui::ComboBox::from_id_salt(("element-option", id.0)).selected_text(current_label).show_ui(ui, |ui| {
                for (o, name) in &choices {
                    ui.selectable_value(&mut chosen, *o, name);
                }
            });
        });
    });
    if chosen != current {
        let command = cmd::ElementsOptionSet { ids: vec![id.0], option: chosen.map(|o| Ref::Id(o.0)) };
        crate::actions::run_command(state, command);
    }
}
