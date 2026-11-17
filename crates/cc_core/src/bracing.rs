//! Parametric bracing: a host element that holds the parameters of the
//! braces of one bay of a frame (the bay line between two column axes, the
//! storey height, the number of storeys, the pattern and the brace type) and
//! owns the ordinary beams made from them, kept in step like a truss's (see
//! [`crate::member_host`]).
//!
//! Local coordinates: x along start → end (the bay, `L` wide), heights above
//! the level from `base_offset`, storey `s` between `s · height` and
//! `(s + 1) · height`.

use crate::member_host::{MemberHost, RoleKey, fix_type, reset, within};
use crate::{
    Beam, BeamVertical, Category, ElementId, ElementKind, ElementType, ElementTypeId, PlanTransform, Project, TypeData,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// The bay width, column axis to column axis, meters.
pub const LENGTH: RangeInclusive<f64> = 0.5..=200.0;
/// One storey, meters.
pub const HEIGHT: RangeInclusive<f64> = 0.5..=50.0;
pub const STORIES: RangeInclusive<u32> = 1..=100;
/// The foot of the lowest storey above the level, meters.
pub const BASE_OFFSET: RangeInclusive<f64> = -1e4..=1e4;

pub const DEFAULT_LENGTH: f64 = 5.0;
pub const DEFAULT_HEIGHT: f64 = 3.0;

/// How the braces cross a storey of the bay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BracingPattern {
    /// Two braces from the top corners down to the middle of the bay.
    V,
    /// Two braces from the bottom corners up to the middle of the bay (an inverted V).
    Chevron,
    /// One brace from a bottom corner to the opposite top corner.
    Diagonal,
    /// Both diagonals.
    #[default]
    X,
    /// Four braces from the mid-height of each column to the middle of the
    /// bay at the top and at the bottom of the storey.
    K,
}

impl BracingPattern {
    pub const ALL: [BracingPattern; 5] =
        [BracingPattern::V, BracingPattern::Chevron, BracingPattern::Diagonal, BracingPattern::X, BracingPattern::K];

    pub fn code(self) -> &'static str {
        match self {
            BracingPattern::V => "v",
            BracingPattern::Chevron => "chevron",
            BracingPattern::Diagonal => "diagonal",
            BracingPattern::X => "x",
            BracingPattern::K => "k",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            BracingPattern::V => "bracing-v",
            BracingPattern::Chevron => "bracing-chevron",
            BracingPattern::Diagonal => "bracing-diagonal",
            BracingPattern::X => "bracing-x",
            BracingPattern::K => "bracing-k",
        }
    }

    /// How many braces it puts in a storey.
    pub fn per_story(self) -> usize {
        match self {
            BracingPattern::Diagonal => 1,
            BracingPattern::V | BracingPattern::Chevron | BracingPattern::X => 2,
            BracingPattern::K => 4,
        }
    }
}

/// A brace's place: its storey (from the bottom) and its index in the pattern there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BracingRole {
    pub story: u16,
    pub index: u16,
}

impl BracingRole {
    pub fn key(self) -> RoleKey {
        RoleKey::new(0, self.story, self.index)
    }

    pub fn from_key(key: RoleKey) -> Option<Self> {
        let (kind, story, index) = key.parts();
        (kind == 0).then_some(Self { story, index })
    }
}

/// A brace the bracing manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BracingMember {
    pub role: BracingRole,
    pub id: ElementId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bracing {
    /// The bay in plan: the axes of its two columns.
    pub start: DVec2,
    pub end: DVec2,
    /// The foot of the lowest storey above the level, meters.
    #[serde(default)]
    pub base_offset: f64,
    /// One storey, meters.
    pub height: f64,
    /// Storeys braced, one above the other.
    #[serde(default = "one")]
    pub stories: u32,
    #[serde(default)]
    pub pattern: BracingPattern,
    /// A diagonal rises toward the start instead of the end.
    #[serde(default)]
    pub flip: bool,
    /// Diagonals change direction storey by storey (a zigzag).
    #[serde(default)]
    pub alternate: bool,
    /// The beam type of the braces.
    pub brace_type: ElementTypeId,
    #[serde(default)]
    pub members: Vec<BracingMember>,
    #[serde(default)]
    pub detached: Vec<BracingRole>,
}

fn one() -> u32 {
    1
}

/// The type a project gets with its first bracing.
pub fn host_type(id: ElementTypeId, name: String) -> ElementType {
    ElementType { id, name, data: TypeData::Bracing {}, info: Default::default(), uid: Default::default() }
}

/// A point of a storey: along the bay (0 the start, 0.5 the middle, 1 the
/// end) and up the storey (0 its foot, 1 its top).
type Node = (f64, f64);

