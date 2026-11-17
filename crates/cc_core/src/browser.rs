//! Project browser organization: named schemes that group the views of the
//! project browser by view type, level, discipline or phase, in that order
//! of nesting. The schemes and the one in use are project settings; with no
//! scheme in use the browser lists the views by type alone.
//!
//! [`tree`] builds what the browser shows: at each depth the views that have
//! a value for the criterion go into a group per value, and the others (a
//! section has no level) go on to the next criterion beside those groups.

use crate::{LevelId, MepDiscipline, PhaseId, Project, View, ViewCategory, ViewId, ViewKind};
use serde::{Deserialize, Serialize};

/// Most schemes a project keeps.
pub const MAX_SCHEMES: usize = 32;
/// Most criteria a scheme nests.
pub const MAX_CRITERIA: usize = 4;
/// Longest scheme name, characters.
pub const MAX_NAME: usize = 80;

/// The grouping of a project without a scheme in use: by view type.
pub const DEFAULT_GROUPING: [GroupBy; 1] = [GroupBy::ViewType];

/// What a level of the browser tree groups the views by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupBy {
    /// Floor plans, ceiling plans, 3D views, sections, sheets…
    ViewType,
    /// The level of a floor or ceiling plan, lowest first.
    Level,
    /// Architecture, structure or a building service (plans only).
    Discipline,
    /// The phase a view shows, earliest first.
    Phase,
}

impl GroupBy {
    pub const ALL: [GroupBy; 4] = [GroupBy::ViewType, GroupBy::Level, GroupBy::Discipline, GroupBy::Phase];

    /// Name in commands (`view_type`, `level`, `discipline`, `phase`).
    pub fn name(self) -> &'static str {
        match self {
            GroupBy::ViewType => "view_type",
            GroupBy::Level => "level",
            GroupBy::Discipline => "discipline",
            GroupBy::Phase => "phase",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Translation key of the criterion's name.
    pub fn key(self) -> &'static str {
        match self {
            GroupBy::ViewType => "browser-group-view-type",
            GroupBy::Level => "browser-group-level",
            GroupBy::Discipline => "browser-group-discipline",
            GroupBy::Phase => "browser-group-phase",
        }
    }
}

/// The discipline a plan belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Discipline {
    Architecture,
    Structure,
    Services(MepDiscipline),
}

impl Discipline {
    /// Name in command results (`architecture`, `structure`, `plumbing`…).
    pub fn name(self) -> &'static str {
        match self {
            Discipline::Architecture => "architecture",
            Discipline::Structure => "structure",
            Discipline::Services(d) => d.name(),
        }
    }

    /// Translation key of the discipline's name.
    pub fn key(self) -> &'static str {
        match self {
            Discipline::Architecture => "discipline-architecture",
            Discipline::Structure => "discipline-structure",
            Discipline::Services(d) => d.key(),
        }
    }

    fn rank(self) -> usize {
        match self {
            Discipline::Architecture => 0,
            Discipline::Structure => 1,
            Discipline::Services(d) => 2 + MepDiscipline::ALL.iter().position(|x| *x == d).unwrap_or(0),
        }
    }

    /// The discipline of a view: plans have one, other views none.
    pub fn of(view: &View) -> Option<Self> {
        match &view.kind {
            ViewKind::FloorPlan { mep: Some(d), .. } => Some(Discipline::Services(*d)),
            ViewKind::FloorPlan { structural: true, .. } => Some(Discipline::Structure),
            ViewKind::FloorPlan { .. } | ViewKind::CeilingPlan { .. } => Some(Discipline::Architecture),
            _ => None,
        }
    }
}

/// A named way to group the browser's views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserScheme {
    pub id: crate::BrowserSchemeId,
    pub name: String,
    /// The criteria, outermost first (no repeats, at most [`MAX_CRITERIA`]).
    #[serde(default)]
    pub group_by: Vec<GroupBy>,
}

/// The project's browser schemes and the one in use.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BrowserOrganization {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schemes: Vec<BrowserScheme>,
    /// The scheme the browser follows (`None`: by view type alone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<crate::BrowserSchemeId>,
}

impl BrowserOrganization {
    /// Nothing to save: no schemes, the default in use.
    pub fn is_default(&self) -> bool {
        self.schemes.is_empty() && self.active.is_none()
    }

    pub fn scheme(&self, id: crate::BrowserSchemeId) -> Option<&BrowserScheme> {
        self.schemes.iter().find(|s| s.id == id)
    }

    /// The scheme in use, if any.
    pub fn in_use(&self) -> Option<&BrowserScheme> {
        self.active.and_then(|id| self.scheme(id))
    }

    /// The criteria the browser groups by now.
    pub fn grouping(&self) -> &[GroupBy] {
        self.in_use().map_or(&DEFAULT_GROUPING, |s| &s.group_by)
    }

