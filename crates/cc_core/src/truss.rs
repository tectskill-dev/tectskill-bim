//! Parametric trusses: a host element that holds the parameters of a plane
//! truss (its span line, shape, height, web pattern, panels, member types
//! and copies) and owns the ordinary beams (chords, diagonals) and columns
//! (verticals) made from them, kept in step like a steel building's (see
//! [`crate::member_host`]).
//!
//! Local coordinates: x along start → end (`L` long), the copies to the
//! left of it every `spacing`, heights above the level from `base_offset`
//! (the bottom chord's axis).

use crate::member_host::{MemberHost, RoleKey, fix_type, reset, within};
use crate::{
    Beam, BeamVertical, Category, Column, ElementId, ElementKind, ElementType, ElementTypeId, PlanTransform, Project,
    TypeData, WallTop,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// Most members one truss (all copies) may make; [`Truss::check`] refuses
/// a truss whose layout would make more.
pub const MAX_MEMBERS: usize = 4000;
/// A vertical shorter than this is skipped, and so is a diagonal whose ends
/// differ less in height (it would lie on the bottom chord), meters.
pub const MIN_VERTICAL: f64 = 0.05;

pub const LENGTH: RangeInclusive<f64> = 0.5..=200.0;
pub const HEIGHT: RangeInclusive<f64> = 0.1..=20.0;
pub const PANELS: RangeInclusive<u32> = 2..=64;
pub const COPIES: RangeInclusive<u32> = 1..=200;
pub const SPACING: RangeInclusive<f64> = 0.3..=20.0;
/// The bottom chord's axis above the level, meters.
pub const BASE_OFFSET: RangeInclusive<f64> = -1e4..=1e4;

pub const DEFAULT_LENGTH: f64 = 6.0;
pub const DEFAULT_HEIGHT: f64 = 1.0;
pub const DEFAULT_SPACING: f64 = 1.5;
/// The panel length the default panel count aims at, meters.
pub const PANEL_LENGTH: f64 = 1.5;

/// The outline of the truss: chords parallel, the top chord rising toward
/// the end (mono), or toward mid-span from both supports (gable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TrussShape {
    #[default]
    Flat,
    Mono,
    Gable,
}

impl TrussShape {
    pub const ALL: [TrussShape; 3] = [TrussShape::Flat, TrussShape::Mono, TrussShape::Gable];

    pub fn code(self) -> &'static str {
        match self {
            TrussShape::Flat => "flat",
            TrussShape::Mono => "mono",
            TrussShape::Gable => "gable",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            TrussShape::Flat => "roof-flat",
            TrussShape::Mono => "roof-shed",
            TrussShape::Gable => "roof-gable",
        }
    }
}

/// The web between the chords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TrussPattern {
    #[default]
    Pratt,
    Howe,
    Warren,
    KingPost,
    QueenPost,
    Fink,
}

impl TrussPattern {
    pub const ALL: [TrussPattern; 6] = [
        TrussPattern::Pratt,
        TrussPattern::Howe,
        TrussPattern::Warren,
        TrussPattern::KingPost,
        TrussPattern::QueenPost,
        TrussPattern::Fink,
    ];

    pub fn code(self) -> &'static str {
        match self {
            TrussPattern::Pratt => "pratt",
            TrussPattern::Howe => "howe",
            TrussPattern::Warren => "warren",
            TrussPattern::KingPost => "kingpost",
            TrussPattern::QueenPost => "queenpost",
            TrussPattern::Fink => "fink",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code().eq_ignore_ascii_case(code.trim()))
    }

    pub fn key(self) -> &'static str {
        match self {
            TrussPattern::Pratt => "truss-pratt",
            TrussPattern::Howe => "truss-howe",
            TrussPattern::Warren => "truss-warren",
            TrussPattern::KingPost => "truss-king-post",
            TrussPattern::QueenPost => "truss-queen-post",
            TrussPattern::Fink => "truss-fink",
        }
    }

    /// Whether it is made for a gable only.
    pub fn needs_gable(self) -> bool {
        matches!(self, TrussPattern::KingPost | TrussPattern::QueenPost | TrussPattern::Fink)
    }

    /// The fewest panels it can be laid out on.
    pub fn min_panels(self) -> u32 {
        match self {
            TrussPattern::QueenPost => 4,
            TrussPattern::Fink => 6,
            _ => 2,
        }
    }
}

/// What a member is in its truss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrussRoleKind {
    TopChord,
    BottomChord,
    Diagonal,
    Vertical,
}

impl TrussRoleKind {
    pub const ALL: [TrussRoleKind; 4] =
        [TrussRoleKind::TopChord, TrussRoleKind::BottomChord, TrussRoleKind::Diagonal, TrussRoleKind::Vertical];
}

/// A member's place: its kind, its copy and its index there (chords: the
/// panel; verticals: the panel point; diagonals: their place in the pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrussRole {
    pub kind: TrussRoleKind,
    pub copy: u16,
    pub index: u16,
}

