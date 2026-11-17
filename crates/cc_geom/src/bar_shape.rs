//! The shape of a bar as a bending schedule describes it: open or closed,
//! its legs in order with their lengths, the radii and turns of arc legs
//! and the bends between legs, read from the bar's center line. Independent
//! of where the bar lies, so bars of one shape compare equal wherever they
//! are ([`ShapeSummary::matches`]).

use crate::rebar::Bar;
use glam::DVec3;

/// Segments shorter than this are no segments.
const MIN_SEGMENT: f64 = 1e-7;
/// Turns smaller than this are none: the segments make one straight leg, radians.
const STRAIGHT: f64 = 1e-4;
/// Chords of an arc turn by at most this at each point, radians (round ties
/// are drawn with 16 chords).
const ARC_TURN: f64 = 25.0 * std::f64::consts::PI / 180.0;
/// Two chords alone make an arc only when they turn by at most this, radians.
const SHORT_ARC_TURN: f64 = 6.5 * std::f64::consts::PI / 180.0;
/// At most this many points of a bar are read; the rest of a longer one is left out.
const MAX_POINTS: usize = 4096;

/// One leg of a [`ShapeSummary`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegSummary {
    /// Length along the leg, meters.
    pub length: f64,
    /// Center-line radius of an arc leg; none for a straight one.
    pub radius: Option<f64>,
    /// Angle an arc leg turns through, radians (0 for a straight leg),
    /// signed like the bends.
    pub sweep: f64,
    /// Turn at the corner after this leg, radians: positive the way the bar
    /// first turns (along an arc or at a corner), negative the other way; 0
    /// after the last leg of an open bar (a closed bar's last leg ends at
    /// its closing corner). Bars that do not lie in a plane take the sign
    /// of the turn's axis against the first.
    pub bend: f64,
    /// The leg is part of a hook.
    pub hook: bool,
}

/// The shape of a bar: see the module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeSummary {
    pub closed: bool,
    pub legs: Vec<LegSummary>,
}

impl ShapeSummary {
    /// Length of the bar.
    pub fn length(&self) -> f64 {
        self.legs.iter().map(|l| l.length).sum()
    }

    /// Whether the bar is one straight leg.
    pub fn is_straight(&self) -> bool {
        !self.closed && self.legs.len() == 1 && self.legs.iter().all(|l| l.radius.is_none())
    }

    /// The same shape read from its other end.
    pub fn reversed(&self) -> ShapeSummary {
        let n = self.legs.len();
        let closing = if self.closed { self.legs.last().map_or(0.0, |l| -l.bend) } else { 0.0 };
        let legs = self
            .legs
            .iter()
            .rev()
            .enumerate()
            .map(|(j, old)| {
                // The corner after the new leg `j` is the one before the old leg `n - 1 - j`.
                let bend = match (n - 1 - j).checked_sub(1).and_then(|k| self.legs.get(k)) {
                    Some(before) => -before.bend,
                    None => closing,
                };
                LegSummary { sweep: -old.sweep, bend, ..*old }
            })
            .collect();
        ShapeSummary { closed: self.closed, legs }
    }

    /// Whether `other` is the same shape: as many legs, of lengths within
    /// `length_tol` and radii alike, turns within `angle_tol`, read from
    /// either end, either way round (mirrored), and a closed bar from any
    /// corner.
    pub fn matches(&self, other: &ShapeSummary, length_tol: f64, angle_tol: f64) -> bool {
        let n = self.legs.len();
        if self.closed != other.closed || n != other.legs.len() {
            return false;
        }
        let same = |a: &LegSummary, b: &LegSummary, sign: f64| {
            (a.length - b.length).abs() <= length_tol
                && match (a.radius, b.radius) {
                    (Some(x), Some(y)) => (x - y).abs() <= length_tol,
                    (None, None) => true,
                    _ => false,
                }
                && (a.sweep - sign * b.sweep).abs() <= angle_tol
                && (a.bend - sign * b.bend).abs() <= angle_tol
                && a.hook == b.hook
        };
        // A closed bar may start at any of its corners (bounded: long ones from their first only).
        let shifts = if self.closed && n <= 64 { n.max(1) } else { 1 };
        let reversed = other.reversed();
        [other, &reversed].into_iter().any(|o| {
            (0..shifts).any(|k| {
                [1.0, -1.0].into_iter().any(|sign| {
                    (0..n).all(|i| match (self.legs.get(i), o.legs.get((i + k) % n.max(1))) {
                        (Some(a), Some(b)) => same(a, b, sign),
                        _ => false,
                    })
                })
            })
        })
    }
}

