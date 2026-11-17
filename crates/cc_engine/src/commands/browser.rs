//! Project browser organization (see [`cc_core::browser`]): named schemes
//! that group the browser's views by view type, level, discipline or phase.
//! The schemes and the one in use are project settings, changed in one undo
//! step each; `browser_tree` gives the tree the browser shows.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::browser::{self, BrowserScheme, GroupBy, GroupKey, MAX_CRITERIA, MAX_NAME, MAX_SCHEMES, Node};
use cc_core::{BrowserSchemeId, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The criteria a new scheme groups by when none are given.
pub const NEW_SCHEME: [GroupBy; 2] = [GroupBy::ViewType, GroupBy::Level];

fn names(criteria: &[GroupBy]) -> Vec<&'static str> {
    criteria.iter().map(|g| g.name()).collect()
}

/// The criteria named in `group_by`: known names, no repeats, a bounded count.
fn criteria_arg(group_by: &[String]) -> Result<Vec<GroupBy>> {
    let mut out = Vec::new();
    for name in group_by {
        let Some(g) = GroupBy::from_name(name) else {
            let known = names(&GroupBy::ALL).join(", ");
            return Err(EngineError::invalid(format!("unknown grouping '{name}' ({known})")));
        };
        if out.contains(&g) {
            return Err(EngineError::invalid(format!("'{}' is given twice", g.name())));
        }
        out.push(g);
    }
    if out.len() > MAX_CRITERIA {
        return Err(EngineError::invalid(format!("a scheme groups by at most {MAX_CRITERIA} criteria")));
    }
    Ok(out)
}

/// A free name for a scheme: not empty, not another scheme's.
fn name_arg(p: &Project, name: &str, current: Option<BrowserSchemeId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(EngineError::invalid(format!("a browser scheme needs a name of 1 to {MAX_NAME} characters")));
    }
    if p.browser.schemes.iter().any(|s| Some(s.id) != current && s.name.trim().eq_ignore_ascii_case(name)) {
        return Err(EngineError::invalid(format!("a browser scheme named '{name}' already exists")));
    }
    Ok(name.to_string())
}

fn scheme_json(p: &Project, s: &BrowserScheme) -> Value {
    json!({
        "id": s.id.0,
        "name": s.name,
        "group_by": names(&s.group_by),
        "in_use": p.browser.active == Some(s.id),
    })
}

fn scheme_out(s: &Session, id: BrowserSchemeId) -> Result<Value> {
    let p = s.project();
    p.browser.scheme(id).map(|sc| scheme_json(p, sc)).ok_or_else(|| EngineError::not_found("browser scheme"))
}

/// The project browser's organization schemes: each with its name and the
/// criteria it groups the views by, outermost first, and whether it is in
/// use. `in_use` is null while the browser lists the views by type alone
/// (the default), and `criteria` are the names `group_by` takes.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BrowserSchemesList {}

impl Command for BrowserSchemesList {
    const NAME: &'static str = "browser_schemes_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        Ok(json!({
            "in_use": p.browser.in_use().map(|sc| sc.id.0),
            "default": names(&browser::DEFAULT_GROUPING),
            "criteria": names(&GroupBy::ALL),
            "schemes": p.browser.schemes.iter().map(|sc| scheme_json(p, sc)).collect::<Vec<_>>(),
        }))
    }
}

/// Create a project browser organization scheme: a named way to group the
/// browser's views. `group_by` lists the criteria from the outermost group
/// in (`view_type`, `level`, `discipline`, `phase`); a view without a value
/// for one (a section has no level) stays beside that criterion's groups.
/// The new scheme is put in use unless `use_it` is false.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BrowserSchemeCreate {
    /// Name (unique among the schemes).
    pub name: String,
    /// Criteria, outermost first: `view_type`, `level`, `discipline`,
    /// `phase` (default: `["view_type", "level"]`).
    #[serde(default)]
    pub group_by: Option<Vec<String>>,
    /// Make the browser follow the new scheme (default true).
    #[serde(default)]
    pub use_it: Option<bool>,
}

impl Command for BrowserSchemeCreate {
    const NAME: &'static str = "browser_scheme_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        if p.browser.schemes.len() >= MAX_SCHEMES {
            return Err(EngineError::rejected(format!("a project has at most {MAX_SCHEMES} browser schemes")));
        }
        let name = name_arg(p, &self.name, None)?;
        let group_by = match &self.group_by {
            Some(g) => criteria_arg(g)?,
            None => NEW_SCHEME.to_vec(),
        };
        let use_it = self.use_it.unwrap_or(true);
        let id = edit(s, t!("tx-browser-organization"), |e| {
            let id = BrowserSchemeId(e.alloc_id());
            let b = e.browser_mut();
            b.schemes.push(BrowserScheme { id, name, group_by });
            if use_it {
                b.active = Some(id);
            }
            Ok(id)
        })?;
        scheme_out(s, id)
    }
}