impl TrussRole {
    /// A column for verticals, a beam for the rest.
    pub fn fits(self, kind: &ElementKind) -> bool {
        match self.kind {
            TrussRoleKind::Vertical => matches!(kind, ElementKind::Column(_)),
            _ => matches!(kind, ElementKind::Beam(_)),
        }
    }

    pub fn key(self) -> RoleKey {
        RoleKey::new(self.kind as u8, self.copy, self.index)
    }

    pub fn from_key(key: RoleKey) -> Option<Self> {
        let (kind, copy, index) = key.parts();
        Some(Self { kind: *TrussRoleKind::ALL.get(usize::from(kind))?, copy, index })
    }
}

/// A member the truss manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrussMember {
    pub role: TrussRole,
    pub id: ElementId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Truss {
    /// The span line in plan, between the supports.
    pub start: DVec2,
    pub end: DVec2,
    /// The bottom chord's axis above the level at the supports, meters.
    pub base_offset: f64,
    #[serde(default)]
    pub shape: TrussShape,
    /// Chord axis to chord axis (flat), or the rise at the apex or high end, meters.
    pub height: f64,
    #[serde(default)]
    pub pattern: TrussPattern,
    /// Bottom-chord panels (even on a gable).
    pub panels: u32,
    pub top_chord_type: ElementTypeId,
    pub bottom_chord_type: ElementTypeId,
    pub web_type: ElementTypeId,
    /// The column type of the verticals.
    pub post_type: ElementTypeId,
    #[serde(default = "one")]
    pub copies: u32,
    /// Copy to copy, to the left of start → end, meters.
    #[serde(default = "default_spacing")]
    pub spacing: f64,
    #[serde(default)]
    pub members: Vec<TrussMember>,
    #[serde(default)]
    pub detached: Vec<TrussRole>,
}

fn one() -> u32 {
    1
}

fn default_spacing() -> f64 {
    DEFAULT_SPACING
}

/// About one panel per [`PANEL_LENGTH`], 2 to 64, even on a gable.
pub fn default_panels(length: f64, shape: TrussShape) -> u32 {
    let n = if length.is_finite() { (length / PANEL_LENGTH).round().clamp(2.0, 64.0) as u32 } else { 4 };
    if shape == TrussShape::Gable && n % 2 == 1 { n + 1 } else { n }
}

/// The type a project gets with its first truss.
pub fn host_type(id: ElementTypeId, name: String) -> ElementType {
    ElementType { id, name, data: TypeData::Truss {}, info: Default::default(), uid: Default::default() }
}

/// A panel point: its index along the span and whether it is on the top chord.
type Node = (usize, bool);
/// A diagonal between two panel points.
type Diagonal = (Node, Node);

/// The panel point nearest `n / part`, kept off the supports and mid-span;
/// `None` when the half span has no inner point.
fn nearest(n: usize, part: f64) -> Option<usize> {
    let half = n / 2;
    (half >= 2).then(|| ((n as f64 / part).round() as usize).clamp(1, half - 1))
}

/// The web of `pattern` over `n` panels: the panel points with a vertical,
/// and the diagonals, in a fixed order (a diagonal's index is its place here).
fn web(pattern: TrussPattern, n: usize) -> (Vec<usize>, Vec<(Node, Node)>) {
    let mid = n / 2;
    // Panel i lies left of mid-span.
    let left = |i: usize| 2 * i + 1 < n;
    let inner: Vec<usize> = (1..n).collect();
    match pattern {
        // Diagonals slope down toward the middle.
        TrussPattern::Pratt => (
            inner,
            (0..n).map(|i| if left(i) { ((i, true), (i + 1, false)) } else { ((i, false), (i + 1, true)) }).collect(),
        ),
        // Diagonals slope up toward the middle.
        TrussPattern::Howe => (
            inner,
            (0..n).map(|i| if left(i) { ((i, false), (i + 1, true)) } else { ((i, true), (i + 1, false)) }).collect(),
        ),
        TrussPattern::Warren => (
            Vec::new(),
            (0..n)
                .map(|i| if i % 2 == 1 { ((i, true), (i + 1, false)) } else { ((i, false), (i + 1, true)) })
                .collect(),
        ),
        TrussPattern::KingPost => {
            let diagonals = nearest(n, 4.0)
                .map(|q| vec![((mid, false), (q, true)), ((mid, false), (n - q, true))])
                .unwrap_or_default();
            (vec![mid], diagonals)
        }
        TrussPattern::QueenPost => match nearest(n, 3.0) {
            Some(t) => (vec![t, n - t], vec![((t, false), (mid, true)), ((n - t, false), (mid, true))]),
            None => (Vec::new(), Vec::new()),
        },
        TrussPattern::Fink => {
            let q = (n / 4).max(1);
            let t = n.div_ceil(3).max(q + 1);
            (
                Vec::new(),
                vec![
                    ((t, false), (q, true)),
                    ((t, false), (mid, true)),
                    ((n - t, false), (n - q, true)),
                    ((n - t, false), (mid, true)),
                ],
            )
        }
    }
}

