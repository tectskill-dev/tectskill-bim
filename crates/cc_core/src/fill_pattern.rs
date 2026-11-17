//! The project's own fill patterns (hatches): families of parallel, possibly
//! dashed lines, as in the plain-text `.pat` format CAD programs share (see
//! [`crate::pat`]). Materials, filled region types and view overrides refer
//! to one with [`Pattern::Custom`].
//!
//! Lengths are millimeters: on paper for drafting patterns, which keep their
//! printed size at any view scale, and in the real world for model patterns
//! (tiles, brick courses), which scale with the view. A pattern's `scale`
//! multiplies every length.

use crate::{
    ElementTypeId, FillPatternId, GraphicOverride, Material, Pattern, PatternFill, PhaseGraphics, Project, TemplateId,
    TypeData, ViewGraphics, ViewId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most custom patterns in a project (a crafted file).
pub const MAX_FILL_PATTERNS: usize = 1000;
/// Most line families in one pattern.
pub const MAX_FAMILIES: usize = 128;
/// Most dash values in one family.
pub const MAX_DASHES: usize = 32;
/// Longest pattern name, characters.
pub const MAX_NAME: usize = 100;
/// Longest description, characters.
pub const MAX_DESCRIPTION: usize = 200;
/// Smallest distance between the lines of a family, millimeters (before the scale).
pub const MIN_OFFSET: f64 = 0.01;
/// Shortest repeat of a dash list, millimeters (before the scale).
pub const MIN_PERIOD: f64 = 0.01;
/// Largest length or coordinate, millimeters.
pub const MAX_LENGTH: f64 = 100_000.0;
/// Range of a pattern's scale.
pub const MIN_SCALE: f64 = 0.01;
pub const MAX_SCALE: f64 = 100.0;

/// How a pattern's lengths are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FillPatternKind {
    /// Millimeters on paper: the same printed size at any view scale.
    #[default]
    Drafting,
    /// Millimeters in the real world: scales with the view.
    Model,
}

impl FillPatternKind {
    pub fn key(self) -> &'static str {
        match self {
            FillPatternKind::Drafting => "fill-pattern-kind-drafting",
            FillPatternKind::Model => "fill-pattern-kind-model",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            FillPatternKind::Drafting => "drafting",
            FillPatternKind::Model => "model",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "drafting" => Some(FillPatternKind::Drafting),
            "model" => Some(FillPatternKind::Model),
            _ => None,
        }
    }
}

/// One family of parallel lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternLine {
    /// Direction of the lines, degrees counterclockwise from +x.
    pub angle: f64,
    /// A point the first line passes through (where its dashes start), mm.
    #[serde(default)]
    pub origin: [f64; 2],
    /// From one line to the next, in the line's own axes: along it (which
    /// staggers the dashes) and across it (the spacing), mm.
    pub offset: [f64; 2],
    /// Dash lengths in turn, mm: positive drawn, negative a gap, zero a
    /// dot. Empty: continuous lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dashes: Vec<f64>,
}

impl PatternLine {
    /// Continuous lines at `angle`, `spacing` apart.
    pub fn solid(angle: f64, spacing: f64) -> Self {
        Self { angle, origin: [0.0, 0.0], offset: [0.0, spacing], dashes: Vec::new() }
    }

    /// The length of one repeat of the dashes (0 for continuous lines).
    pub fn period(&self) -> f64 {
        self.dashes.iter().map(|d| d.abs()).sum()
    }

    /// Why the family cannot be drawn, if it cannot.
    pub fn problem(&self) -> Option<String> {
        let ok = |v: f64| v.is_finite() && v.abs() <= MAX_LENGTH;
        if !self.angle.is_finite() || self.angle.abs() > 1e6 {
            return Some("the angle is not a number".into());
        }
        if !self.origin.iter().chain(&self.offset).all(|v| ok(*v)) {
            return Some(format!("origin and offset must be numbers up to {MAX_LENGTH} mm"));
        }
        if self.offset[1].abs() < MIN_OFFSET {
            return Some(format!("the lines must be at least {MIN_OFFSET} mm apart"));
        }
        if self.dashes.len() > MAX_DASHES {
            return Some(format!("a family has at most {MAX_DASHES} dashes"));
        }
        if !self.dashes.iter().all(|d| ok(*d)) {
            return Some(format!("dashes must be numbers up to {MAX_LENGTH} mm"));
        }
        if !self.dashes.is_empty() && self.period() < MIN_PERIOD {
            return Some(format!("the dashes must repeat over at least {MIN_PERIOD} mm"));
        }
        None
    }
}

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