/// Change a project browser organization scheme: its name or the criteria
/// it groups the views by. The browser follows at once when it is in use.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BrowserSchemeSet {
    /// Scheme id or name.
    pub scheme: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New criteria, outermost first: `view_type`, `level`, `discipline`, `phase`.
    #[serde(default)]
    pub group_by: Option<Vec<String>>,
}

impl Command for BrowserSchemeSet {
    const NAME: &'static str = "browser_scheme_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::browser_scheme(p, &self.scheme)?;
        let name = self.name.as_deref().map(|n| name_arg(p, n, Some(id))).transpose()?;
        let group_by = self.group_by.as_deref().map(criteria_arg).transpose()?;
        edit(s, t!("tx-browser-organization"), |e| {
            let scheme = e
                .browser_mut()
                .schemes
                .iter_mut()
                .find(|sc| sc.id == id)
                .ok_or_else(|| EngineError::not_found("browser scheme"))?;
            if let Some(n) = name {
                scheme.name = n;
            }
            if let Some(g) = group_by {
                scheme.group_by = g;
            }
            Ok(())
        })?;
        scheme_out(s, id)
    }
}

/// Delete a project browser organization scheme. When it was in use the
/// browser goes back to listing the views by type.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BrowserSchemeDelete {
    /// Scheme id or name.
    pub scheme: Ref,
}

impl Command for BrowserSchemeDelete {
    const NAME: &'static str = "browser_scheme_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::browser_scheme(s.project(), &self.scheme)?;
        edit(s, t!("tx-browser-organization"), |e| {
            let b = e.browser_mut();
            b.schemes.retain(|sc| sc.id != id);
            if b.active == Some(id) {
                b.active = None;
            }
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0 }))
    }
}

/// Choose the scheme the project browser follows. Without `scheme` (or
/// with null) the browser lists the views by type alone, the default.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BrowserSchemeUse {
    /// Scheme id or name; omitted or null for the default list by view type.
    #[serde(default)]
    pub scheme: Option<Ref>,
}

impl Command for BrowserSchemeUse {
    const NAME: &'static str = "browser_scheme_use";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = self.scheme.as_ref().map(|r| refs::browser_scheme(p, r)).transpose()?;
        if p.browser.active != id {
            edit(s, t!("tx-browser-organization"), |e| {
                e.browser_mut().active = id;
                Ok(())
            })?;
        }
        let p = s.project();
        Ok(json!({
            "in_use": p.browser.in_use().map(|sc| sc.id.0),
            "group_by": names(p.browser.grouping()),
        }))
    }
}

/// A group's name as the browser shows it, and what it stands for.
fn group_json(p: &Project, key: GroupKey) -> (String, Value) {
    match key {
        GroupKey::ViewType(c) => (crate::i18n::tr(c.key()), json!(format!("{c:?}"))),
        GroupKey::Level(l) => (p.level_name(l).to_string(), json!(l.0)),
        GroupKey::Discipline(d) => (crate::i18n::tr(d.key()), json!(d.name())),
        GroupKey::Phase(ph) => {
            (p.phases.iter().find(|x| x.id == ph).map(|x| x.name.clone()).unwrap_or_default(), json!(ph.0))
        }
    }
}

fn node_json(p: &Project, node: &Node) -> Value {
    match node {
        Node::View(id) => {
            let label = p.view(*id).map(browser::label).unwrap_or_default();
            json!({ "view": id.0, "name": label })
        }
        Node::Group { key, children } => {
            let (name, value) = group_json(p, *key);
            json!({
                "group": key.criterion().name(),
                "name": name,
                "value": value,
                "views": node.view_count(),
                "children": children.iter().map(|c| node_json(p, c)).collect::<Vec<_>>(),
            })
        }
    }
}

/// The project browser's tree: the views grouped as the scheme in use (or
/// `scheme`, or the criteria in `group_by`) says. A group row has `group`
/// (its criterion), `name`, `value` (the view type, level id, discipline or
/// phase id), how many `views` it holds and its `children`; a view row has
/// `view` (its id) and `name`. Plans come in the order of their levels.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BrowserTree {
    /// Scheme id or name (default: the scheme in use).
    #[serde(default)]
    pub scheme: Option<Ref>,
    /// Criteria to group by instead of a scheme's, outermost first:
    /// `view_type`, `level`, `discipline`, `phase`.
    #[serde(default)]
    pub group_by: Option<Vec<String>>,
}

impl Command for BrowserTree {
    const NAME: &'static str = "browser_tree";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        if self.scheme.is_some() && self.group_by.is_some() {
            return Err(EngineError::invalid("give `scheme` or `group_by`, not both"));
        }
        let criteria = match (&self.scheme, &self.group_by) {
            (Some(r), _) => {
                let id = refs::browser_scheme(p, r)?;
                p.browser.scheme(id).map(|sc| sc.group_by.clone()).unwrap_or_default()
            }
            (None, Some(g)) => criteria_arg(g)?,
            (None, None) => p.browser.grouping().to_vec(),
        };
        let tree = browser::tree(p, &criteria);
        Ok(json!({
            "group_by": names(&criteria),
            "tree": tree.iter().map(|n| node_json(p, n)).collect::<Vec<_>>(),
        }))
    }
}
