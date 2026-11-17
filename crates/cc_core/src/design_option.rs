//! Design options: alternative sets of elements within one project.
//!
//! The project holds [`OptionSet`]s (say "Entrance" or "Kitchen layout"),
//! each with an ordered list of [`DesignOption`]s and one *primary* option.
//! An element belongs to the main model (`Element::design_option` is
//! `None`), shown with every option, or to exactly one option. A view shows
//! the main model plus one option of each set: the one it chooses
//! (`View::design_options`), else the option being edited when it belongs to
//! that set ([`Project::active_option`]), else the set's primary option
//! ("automatic"). Elements of the other options are left out of the view
//! altogether: drawings, schedules, joins and room boundaries.
//!
//! Hosted elements follow their hosts (a door is in its wall's option, see
//! [`Project::option_fixes`]); annotations belong to no option. Deleting an
//! option deletes its elements; accepting a set's primary option moves its
//! elements into the main model and deletes the set with its other options
//! and their elements. IFC export writes the main model and the primary
//! options only.

use crate::{Element, ElementId, OptionId, OptionSetId, Project, View};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Most option sets in a project.
pub const MAX_OPTION_SETS: usize = 100;
/// Most options in a set.
pub const MAX_OPTIONS: usize = 100;
/// Longest host chain followed (door → wall …).
const MAX_HOST_CHAIN: usize = 8;

/// One alternative of an option set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignOption {
    pub id: OptionId,
    pub name: String,
}

/// A named group of mutually exclusive design options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionSet {
    pub id: OptionSetId,
    pub name: String,
    /// The alternatives, in display order (never empty in a repaired project).
    pub options: Vec<DesignOption>,
    /// The option views show by default and exports write.
    pub primary: OptionId,
}

impl OptionSet {
    pub fn option(&self, id: OptionId) -> Option<&DesignOption> {
        self.options.iter().find(|o| o.id == id)
    }

    pub fn contains(&self, id: OptionId) -> bool {
        self.option(id).is_some()
    }

    /// The primary option (the first one when the primary is unknown).
    pub fn primary_option(&self) -> Option<OptionId> {
        if self.contains(self.primary) { Some(self.primary) } else { self.options.first().map(|o| o.id) }
    }
}

/// The element whose option decides an element's (the wall of a door…).
fn option_host(e: &Element) -> Option<ElementId> {
    crate::phase::phase_host(e)
}

impl Project {
    pub fn option_set(&self, id: OptionSetId) -> Option<&OptionSet> {
        self.option_sets.iter().find(|s| s.id == id)
    }

    /// The set an option belongs to.
    pub fn option_set_of(&self, option: OptionId) -> Option<&OptionSet> {
        self.option_sets.iter().find(|s| s.contains(option))
    }

    /// An option and its set.
    pub fn design_option(&self, id: OptionId) -> Option<(&OptionSet, &DesignOption)> {
        self.option_sets.iter().find_map(|s| s.option(id).map(|o| (s, o)))
    }

    /// "Set: Option", or empty for unknown options.
    pub fn option_label(&self, id: OptionId) -> String {
        self.design_option(id).map(|(s, o)| format!("{}: {}", s.name, o.name)).unwrap_or_default()
    }

    /// The option being edited, when it still exists.
    pub fn active_option(&self) -> Option<OptionId> {
        self.active_option.filter(|o| self.option_set_of(*o).is_some())
    }

    /// The primary option of every set (what exports write).
    pub fn primary_options(&self) -> BTreeSet<OptionId> {
        self.option_sets.iter().filter_map(OptionSet::primary_option).collect()
    }

    /// The option of each set that `view` shows (see the module docs); with
    /// no view, what views without a choice of their own show.
    pub fn shown_options(&self, view: Option<&View>) -> BTreeSet<OptionId> {
        let active = self.active_option();
        self.option_sets
            .iter()
            .filter_map(|s| {
                let chosen = view.and_then(|v| v.design_options.get(&s.id)).copied().filter(|o| s.contains(*o));
                chosen.or(active.filter(|a| s.contains(*a))).or_else(|| s.primary_option())
            })
            .collect()
    }

