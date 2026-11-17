//! The New Component dialog: a name and the kind the component is, picked
//! from a list that shows the parameters each kind starts with (a generic
//! component starts with none: they are the user's to add). Create makes
//! the component through `component_create` and opens it in the component
//! environment, where it is shaped; cancelling the environment removes it.

use crate::dialog::Dialog;
use crate::state::AppState;
use crate::t;
use crate::theme;
use cc_core::{Category, ComponentKind, ElementTypeId, TypeData};
use egui::{Context, Ui};

/// Width of the list of kinds.
const KINDS_WIDTH: f32 = 170.0;
/// Height of the two columns: the kinds listed, with room to spare.
const COLUMNS_HEIGHT: f32 = 236.0;
/// Width of the parameters of the kind picked.
const PARAMS_WIDTH: f32 = 250.0;

/// The dialog's state: the name typed and the kind picked.
#[derive(Debug, Clone)]
pub struct NewComponent {
    pub name: String,
    pub kind: ComponentKind,
}

/// The kinds a new component may be: title blocks and detail components
/// are 2D and made in their own places.
pub(crate) fn kinds() -> Vec<ComponentKind> {
    ComponentKind::ALL.into_iter().filter(|k| !k.is_2d()).collect()
}

/// `name`, or `name 2`, `name 3`… when a component of the project has it:
/// types of one component name share their definition.
fn free_name(state: &AppState, name: &str) -> String {
    let taken = |n: &str| {
        state
            .project()
            .types
            .values()
            .any(|t| matches!(&t.data, TypeData::Component { component, .. } if component == n))
            || state.project().types.values().any(|t| t.name == n)
    };
    if !taken(name) {
        return name.to_string();
    }
    (2..10_000).map(|k| format!("{name} {k}")).find(|n| !taken(n)).unwrap_or_else(|| name.to_string())
}

/// Open the dialog (kept as it is when already open).
pub fn open(state: &mut AppState) {
    if state.dialogs.new_component.is_none() {
        let name = free_name(state, &t!("comp-new-name"));
        state.dialogs.new_component = Some(NewComponent { name, kind: ComponentKind::Generic });
    }
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut new) = state.dialogs.new_component.take() else { return };
    let mut accepted = false;
    let mut create = 0;
    let response = Dialog::new("new-component", t!("dlg-component-new")).width(KINDS_WIDTH + PARAMS_WIDTH + 24.0).show(
        ctx,
        |ui| accepted = body(ui, &mut new),
        |f| {
            create = f.primary(t!("new-project-create"));
            f.secondary(t!("action-cancel"));
        },
    );
    match response.clicked {
        Some(i) if i == create => accepted = true,
        Some(_) => return,
        None if response.dismissed => return,
        None => {}
    }
    if !accepted || new.name.trim().is_empty() {
        state.dialogs.new_component = Some(new);
        return;
    }
    self::create(state, new.name.trim(), new.kind);
}

/// The name, the kinds and what the kind picked starts with. True when a
/// kind was double-clicked, which creates the component at once.
fn body(ui: &mut Ui, new: &mut NewComponent) -> bool {
    let mut accepted = false;
    ui.horizontal(|ui| {
        ui.label(theme::muted(t!("comp-name")));
        ui.add(egui::TextEdit::singleline(&mut new.name).desired_width(f32::INFINITY));
    });
    ui.add_space(6.0);
    ui.label(theme::muted(t!("new-component-intro")));
    ui.add_space(4.0);
    let columns = egui::vec2(ui.available_width(), COLUMNS_HEIGHT);
    ui.allocate_ui_with_layout(columns, egui::Layout::left_to_right(egui::Align::Min), |ui| {
        // The separator between the columns is as tall as they are.
        ui.set_height(COLUMNS_HEIGHT);
        ui.allocate_ui_with_layout(
            egui::vec2(KINDS_WIDTH, 0.0),
            egui::Layout::top_down_justified(egui::Align::Min),
            |ui| {
                for kind in kinds() {
                    let r = ui.selectable_label(new.kind == kind, crate::i18n::tr(kind.key()));
                    if r.clicked() || r.double_clicked() {
                        new.kind = kind;
                    }
                    accepted |= r.double_clicked();
                }
            },
        );
        ui.separator();
        ui.allocate_ui_with_layout(egui::vec2(PARAMS_WIDTH, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(PARAMS_WIDTH);
            starts_with(ui, new.kind);
        });
    });
    accepted
}

