//! Design options: option sets, their options and primary option, the
//! option elements belong to, the option each view shows and the option
//! being edited (see [`cc_core::design_option`]).

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::design_option::{MAX_OPTION_SETS, MAX_OPTIONS};
use cc_core::{DesignOption, ElementId, OptionId, OptionSet, OptionSetId, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Longest name accepted.
const MAX_TEXT: usize = 200;

/// An option as other queries show it: `{id, name, set}`, or `null`.
pub(crate) fn option_ref_json(p: &Project, id: Option<OptionId>) -> Value {
    match id.and_then(|id| p.design_option(id)) {
        Some((s, o)) => json!({"id": o.id.0, "name": o.name, "set": {"id": s.id.0, "name": s.name}}),
        None => Value::Null,
    }
}

/// The option of each set a view shows, as `view_get` shows it: the
/// view's own choice (`null`: automatic) and the option shown.
pub(crate) fn view_options_json(p: &Project, v: &cc_core::View) -> Value {
    if !v.kind.is_phased() {
        return Value::Null;
    }
    let shown = p.shown_options(Some(v));
    p.option_sets
        .iter()
        .map(|s| {
            let chosen = v.design_options.get(&s.id).copied();
            let showing = s.options.iter().find(|o| shown.contains(&o.id)).map(|o| o.id);
            json!({
                "set": {"id": s.id.0, "name": s.name},
                "choice": option_ref_json(p, chosen),
                "shown": option_ref_json(p, showing),
            })
        })
        .collect()
}

fn set_json(p: &Project, s: &OptionSet) -> Value {
    let primary = s.primary_option();
    json!({
        "id": s.id.0,
        "name": s.name,
        "primary": primary.map(|o| o.0),
        "options": s.options.iter().map(|o| json!({
            "id": o.id.0,
            "name": o.name,
            "primary": Some(o.id) == primary,
            "active": p.active_option() == Some(o.id),
            "elements": p.option_elements(o.id).count(),
        })).collect::<Vec<_>>(),
    })
}

fn checked_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_TEXT || name.contains(':') {
        return Err(EngineError::invalid("the name is empty, too long or has a ':'"));
    }
    Ok(name.to_string())
}

fn set_name(p: &Project, name: &str, except: Option<OptionSetId>) -> Result<String> {
    let name = checked_name(name)?;
    if p.option_sets.iter().any(|s| Some(s.id) != except && s.name.eq_ignore_ascii_case(&name)) {
        return Err(EngineError::rejected(t!("msg-name-taken", "name" => name)));
    }
    Ok(name)
}

fn option_name(s: &OptionSet, name: &str, except: Option<OptionId>) -> Result<String> {
    let name = checked_name(name)?;
    if s.options.iter().any(|o| Some(o.id) != except && o.name.eq_ignore_ascii_case(&name)) {
        return Err(EngineError::rejected(t!("msg-name-taken", "name" => name)));
    }
    Ok(name)
}

/// A name for a new option of `s`: "Option 2"…
fn next_option_name(s: &OptionSet) -> String {
    (1..=MAX_OPTIONS + 1)
        .map(|n| t!("option-default-name", "n" => n))
        .find(|n| !s.options.iter().any(|o| o.name.eq_ignore_ascii_case(n)))
        .unwrap_or_default()
}

fn set_out(s: &Session, id: OptionSetId) -> Result<Value> {
    let p = s.project();
    p.option_set(id).map(|x| set_json(p, x)).ok_or_else(|| EngineError::not_found("design option set"))
}

/// Delete elements (and what they host) in one edit step.
fn remove_all(e: &mut cc_core::EditCtx, ids: &[ElementId]) {
    for id in ids {
        e.remove_element(*id);
    }
}

/// The project's design option sets with their options (the primary one
/// marked, how many elements each holds) and the option being edited.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionsList {}

impl Command for OptionsList {
    const NAME: &'static str = "options_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        Ok(json!({
            "sets": p.option_sets.iter().map(|x| set_json(p, x)).collect::<Vec<_>>(),
            "active": option_ref_json(p, p.active_option()),
        }))
    }
}

/// Add a design option set: a named group of alternatives, of which views
/// show one at a time. It starts with the options named in `options` (the
/// first is primary), or one option.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionSetCreate {
    /// Name (unique, without ':').
    pub name: String,
    /// Names of its options (default: one, "Option 1").
    #[serde(default)]
    pub options: Vec<String>,
}

