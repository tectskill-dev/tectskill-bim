//! Bar marks: the mark of every bar of every reinforcement set (see
//! [`cc_core::bar_marks`]), resolved by a pure, deterministic function of
//! the project and its bars.
//!
//! A mark names one shape: the same diameter, steel grade and
//! [`ShapeSummary`] within the project's length tolerance and
//! [`ANGLE_TOLERANCE_DEG`] (read from either end, mirrored, a closed bar
//! from any corner). In order:
//! 1. a set with a `bar_mark` override gives all its bars that mark (bars of
//!    different shapes make one [`Mark`] each, of the same text);
//! 2. a bar whose shape matches an entry of the frozen marks table takes the
//!    first such entry's mark;
//! 3. the shapes left are numbered with the prefix after the highest frozen
//!    number (from `start`, skipping marks already used), in the stable order
//!    partition, diameter descending, length descending, shape; a bar joins
//!    the first numbered shape it matches.
//!
//! The marks come sorted naturally (N2 before N10), each with its count,
//! total length (the exact sum of its bars) and weight.

use crate::bar_shape::{LegSummary, ShapeSummary, summary};
use crate::rebar::RebarGeom;
use cc_core::bar_marks::{ANGLE_TOLERANCE_DEG, BarMarking, MAX_MARK_LEGS, MarkLeg, MarkShape};
use cc_core::{ElementId, ElementKind, Project};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// Where a mark comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkSource {
    /// An entry of the frozen marks table (its index).
    Frozen(usize),
    /// A set's `bar_mark` override.
    Override,
    /// Numbered for a shape no frozen entry holds.
    Numbered,
}

/// One mark: a text naming bars of one diameter, grade and shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub mark: String,
    pub diameter: f64,
    pub grade: String,
    /// The shape, in its canonical form (see [`canonical`]).
    pub shape: ShapeSummary,
    pub source: MarkSource,
    /// The first bar of the mark (set, index among its bars); none for a
    /// frozen entry no bar uses.
    pub first: Option<(ElementId, usize)>,
    /// How many bars have the mark.
    pub count: usize,
    /// Their lengths added up, meters.
    pub total_length: f64,
    /// Their weight, kilograms (each set's mass per meter).
    pub weight: f64,
    /// The longest of them, meters.
    pub longest: f64,
    /// The sets with bars of the mark, in id order.
    pub sets: Vec<ElementId>,
}

impl Mark {
    /// Mean length of one bar (the shape's when no bar uses the mark).
    pub fn unit_length(&self) -> f64 {
        if self.count == 0 { self.shape.length() } else { self.total_length / self.count as f64 }
    }

    pub fn frozen(&self) -> bool {
        matches!(self.source, MarkSource::Frozen(_))
    }
}

/// The marks of a project's bars.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BarMarks {
    /// Sorted naturally by mark, then diameter (largest first).
    pub marks: Vec<Mark>,
    /// Per set: the index in `marks` of each of its bars, in the bars' order.
    pub bars: BTreeMap<ElementId, Vec<usize>>,
}

impl BarMarks {
    /// The mark of bar `index` of set `set`.
    pub fn of_bar(&self, set: ElementId, index: usize) -> Option<&Mark> {
        self.bars.get(&set)?.get(index).and_then(|m| self.marks.get(*m))
    }

    /// The marks of set `set`'s bars, each once, in the order its bars first have them.
    pub fn of_set(&self, set: ElementId) -> Vec<&Mark> {
        let mut seen = BTreeSet::new();
        self.bars
            .get(&set)
            .into_iter()
            .flatten()
            .filter(|m| seen.insert(**m))
            .filter_map(|m| self.marks.get(*m))
            .collect()
    }
}

/// The marks of `rebar` (the model's bars, `ModelGeometry::rebar`) with the
/// project's marking and the sets' overrides.
pub fn resolve(project: &Project, rebar: &BTreeMap<ElementId, RebarGeom>) -> BarMarks {
    resolve_with(project, rebar, &project.bar_marking, true)
}