    /// Fix what a crafted file may hold: too many schemes, repeated or zero
    /// ids, empty or overlong names, repeated or too many criteria, a scheme
    /// in use that does not exist. Returns how many things changed.
    pub fn repair(&mut self) -> usize {
        let mut repairs = 0;
        let mut seen = std::collections::BTreeSet::new();
        let before = self.schemes.len();
        self.schemes.retain(|s| s.id.0 != 0 && seen.len() < MAX_SCHEMES && seen.insert(s.id));
        repairs += before - self.schemes.len();
        for s in &mut self.schemes {
            let name: String = s.name.trim().chars().take(MAX_NAME).collect();
            let name = if name.is_empty() { s.id.to_string() } else { name };
            if name != s.name {
                s.name = name;
                repairs += 1;
            }
            let criteria = distinct(&s.group_by);
            if criteria != s.group_by {
                s.group_by = criteria;
                repairs += 1;
            }
        }
        if self.active.is_some() && self.in_use().is_none() {
            self.active = None;
            repairs += 1;
        }
        repairs
    }
}

/// `criteria` without repeats, cut at [`MAX_CRITERIA`].
pub fn distinct(criteria: &[GroupBy]) -> Vec<GroupBy> {
    let mut out: Vec<GroupBy> = Vec::new();
    for c in criteria {
        if !out.contains(c) && out.len() < MAX_CRITERIA {
            out.push(*c);
        }
    }
    out
}

/// What a group of the browser tree stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKey {
    ViewType(ViewCategory),
    Level(LevelId),
    Discipline(Discipline),
    Phase(PhaseId),
}

impl GroupKey {
    /// The criterion the group comes from.
    pub fn criterion(self) -> GroupBy {
        match self {
            GroupKey::ViewType(_) => GroupBy::ViewType,
            GroupKey::Level(_) => GroupBy::Level,
            GroupKey::Discipline(_) => GroupBy::Discipline,
            GroupKey::Phase(_) => GroupBy::Phase,
        }
    }
}

/// A row of the browser tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Group { key: GroupKey, children: Vec<Node> },
    View(ViewId),
}

impl Node {
    /// How many views the row holds (a view row holds itself).
    pub fn view_count(&self) -> usize {
        match self {
            Node::View(_) => 1,
            Node::Group { children, .. } => children.iter().map(Node::view_count).sum(),
        }
    }
}

/// A view's name in the browser: sheets go by number and name.
pub fn label(view: &View) -> String {
    match &view.kind {
        ViewKind::Sheet { number, .. } => format!("{number} - {}", view.name),
        _ => view.name.clone(),
    }
}

/// The value `view` has for `criterion`, with its place among the groups.
fn key_of(project: &Project, view: &View, criterion: GroupBy) -> Option<(usize, GroupKey)> {
    match criterion {
        GroupBy::ViewType => {
            let category = view.category();
            let rank = ViewCategory::ALL.iter().position(|c| *c == category)?;
            Some((rank, GroupKey::ViewType(category)))
        }
        GroupBy::Level => {
            let level = view.level()?;
            let rank = project.levels.iter().position(|l| l.id == level)?;
            Some((rank, GroupKey::Level(level)))
        }
        GroupBy::Discipline => Discipline::of(view).map(|d| (d.rank(), GroupKey::Discipline(d))),
        GroupBy::Phase => {
            if !view.kind.is_phased() {
                return None;
            }
            // An unassigned view shows the last phase.
            let rank =
                project.phases.iter().position(|p| p.id == view.phase).or(project.phases.len().checked_sub(1))?;
            project.phases.get(rank).map(|p| (rank, GroupKey::Phase(p.id)))
        }
    }
}

/// The views of the project in the browser's order: plans by their level's
/// elevation, then every view by its label.
fn sorted(project: &Project) -> Vec<&View> {
    let mut views: Vec<(f64, String, &View)> =
        project.views.values().map(|v| (v.level().map_or(0.0, |l| project.level_elevation(l)), label(v), v)).collect();
    views.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then(a.2.id.cmp(&b.2.id)));
    views.into_iter().map(|(_, _, v)| v).collect()
}

fn build(project: &Project, views: Vec<&View>, criteria: &[GroupBy], root: bool) -> Vec<Node> {
    let Some((criterion, rest)) = criteria.split_first() else {
        return views.into_iter().map(|v| Node::View(v.id)).collect();
    };
    let mut groups: Vec<(usize, GroupKey, Vec<&View>)> = Vec::new();
    // At the root every view type shows, empty or not: views are added from its row.
    if root && *criterion == GroupBy::ViewType {
        groups.extend(ViewCategory::ALL.iter().enumerate().map(|(i, c)| (i, GroupKey::ViewType(*c), Vec::new())));
    }
    let mut loose = Vec::new();
    for view in views {
        match key_of(project, view, *criterion) {
            Some((rank, key)) => match groups.iter_mut().find(|g| g.1 == key) {
                Some(group) => group.2.push(view),
                None => groups.push((rank, key, vec![view])),
            },
            None => loose.push(view),
        }
    }
    groups.sort_by_key(|g| g.0);
    let mut nodes: Vec<Node> = groups
        .into_iter()
        .map(|(_, key, views)| Node::Group { key, children: build(project, views, rest, false) })
        .collect();
    nodes.extend(build(project, loose, rest, false));
    nodes
}