/// A custom fill pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillPattern {
    pub id: FillPatternId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub kind: FillPatternKind,
    /// Multiplies every length of the families.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: f64,
    pub lines: Vec<PatternLine>,
}

/// The project's fill patterns, by id.
pub type FillPatterns = BTreeMap<FillPatternId, FillPattern>;

impl FillPattern {
    /// Why the pattern cannot be kept as it is, if it cannot.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() || self.name.chars().count() > MAX_NAME {
            return Some(format!("a fill pattern needs a name of 1 to {MAX_NAME} characters"));
        }
        if self.description.chars().count() > MAX_DESCRIPTION {
            return Some(format!("the description is longer than {MAX_DESCRIPTION} characters"));
        }
        if !(self.scale.is_finite() && (MIN_SCALE..=MAX_SCALE).contains(&self.scale)) {
            return Some(format!("the scale must be between {MIN_SCALE} and {MAX_SCALE}"));
        }
        if self.lines.is_empty() {
            return Some("a fill pattern needs at least one line family".into());
        }
        if self.lines.len() > MAX_FAMILIES {
            return Some(format!("a fill pattern has at most {MAX_FAMILIES} line families"));
        }
        self.lines.iter().enumerate().find_map(|(i, l)| l.problem().map(|p| format!("line family {}: {p}", i + 1)))
    }

    /// Fix what a crafted file may hold: names and descriptions within
    /// their limits, a valid scale, families that can be drawn and not too
    /// many. Returns how many things it changed.
    pub fn repair(&mut self) -> usize {
        let mut n = 0;
        if self.name.trim().is_empty() {
            self.name = format!("Pattern {}", self.id.0);
            n += 1;
        }
        if self.name.chars().count() > MAX_NAME {
            self.name = self.name.chars().take(MAX_NAME).collect();
            n += 1;
        }
        if self.description.chars().count() > MAX_DESCRIPTION {
            self.description = self.description.chars().take(MAX_DESCRIPTION).collect();
            n += 1;
        }
        if !(self.scale.is_finite() && (MIN_SCALE..=MAX_SCALE).contains(&self.scale)) {
            self.scale = 1.0;
            n += 1;
        }
        if self.lines.len() > MAX_FAMILIES {
            self.lines.truncate(MAX_FAMILIES);
            n += 1;
        }
        let before = self.lines.len();
        self.lines.retain(|l| l.problem().is_none());
        n + before - self.lines.len()
    }

    /// The pattern as a `.pat` file entry (see [`crate::pat::write`]).
    pub fn to_pat(&self) -> String {
        crate::pat::write(std::slice::from_ref(self))
    }
}

/// A place that holds a pattern: a fill's own (where `Pattern::None` means
/// nothing is drawn) or a view override's (where `None` keeps what the view
/// draws).
pub enum PatternSlot<'a> {
    Fill(&'a mut Pattern),
    Override(&'a mut Option<Pattern>),
}

impl PatternSlot<'_> {
    pub fn get(&self) -> Option<Pattern> {
        match self {
            PatternSlot::Fill(p) => Some(**p),
            PatternSlot::Override(p) => **p,
        }
    }

    /// Put `with` in place of the pattern.
    pub fn set(&mut self, with: Pattern) {
        match self {
            PatternSlot::Fill(p) => **p = with,
            PatternSlot::Override(p) => **p = Some(with),
        }
    }

    /// Forget the pattern: no lines in a fill, no override in a view.
    pub fn clear(&mut self) {
        match self {
            PatternSlot::Fill(p) => **p = Pattern::None,
            PatternSlot::Override(p) => **p = None,
        }
    }
}

/// Visits every pattern of a fill.
pub fn visit_fill(f: &mut PatternFill, v: &mut dyn FnMut(PatternSlot)) {
    v(PatternSlot::Fill(&mut f.foreground));
    v(PatternSlot::Fill(&mut f.background));
}

/// Visits the patterns of a material (surface and cut).
pub fn visit_material(m: &mut Material, v: &mut dyn FnMut(PatternSlot)) {
    visit_fill(&mut m.graphics.surface, v);
    visit_fill(&mut m.graphics.cut, v);
}