/// A chord or diagonal from `a` to `b` (heights above the level), its axis through both.
fn member(a: DVec2, za: f64, b: DVec2, zb: f64) -> Beam {
    let mut beam = Beam::new(a, b, za);
    beam.vertical = BeamVertical::Center;
    if (zb - za).abs() > 1e-9 {
        beam.end_offset = Some(zb);
    }
    beam
}

/// Members collected by a layout.
struct Members(Vec<(TrussRole, ElementTypeId, ElementKind)>);

impl Members {
    fn push(&mut self, kind: TrussRoleKind, copy: u32, index: usize, type_id: ElementTypeId, element: ElementKind) {
        let (Ok(copy), Ok(index)) = (u16::try_from(copy), u16::try_from(index)) else { return };
        self.0.push((TrussRole { kind, copy, index }, type_id, element));
    }
}

impl Truss {
    /// A flat Pratt truss 6 m long on the x axis, 1 m deep, at the level.
    pub fn new(chord_type: ElementTypeId, web_type: ElementTypeId, post_type: ElementTypeId) -> Self {
        Self {
            start: DVec2::ZERO,
            end: DVec2::new(DEFAULT_LENGTH, 0.0),
            base_offset: 0.0,
            shape: TrussShape::Flat,
            height: DEFAULT_HEIGHT,
            pattern: TrussPattern::Pratt,
            panels: default_panels(DEFAULT_LENGTH, TrussShape::Flat),
            top_chord_type: chord_type,
            bottom_chord_type: chord_type,
            web_type,
            post_type,
            copies: 1,
            spacing: DEFAULT_SPACING,
            members: Vec::new(),
            detached: Vec::new(),
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    /// The unit vector along start → end (zero when they meet).
    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    /// The unit vector to the left of start → end, where copies go.
    pub fn left(&self) -> DVec2 {
        self.direction().perp()
    }

    /// The top chord's axis above the bottom chord's at distance `x` from the start.
    pub fn rise(&self, x: f64) -> f64 {
        let l = self.length();
        if l <= 0.0 {
            return 0.0;
        }
        let t = (x / l).clamp(0.0, 1.0);
        match self.shape {
            TrussShape::Flat => self.height,
            TrussShape::Mono => self.height * t,
            TrussShape::Gable => self.height * (1.0 - (2.0 * t - 1.0).abs()),
        }
    }

    /// The pattern it is laid out with: a gable-only pattern falls back to
    /// Pratt on another shape, or on fewer panels than it needs.
    pub fn pattern_used(&self) -> TrussPattern {
        let p = self.pattern;
        if p.needs_gable() && (self.shape != TrussShape::Gable || self.panels < p.min_panels()) {
            TrussPattern::Pratt
        } else {
            p
        }
    }

    /// Each copy's span line.
    pub fn span_lines(&self) -> Vec<[DVec2; 2]> {
        let left = self.left();
        (0..self.copies.min(*COPIES.end()))
            .map(|k| {
                let shift = left * (f64::from(k) * self.spacing);
                [self.start + shift, self.end + shift]
            })
            .collect()
    }

    /// Whether every number is in its range (the first one that is not, named).
    pub fn check(&self) -> Result<(), String> {
        if !(self.start.is_finite() && self.end.is_finite() && self.base_offset.is_finite()) {
            return Err("start, end and base_offset must be finite numbers".into());
        }
        within("base_offset", self.base_offset, &BASE_OFFSET)?;
        within("the span length", self.length(), &LENGTH)?;
        within("height", self.height, &HEIGHT)?;
        within("spacing", self.spacing, &SPACING)?;
        if !PANELS.contains(&self.panels) {
            return Err(format!("panels must be between {} and {}", PANELS.start(), PANELS.end()));
        }
        if !COPIES.contains(&self.copies) {
            return Err(format!("copies must be between {} and {}", COPIES.start(), COPIES.end()));
        }
        if self.shape == TrussShape::Gable && self.panels % 2 == 1 {
            return Err("a gable truss needs an even number of panels".into());
        }
        let total = self.members_per_copy() * self.copies as usize;
        if total > MAX_MEMBERS {
            return Err(format!(
                "the trusses would have {total} members, more than {MAX_MEMBERS}: use fewer copies or panels"
            ));
        }
        Ok(())
    }

    /// The panel points with a vertical and the diagonals (with their index
    /// in the pattern) of one copy, leaving out what is too short to make: a
    /// vertical lower than [`MIN_VERTICAL`], a diagonal lying on the bottom
    /// chord. A flat truss gets posts at both supports, a mono truss at its
    /// high end.
    fn web_kept(&self) -> (Vec<usize>, Vec<(usize, Diagonal)>) {
        let n = self.panels as usize;
        let (mut posts, diagonals) = web(self.pattern_used(), n);
        match self.shape {
            TrussShape::Flat => {
                posts.retain(|&i| i != 0 && i != n);
                posts.insert(0, 0);
                posts.push(n);
            }
            TrussShape::Mono if !posts.contains(&n) => posts.push(n),
            _ => {}
        }
        let x = |i: usize| self.length() * i as f64 / n.max(1) as f64;
        let z = |(i, top): Node| if top { self.rise(x(i)) } else { 0.0 };
        posts.retain(|&i| self.rise(x(i)) >= MIN_VERTICAL);
        let diagonals =
            diagonals.into_iter().enumerate().filter(|&(_, (a, b))| (z(a) - z(b)).abs() >= MIN_VERTICAL).collect();
        (posts, diagonals)
    }

    /// How many members one copy makes.
    pub fn members_per_copy(&self) -> usize {
        let (posts, diagonals) = self.web_kept();
        2 * self.panels as usize + posts.len() + diagonals.len()
    }

    /// Whether the chord and web types are beam types and the post type a column type.
    pub fn check_types(&self, project: &Project) -> Result<(), String> {
        let is = |t: ElementTypeId, c: Category| project.element_type(t).is_some_and(|x| x.category() == c);
        if !is(self.post_type, Category::Column) {
            return Err(format!("type {} is not a column type", self.post_type.0));
        }
        match [self.top_chord_type, self.bottom_chord_type, self.web_type].into_iter().find(|t| !is(*t, Category::Beam))
        {
            Some(t) => Err(format!("type {} is not a beam type", t.0)),
            None => Ok(()),
        }
    }

    /// Put every out-of-range number back to its default (an odd gable
    /// count up to the next even one); returns how many.
    pub fn reset_out_of_range(&mut self) -> usize {
        let mut n = 0;
        let length = self.length();
        if !(length.is_finite() && LENGTH.contains(&length)) {
            let d = if self.direction() == DVec2::ZERO { DVec2::X } else { self.direction() };
            self.end = self.start + d * DEFAULT_LENGTH;
            n += 1;
        }
        n += reset(&mut self.height, &HEIGHT, DEFAULT_HEIGHT) + reset(&mut self.spacing, &SPACING, DEFAULT_SPACING);
        if !(self.base_offset.is_finite() && BASE_OFFSET.contains(&self.base_offset)) {
            self.base_offset = 0.0;
            n += 1;
        }
        if !PANELS.contains(&self.panels) {
            self.panels = default_panels(self.length(), self.shape);
            n += 1;
        }
        if self.shape == TrussShape::Gable && self.panels % 2 == 1 {
            self.panels += 1;
            n += 1;
        }
        if !COPIES.contains(&self.copies) {
            self.copies = 1;
            n += 1;
        }
        let per_copy = self.members_per_copy().max(1);
        if per_copy * self.copies as usize > MAX_MEMBERS {
            self.copies = u32::try_from(MAX_MEMBERS / per_copy).unwrap_or(1).max(1);
            n += 1;
        }
        n
    }

    /// The member types it uses.
    pub fn type_ids(&self) -> impl Iterator<Item = ElementTypeId> + '_ {
        [self.top_chord_type, self.bottom_chord_type, self.web_type, self.post_type].into_iter()
    }

    /// Move, turn or mirror it. A mirror maps the whole row of copies: the
    /// new span line is the mirrored last copy's, so the copies stay where
    /// the mirror puts them and a mono truss keeps its high end.
    pub fn transform(&mut self, t: &PlanTransform) {
        let shift = if t.mirror {
            self.left() * (f64::from(self.copies.saturating_sub(1)) * self.spacing)
        } else {
            DVec2::ZERO
        };
        self.start = t.apply(self.start + shift);
        self.end = t.apply(self.end + shift);
    }

    /// Its members: role, type and element, copy by copy (chords, verticals,
    /// diagonals). Empty when a number is out of range or the members would
    /// be more than [`MAX_MEMBERS`] (see [`Truss::check`]).
    pub fn layout(&self) -> Vec<(TrussRole, ElementTypeId, ElementKind)> {
        let mut out = Members(Vec::new());
        if self.check().is_err() {
            return out.0;
        }
        let n = self.panels as usize;
        let (l, dir, left) = (self.length(), self.direction(), self.left());
        let (posts, diagonals) = self.web_kept();
        let x = |i: usize| l * i as f64 / n as f64;
        let z = |(i, top): Node| self.base_offset + if top { self.rise(x(i)) } else { 0.0 };
        let angle = dir.to_angle().to_degrees();
        for k in 0..self.copies {
            let origin = self.start + left * (f64::from(k) * self.spacing);
            let at = |i: usize| origin + dir * x(i);
            for i in 0..n {
                let top = member(at(i), z((i, true)), at(i + 1), z((i + 1, true)));
                out.push(TrussRoleKind::TopChord, k, i, self.top_chord_type, ElementKind::Beam(top));
                let bottom = member(at(i), z((i, false)), at(i + 1), z((i + 1, false)));
                out.push(TrussRoleKind::BottomChord, k, i, self.bottom_chord_type, ElementKind::Beam(bottom));
            }
            for &i in &posts {
                let column = Column {
                    position: at(i),
                    rotation_deg: angle,
                    base_offset: self.base_offset,
                    height: self.rise(x(i)),
                    top: WallTop::Unconnected,
                    structural: true,
                };
                out.push(TrussRoleKind::Vertical, k, i, self.post_type, ElementKind::Column(column));
            }
            for &(j, (a, b)) in &diagonals {
                let beam = member(at(a.0), z(a), at(b.0), z(b));
                out.push(TrussRoleKind::Diagonal, k, j, self.web_type, ElementKind::Beam(beam));
            }
        }
        out.0
    }
}

impl MemberHost for Truss {
    fn member_types(&self) -> Vec<ElementTypeId> {
        self.type_ids().collect()
    }