/// Natural order of marks: runs of digits compare as numbers (N2 before N10).
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        s.push(c);
                    }
                    s
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let order = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

/// The turn tolerance, radians.
fn angle_tol() -> f64 {
    ANGLE_TOLERANCE_DEG.to_radians()
}

/// A shape's legs as numbers to order by: lengths and radii to 0.1 mm,
/// turns to 0.1 degree, hooks.
fn key(s: &ShapeSummary) -> Vec<i64> {
    let q = |v: f64, k: f64| if v.is_finite() { (v * k).round() as i64 } else { 0 };
    let mut out = vec![i64::from(s.closed)];
    for l in &s.legs {
        out.extend([
            q(l.length, 1e4),
            l.radius.map_or(-1, |r| q(r, 1e4)),
            q(l.sweep.to_degrees(), 10.0),
            q(l.bend.to_degrees(), 10.0),
            i64::from(l.hook),
        ]);
    }
    out
}

/// The canonical reading of a shape: of the readings from either end,
/// mirrored and (closed bars of up to 64 legs) from any corner, the one with
/// the least [`key`]. Shapes that match read the same, up to the tolerance.
pub fn canonical(s: &ShapeSummary) -> ShapeSummary {
    let n = s.legs.len();
    let shifts = if s.closed && n <= 64 { n.max(1) } else { 1 };
    let mut best: Option<(Vec<i64>, ShapeSummary)> = None;
    for o in [s.clone(), s.reversed()] {
        for k in 0..shifts {
            for sign in [1.0, -1.0] {
                let legs = (0..n)
                    .filter_map(|i| o.legs.get((i + k) % n.max(1)))
                    .map(|l| LegSummary { sweep: sign * l.sweep, bend: sign * l.bend, ..*l })
                    .collect();
                let c = ShapeSummary { closed: s.closed, legs };
                let kc = key(&c);
                if best.as_ref().is_none_or(|(kb, _)| kc < *kb) {
                    best = Some((kc, c));
                }
            }
        }
    }
    best.map_or_else(|| s.clone(), |(_, c)| c)
}

/// A shape as the frozen marks table keeps it (turns in degrees); none for
/// shapes of more than [`MAX_MARK_LEGS`] legs or that are not usable.
pub fn to_mark_shape(s: &ShapeSummary) -> Option<MarkShape> {
    let legs = s
        .legs
        .iter()
        .map(|l| MarkLeg {
            length: l.length,
            radius: l.radius,
            sweep: l.sweep.to_degrees(),
            bend: l.bend.to_degrees(),
            hook: l.hook,
        })
        .collect();
    Some(MarkShape { closed: s.closed, legs }).filter(|m| m.legs.len() <= MAX_MARK_LEGS && m.is_valid())
}

/// The summary of a frozen shape.
pub fn from_mark_shape(m: &MarkShape) -> ShapeSummary {
    let legs = m
        .legs
        .iter()
        .map(|l| LegSummary {
            length: l.length,
            radius: l.radius,
            sweep: l.sweep.to_radians(),
            bend: l.bend.to_radians(),
            hook: l.hook,
        })
        .collect();
    ShapeSummary { closed: m.closed, legs }
}

/// Whether bars of these diameters, grades and shapes are one mark.
fn same(a: (f64, &str, &ShapeSummary), b: (f64, &str, &ShapeSummary), tol: f64) -> bool {
    let (da, ga, sa) = a;
    let (db, gb, sb) = b;
    (da - db).abs() <= 1e-9
        && ga == gb
        && sa.closed == sb.closed
        && sa.legs.len() == sb.legs.len()
        && (sa.length() - sb.length()).abs() <= tol * sa.legs.len() as f64 + 1e-9
        && sa.matches(sb, tol + 1e-9, angle_tol())
}