/// The parameters a component of `kind` starts with, and their values.
fn starts_with(ui: &mut Ui, kind: ComponentKind) {
    ui.label(theme::title(t!("comp-params")));
    let params = cc_engine::commands::types::starter_def(kind).map(|def| def.params).unwrap_or_default();
    if params.is_empty() {
        ui.add(egui::Label::new(theme::muted(t!("new-component-generic"))).wrap());
    }
    egui::Grid::new("new-component-params").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
        for p in &params {
            ui.label(&p.name);
            ui.label(theme::muted(cc_views::format::length(p.value)));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.add(egui::Label::new(theme::muted(t!("new-component-hint")).size(11.5)).wrap());
}

/// Create the component of `kind` and open it in the component environment.
/// *Cancel Editing* there removes it, and *Finish* picks the tool that
/// places it.
pub fn create(state: &mut AppState, name: &str, kind: ComponentKind) {
    let depth = state.session.doc.undo_depth();
    let command = cc_engine::commands::types::ComponentCreate {
        name: free_name(state, name),
        def: serde_json::Value::Null,
        kind: Some(format!("{kind:?}")),
    };
    let Some(id) = crate::actions::run_command(state, command).and_then(|o| o["id"].as_u64()) else { return };
    crate::component_env::open_new(state, ElementTypeId(id), depth);
}

/// Pick the tool that places the component type `type_id`: doors and
/// windows go in walls.
pub fn place(state: &mut AppState, type_id: ElementTypeId) {
    let Some(ty) = state.project().element_type(type_id) else { return };
    let (category, name) = (ty.category(), ty.name.clone());
    state.set_tool(match category {
        Category::Door | Category::Window | Category::Niche => crate::tools::Tool::Hosted { category, type_id },
        _ => crate::tools::Tool::Component { type_id },
    });
    state.message = t!("msg-component-created", "name" => name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Document;

    fn state() -> AppState {
        AppState::new(Document::new(crate::actions::new_project()))
    }

    fn component_names(state: &AppState) -> Vec<String> {
        state
            .project()
            .types
            .values()
            .filter_map(|t| match &t.data {
                TypeData::Component { component, .. } => Some(component.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_new_component_opens_in_the_environment_and_cancel_removes_it() {
        let mut state = state();
        let before = component_names(&state).len();
        let depth = state.session.doc.undo_depth();
        create(&mut state, "Bancada", ComponentKind::Casework);
        let env = state.component_env.as_ref().expect("the environment opens");
        let Some(TypeData::Component { component, def }) = state.project().element_type(env.type_id).map(|t| &t.data)
        else {
            panic!("a component type")
        };
        assert_eq!((component.as_str(), def.kind, def.params.len()), ("Bancada", ComponentKind::Casework, 3));
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
        crate::component_env::cancel(&mut state);
        assert_eq!(component_names(&state).len(), before);
        assert_eq!(state.session.doc.undo_depth(), depth);
    }

    #[test]
    fn finishing_a_new_component_picks_its_tool() {
        let mut state = state();
        create(&mut state, "Bancada", ComponentKind::Generic);
        let made = state.component_env.as_ref().unwrap().type_id;
        crate::component_env::finish(&mut state);
        assert!(matches!(state.tool, crate::tools::Tool::Component { type_id } if type_id == made));
        // A door goes in a wall.
        create(&mut state, "Porta nova", ComponentKind::Door);
        let made = state.component_env.as_ref().unwrap().type_id;
        crate::component_env::finish(&mut state);
        assert!(matches!(
            state.tool,
            crate::tools::Tool::Hosted { category: Category::Door, type_id } if type_id == made
        ));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let mut state = state();
        create(&mut state, "Bancada", ComponentKind::Generic);
        crate::component_env::finish(&mut state);
        create(&mut state, "Bancada", ComponentKind::Furniture);
        crate::component_env::finish(&mut state);
        let names = component_names(&state);
        assert!(names.contains(&"Bancada".to_string()) && names.contains(&"Bancada 2".to_string()), "{names:?}");
    }

    #[test]
    fn title_blocks_and_details_are_not_offered() {
        assert!(!kinds().contains(&ComponentKind::TitleBlock) && !kinds().contains(&ComponentKind::Detail));
    }

    #[test]
    fn loading_a_component_file_adds_renamed_types_in_one_step() {
        let mut state = state();
        let file = cc_core::component_file::extract(state.project(), "Mesa").unwrap();
        let text = cc_core::component_file::to_ron(&file).unwrap();
        let path = std::env::temp_dir().join(format!("ccad-mesa-{}.ccomp", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let before = state.project().types_of(Category::Component).count();
        let depth = state.session.doc.undo_depth();
        crate::actions::load_component_path(&mut state, path);
        crate::jobs::wait(&mut state);
        let crate::tools::Tool::Component { type_id: first } = state.tool.clone() else { panic!("tool") };
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
        assert_eq!(state.project().types_of(Category::Component).count(), before + 1);
        assert_eq!(state.project().type_name(first), "Mesa 120 x 80 (2)");
        state.session.doc.undo();
        assert_eq!(state.project().types_of(Category::Component).count(), before);
    }
}