/// Visits the patterns of a type: a filled region's fill, the hatches of a
/// component's 2D pieces.
pub fn visit_type(t: &mut TypeData, v: &mut dyn FnMut(PatternSlot)) {
    match t {
        TypeData::FilledRegion { fill } => visit_fill(fill, v),
        TypeData::Component { def, .. } => {
            for item in &mut def.sheet {
                if let crate::title_block::SheetShape::Stroke { hatch: Some(fill), .. } = &mut item.shape {
                    visit_fill(fill, v);
                }
            }
        }
        _ => {}
    }
}

/// Visits the fill patterns of an override.
pub fn visit_override(o: &mut GraphicOverride, v: &mut dyn FnMut(PatternSlot)) {
    v(PatternSlot::Override(&mut o.surface_fill.pattern));
    v(PatternSlot::Override(&mut o.cut_fill.pattern));
}

/// Visits the patterns of a view's (or template's) graphics.
pub fn visit_view_graphics(g: &mut ViewGraphics, v: &mut dyn FnMut(PatternSlot)) {
    for c in g.categories.values_mut() {
        visit_override(&mut c.overrides, v);
    }
    for f in &mut g.filters {
        visit_override(&mut f.overrides, v);
    }
    for o in g.elements.values_mut() {
        visit_override(o, v);
    }
}

/// Visits the patterns of the phase graphics.
pub fn visit_phase_graphics(g: &mut PhaseGraphics, v: &mut dyn FnMut(PatternSlot)) {
    for o in [&mut g.existing, &mut g.demolished, &mut g.new, &mut g.temporary] {
        visit_override(o, v);
    }
}

/// Whether anything `visit` reaches holds `pattern` (on a copy).
type Visit<T> = fn(&mut T, &mut dyn FnMut(PatternSlot));

fn holds<T: Clone>(x: &T, pattern: Pattern, visit: Visit<T>) -> bool {
    let mut copy = x.clone();
    let mut found = false;
    visit(&mut copy, &mut |s| found |= s.get() == Some(pattern));
    found
}

/// What uses a custom pattern.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatternUsage {
    pub materials: Vec<String>,
    pub types: Vec<ElementTypeId>,
    pub views: Vec<ViewId>,
    pub templates: Vec<TemplateId>,
    pub phase_graphics: bool,
}

impl PatternUsage {
    pub fn count(&self) -> usize {
        self.materials.len()
            + self.types.len()
            + self.views.len()
            + self.templates.len()
            + usize::from(self.phase_graphics)
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }
}

/// A type's data worth copying to look for patterns (not every type).
fn may_hold_patterns(t: &TypeData) -> bool {
    match t {
        TypeData::FilledRegion { .. } => true,
        TypeData::Component { def, .. } => !def.sheet.is_empty(),
        _ => false,
    }
}

impl Project {
    /// The custom pattern `id`.
    pub fn fill_pattern(&self, id: FillPatternId) -> Option<&FillPattern> {
        self.fill_patterns.get(&id)
    }

    /// The custom pattern called `name` (ignoring case).
    pub fn fill_pattern_named(&self, name: &str) -> Option<&FillPattern> {
        let name = name.trim();
        self.fill_patterns.values().find(|p| p.name.trim().eq_ignore_ascii_case(name))
    }

    /// Whether `pattern` can be drawn here: a built-in, or a custom pattern
    /// the project has.
    pub fn has_pattern(&self, pattern: Pattern) -> bool {
        match pattern {
            Pattern::Custom(id) => self.fill_patterns.contains_key(&id),
            _ => true,
        }
    }

    /// Everything that uses the custom pattern `id`.
    pub fn fill_pattern_usage(&self, id: FillPatternId) -> PatternUsage {
        self.pattern_usage(Pattern::Custom(id))
    }