const X_BRACES: [(Node, Node); 2] = [((0.0, 0.0), (1.0, 1.0)), ((0.0, 1.0), (1.0, 0.0))];

impl Bracing {
    /// An X bracing of one 3 m storey over a 5 m bay on the x axis, at the level.
    pub fn new(brace_type: ElementTypeId) -> Self {
        Self {
            start: DVec2::ZERO,
            end: DVec2::new(DEFAULT_LENGTH, 0.0),
            base_offset: 0.0,
            height: DEFAULT_HEIGHT,
            stories: 1,
            pattern: BracingPattern::X,
            flip: false,
            alternate: false,
            brace_type,
            members: Vec::new(),
            detached: Vec::new(),
        }
    }

    /// The bay width.
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    /// Foot to top of all its storeys, meters.
    pub fn total_height(&self) -> f64 {
        self.height * f64::from(self.stories)
    }

    /// The braces of storey `story` (their index is their place here).
    fn braces(&self, story: u32) -> Vec<(Node, Node)> {
        match self.pattern {
            BracingPattern::V => vec![((0.0, 1.0), (0.5, 0.0)), ((1.0, 1.0), (0.5, 0.0))],
            BracingPattern::Chevron => vec![((0.0, 0.0), (0.5, 1.0)), ((1.0, 0.0), (0.5, 1.0))],
            BracingPattern::Diagonal => {
                let turned = self.flip != (self.alternate && story % 2 == 1);
                vec![X_BRACES[usize::from(turned)]]
            }
            BracingPattern::X => X_BRACES.to_vec(),
            BracingPattern::K => vec![
                ((0.0, 0.5), (0.5, 1.0)),
                ((1.0, 0.5), (0.5, 1.0)),
                ((0.0, 0.5), (0.5, 0.0)),
                ((1.0, 0.5), (0.5, 0.0)),
            ],
        }
    }

    /// Whether every number is in its range (the first one that is not, named).
    pub fn check(&self) -> Result<(), String> {
        if !(self.start.is_finite() && self.end.is_finite()) {
            return Err("start and end must be finite numbers".into());
        }
        within("base_offset", self.base_offset, &BASE_OFFSET)?;
        within("the bay width", self.length(), &LENGTH)?;
        within("height", self.height, &HEIGHT)?;
        if !STORIES.contains(&self.stories) {
            return Err(format!("stories must be between {} and {}", STORIES.start(), STORIES.end()));
        }
        Ok(())
    }

    /// Whether the brace type is a beam type.
    pub fn check_types(&self, project: &Project) -> Result<(), String> {
        if project.element_type(self.brace_type).is_some_and(|t| t.category() == Category::Beam) {
            Ok(())
        } else {
            Err(format!("type {} is not a beam type", self.brace_type.0))
        }
    }

    /// Put every out-of-range number back to its default; returns how many.
    pub fn reset_out_of_range(&mut self) -> usize {
        let mut n = 0;
        let length = self.length();
        if !(length.is_finite() && LENGTH.contains(&length)) {
            let d = (self.end - self.start).normalize_or_zero();
            let d = if d == DVec2::ZERO || !d.is_finite() { DVec2::X } else { d };
            self.end = self.start + d * DEFAULT_LENGTH;
            n += 1;
        }
        n += reset(&mut self.height, &HEIGHT, DEFAULT_HEIGHT) + reset(&mut self.base_offset, &BASE_OFFSET, 0.0);
        if !STORIES.contains(&self.stories) {
            self.stories = 1;
            n += 1;
        }
        n
    }

    /// Move, turn or mirror it (a mirrored diagonal rises toward the mirrored end).
    pub fn transform(&mut self, t: &PlanTransform) {
        self.start = t.apply(self.start);
        self.end = t.apply(self.end);
    }

    /// Its braces: role, type and beam, storey by storey from the bottom.
    /// Empty when a number is out of range (see [`Bracing::check`]).
    pub fn layout(&self) -> Vec<(BracingRole, ElementTypeId, ElementKind)> {
        let mut out = Vec::new();
        if self.check().is_err() {
            return out;
        }
        let at = |(u, _): Node| self.start.lerp(self.end, u);
        for s in 0..self.stories {
            let Ok(story) = u16::try_from(s) else { break };
            let z = |(_, v): Node| self.base_offset + self.height * (f64::from(s) + v);
            for (index, (a, b)) in (0u16..).zip(self.braces(s)) {
                let mut beam = Beam::new(at(a), at(b), z(a));
                beam.vertical = BeamVertical::Center;
                beam.end_offset = Some(z(b));
                out.push((BracingRole { story, index }, self.brace_type, ElementKind::Beam(beam)));
            }
        }
        out
    }
}