    /// Whether `e` is part of a model showing the options `shown`.
    pub fn option_shows(&self, e: &Element, shown: &BTreeSet<OptionId>) -> bool {
        e.design_option.is_none_or(|o| shown.contains(&o))
    }

    /// Whether showing the options `shown` leaves any element out.
    pub fn options_hide_any(&self, shown: &BTreeSet<OptionId>) -> bool {
        self.elements.values().any(|e| !self.option_shows(e, shown))
    }

    /// A copy of the project with only the main model and the options
    /// `shown` (joins between what is left), or `None` when nothing would
    /// be left out.
    pub fn with_options(&self, shown: &BTreeSet<OptionId>) -> Option<Project> {
        if !self.options_hide_any(shown) {
            return None;
        }
        let mut p = self.clone();
        p.elements.retain(|_, e| self.option_shows(e, shown));
        p.joins.retain(|(a, b)| p.elements.contains_key(a) && p.elements.contains_key(b));
        p.pinned.retain(|id| p.elements.contains_key(id));
        Some(p)
    }

    /// Elements of one option.
    pub fn option_elements(&self, option: OptionId) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.design_option == Some(option))
    }

    /// The options that would make elements consistent: annotations in no
    /// option, unknown options reset to the main model, hosted elements in
    /// their host's option. Returns `(element, option)` for each element
    /// that needs a change.
    pub fn option_fixes(&self) -> Vec<(ElementId, Option<OptionId>)> {
        let valid = |o: Option<OptionId>| o.filter(|o| self.option_set_of(*o).is_some());
        let mut out = Vec::new();
        for e in self.elements.values() {
            let want = if !e.category().is_phased() {
                None
            } else {
                // Up the host chain to the element that decides.
                let mut top = e;
                for _ in 0..MAX_HOST_CHAIN {
                    match option_host(top).and_then(|h| self.element(h)) {
                        Some(h) if h.id != e.id => top = h,
                        _ => break,
                    }
                }
                valid(top.design_option)
            };
            if want != e.design_option {
                out.push((e.id, want));
            }
        }
        out
    }

    /// Choices of `choices` that name no set, or an option outside its set.
    pub(crate) fn stale_choices(&self, choices: &BTreeMap<OptionSetId, OptionId>) -> bool {
        choices.iter().any(|(s, o)| self.option_set(*s).is_none_or(|s| !s.contains(*o)))
    }

    /// Drop the choices of `choices` that name no set, or an option outside
    /// its set; returns how many.
    pub(crate) fn clean_choices(&self, choices: &mut BTreeMap<OptionSetId, OptionId>) -> usize {
        let before = choices.len();
        choices.retain(|s, o| self.option_set(*s).is_some_and(|s| s.contains(*o)));
        before - choices.len()
    }

    /// Sets, options, element options and view choices; returns how many
    /// repairs were needed.
    pub(crate) fn repair_options(&mut self) -> usize {
        let mut repairs = 0;
        let mut set_ids = BTreeSet::new();
        let mut option_ids = BTreeSet::new();
        let before = self.option_sets.len();
        self.option_sets.retain(|s| s.id.0 != 0 && set_ids.insert(s.id));
        self.option_sets.truncate(MAX_OPTION_SETS);
        repairs += before - self.option_sets.len();
        for s in &mut self.option_sets {
            let before = s.options.len();
            s.options.retain(|o| o.id.0 != 0 && option_ids.insert(o.id));
            s.options.truncate(MAX_OPTIONS);
            repairs += before - s.options.len();
            if s.name.trim().is_empty() {
                s.name = s.id.to_string();
                repairs += 1;
            }
            for o in &mut s.options {
                if o.name.trim().is_empty() {
                    o.name = o.id.to_string();
                    repairs += 1;
                }
            }
            if let Some(primary) = s.primary_option()
                && primary != s.primary
            {
                s.primary = primary;
                repairs += 1;
            }
        }
        let before = self.option_sets.len();
        self.option_sets.retain(|s| !s.options.is_empty());
        repairs += before - self.option_sets.len();
        let fixes = self.option_fixes();
        repairs += fixes.len();
        for (id, option) in fixes {
            if let Some(e) = self.elements.get_mut(&id) {
                e.design_option = option;
            }
        }
        let mut views = std::mem::take(&mut self.views);
        for v in views.values_mut() {
            repairs += self.clean_choices(&mut v.design_options);
            if !v.kind.is_phased() && !v.design_options.is_empty() {
                repairs += v.design_options.len();
                v.design_options.clear();
            }
        }
        self.views = views;
        let mut templates = std::mem::take(&mut self.templates);
        for t in templates.values_mut() {
            repairs += self.clean_choices(&mut t.design_options);
        }
        self.templates = templates;
        if self.active_option.is_some() && self.active_option().is_none() {
            self.active_option = None;
        }
        repairs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementKind, ElementTypeId, LevelId, ViewId, ViewKind, Wall};
    use glam::DVec2;

    fn wall(p: &mut Project, option: Option<OptionId>) -> ElementId {
        let id = ElementId(p.alloc_id());
        p.elements.insert(
            id,
            Element {
                id,
                level: LevelId(1),
                type_id: ElementTypeId(1),
                mark: String::new(),
                comments: String::new(),
                kind: ElementKind::Wall(Wall { start: DVec2::ZERO, end: DVec2::X, ..Default::default() }),
                uid: crate::Uid::generate(),
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: option,
                paints: Vec::new(),
            },
        );
        id
    }

    fn set(p: &mut Project, names: &[&str]) -> (OptionSetId, Vec<OptionId>) {
        let id = OptionSetId(p.alloc_id());
        let options: Vec<DesignOption> =
            names.iter().map(|n| DesignOption { id: OptionId(p.alloc_id()), name: n.to_string() }).collect();
        let ids: Vec<OptionId> = options.iter().map(|o| o.id).collect();
        p.option_sets.push(OptionSet { id, name: "Set".into(), primary: ids[0], options });
        (id, ids)
    }

    #[test]
    fn views_show_their_choice_else_the_active_else_the_primary() {
        let mut p = Project::empty();
        let (s, o) = set(&mut p, &["A", "B"]);
        let main = wall(&mut p, None);
        let a = wall(&mut p, Some(o[0]));
        let b = wall(&mut p, Some(o[1]));
        let mut view = View::new(ViewId(p.alloc_id()), "v", 50, ViewKind::floor_plan(LevelId(1)));
        let shows = |p: &Project, v: &View, id| p.option_shows(p.element(id).unwrap(), &p.shown_options(Some(v)));
        assert!(shows(&p, &view, main) && shows(&p, &view, a) && !shows(&p, &view, b));
        p.active_option = Some(o[1]);
        assert!(shows(&p, &view, b) && !shows(&p, &view, a));
        view.design_options.insert(s, o[0]);
        assert!(shows(&p, &view, a) && !shows(&p, &view, b));
        assert_eq!(p.primary_options(), [o[0]].into());
        let filtered = p.with_options(&p.primary_options()).unwrap();
        assert!(filtered.element(b).is_none() && filtered.element(main).is_some());
    }

    #[test]
    fn repair_drops_dangling_options_and_hosted_follow_hosts() {
        let mut p = Project::empty();
        let (s, o) = set(&mut p, &["A", "B"]);
        let w = wall(&mut p, Some(o[1]));
        let lost = wall(&mut p, Some(OptionId(9999)));
        let door = ElementId(p.alloc_id());
        let mut d = p.element(w).unwrap().clone();
        d.id = door;
        d.kind = ElementKind::Door(crate::Hosted {
            host: w,
            offset: 0.5,
            sill_height: 0.0,
            flip_hand: false,
            flip_facing: false,
        });
        d.design_option = None;
        p.elements.insert(door, d);
        let mut view = View::new(ViewId(p.alloc_id()), "v", 50, ViewKind::floor_plan(LevelId(1)));
        view.design_options.insert(s, OptionId(9999));
        p.views.insert(view.id, view.clone());
        p.option_sets[0].primary = OptionId(12345);
        let repairs = p.repair_options();
        assert!(repairs >= 4, "{repairs}");
        assert_eq!(p.element(door).unwrap().design_option, Some(o[1]));
        assert_eq!(p.element(lost).unwrap().design_option, None);
        assert!(p.view(view.id).unwrap().design_options.is_empty());
        assert_eq!(p.option_sets[0].primary, o[0]);
        assert_eq!(p.repair_options(), 0);
    }
}