/// The shape of `bar`, read from its center line. Points in a line make one
/// straight leg; equal chords turning evenly by a small angle make one arc
/// leg, with the radius and the length of the arc through their points. A
/// bar whose true length differs from its center line's (the hooks of a
/// tie, counted but not drawn) has the difference spread over its legs.
pub fn summary(bar: &Bar) -> ShapeSummary {
    let mut pts: Vec<DVec3> = Vec::with_capacity(bar.points.len().min(MAX_POINTS));
    for p in bar.points.iter().take(MAX_POINTS).filter(|p| p.is_finite()) {
        if pts.last().is_none_or(|q| q.distance(*p) > MIN_SEGMENT) {
            pts.push(*p);
        }
    }
    let mut closed = bar.closed;
    if closed && pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) <= MIN_SEGMENT) {
        pts.pop();
    }
    if pts.len() < 3 {
        closed = false;
    }
    if pts.len() < 2 {
        return ShapeSummary::default();
    }
    // Segments, and the turn after each (none after the last of an open bar).
    let count = if closed { pts.len() } else { pts.len() - 1 };
    let seg = |i: usize| -> DVec3 {
        match (pts.get(i % pts.len()), pts.get((i + 1) % pts.len())) {
            (Some(a), Some(b)) => *b - *a,
            _ => DVec3::ZERO,
        }
    };
    let mut lengths: Vec<f64> = (0..count).map(|i| seg(i).length()).collect();
    let mut axis: Option<DVec3> = None;
    let mut turns: Vec<f64> = (0..count)
        .map(|i| {
            if !closed && i + 1 == count {
                return 0.0;
            }
            let (a, b) = (seg(i).normalize_or_zero(), seg(i + 1).normalize_or_zero());
            let cross = a.cross(b);
            let angle = cross.length().atan2(a.dot(b));
            if angle < STRAIGHT {
                return 0.0;
            }
            // Signed against the first turn's axis: positive the way the bar first turns.
            let first = *axis.get_or_insert(cross);
            if cross.dot(first) >= 0.0 { angle } else { -angle }
        })
        .collect();
    // Segments in a line are one: each takes in those it runs on into.
    let mut i = 0;
    while i < lengths.len() {
        let last_open = !closed && i + 1 == lengths.len();
        if lengths.len() > 1 && !last_open && turns.get(i).is_some_and(|t| *t == 0.0) {
            let next = (i + 1) % lengths.len();
            let (len, turn) = (lengths.get(next).copied().unwrap_or(0.0), turns.get(next).copied().unwrap_or(0.0));
            if let (Some(l), Some(t)) = (lengths.get_mut(i), turns.get_mut(i)) {
                *l += len;
                *t = turn;
            }
            lengths.remove(next);
            turns.remove(next);
            if next < i {
                i -= 1;
            }
            if lengths.len() == 1 {
                break;
            }
        } else {
            i += 1;
        }
    }
    let n = lengths.len();
    if n == 1 {
        // A closed line folded onto itself is no loop.
        let leg = LegSummary { length: lengths.iter().sum(), radius: None, sweep: 0.0, bend: 0.0, hook: false };
        return scaled(ShapeSummary { closed: false, legs: vec![leg] }, bar.true_length);
    }
    // A point where the bar runs on along an arc: equal chords either side, a small turn.
    let joint = |i: usize| !(!closed && i + 1 >= n);
    let even = |a: f64, b: f64| (a - b).abs() <= 1e-3 * a.max(b).max(1e-9);
    let smooth = |i: usize| {
        let (Some(a), Some(b), Some(t)) = (lengths.get(i), lengths.get((i + 1) % n), turns.get(i)) else {
            return false;
        };
        joint(i) && t.abs() > 0.0 && t.abs() <= ARC_TURN && even(*a, *b)
    };
    let alike = |i: usize, j: usize| match (turns.get(i), turns.get(j)) {
        (Some(a), Some(b)) => (a - b).abs() <= 1e-3,
        _ => false,
    };
    // A closed bar is read from after a corner, so that no arc runs across its start.
    let start = if closed { (0..n).find(|i| !smooth(*i)).map_or(0, |i| (i + 1) % n) } else { 0 };
    let at = |k: usize| (start + k) % n;
    let mut legs = Vec::new();
    let mut k = 0;
    while k < n {
        let first = at(k);
        // The chords that follow on the same arc.
        let mut run = 0;
        while k + run + 1 < n && smooth(at(k + run)) && alike(at(k + run), first) {
            run += 1;
        }
        let turn = turns.get(first).copied().unwrap_or(0.0);
        let chord = lengths.get(first).copied().unwrap_or(0.0);
        let whole = closed && run + 1 == n && smooth(at(k + run)) && alike(at(k + run), first);
        if run >= 2 || (run == 1 && turn.abs() <= SHORT_ARC_TURN) || whole {
            let chords = run + 1;
            let radius = chord / (2.0 * (turn.abs() / 2.0).sin()).max(1e-12);
            // Each chord spans one turn of the arc; a ring turns once around.
            let sweep = if whole { turn.signum() * std::f64::consts::TAU } else { turn * chords as f64 };
            let last = at(k + run);
            let bend = if whole || !joint(last) { 0.0 } else { turns.get(last).copied().unwrap_or(0.0) };
            legs.push(LegSummary { length: radius * sweep.abs(), radius: Some(radius), sweep, bend, hook: false });
            k += chords;
        } else {
            let bend = if joint(first) { turn } else { 0.0 };
            legs.push(LegSummary { length: chord, radius: None, sweep: 0.0, bend, hook: false });
            k += 1;
        }
    }
    scaled(ShapeSummary { closed, legs }, bar.true_length)
}