impl MemberHost for Bracing {
    fn member_types(&self) -> Vec<ElementTypeId> {
        vec![self.brace_type]
    }

    fn member_layout(&self, _project: &Project) -> Vec<(RoleKey, ElementTypeId, ElementKind)> {
        self.layout().into_iter().map(|(r, t, k)| (r.key(), t, k)).collect()
    }

    fn member_list(&self) -> Vec<(RoleKey, ElementId)> {
        self.members.iter().map(|m| (m.role.key(), m.id)).collect()
    }

    fn set_member_list(&mut self, members: &[(RoleKey, ElementId)]) {
        self.members =
            members.iter().filter_map(|&(k, id)| Some(BracingMember { role: BracingRole::from_key(k)?, id })).collect();
    }

    fn detached_roles(&self) -> Vec<RoleKey> {
        self.detached.iter().map(|r| r.key()).collect()
    }

    fn set_detached_roles(&mut self, roles: &[RoleKey]) {
        self.detached = roles.iter().filter_map(|k| BracingRole::from_key(*k)).collect();
    }

    fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool {
        BracingRole::from_key(role).is_some() && matches!(kind, ElementKind::Beam(_))
    }

    fn check_parameters(&self, project: &Project) -> Result<(), String> {
        self.check().and_then(|()| self.check_types(project))
    }