impl Command for OptionSetCreate {
    const NAME: &'static str = "option_set_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = set_name(p, &self.name, None)?;
        if p.option_sets.len() >= MAX_OPTION_SETS {
            return Err(EngineError::invalid(format!("at most {MAX_OPTION_SETS} design option sets")));
        }
        if self.options.len() > MAX_OPTIONS {
            return Err(EngineError::invalid(format!("at most {MAX_OPTIONS} options in a set")));
        }
        let mut names: Vec<String> = Vec::new();
        for n in &self.options {
            let n = checked_name(n)?;
            if names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                return Err(EngineError::rejected(t!("msg-name-taken", "name" => n)));
            }
            names.push(n);
        }
        if names.is_empty() {
            names.push(t!("option-default-name", "n" => 1));
        }
        let id = edit(s, t!("tx-option-set-create"), |e| {
            let id = OptionSetId(e.alloc_id());
            let options: Vec<DesignOption> =
                names.into_iter().map(|name| DesignOption { id: OptionId(e.alloc_id()), name }).collect();
            let primary = options.first().map(|o| o.id).unwrap_or(OptionId(0));
            e.option_sets_mut().push(OptionSet { id, name, options, primary });
            Ok(id)
        })?;
        set_out(s, id)
    }
}

/// Rename a design option set.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionSetRename {
    /// Set id or name.
    pub set: Ref,
    /// New name (unique, without ':').
    pub name: String,
}

impl Command for OptionSetRename {
    const NAME: &'static str = "option_set_rename";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::option_set(p, &self.set)?;
        let name = set_name(p, &self.name, Some(id))?;
        edit(s, t!("tx-option-set-modify"), |e| {
            if let Some(x) = e.option_sets_mut().iter_mut().find(|x| x.id == id) {
                x.name = name;
            }
            Ok(())
        })?;
        set_out(s, id)
    }
}

/// Delete a design option set with all its options and the elements in
/// them (the main model stays).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionSetDelete {
    /// Set id or name.
    pub set: Ref,
}

impl Command for OptionSetDelete {
    const NAME: &'static str = "option_set_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::option_set(p, &self.set)?;
        let set = p.option_set(id).cloned().ok_or_else(|| EngineError::not_found("design option set"))?;
        let doomed: Vec<ElementId> =
            p.elements.values().filter(|e| e.design_option.is_some_and(|o| set.contains(o))).map(|e| e.id).collect();
        edit(s, t!("tx-option-set-delete"), |e| {
            remove_all(e, &doomed);
            e.option_sets_mut().retain(|x| x.id != id);
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0, "elements_deleted": doomed.len() }))
    }
}

/// Add an option to a design option set (at the end).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionCreate {
    /// Set id or name.
    pub set: Ref,
    /// Name, unique in the set (default: "Option n").
    #[serde(default)]
    pub name: Option<String>,
    /// Make it the set's primary option.
    #[serde(default)]
    pub primary: bool,
}

impl Command for OptionCreate {
    const NAME: &'static str = "option_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let set = refs::option_set(p, &self.set)?;
        let x = p.option_set(set).ok_or_else(|| EngineError::not_found("design option set"))?;
        if x.options.len() >= MAX_OPTIONS {
            return Err(EngineError::invalid(format!("at most {MAX_OPTIONS} options in a set")));
        }
        let name = match &self.name {
            Some(n) => option_name(x, n, None)?,
            None => next_option_name(x),
        };
        let primary = self.primary;
        let id = edit(s, t!("tx-option-create"), |e| {
            let id = OptionId(e.alloc_id());
            if let Some(x) = e.option_sets_mut().iter_mut().find(|x| x.id == set) {
                x.options.push(DesignOption { id, name });
                if primary {
                    x.primary = id;
                }
            }
            Ok(id)
        })?;
        Ok(json!({ "option": option_ref_json(s.project(), Some(id)), "set": set_out(s, set)? }))
    }
}

/// Rename a design option, make it its set's primary option, or move it
/// to another place in the set's list (`index`, 0 = first).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionSetProps {
    /// Option id, name, or "Set: Option".
    pub option: Ref,
    /// New name (unique in the set, without ':').
    #[serde(default)]
    pub name: Option<String>,
    /// `true` makes it the primary option (views showing the set
    /// automatically, and exports, then use it).
    #[serde(default)]
    pub primary: Option<bool>,
    /// New position in the set's list.
    #[serde(default)]
    pub index: Option<usize>,
}