/// `shape` with its legs adding up to `true_length`, when the bar has one:
/// the difference goes to its arc legs (their chords measure them short),
/// or to every leg of a bar without arcs (hooks counted but not drawn).
fn scaled(mut shape: ShapeSummary, true_length: Option<f64>) -> ShapeSummary {
    let Some(want) = true_length.filter(|l| l.is_finite() && *l > 0.0) else { return shape };
    let total = shape.length();
    if (want - total).abs() <= 1e-9 || total <= 0.0 {
        return shape;
    }
    let arcs: f64 = shape.legs.iter().filter(|l| l.radius.is_some()).map(|l| l.length).sum();
    let straight = total - arcs;
    if arcs > 0.0 && want > straight {
        let k = (want - straight) / arcs;
        shape.legs.iter_mut().filter(|l| l.radius.is_some()).for_each(|l| l.length *= k);
    } else {
        let k = want / total;
        shape.legs.iter_mut().for_each(|l| l.length *= k);
    }
    shape
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    fn v(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn straight_bars_and_bends() {
        // Points in a line are one leg.
        let s = summary(&Bar::new(vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(3.0, 0.0, 0.0)], false));
        assert!(s.is_straight() && close(s.length(), 3.0));
        // An L, and a Z whose second bend turns the other way.
        let l = summary(&Bar::new(vec![v(0.0, 0.0, 0.3), v(0.0, 0.0, 0.0), v(4.0, 0.0, 0.0)], false));
        assert_eq!(l.legs.len(), 2);
        assert!(close(l.legs[0].length, 0.3) && close(l.legs[0].bend, FRAC_PI_2) && close(l.legs[1].bend, 0.0));
        let z =
            summary(&Bar::new(vec![v(0.0, 0.0, 0.3), v(0.0, 0.0, 0.0), v(4.0, 0.0, 0.0), v(4.0, 0.0, -0.3)], false));
        assert!(close(z.legs[0].bend, FRAC_PI_2) && close(z.legs[1].bend, -FRAC_PI_2));
        // The same shapes elsewhere, read from the other end or mirrored, match.
        let back = summary(&Bar::new(vec![v(9.0, 5.0, 1.0), v(5.0, 5.0, 1.0), v(5.0, 5.3, 1.0)], false));
        assert!(l.matches(&back, 1e-6, 1e-6) && !l.matches(&z, 1e-6, 1e-6));
        assert!(!l.matches(
            &summary(&Bar::new(vec![v(0.0, 0.0, 0.31), v(0.0, 0.0, 0.0), v(4.0, 0.0, 0.0)], false)),
            0.005,
            0.01
        ));
        // Nothing to read: no legs.
        assert!(summary(&Bar::new(vec![v(1.0, 1.0, 1.0)], false)).legs.is_empty());
        assert!(summary(&Bar::new(vec![v(f64::NAN, 0.0, 0.0), v(1.0, 0.0, 0.0)], false)).legs.is_empty());
    }

    #[test]
    fn stirrups_close_on_their_corners() {
        let loop4 = vec![v(0.0, 0.0, 0.0), v(0.1, 0.0, 0.0), v(0.1, 0.0, 0.3), v(0.0, 0.0, 0.3)];
        let s = summary(&Bar::new(loop4.clone(), true));
        assert!(s.closed && s.legs.len() == 4 && close(s.length(), 0.8));
        assert!(s.legs.iter().all(|l| close(l.bend, FRAC_PI_2) && l.radius.is_none()));
        // From another corner and the other way round it is the same stirrup; a wider one is not.
        let mut other = loop4.clone();
        other.rotate_left(1);
        other.reverse();
        assert!(s.matches(&summary(&Bar::new(other, true)), 1e-6, 1e-6));
        let wide = vec![v(0.0, 0.0, 0.0), v(0.2, 0.0, 0.0), v(0.2, 0.0, 0.2), v(0.0, 0.0, 0.2)];
        assert!(!s.matches(&summary(&Bar::new(wide, true)), 0.005, 0.01));
    }

    #[test]
    fn chords_read_as_arcs() {
        // A quarter circle of radius 2.5 m in chords of 3 degrees.
        let arc = |r: f64, sweep: f64, n: usize| -> Vec<DVec3> {
            (0..=n).map(|i| (glam::DVec2::from_angle(sweep * i as f64 / n as f64) * r).extend(1.0)).collect()
        };
        let s = summary(&Bar { points: arc(2.5, FRAC_PI_2, 30), closed: false, true_length: Some(2.5 * FRAC_PI_2) });
        assert_eq!(s.legs.len(), 1);
        let leg = s.legs[0];
        assert!(close(leg.radius.unwrap(), 2.5) && close(leg.length, 2.5 * FRAC_PI_2), "{leg:?}");
        assert!((leg.sweep - FRAC_PI_2).abs() < 1e-9 && close(leg.bend, 0.0));
        // Another radius is another shape; the same arc elsewhere, turning the other way, is the same.
        let other = summary(&Bar { points: arc(2.6, FRAC_PI_2, 30), closed: false, true_length: None });
        assert!(!s.matches(&other, 0.005, 0.01));
        let mirrored: Vec<DVec3> = arc(2.5, FRAC_PI_2, 30).into_iter().map(|p| v(p.x + 3.0, -p.y, 0.0)).collect();
        assert!(s.matches(&summary(&Bar::new(mirrored, false)), 1e-6, 1e-6));
        // A closed ring: one leg, a whole turn, the circle's length.
        let ring: Vec<DVec3> = arc(1.0, TAU, 120).into_iter().take(120).collect();
        let r = summary(&Bar { points: ring, closed: true, true_length: Some(TAU) });
        assert!(r.closed && r.legs.len() == 1 && close(r.legs[0].radius.unwrap(), 1.0) && close(r.length(), TAU));
        // A round tie drawn with 16 chords is a ring too.
        let tie: Vec<DVec3> = arc(0.2, TAU, 16).into_iter().take(16).collect();
        let t = summary(&Bar::new(tie, true));
        assert!(t.legs.len() == 1 && close(t.legs[0].radius.unwrap(), 0.2) && close(t.length(), 0.4 * PI));
        // A straight leg, then an arc: two legs.
        let mut hooked = vec![v(2.5, -1.0, 1.0)];
        hooked.extend(arc(2.5, FRAC_PI_2, 30));
        let h = summary(&Bar::new(hooked, false));
        assert!(h.legs.len() == 2 && h.legs[0].radius.is_none() && close(h.legs[0].length, 1.0), "{h:?}");
        assert!(close(h.legs[1].radius.unwrap(), 2.5));
    }

    #[test]
    fn hooks_counted_in_the_length_are_spread_over_the_legs() {
        // A tie drawn as its straight part, 0.15 m of hooks counted in its length.
        let tie = Bar { points: vec![v(0.0, 0.0, 0.0), v(0.2, 0.0, 0.0)], closed: false, true_length: Some(0.35) };
        let s = summary(&tie);
        assert!(s.is_straight() && close(s.length(), 0.35));
    }
}