    fn repair_parameters(&mut self, project: &Project) -> Option<usize> {
        let n = self.reset_out_of_range();
        Some(n + fix_type(project, &mut self.brace_type, Category::Beam)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Element};

    /// A document with an X bracing of one 3 m storey from (0, 0) to (5, 0),
    /// its braces of the template's beam type, changed by `f` before it is added.
    fn site_with(f: impl FnOnce(&mut Bracing)) -> (Document, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let host = doc.edit("bracing", |e| {
            let level = e.project().levels[0].id;
            let beam = e.project().default_type(Category::Beam).expect("a beam type");
            let ty = ElementTypeId(e.alloc_id());
            e.add_type(host_type(ty, "Bracing".into()));
            let mut b = Bracing::new(beam);
            f(&mut b);
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id: ty,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Bracing(Box::new(b)),
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: Vec::new(),
            })
        });
        (doc, host)
    }

    fn bracing(doc: &Document, host: ElementId) -> Bracing {
        match doc.project.element(host).map(|e| &e.kind) {
            Some(ElementKind::Bracing(b)) => (**b).clone(),
            _ => panic!("not a bracing"),
        }
    }

    /// The ends of every brace as `[x, z]` pairs (the bay lies on the x axis).
    fn ends(b: &Bracing) -> Vec<[[f64; 2]; 2]> {
        b.layout()
            .into_iter()
            .map(|(_, _, k)| {
                let ElementKind::Beam(m) = k else { panic!("a beam") };
                assert_eq!(m.vertical, BeamVertical::Center);
                [[m.start.x, m.top_offset], [m.end.x, m.end_offset.unwrap_or(m.top_offset)]]
            })
            .collect()
    }

    #[test]
    fn each_pattern_lays_out_its_braces() {
        let (doc, host) = site_with(|b| b.base_offset = 1.0);
        let mut b = bracing(&doc, host);
        let cases: [(BracingPattern, Vec<[[f64; 2]; 2]>); 5] = [
            (BracingPattern::V, vec![[[0.0, 4.0], [2.5, 1.0]], [[5.0, 4.0], [2.5, 1.0]]]),
            (BracingPattern::Chevron, vec![[[0.0, 1.0], [2.5, 4.0]], [[5.0, 1.0], [2.5, 4.0]]]),
            (BracingPattern::Diagonal, vec![[[0.0, 1.0], [5.0, 4.0]]]),
            (BracingPattern::X, vec![[[0.0, 1.0], [5.0, 4.0]], [[0.0, 4.0], [5.0, 1.0]]]),
            (
                BracingPattern::K,
                vec![
                    [[0.0, 2.5], [2.5, 4.0]],
                    [[5.0, 2.5], [2.5, 4.0]],
                    [[0.0, 2.5], [2.5, 1.0]],
                    [[5.0, 2.5], [2.5, 1.0]],
                ],
            ),
        ];
        for (pattern, want) in cases {
            b.pattern = pattern;
            assert_eq!(ends(&b), want, "{pattern:?}");
            assert_eq!(want.len(), pattern.per_story());
            assert_eq!(BracingPattern::from_code(pattern.code()), Some(pattern));
        }
        assert_eq!(BracingPattern::from_code(" Chevron "), Some(BracingPattern::Chevron));
        assert_eq!(BracingPattern::from_code("w"), None);
    }

    #[test]
    fn storeys_stack_and_diagonals_flip_or_alternate() {
        let (doc, host) = site_with(|b| (b.stories, b.pattern) = (3, BracingPattern::Diagonal));
        let mut b = bracing(&doc, host);
        let rising =
            |b: &Bracing| -> Vec<bool> { ends(b).iter().map(|[a, c]| (c[1] > a[1]) == (c[0] > a[0])).collect() };
        assert_eq!(ends(&b)[2], [[0.0, 6.0], [5.0, 9.0]], "the third storey");
        assert_eq!(rising(&b), [true, true, true]);
        b.alternate = true;
        assert_eq!(rising(&b), [true, false, true]);
        b.flip = true;
        assert_eq!(rising(&b), [false, true, false]);
        b.pattern = BracingPattern::K;
        assert_eq!(b.layout().len(), 12);
        assert!((b.total_height() - 9.0).abs() < 1e-9);
    }

    #[test]
    fn out_of_range_values_are_refused_and_reset() {
        let (doc, host) = site_with(|_| {});
        let mut b = bracing(&doc, host);
        (b.height, b.stories, b.base_offset, b.end) = (0.0, 0, f64::NAN, DVec2::new(0.1, 0.0));
        assert!(b.check().is_err());
        assert!(b.layout().is_empty());
        assert_eq!(b.reset_out_of_range(), 4);
        assert!(b.check().is_ok());
        assert_eq!((b.height, b.stories, b.base_offset), (DEFAULT_HEIGHT, 1, 0.0));
        assert!((b.length() - DEFAULT_LENGTH).abs() < 1e-9);
    }

    #[test]
    fn a_new_bracing_gets_its_braces_and_keeps_their_ids() {
        let (mut doc, host) = site_with(|_| {});
        let before = bracing(&doc, host);
        assert_eq!(before.members.len(), 2);
        assert_eq!(doc.project.clone().repair(), 0, "a consistent model");
        let start = doc.project.elements.clone();
        doc.edit("set", |e| {
            if let Some(ElementKind::Bracing(b)) = e.element_mut(host).map(|x| &mut x.kind) {
                (b.pattern, b.stories) = (BracingPattern::K, 2);
            }
        });
        let after = bracing(&doc, host);
        assert_eq!(after.members.len(), 8);
        assert_eq!(after.members[0].id, before.members[0].id, "same id for the first brace");
        assert!(doc.undo());
        assert_eq!(doc.project.elements, start, "one undo step");
    }

    #[test]
    fn deleting_the_bracing_deletes_its_braces_and_a_deleted_brace_is_detached() {
        let (mut doc, host) = site_with(|_| {});
        let ids = doc.project.host_members_of(host);
        doc.edit("delete one", |e| e.remove_element(ids[0]));
        let b = bracing(&doc, host);
        assert_eq!(b.detached, [BracingRole { story: 0, index: 0 }]);
        doc.edit("set", |e| {
            if let Some(ElementKind::Bracing(b)) = e.element_mut(host).map(|x| &mut x.kind) {
                b.height = 4.0;
            }
        });
        assert_eq!(bracing(&doc, host).members.len(), 1, "not made again");
        doc.edit("delete", |e| e.remove_element(host));
        assert!(ids.iter().all(|id| doc.project.element(*id).is_none()));
    }

    #[test]
    fn a_mirror_keeps_the_diagonal_a_mirror_image() {
        let (doc, host) = site_with(|b| b.pattern = BracingPattern::Diagonal);
        let mut b = bracing(&doc, host);
        b.transform(&PlanTransform::mirror_across(DVec2::new(2.5, 0.0), DVec2::new(0.0, 1.0)));
        // It rose toward x = 5; mirrored about x = 2.5 it rises toward x = 0.
        assert_eq!(ends(&b), [[[5.0, 0.0], [0.0, 3.0]]]);
    }

    #[test]
    fn repair_gives_a_bad_brace_type_the_default() {
        let (doc, host) = site_with(|_| {});
        let mut p = doc.project.clone();
        let column = p.default_type(Category::Column).expect("a column type");
        if let Some(ElementKind::Bracing(b)) = p.elements.get_mut(&host).map(|e| &mut e.kind) {
            b.brace_type = column;
            b.stories = 1000;
        }
        assert_eq!(p.repair(), 2);
        let Some(ElementKind::Bracing(b)) = p.element(host).map(|e| &e.kind) else { panic!("kept") };
        assert_eq!(p.element_type(b.brace_type).map(|t| t.category()), Some(Category::Beam));
        assert_eq!(p.repair(), 0);
    }
}