impl Command for OptionSetProps {
    const NAME: &'static str = "option_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::design_option(p, &self.option)?;
        let set = p.option_set_of(id).ok_or_else(|| EngineError::not_found("design option"))?;
        let set_id = set.id;
        let name = self.name.as_deref().map(|n| option_name(set, n, Some(id))).transpose()?;
        if self.primary == Some(false) && set.primary_option() == Some(id) {
            return Err(EngineError::invalid("make another option primary instead"));
        }
        let (primary, index) = (self.primary == Some(true), self.index);
        edit(s, t!("tx-option-modify"), |e| {
            let Some(x) = e.option_sets_mut().iter_mut().find(|x| x.id == set_id) else { return Ok(()) };
            if let Some(o) = x.options.iter_mut().find(|o| o.id == id)
                && let Some(n) = name
            {
                o.name = n;
            }
            if primary {
                x.primary = id;
            }
            if let (Some(to), Some(from)) = (index, x.options.iter().position(|o| o.id == id)) {
                let o = x.options.remove(from);
                x.options.insert(to.min(x.options.len()), o);
            }
            Ok(())
        })?;
        Ok(json!({ "option": option_ref_json(s.project(), Some(id)), "set": set_out(s, set_id)? }))
    }
}

/// Delete a design option and the elements in it (with what they host).
/// The primary option cannot be deleted: make another option primary
/// first, or delete the whole set.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionDelete {
    /// Option id, name, or "Set: Option".
    pub option: Ref,
}

impl Command for OptionDelete {
    const NAME: &'static str = "option_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::design_option(p, &self.option)?;
        let set = p.option_set_of(id).ok_or_else(|| EngineError::not_found("design option"))?;
        let set_id = set.id;
        if set.primary_option() == Some(id) {
            return Err(EngineError::rejected(t!("msg-option-primary-delete")));
        }
        let doomed: Vec<ElementId> = p.option_elements(id).map(|e| e.id).collect();
        edit(s, t!("tx-option-delete"), |e| {
            remove_all(e, &doomed);
            if let Some(x) = e.option_sets_mut().iter_mut().find(|x| x.id == set_id) {
                x.options.retain(|o| o.id != id);
            }
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0, "elements_deleted": doomed.len(), "set": set_out(s, set_id)? }))
    }
}

/// Accept a set's primary option: its elements join the main model, and
/// the set is deleted with its other options and their elements.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionAcceptPrimary {
    /// Set id or name.
    pub set: Ref,
}

impl Command for OptionAcceptPrimary {
    const NAME: &'static str = "option_accept_primary";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::option_set(p, &self.set)?;
        let set = p.option_set(id).cloned().ok_or_else(|| EngineError::not_found("design option set"))?;
        let primary = set.primary_option();
        let (kept, doomed): (Vec<_>, Vec<_>) = p
            .elements
            .values()
            .filter(|e| e.design_option.is_some_and(|o| set.contains(o)))
            .partition(|e| e.design_option == primary);
        let kept: Vec<ElementId> = kept.iter().map(|e| e.id).collect();
        let doomed: Vec<ElementId> = doomed.iter().map(|e| e.id).collect();
        edit(s, t!("tx-option-accept"), |e| {
            remove_all(e, &doomed);
            for x in &kept {
                if let Some(el) = e.element_mut(*x) {
                    el.design_option = None;
                }
            }
            e.option_sets_mut().retain(|x| x.id != id);
            Ok(())
        })?;
        Ok(json!({ "accepted": primary.map(|o| o.0), "elements_kept": kept.len(), "elements_deleted": doomed.len() }))
    }
}

/// Move elements into a design option, or back to the main model with
/// `null`. Doors, windows and other hosted elements follow their host's
/// option (moving a wall moves them); annotations belong to no option.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ElementsOptionSet {
    /// Element ids.
    pub ids: Vec<u64>,
    /// Option id, name or "Set: Option"; `null` for the main model.
    pub option: Option<Ref>,
}