/// The browser tree of `project` grouped by `criteria`, outermost first.
pub fn tree(project: &Project, criteria: &[GroupBy]) -> Vec<Node> {
    build(project, sorted(project), &distinct(criteria), true)
}

impl Project {
    /// The browser tree under the scheme in use.
    pub fn browser_tree(&self) -> Vec<Node> {
        tree(self, self.browser.grouping())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrowserSchemeId, Level};

    fn project() -> Project {
        let mut p = Project::empty();
        p.levels = vec![
            Level { id: LevelId(1), name: "Ground".into(), elevation: 0.0 },
            Level { id: LevelId(2), name: "Upper".into(), elevation: 3.0 },
        ];
        let mut add = |id: u64, name: &str, kind: ViewKind| {
            p.views.insert(ViewId(id), View::new(ViewId(id), name, 50, kind));
        };
        add(10, "Upper", ViewKind::floor_plan(LevelId(2)));
        add(11, "Ground", ViewKind::floor_plan(LevelId(1)));
        add(12, "Walls", ViewKind::floor_plan(LevelId(1)));
        add(13, "Ground", ViewKind::ceiling_plan(LevelId(1)));
        add(14, "Notes", ViewKind::Drafting {});
        if let Some(ViewKind::FloorPlan { mep, .. }) = p.views.get_mut(&ViewId(12)).map(|v| &mut v.kind) {
            *mep = Some(MepDiscipline::Electrical);
        }
        p
    }

    fn views(nodes: &[Node]) -> Vec<u64> {
        nodes.iter().filter_map(|n| if let Node::View(v) = n { Some(v.0) } else { None }).collect()
    }

    fn group(nodes: &[Node], key: GroupKey) -> &[Node] {
        nodes
            .iter()
            .find_map(|n| match n {
                Node::Group { key: k, children } if *k == key => Some(children.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no group {key:?}"))
    }

    #[test]
    fn the_default_lists_every_view_type_with_its_views_in_level_order() {
        let p = project();
        let tree = p.browser_tree();
        assert_eq!(tree.len(), ViewCategory::ALL.len(), "empty types show too");
        assert_eq!(views(group(&tree, GroupKey::ViewType(ViewCategory::FloorPlan))), [11, 12, 10]);
        assert_eq!(views(group(&tree, GroupKey::ViewType(ViewCategory::Drafting))), [14]);
        assert_eq!(tree.iter().map(Node::view_count).sum::<usize>(), 5);
    }

    #[test]
    fn views_nest_by_type_then_level_and_those_without_a_level_stay_under_their_type() {
        let p = project();
        let tree = tree(&p, &[GroupBy::ViewType, GroupBy::Level]);
        let plans = group(&tree, GroupKey::ViewType(ViewCategory::FloorPlan));
        assert_eq!(plans.len(), 2);
        assert!(matches!(plans[0], Node::Group { key: GroupKey::Level(LevelId(1)), .. }), "lowest level first");
        assert_eq!(views(group(plans, GroupKey::Level(LevelId(1)))), [11, 12]);
        assert_eq!(views(group(plans, GroupKey::Level(LevelId(2)))), [10]);
        assert_eq!(views(group(&tree, GroupKey::ViewType(ViewCategory::Drafting))), [14]);
    }

    #[test]
    fn a_level_first_scheme_keeps_the_other_views_beside_the_levels() {
        let p = project();
        let tree = tree(&p, &[GroupBy::Level, GroupBy::Discipline, GroupBy::Level]);
        let ground = group(&tree, GroupKey::Level(LevelId(1)));
        assert_eq!(views(group(ground, GroupKey::Discipline(Discipline::Architecture))), [11, 13]);
        assert_eq!(views(group(ground, GroupKey::Discipline(Discipline::Services(MepDiscipline::Electrical)))), [12]);
        // The drafting view has neither a level nor a discipline.
        assert_eq!(views(&tree), [14]);
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn repair_drops_what_a_crafted_file_may_hold() {
        let scheme = |id: u64, name: &str, group_by: Vec<GroupBy>| BrowserScheme {
            id: BrowserSchemeId(id),
            name: name.into(),
            group_by,
        };
        let mut b = BrowserOrganization {
            schemes: vec![
                scheme(5, "  By level ", vec![GroupBy::Level, GroupBy::Level, GroupBy::ViewType]),
                scheme(5, "Twin", vec![]),
                scheme(0, "Zero", vec![]),
                scheme(6, " ", vec![]),
            ],
            active: Some(BrowserSchemeId(9)),
        };
        assert_eq!(b.repair(), 6);
        assert_eq!(b.schemes.len(), 2);
        assert_eq!(b.schemes[0].name, "By level");
        assert_eq!(b.schemes[0].group_by, [GroupBy::Level, GroupBy::ViewType]);
        assert_eq!(b.schemes[1].name, "BO6");
        assert_eq!(b.active, None);
        assert_eq!(b.repair(), 0);
        assert_eq!(b.grouping(), DEFAULT_GROUPING);
    }
}