    fn member_layout(&self, _project: &Project) -> Vec<(RoleKey, ElementTypeId, ElementKind)> {
        self.layout().into_iter().map(|(r, t, k)| (r.key(), t, k)).collect()
    }

    fn member_list(&self) -> Vec<(RoleKey, ElementId)> {
        self.members.iter().map(|m| (m.role.key(), m.id)).collect()
    }

    fn set_member_list(&mut self, members: &[(RoleKey, ElementId)]) {
        self.members =
            members.iter().filter_map(|&(k, id)| Some(TrussMember { role: TrussRole::from_key(k)?, id })).collect();
    }

    fn detached_roles(&self) -> Vec<RoleKey> {
        self.detached.iter().map(|r| r.key()).collect()
    }

    fn set_detached_roles(&mut self, roles: &[RoleKey]) {
        self.detached = roles.iter().filter_map(|k| TrussRole::from_key(*k)).collect();
    }

    fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool {
        TrussRole::from_key(role).is_some_and(|r| r.fits(kind))
    }

    fn check_parameters(&self, project: &Project) -> Result<(), String> {
        self.check().and_then(|()| self.check_types(project))
    }

    fn repair_parameters(&mut self, project: &Project) -> Option<usize> {
        let mut n = self.reset_out_of_range();
        n += fix_type(project, &mut self.top_chord_type, Category::Beam)?;
        n += fix_type(project, &mut self.bottom_chord_type, Category::Beam)?;
        n += fix_type(project, &mut self.web_type, Category::Beam)?;
        n += fix_type(project, &mut self.post_type, Category::Column)?;
        Some(n)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{ColumnSection, Document, Element, LevelId};

    fn add_type(e: &mut crate::EditCtx, data: TypeData) -> ElementTypeId {
        let id = ElementTypeId(e.alloc_id());
        e.add_type(ElementType {
            id,
            name: format!("T{}", id.0),
            data,
            info: Default::default(),
            uid: Default::default(),
        })
    }

    fn element(id: ElementId, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> Element {
        Element {
            id,
            level,
            type_id,
            mark: String::new(),
            comments: String::new(),
            uid: Default::default(),
            kind,
            phase_created: Default::default(),
            phase_demolished: None,
            design_option: None,
            paints: Vec::new(),
        }
    }

    /// A document with a flat Pratt truss from (0, 0) to (6, 0), 1 m deep,
    /// 4 panels, chords and diagonals of a 0.06 x 0.15 beam type and posts
    /// of a 0.06 x 0.1 column type, made by `f` before it is added.
    pub(crate) fn site_with(f: impl FnOnce(&mut Truss)) -> (Document, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let host = doc.edit("truss", |e| {
            let level = e.project().levels[0].id;
            let material = e.project().materials[0].name.clone();
            let beam = add_type(
                e,
                TypeData::Beam {
                    width: 0.06,
                    height: 0.15,
                    material: material.clone(),
                    shape: Default::default(),
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let post = add_type(
                e,
                TypeData::Column {
                    section: ColumnSection::Rectangular { width: 0.06, depth: 0.1 },
                    material,
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let ty = add_type(e, TypeData::Truss {});
            let mut t = Truss::new(beam, beam, post);
            f(&mut t);
            let id = ElementId(e.alloc_id());
            e.add_element(element(id, level, ty, ElementKind::Truss(Box::new(t))))
        });
        (doc, host)
    }

    pub(crate) fn truss(doc: &Document, host: ElementId) -> Truss {
        match doc.project.element(host).map(|e| &e.kind) {
            Some(ElementKind::Truss(t)) => (**t).clone(),
            _ => panic!("not a truss"),
        }
    }

    fn counts(t: &Truss) -> [usize; 4] {
        let l = t.layout();
        TrussRoleKind::ALL.map(|k| l.iter().filter(|(r, _, _)| r.kind == k).count())
    }

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn member_counts_per_shape_and_pattern() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        // [top chords, bottom chords, diagonals, verticals], 4 panels.
        let cases = [
            (TrussShape::Flat, TrussPattern::Pratt, [4, 4, 4, 5]),
            (TrussShape::Flat, TrussPattern::Howe, [4, 4, 4, 5]),
            (TrussShape::Flat, TrussPattern::Warren, [4, 4, 4, 2]),
            // On a gable the end panels' Pratt diagonals would lie on the bottom chord.
            (TrussShape::Gable, TrussPattern::Pratt, [4, 4, 2, 3]),
            (TrussShape::Gable, TrussPattern::Howe, [4, 4, 4, 3]),
            (TrussShape::Gable, TrussPattern::KingPost, [4, 4, 2, 1]),
            // A mono truss gets its high end post; its first Pratt diagonal is skipped.
            (TrussShape::Mono, TrussPattern::Pratt, [4, 4, 3, 4]),
            // Not a gable: king post falls back to Pratt.
            (TrussShape::Flat, TrussPattern::KingPost, [4, 4, 4, 5]),
            // Fink needs 6 panels: Pratt on a 4-panel gable.
            (TrussShape::Gable, TrussPattern::Fink, [4, 4, 2, 3]),
        ];
        for (shape, pattern, want) in cases {
            (t.shape, t.pattern) = (shape, pattern);
            assert_eq!(counts(&t), want, "{shape:?} {pattern:?}");
        }
        t.shape = TrussShape::Gable;
        t.panels = 6;
        for (pattern, want) in [(TrussPattern::QueenPost, [6, 6, 2, 2]), (TrussPattern::Fink, [6, 6, 4, 0])] {
            t.pattern = pattern;
            assert_eq!(counts(&t), want, "{pattern:?}");
        }
        // A flat Pratt truss of n panels: 2n chords, n + 1 verticals (posts at the supports), n diagonals.
        (t.shape, t.pattern, t.panels) = (TrussShape::Flat, TrussPattern::Pratt, 10);
        assert_eq!(counts(&t), [10, 10, 10, 11]);
    }

    #[test]
    fn the_gable_apex_and_the_king_post_members() {
        let (doc, host) = site_with(|t| {
            (t.shape, t.pattern, t.height, t.base_offset) = (TrussShape::Gable, TrussPattern::KingPost, 1.5, 2.0);
        });
        let t = truss(&doc, host);
        let l = t.layout();
        let find = |kind, index| {
            l.iter()
                .find(|(r, _, _)| *r == TrussRole { kind, copy: 0, index })
                .map(|(_, _, k)| k.clone())
                .expect("role")
        };
        // The king post: mid-span, from the bottom chord axis to the apex.
        let ElementKind::Column(post) = find(TrussRoleKind::Vertical, 2) else { panic!("a column") };
        assert!(close(post.position, DVec2::new(3.0, 0.0)));
        assert!((post.base_offset - 2.0).abs() < 1e-9 && (post.height - 1.5).abs() < 1e-9);
        // Two diagonals from its foot to the quarter points of the top chords.
        let ElementKind::Beam(d0) = find(TrussRoleKind::Diagonal, 0) else { panic!("a beam") };
        assert!(close(d0.start, DVec2::new(3.0, 0.0)) && close(d0.end, DVec2::new(1.5, 0.0)));
        assert!((d0.top_offset - 2.0).abs() < 1e-9 && (d0.end_offset.unwrap_or(0.0) - 2.75).abs() < 1e-9);
        let ElementKind::Beam(d1) = find(TrussRoleKind::Diagonal, 1) else { panic!("a beam") };
        assert!(close(d1.end, DVec2::new(4.5, 0.0)));
        // The top chord rises from the support to the apex.
        let ElementKind::Beam(top) = find(TrussRoleKind::TopChord, 1) else { panic!("a beam") };
        assert!((top.top_offset - 2.75).abs() < 1e-9 && (top.end_offset.unwrap_or(0.0) - 3.5).abs() < 1e-9);
        assert_eq!(top.vertical, crate::BeamVertical::Center);
        assert!((t.rise(3.0) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn an_odd_panel_count_on_a_gable_is_refused() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        (t.shape, t.panels) = (TrussShape::Gable, 5);
        assert!(t.check().is_err());
        assert!(t.layout().is_empty());
        assert_eq!(t.reset_out_of_range(), 1);
        assert_eq!(t.panels, 6);
        t.shape = TrussShape::Flat;
        t.panels = 5;
        assert!(t.check().is_ok(), "a flat truss takes any count");
        for bad in [1, 65] {
            t.panels = bad;
            assert!(t.check().is_err(), "{bad}");
        }
    }

    #[test]
    fn copies_are_offset_by_the_spacing_to_the_left() {
        let (doc, host) = site_with(|t| (t.copies, t.spacing) = (3, 2.0));
        let t = truss(&doc, host);
        let l = t.layout();
        assert_eq!(l.len(), 3 * 17);
        let start = |copy| {
            l.iter()
                .find(|(r, _, _)| *r == TrussRole { kind: TrussRoleKind::BottomChord, copy, index: 0 })
                .and_then(|(_, _, k)| if let ElementKind::Beam(b) = k { Some(b.start) } else { None })
                .expect("a bottom chord")
        };
        assert!(close(start(2), DVec2::new(0.0, 4.0)), "left of +x is +y");
        let lines = t.span_lines();
        assert_eq!(lines.len(), 3);
        assert!(close(lines[1][0], DVec2::new(0.0, 2.0)) && close(lines[1][1], DVec2::new(6.0, 2.0)));
    }

    #[test]
    fn a_mirror_maps_every_copy() {
        let (doc, host) = site_with(|t| (t.copies, t.spacing, t.shape) = (2, 2.0, TrussShape::Mono));
        let mut t = truss(&doc, host);
        let m = PlanTransform::mirror_across(DVec2::new(3.0, 0.0), DVec2::new(0.0, 1.0));
        let want: Vec<[DVec2; 2]> = t.span_lines().iter().map(|[a, b]| [m.apply(*a), m.apply(*b)]).collect();
        t.transform(&m);
        let got = t.span_lines();
        for w in &want {
            assert!(got.iter().any(|g| close(g[0], w[0]) && close(g[1], w[1])), "{w:?} in {got:?}");
        }
    }

    #[test]
    fn default_panels_follow_the_span() {
        assert_eq!(default_panels(6.0, TrussShape::Flat), 4);
        assert_eq!(default_panels(7.5, TrussShape::Flat), 5);
        assert_eq!(default_panels(7.5, TrussShape::Gable), 6);
        assert_eq!(default_panels(1.0, TrussShape::Flat), 2);
        assert_eq!(default_panels(500.0, TrussShape::Gable), 64);
        for p in TrussPattern::ALL {
            assert_eq!(TrussPattern::from_code(p.code()), Some(p));
        }
        assert_eq!(TrussShape::from_code(" GABLE "), Some(TrussShape::Gable));
        assert_eq!(TrussPattern::from_code("k"), None);
    }

    #[test]
    fn a_truss_over_the_member_limit_is_refused_whole() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        (t.end, t.panels, t.copies) = (DVec2::new(100.0, 0.0), 64, 200);
        let per_copy = t.members_per_copy();
        assert!(per_copy * 200 > MAX_MEMBERS);
        assert!(t.check().is_err_and(|e| e.contains("members")));
        assert!(t.layout().is_empty(), "never cut short mid-copy");
        // The count is exact: as many copies as fit lay out whole.
        t.copies = u32::try_from(MAX_MEMBERS / per_copy).expect("fits");
        assert!(t.check().is_ok());
        assert_eq!(t.layout().len(), per_copy * t.copies as usize);
        // Repair keeps the copies that fit.
        t.copies = 200;
        assert_eq!(t.reset_out_of_range(), 1);
        assert_eq!(t.copies as usize, MAX_MEMBERS / per_copy);
        // Exact for every shape and pattern too.
        for shape in TrussShape::ALL {
            for pattern in TrussPattern::ALL {
                (t.shape, t.pattern, t.copies, t.panels) = (shape, pattern, 2, 6);
                assert_eq!(t.layout().len(), 2 * t.members_per_copy(), "{shape:?} {pattern:?}");
            }
        }
    }

    #[test]
    fn a_far_base_offset_is_refused_and_repaired() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        t.base_offset = 1e300;
        assert!(t.check().is_err());
        assert_eq!(t.reset_out_of_range(), 1);
        assert_eq!(t.base_offset, 0.0);
    }

    #[test]
    fn a_flat_truss_has_posts_at_both_supports() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        for pattern in [TrussPattern::Pratt, TrussPattern::Howe, TrussPattern::Warren] {
            for panels in [3, 4, 5] {
                (t.pattern, t.panels) = (pattern, panels);
                let posts: Vec<u16> = t
                    .layout()
                    .iter()
                    .filter(|(r, _, _)| r.kind == TrussRoleKind::Vertical)
                    .map(|(r, _, _)| r.index)
                    .collect();
                let n = u16::try_from(panels).expect("small");
                assert!(posts.contains(&0) && posts.contains(&n), "{pattern:?} {panels}: {posts:?}");
            }
        }
    }

    fn member(t: &Truss, kind: TrussRoleKind, copy: u16, index: u16) -> ElementId {
        let role = TrussRole { kind, copy, index };
        t.members.iter().find(|m| m.role == role).map(|m| m.id).expect("member")
    }

    fn set(doc: &mut Document, host: ElementId, f: impl FnOnce(&mut Truss)) {
        doc.edit("set", |e| {
            if let Some(ElementKind::Truss(t)) = e.element_mut(host).map(|x| &mut x.kind) {
                f(t);
            }
        });
    }

    #[test]
    fn a_new_truss_gets_its_members() {
        let (doc, host) = site_with(|_| {});
        let t = truss(&doc, host);
        assert_eq!(t.members.len(), 17);
        let level = doc.project.element(host).map(|e| e.level);
        for m in &t.members {
            let e = doc.project.element(m.id).expect("the member exists");
            assert!(m.role.fits(&e.kind));
            assert_eq!(Some(e.level), level);
        }
        assert_eq!(doc.project.clone().repair(), 0, "a consistent model");
    }

    #[test]
    fn new_panels_keep_the_ids_of_the_roles_that_stay_and_undo_in_one_step() {
        let (mut doc, host) = site_with(|_| {});
        let before = truss(&doc, host);
        let start = doc.project.elements.clone();
        let first = member(&before, TrussRoleKind::BottomChord, 0, 0);
        set(&mut doc, host, |t| t.panels = 6);
        let after = truss(&doc, host);
        assert_eq!(after.members.len(), 12 + 7 + 6);
        assert_eq!(member(&after, TrussRoleKind::BottomChord, 0, 0), first, "same id");
        let Some(ElementKind::Beam(b)) = doc.project.element(first).map(|e| &e.kind) else { panic!("a beam") };
        assert!(close(b.end, DVec2::new(1.0, 0.0)), "now a 1 m panel");
        assert!(doc.undo());
        assert_eq!(truss(&doc, host), before);
        assert_eq!(doc.project.elements, start);
    }

    #[test]
    fn a_detached_member_is_left_alone() {
        let (mut doc, host) = site_with(|_| {});
        let post = member(&truss(&doc, host), TrussRoleKind::Vertical, 0, 2);
        doc.edit("move", |e| e.transform_elements(&[post], &PlanTransform::translate(DVec2::new(0.0, 1.0))));
        let t = truss(&doc, host);
        assert!(t.detached.contains(&TrussRole { kind: TrussRoleKind::Vertical, copy: 0, index: 2 }));
        set(&mut doc, host, |t| t.height = 2.0);
        let Some(ElementKind::Column(c)) = doc.project.element(post).map(|e| &e.kind) else { panic!("a column") };
        assert!(close(c.position, DVec2::new(3.0, 1.0)) && (c.height - 1.0).abs() < 1e-9, "where the user left it");
        assert_eq!(truss(&doc, host).members.len(), 16, "and not made again");
    }

    #[test]
    fn deleting_the_truss_deletes_its_members() {
        let (mut doc, host) = site_with(|t| t.copies = 2);
        let ids = doc.project.host_members_of(host);
        assert_eq!(ids.len(), 34);
        doc.edit("delete", |e| e.remove_element(host));
        assert!(ids.iter().all(|id| doc.project.element(*id).is_none()));
        assert!(doc.undo());
        assert!(ids.iter().all(|id| doc.project.element(*id).is_some()));
    }

    #[test]
    fn a_copy_gets_its_own_members() {
        let (mut doc, host) = site_with(|_| {});
        let original = truss(&doc, host);
        let ids: std::collections::BTreeSet<ElementId> = original.members.iter().map(|m| m.id).collect();
        let mut selection = vec![host];
        selection.extend(ids.iter().copied());
        let before = doc.project.elements.len();
        let copies =
            doc.edit("copy", |e| e.copy_elements(&selection, &[PlanTransform::translate(DVec2::new(0.0, 10.0))]));
        assert_eq!(copies.len(), 1, "only the truss is copied");
        let copy = truss(&doc, copies[0]);
        assert_eq!(copy.members.len(), 17);
        assert!(copy.members.iter().all(|m| !ids.contains(&m.id)));
        assert_eq!(doc.project.elements.len(), before + 18);
        assert_eq!(truss(&doc, host), original);
        assert!(doc.undo());
        assert_eq!(doc.project.elements.len(), before);
    }

    #[test]
    fn a_changed_post_type_regenerates() {
        let (mut doc, host) = site_with(|_| {});
        let t = truss(&doc, host);
        let chord = member(&t, TrussRoleKind::TopChord, 0, 0);
        doc.edit("type", |e| {
            // Any edit of a type the truss uses rewrites its members.
            if let Some(ty) = e.type_mut(t.post_type) {
                ty.name = "Renamed".into();
            }
        });
        assert_eq!(member(&truss(&doc, host), TrussRoleKind::TopChord, 0, 0), chord, "same ids");
    }

    #[test]
    fn repair_resets_bad_values_and_drops_dangling_members() {
        let (doc, host) = site_with(|_| {});
        let t = truss(&doc, host);
        let mut p = doc.project.clone();
        p.elements.remove(&member(&t, TrussRoleKind::Diagonal, 0, 0));
        if let Some(ElementKind::Truss(x)) = p.elements.get_mut(&host).map(|e| &mut e.kind) {
            (x.shape, x.panels) = (TrussShape::Gable, 5);
            x.post_type = x.web_type;
        }
        // Odd gable count, the missing member, the wrong post type.
        assert_eq!(p.repair(), 3);
        let Some(ElementKind::Truss(x)) = p.element(host).map(|e| &e.kind) else { panic!("kept") };
        assert_eq!(x.panels, 6);
        assert_eq!(p.element_type(x.post_type).map(|t| t.category()), Some(Category::Column));
        assert_eq!(p.repair(), 0);
    }
}