/// One bar waiting for a mark.
struct Pending<'a> {
    set: ElementId,
    index: usize,
    partition: &'a str,
    diameter: f64,
    grade: &'a str,
    length: f64,
    shape: ShapeSummary,
}

/// [`resolve`] with `marking` instead of the project's, and the sets'
/// overrides left out unless `overrides`.
pub fn resolve_with(
    project: &Project,
    rebar: &BTreeMap<ElementId, RebarGeom>,
    marking: &BarMarking,
    overrides: bool,
) -> BarMarks {
    let tol = if cc_core::bar_marks::tolerance_ok(marking.length_tolerance) {
        marking.length_tolerance
    } else {
        cc_core::bar_marks::DEFAULT_LENGTH_TOLERANCE
    };
    let mut marks: Vec<Mark> = Vec::new();
    let new_mark = |mark: String, set: &RebarGeom, shape: ShapeSummary, source: MarkSource| Mark {
        mark,
        diameter: set.diameter,
        grade: set.grade.clone(),
        shape,
        source,
        first: None,
        count: 0,
        total_length: 0.0,
        weight: 0.0,
        longest: 0.0,
        sets: Vec::new(),
    };
    // The frozen entries, in table order (usable ones).
    let frozen: Vec<(usize, ShapeSummary)> = marking
        .frozen
        .iter()
        .enumerate()
        .filter(|(_, f)| f.is_valid())
        .map(|(i, f)| (i, from_mark_shape(&f.shape)))
        .collect();
    let mut frozen_mark: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, shape) in &frozen {
        let Some(f) = marking.frozen.get(*i) else { continue };
        frozen_mark.insert(*i, marks.len());
        marks.push(Mark {
            mark: f.mark.clone(),
            diameter: f.diameter,
            grade: f.grade.clone(),
            shape: shape.clone(),
            source: MarkSource::Frozen(*i),
            first: None,
            count: 0,
            total_length: 0.0,
            weight: 0.0,
            longest: 0.0,
            sets: Vec::new(),
        });
    }
    // Marks taken: numbering skips them.
    let mut used: BTreeSet<String> = marking.frozen.iter().map(|f| f.mark.clone()).collect();
    let mut assigned: BTreeMap<ElementId, Vec<Option<usize>>> = BTreeMap::new();
    let mut pending: Vec<Pending> = Vec::new();
    for (id, set) in rebar {
        let element = project.element(*id);
        let (partition, bar_mark) = match element.map(|e| &e.kind) {
            Some(ElementKind::Rebar(r)) => (r.partition.as_str(), r.bar_mark.as_deref()),
            Some(ElementKind::ShapedRebar(r)) => (r.partition.as_str(), r.bar_mark.as_deref()),
            _ => ("", None),
        };
        let bar_mark = bar_mark.map(str::trim).filter(|m| overrides && !m.is_empty());
        let slots = assigned.entry(*id).or_insert_with(|| vec![None; set.bars.len()]);
        for (i, bar) in set.bars.iter().enumerate() {
            let shape = summary(bar);
            let this = (set.diameter, set.grade.as_str(), &shape);
            let found = if let Some(text) = bar_mark {
                used.insert(text.to_string());
                let existing = marks.iter().position(|m| {
                    m.source == MarkSource::Override
                        && m.mark == text
                        && same((m.diameter, &m.grade, &m.shape), this, tol)
                });
                Some(existing.unwrap_or_else(|| {
                    marks.push(new_mark(text.to_string(), set, canonical(&shape), MarkSource::Override));
                    marks.len() - 1
                }))
            } else {
                frozen
                    .iter()
                    .find(|(i, f)| marking.frozen.get(*i).is_some_and(|e| same((e.diameter, &e.grade, f), this, tol)))
                    .and_then(|(i, _)| frozen_mark.get(i).copied())
            };
            match found {
                Some(m) => {
                    if let Some(slot) = slots.get_mut(i) {
                        *slot = Some(m);
                    }
                }
                None => pending.push(Pending {
                    set: *id,
                    index: i,
                    partition,
                    diameter: set.diameter,
                    grade: set.grade.as_str(),
                    length: bar.length(),
                    shape,
                }),
            }
        }
    }
    // The shapes left, numbered in the stable order.
    let mut keyed: Vec<(Vec<i64>, Pending)> = pending.into_iter().map(|p| (key(&canonical(&p.shape)), p)).collect();
    keyed.sort_by(|(ka, a), (kb, b)| {
        a.partition
            .cmp(b.partition)
            .then(b.diameter.total_cmp(&a.diameter))
            .then(b.length.total_cmp(&a.length))
            .then_with(|| ka.cmp(kb))
            .then(a.grade.cmp(b.grade))
            .then(a.set.cmp(&b.set))
            .then(a.index.cmp(&b.index))
    });
    let highest = marking.frozen.iter().filter_map(|f| marking.number_of(&f.mark)).max();
    let mut next = highest.map_or(marking.start, |h| h.saturating_add(1).max(marking.start));
    let numbered_from = marks.len();
    for (_, p) in keyed {
        let this = (p.diameter, p.grade, &p.shape);
        let found = marks
            .get(numbered_from..)
            .and_then(|ms| ms.iter().position(|m| same((m.diameter, &m.grade, &m.shape), this, tol)))
            .map(|k| numbered_from + k);
        let m = match found {
            Some(m) => m,
            None => {
                while used.contains(&marking.mark_of(next)) && next < u32::MAX {
                    next += 1;
                }
                let text = marking.mark_of(next);
                used.insert(text.clone());
                next = next.saturating_add(1);
                let Some(set) = rebar.get(&p.set) else { continue };
                marks.push(new_mark(text, set, canonical(&p.shape), MarkSource::Numbered));
                marks.len() - 1
            }
        };
        if let Some(slot) = assigned.get_mut(&p.set).and_then(|s| s.get_mut(p.index)) {
            *slot = Some(m);
        }
    }
    // Tallies, set by set in id order.
    for (id, slots) in &assigned {
        let Some(set) = rebar.get(id) else { continue };
        let mass = set.mass_per_meter();
        for (i, (slot, bar)) in slots.iter().zip(&set.bars).enumerate() {
            let Some(m) = slot.and_then(|m| marks.get_mut(m)) else { continue };
            let length = bar.length();
            m.first.get_or_insert((*id, i));
            m.count += 1;
            m.total_length += length;
            m.weight += length * mass;
            m.longest = m.longest.max(length);
            if m.sets.last() != Some(id) {
                m.sets.push(*id);
            }
        }
    }
    // Sorted naturally; the bars point at the new places.
    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by(|&a, &b| match (marks.get(a), marks.get(b)) {
        (Some(x), Some(y)) => natural(&x.mark, &y.mark)
            .then(y.diameter.total_cmp(&x.diameter))
            .then_with(|| key(&x.shape).cmp(&key(&y.shape)))
            .then(a.cmp(&b)),
        _ => a.cmp(&b),
    });
    let mut place = vec![0; marks.len()];
    for (new, old) in order.iter().enumerate() {
        if let Some(p) = place.get_mut(*old) {
            *p = new;
        }
    }
    let mut slots: Vec<Option<Mark>> = marks.into_iter().map(Some).collect();
    let marks: Vec<Mark> = order.iter().filter_map(|i| slots.get_mut(*i).and_then(Option::take)).collect();
    let bars = assigned
        .into_iter()
        .map(|(id, s)| (id, s.into_iter().map(|m| m.and_then(|m| place.get(m).copied()).unwrap_or(0)).collect()))
        .collect();
    BarMarks { marks, bars }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebar::Bar;
    use glam::DVec3;

    fn set(id: u64, diameter: f64, bars: Vec<Bar>) -> (ElementId, RebarGeom) {
        let g = RebarGeom {
            element: ElementId(id),
            host: None,
            diameter,
            grade: "CA-50".into(),
            mass_per_meter: None,
            bars,
        };
        (ElementId(id), g)
    }

    fn straight(length: f64, y: f64) -> Bar {
        Bar::new(vec![DVec3::new(0.0, y, 0.0), DVec3::new(length, y, 0.0)], false)
    }

    fn ell(a: f64, b: f64) -> Bar {
        Bar::new(vec![DVec3::new(0.0, 0.0, a), DVec3::ZERO, DVec3::new(b, 0.0, 0.0)], false)
    }

    #[test]
    fn natural_order_reads_numbers() {
        let mut v = vec!["N10", "N2", "N1", "A", "N02"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["A", "N1", "N2", "N02", "N10"]);
    }

    #[test]
    fn shapes_are_numbered_in_the_stable_order() {
        let p = Project::empty();
        // Longer bars first, larger diameters first.
        let rebar: BTreeMap<_, _> = [
            set(1, 0.01, vec![straight(3.0, 0.0), straight(3.002, 1.0), ell(0.3, 4.0)]),
            set(2, 0.0125, vec![straight(2.0, 0.0)]),
        ]
        .into_iter()
        .collect();
        let m = resolve(&p, &rebar);
        let names: Vec<&str> = m.marks.iter().map(|m| m.mark.as_str()).collect();
        assert_eq!(names, ["N1", "N2", "N3"]);
        // 12.5 mm first, then the L (4.3 m), then the 3 m bars (within 5 mm: one mark).
        assert_eq!(m.of_bar(ElementId(2), 0).unwrap().mark, "N1");
        assert_eq!(m.of_bar(ElementId(1), 2).unwrap().mark, "N2");
        assert_eq!(m.of_bar(ElementId(1), 0).unwrap().mark, "N3");
        assert_eq!(m.of_bar(ElementId(1), 1).unwrap().mark, "N3");
        let n3 = &m.marks[2];
        assert_eq!(n3.count, 2);
        assert!((n3.total_length - 6.002).abs() < 1e-12);
        assert_eq!(n3.sets, vec![ElementId(1)]);
        // An L read from its other end is the same shape.
        let back = Bar::new(vec![DVec3::new(4.0, 0.0, 0.0), DVec3::ZERO, DVec3::new(0.0, 0.0, 0.3)], false);
        assert!(same((0.01, "", &summary(&back)), (0.01, "", &summary(&ell(0.3, 4.0))), 0.005));
        assert_eq!(canonical(&summary(&back)), canonical(&summary(&ell(0.3, 4.0))));
    }

    #[test]
    fn frozen_marks_hold_and_numbering_goes_on_after_them() {
        let mut p = Project::empty();
        let shape = to_mark_shape(&canonical(&summary(&straight(3.0, 0.0)))).unwrap();
        p.bar_marking.frozen.push(cc_core::bar_marks::FrozenMark {
            mark: "N7".into(),
            diameter: 0.01,
            grade: "CA-50".into(),
            shape,
        });
        let rebar: BTreeMap<_, _> =
            [set(1, 0.01, vec![straight(5.0, 0.0), straight(3.001, 1.0)])].into_iter().collect();
        let m = resolve(&p, &rebar);
        assert_eq!(m.of_bar(ElementId(1), 1).unwrap().mark, "N7");
        assert!(m.of_bar(ElementId(1), 1).unwrap().frozen());
        assert_eq!(m.of_bar(ElementId(1), 0).unwrap().mark, "N8");
        // Another grade is another mark.
        let mut other = rebar.clone();
        if let Some(g) = other.get_mut(&ElementId(1)) {
            g.grade = "CA-60".into();
        }
        let m = resolve(&p, &other);
        assert_eq!(m.of_bar(ElementId(1), 1).unwrap().mark, "N9");
        assert_eq!(m.marks.iter().find(|x| x.mark == "N7").unwrap().count, 0, "listed, unused");
    }
}