impl Command for ElementsOptionSet {
    const NAME: &'static str = "elements_option_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let ids = refs::elements(p, &self.ids)?;
        let option = self.option.as_ref().map(|r| refs::design_option(p, r)).transpose()?;
        for id in &ids {
            let Some(e) = p.element(*id) else { continue };
            if !e.category().is_phased() {
                return Err(EngineError::rejected(t!("msg-option-annotation")));
            }
            if cc_core::phase::phase_host(e).is_some_and(|h| p.element(h).is_some() && !ids.contains(&h)) {
                return Err(EngineError::rejected(t!("msg-option-hosted")));
            }
        }
        let before: Vec<(ElementId, Option<OptionId>)> = p.elements.values().map(|e| (e.id, e.design_option)).collect();
        edit(s, t!("tx-option-elements"), |e| {
            for id in &ids {
                if let Some(x) = e.element_mut(*id) {
                    x.design_option = option;
                }
            }
            Ok(())
        })?;
        // What changed, hosted elements included.
        let p = s.project();
        let changed: Vec<u64> = before
            .into_iter()
            .filter(|(id, o)| p.element(*id).is_some_and(|e| e.design_option != *o))
            .map(|(id, _)| id.0)
            .collect();
        Ok(json!({ "option": option_ref_json(p, option), "changed": changed }))
    }
}

/// Choose the option a view (plan, elevation, section, 3D view or
/// schedule) shows of a design option set; `null` shows it automatically:
/// the option being edited when it is in that set, else the primary one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewOptionsSet {
    /// View id or name.
    pub view: Ref,
    /// Set id or name.
    pub set: Ref,
    /// Option id or name (in that set), or `null` for automatic.
    pub option: Option<Ref>,
}

/// The option of `set` named `r` (`None`: automatic).
fn option_in(p: &Project, set: OptionSetId, r: Option<&Ref>) -> Result<Option<OptionId>> {
    let Some(r) = r else { return Ok(None) };
    let x = p.option_set(set).ok_or_else(|| EngineError::not_found("design option set"))?;
    let id = match r {
        Ref::Name(n) => x
            .options
            .iter()
            .find(|o| o.name.trim().eq_ignore_ascii_case(n.trim()))
            .map(|o| o.id)
            .map_or_else(|| refs::design_option(p, r), Ok)?,
        Ref::Id(_) => refs::design_option(p, r)?,
    };
    if !x.contains(id) {
        return Err(EngineError::invalid(format!("the option is not in the set '{}'", x.name)));
    }
    Ok(Some(id))
}

impl Command for ViewOptionsSet {
    const NAME: &'static str = "view_options_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let view = refs::view(p, &self.view)?;
        let v = p.view(view).ok_or_else(|| EngineError::not_found("view"))?;
        if !v.kind.is_phased() {
            return Err(EngineError::rejected(t!("msg-option-view")));
        }
        if let Some(t) = v.template.and_then(|t| p.template(t)).filter(|t| t.include.design_options) {
            return Err(EngineError::rejected(t!("msg-controlled-by-template", "template" => t.name.clone())));
        }
        let set = refs::option_set(p, &self.set)?;
        let option = option_in(p, set, self.option.as_ref())?;
        edit(s, t!("tx-view-options"), |e| {
            let v = e.view_mut(view).ok_or_else(|| EngineError::not_found("view"))?;
            match option {
                Some(o) => v.design_options.insert(set, o),
                None => v.design_options.remove(&set),
            };
            Ok(())
        })?;
        let p = s.project();
        Ok(json!({ "view": view.0, "design_options": p.view(view).map(|v| view_options_json(p, v)) }))
    }
}

/// A choice of option for one set (templates).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionChoice {
    /// Set id or name.
    pub set: Ref,
    /// Option id or name (in that set), or `null` for automatic.
    pub option: Option<Ref>,
}

/// The choices as a template keeps them (automatic ones left out).
pub(crate) fn choices(p: &Project, list: &[OptionChoice]) -> Result<BTreeMap<OptionSetId, OptionId>> {
    let mut out = BTreeMap::new();
    for c in list {
        let set = refs::option_set(p, &c.set)?;
        if let Some(o) = option_in(p, set, c.option.as_ref())? {
            out.insert(set, o);
        }
    }
    Ok(out)
}

/// Set the design option this session edits (`null`: the main model):
/// new model elements go into it, and views without a choice of their own
/// show it for its set. Display state: not saved, not undone.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionActiveSet {
    /// Option id, name or "Set: Option"; `null` for the main model.
    #[serde(default)]
    pub option: Option<Ref>,
}

impl Command for OptionActiveSet {
    const NAME: &'static str = "option_active_set";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let option = self.option.as_ref().map(|r| refs::design_option(s.project(), r)).transpose()?;
        s.doc.set_active_option(option);
        Ok(json!({ "active": option_ref_json(s.project(), s.project().active_option()) }))
    }
}