    /// Everything that uses `p` (a built-in or custom pattern).
    pub fn pattern_usage(&self, p: Pattern) -> PatternUsage {
        PatternUsage {
            materials: self.materials.iter().filter(|m| holds(*m, p, visit_material)).map(|m| m.name.clone()).collect(),
            types: self
                .types
                .values()
                .filter(|t| may_hold_patterns(&t.data) && holds(&t.data, p, visit_type))
                .map(|t| t.id)
                .collect(),
            views: self.views.values().filter(|v| holds(&v.graphics, p, visit_view_graphics)).map(|v| v.id).collect(),
            templates: self
                .templates
                .values()
                .filter(|t| holds(&t.graphics, p, visit_view_graphics))
                .map(|t| t.id)
                .collect(),
            phase_graphics: holds(&self.phase_graphics, p, visit_phase_graphics),
        }
    }

    /// Visits every pattern the project holds (materials, types, views,
    /// templates and phase graphics).
    pub fn visit_patterns(&mut self, v: &mut dyn FnMut(PatternSlot)) {
        for m in &mut self.materials {
            visit_material(m, v);
        }
        for t in self.types.values_mut() {
            visit_type(&mut t.data, v);
        }
        for view in self.views.values_mut() {
            visit_view_graphics(&mut view.graphics, v);
        }
        for t in self.templates.values_mut() {
            visit_view_graphics(&mut t.graphics, v);
        }
        visit_phase_graphics(&mut self.phase_graphics, v);
    }

    /// See [`crate::fill_pattern`]: ids that match their keys, patterns
    /// within their limits, a bounded count, and references to patterns
    /// that are gone dropped. Returns how many things it changed.
    pub(crate) fn repair_fill_patterns(&mut self) -> usize {
        let mut repairs = 0;
        if self.fill_patterns.len() > MAX_FILL_PATTERNS {
            repairs += self.fill_patterns.len() - MAX_FILL_PATTERNS;
            let keep: Vec<_> = self.fill_patterns.keys().take(MAX_FILL_PATTERNS).copied().collect();
            self.fill_patterns.retain(|k, _| keep.contains(k));
        }
        for (id, p) in self.fill_patterns.iter_mut() {
            if p.id != *id {
                p.id = *id;
                repairs += 1;
            }
            repairs += p.repair();
        }
        let known: std::collections::BTreeSet<FillPatternId> = self.fill_patterns.keys().copied().collect();
        self.visit_patterns(&mut |mut slot| {
            if let Some(Pattern::Custom(id)) = slot.get()
                && !known.contains(&id)
            {
                slot.clear();
                repairs += 1;
            }
        });
        repairs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(id: u64) -> FillPattern {
        FillPattern {
            id: FillPatternId(id),
            name: "Tiles".into(),
            description: String::new(),
            kind: FillPatternKind::Model,
            scale: 1.0,
            lines: vec![PatternLine::solid(0.0, 300.0), PatternLine::solid(90.0, 300.0)],
        }
    }

    #[test]
    fn problems_are_found_and_repaired() {
        let mut p = pattern(3);
        assert_eq!(p.problem(), None);
        p.lines.push(PatternLine::solid(45.0, 0.0));
        p.lines.push(PatternLine { dashes: vec![f64::NAN], ..PatternLine::solid(0.0, 1.0) });
        p.lines.push(PatternLine { dashes: vec![0.0, 0.0], ..PatternLine::solid(0.0, 1.0) });
        p.scale = f64::INFINITY;
        p.name = "  ".into();
        assert!(p.problem().is_some());
        assert_eq!(p.repair(), 5);
        assert_eq!(p.problem(), None);
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.name, "Pattern 3");
    }

    #[test]
    fn missing_patterns_are_cleared_from_materials_and_overrides() {
        let mut project = Project::from_template();
        let id = FillPatternId(project.max_id() + 1);
        project.fill_patterns.insert(id, pattern(id.0));
        let gone = Pattern::Custom(FillPatternId(id.0 + 1));
        project.materials[0].graphics.cut.foreground = Pattern::Custom(id);
        project.materials[0].graphics.surface.foreground = gone;
        project.phase_graphics.new.cut_fill.pattern = Some(gone);
        let usage = project.fill_pattern_usage(id);
        assert_eq!(usage.materials, vec![project.materials[0].name.clone()]);
        assert_eq!(project.repair_fill_patterns(), 2);
        assert_eq!(project.materials[0].graphics.surface.foreground, Pattern::None);
        assert_eq!(project.materials[0].graphics.cut.foreground, Pattern::Custom(id));
        assert_eq!(project.phase_graphics.new.cut_fill.pattern, None);
        assert_eq!(project.repair_fill_patterns(), 0);
    }
}
