//! Geometry edits of linear elements (walls, beams, grids): trim or extend
//! to a corner or to a reference line, split, offset and align. Arc walls
//! take part on their circle: they trim and extend along it, and meet lines
//! and other arcs where the circles cross.
//!
//! Picks are plan points on the element: the picked part is the part kept.
//!
//! Detail lines take part too, through their curve and
//! [`crate::sketch_edit`]: they trim and extend to other detail lines and
//! to the lines of walls, beams and grids, split and offset.

use crate::arc::{Arc, MAX_BULGE};
use crate::sketch::SketchCurve;
use crate::sketch_edit;
use crate::{EditCtx, Element, ElementId, ElementKind, Project};
use glam::DVec2;

/// Where a linear element runs: a straight line, or an arc (arc walls and arc beams).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Path {
    Line(DVec2, DVec2),
    Arc(Arc),
}

/// The path of a linear element, if it has one.
pub fn path_of(e: &Element) -> Option<Path> {
    if let ElementKind::DetailLine(l) = &e.kind {
        return match l.curve {
            SketchCurve::Line { a, b } => Some(Path::Line(a, b)),
            SketchCurve::Arc { a, b, bulge } => Some(Arc::from_bulge(a, b, bulge).map_or(Path::Line(a, b), Path::Arc)),
            _ => None,
        };
    }
    let (a, b) = line_of(e)?;
    match arc_of(e) {
        Some(arc) => Some(Path::Arc(arc)),
        None => Some(Path::Line(a, b)),
    }
}

/// The arc of an arc wall's or arc beam's location line.
fn arc_of(e: &Element) -> Option<Arc> {
    match &e.kind {
        ElementKind::Wall(w) => w.arc(),
        ElementKind::Beam(b) => b.arc(),
        _ => None,
    }
}

/// The bulge of a wall's or beam's location line.
fn bulge_mut(e: &mut Element) -> Option<&mut f64> {
    match &mut e.kind {
        ElementKind::Wall(w) => Some(&mut w.bulge),
        ElementKind::Beam(b) => Some(&mut b.bulge),
        _ => None,
    }
}

impl Path {
    fn length(&self) -> f64 {
        match self {
            Path::Line(a, b) => a.distance(*b),
            Path::Arc(c) => c.length(),
        }
    }

    /// Distance along the path from its start to (the projection of) `p`;
    /// negative before the start, past the length after the end.
    fn param(&self, p: DVec2) -> f64 {
        match self {
            Path::Line(a, b) => (p - *a).dot((*b - *a).normalize_or_zero()),
            Path::Arc(c) => c.locate(p).0,
        }
    }

    /// Whether `p` (a point on its line or circle) lies on the path itself.
    fn holds(&self, p: DVec2) -> bool {
        (-1e-6..=self.length() + 1e-6).contains(&self.param(p))
    }

    /// Points where the infinite line or full circle of `self` meets that of `other`.
    fn crossings(&self, other: &Path) -> Vec<DVec2> {
        match (self, other) {
            (Path::Line(a, b), Path::Line(c, d)) => {
                crossing(*a, *b, *c, *d).map(|t| *a + (*b - *a) * t).into_iter().collect()
            }
            (Path::Line(a, b), Path::Arc(c)) | (Path::Arc(c), Path::Line(a, b)) => {
                line_circle(*a, *b, c.center, c.radius)
            }
            (Path::Arc(c), Path::Arc(d)) => circle_circle(c.center, c.radius, d.center, d.radius),
        }
    }

    /// The part of the path between `t0` and `t1` along it (beyond the ends
    /// it continues on its line or circle): ends and bulge.
    fn part(&self, t0: f64, t1: f64) -> Option<(DVec2, DVec2, f64)> {
        if t1 - t0 < 1e-4 {
            return None;
        }
        match self {
            Path::Line(a, b) => {
                let dir = (*b - *a).normalize_or_zero();
                Some((*a + dir * t0, *a + dir * t1, 0.0))
            }
            Path::Arc(c) => {
                let bulge = c.bulge_between(t0, t1);
                (bulge.is_finite() && bulge.abs() <= MAX_BULGE).then(|| (c.point(t0), c.point(t1), bulge))
            }
        }
    }

    /// The path cut or extended to `x` (a point on its line or circle),
    /// keeping the side of `pick`: the new part and the distance its start
    /// moved along the path.
    fn trimmed(&self, pick: DVec2, x: DVec2) -> Option<Trimmed> {
        let (t, tp, len) = (self.param(x), self.param(pick), self.length());
        let (t0, t1) = if tp < t { (0.0, t) } else { (t, len) };
        let (start, end, bulge) = self.part(t0, t1)?;
        Some(Trimmed { start, end, bulge, shift: t0, change: ((t1 - t0) - len).abs() })
    }
}

/// A trimmed or extended path.
#[derive(Debug, Clone, Copy)]
struct Trimmed {
    start: DVec2,
    end: DVec2,
    bulge: f64,
    /// How far the start moved along the path (openings move back by it).
    shift: f64,
    /// How much the length changed.
    change: f64,
}

/// Preference among the crossings of two paths (an arc's circle crosses a
/// line or another circle twice): fewest of the two paths missing it (a
/// crossing on the elements themselves first), then the least change.
fn rank(x: DVec2, p: &Path, q: &Path, change: f64) -> (u8, f64) {
    (u8::from(!p.holds(x)) + u8::from(!q.holds(x)), change)
}

fn better(a: (u8, f64), b: (u8, f64)) -> std::cmp::Ordering {
    a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
}

/// Points where the infinite line `a`-`b` meets the circle.
fn line_circle(a: DVec2, b: DVec2, center: DVec2, radius: f64) -> Vec<DVec2> {
    let Some(d) = (b - a).try_normalize() else { return Vec::new() };
    let foot = a + d * (center - a).dot(d);
    let h2 = radius * radius - foot.distance_squared(center);
    if h2 < -1e-12 {
        return Vec::new();
    }
    let h = h2.max(0.0).sqrt();
    if h < 1e-9 { vec![foot] } else { vec![foot - d * h, foot + d * h] }
}

/// Points where two circles meet.
fn circle_circle(c0: DVec2, r0: f64, c1: DVec2, r1: f64) -> Vec<DVec2> {
    let dist = c0.distance(c1);
    if dist < 1e-9 || dist > r0 + r1 + 1e-9 || dist < (r0 - r1).abs() - 1e-9 {
        return Vec::new();
    }
    let u = (c1 - c0) / dist;
    let along = (dist * dist + r0 * r0 - r1 * r1) / (2.0 * dist);
    let h = (r0 * r0 - along * along).max(0.0).sqrt();
    let foot = c0 + u * along;
    if h < 1e-9 { vec![foot] } else { vec![foot + u.perp() * h, foot - u.perp() * h] }
}

/// The curve of an element as a sketch curve: a detail line's own, or the
/// location line of a wall or beam (an arc for arc walls and arc beams) or grid.
pub fn curve_of(e: &Element) -> Option<SketchCurve> {
    match &e.kind {
        ElementKind::DetailLine(l) => Some(l.curve),
        ElementKind::Wall(w) if w.bulge != 0.0 => Some(SketchCurve::Arc { a: w.start, b: w.end, bulge: w.bulge }),
        ElementKind::Beam(b) if b.bulge != 0.0 => Some(SketchCurve::Arc { a: b.start, b: b.end, bulge: b.bulge }),
        _ => line_of(e).map(|(a, b)| SketchCurve::Line { a, b }),
    }
}

/// The curve of a detail line.
fn detail_curve(project: &Project, id: ElementId) -> Option<SketchCurve> {
    match project.element(id).map(|e| &e.kind) {
        Some(ElementKind::DetailLine(l)) => Some(l.curve),
        _ => None,
    }
}

/// Location line of a linear element, if it has one.
pub fn line_of(e: &Element) -> Option<(DVec2, DVec2)> {
    match &e.kind {
        ElementKind::Wall(w) => Some((w.start, w.end)),
        ElementKind::Beam(b) => Some((b.start, b.end)),
        ElementKind::Grid(g) => Some((g.start, g.end)),
        _ => None,
    }
}

fn set_line(e: &mut Element, start: DVec2, end: DVec2) {
    match &mut e.kind {
        ElementKind::Wall(w) => (w.start, w.end) = (start, end),
        ElementKind::Beam(b) => (b.start, b.end) = (start, end),
        ElementKind::Grid(g) => (g.start, g.end) = (start, end),
        _ => {}
    }
}

/// Parameter on `a`-`b` where the infinite lines `a`-`b` and `c`-`d` cross.
fn crossing(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<f64> {
    let (u, v) = (b - a, d - c);
    let den = u.perp_dot(v);
    if den.abs() < 1e-9 * u.length() * v.length() {
        return None;
    }
    Some((c - a).perp_dot(v) / den)
}

/// Ends of other walls on the same level that meet `point` (within 1 cm),
/// as `(wall, is_start)`.
pub fn connected_ends(project: &Project, id: ElementId, point: DVec2) -> Vec<(ElementId, bool)> {
    let Some(me) = project.element(id).filter(|e| e.as_wall().is_some()) else { return Vec::new() };
    project
        .walls()
        .filter(|(e, _)| e.id != id && e.level == me.level)
        .filter_map(|(e, w)| {
            if w.start.distance(point) < 0.01 {
                Some((e.id, true))
            } else if w.end.distance(point) < 0.01 {
                Some((e.id, false))
            } else {
                None
            }
        })
        .collect()
}

/// Why an edit could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    /// The element has no location line (or does not exist).
    NotLinear,
    /// The lines are parallel.
    Parallel,
    /// The result would have no length.
    Degenerate,
    /// The edit needs a straight line and the wall is an arc.
    Curved,
}

fn line(project: &Project, id: ElementId) -> Result<(DVec2, DVec2), EditError> {
    project.element(id).and_then(line_of).ok_or(EditError::NotLinear)
}

/// The path of an element (line or arc).
pub fn path(project: &Project, id: ElementId) -> Result<Path, EditError> {
    project.element(id).and_then(path_of).ok_or(EditError::NotLinear)
}

/// Hosted openings move back by `shift` along the wall (a moved start) and stay within its new length.
fn keep_openings(ctx: &mut EditCtx, wall: ElementId, shift: f64, length: f64) {
    let hosted: Vec<ElementId> = ctx.project().hosted_by(wall).map(|h| h.id).collect();
    for h in hosted {
        if let Some(x) = ctx.element_mut(h).and_then(|e| e.as_hosted_mut()) {
            x.offset = (x.offset - shift).clamp(0.0, length.max(0.0));
        }
    }
}

impl EditCtx<'_> {
    fn apply_line(&mut self, id: ElementId, (s, e): (DVec2, DVec2)) {
        let old = self.project().element(id).and_then(line_of);
        if let Some(el) = self.element_mut(id) {
            set_line(el, s, e);
        }
        let new_length = self.project().element(id).and_then(|e| e.as_wall()).map(|w| w.length());
        if let Some((old_s, _)) = old {
            let dir = (e - s).normalize_or_zero();
            keep_openings(self, id, (s - old_s).dot(dir), new_length.unwrap_or(s.distance(e)));
        }
    }

    /// Give `id` the trimmed path `t` (an arc wall keeps its circle).
    fn apply_trimmed(&mut self, id: ElementId, t: Trimmed) {
        if let Some(el) = self.element_mut(id) {
            set_line(el, t.start, t.end);
            if let Some(bulge) = bulge_mut(el) {
                *bulge = t.bulge;
            }
        }
        let length =
            self.project().element(id).and_then(|e| e.as_wall()).map_or(t.start.distance(t.end), |w| w.length());
        keep_openings(self, id, t.shift, length);
    }

    fn set_detail_curve(&mut self, id: ElementId, curve: SketchCurve) {
        if let Some(ElementKind::DetailLine(l)) = self.element_mut(id).map(|e| &mut e.kind) {
            l.curve = curve;
        }
    }

    /// A copy of detail line `id` with `curve`.
    fn add_detail_copy(&mut self, id: ElementId, curve: SketchCurve) -> Result<ElementId, EditError> {
        let Some(mut copy) = self.project().element(id).cloned() else { return Err(EditError::NotLinear) };
        copy.id = ElementId(self.alloc_id());
        copy.uid = Default::default();
        if let ElementKind::DetailLine(l) = &mut copy.kind {
            l.curve = curve;
        }
        Ok(self.add_element(copy))
    }

    /// Trim or extend detail line `id` to the curve `boundary` (where its
    /// line, circle or ellipse runs), keeping the picked part.
    pub fn trim_detail(&mut self, id: ElementId, pick: DVec2, boundary: &SketchCurve) -> Result<(), EditError> {
        let c = detail_curve(self.project(), id).ok_or(EditError::NotLinear)?;
        let new = sketch_edit::trim_extend(&c, pick, boundary).ok_or(EditError::Degenerate)?;
        self.set_detail_curve(id, new);
        Ok(())
    }

    /// Trim or extend two elements so they meet at a corner, keeping the
    /// picked parts. Lines meet where they cross; an arc wall meets a line
    /// or another arc where its circle crosses it (of two crossings, the one
    /// on the elements themselves, else the one that changes them least).
    pub fn trim_corner(&mut self, a: ElementId, pick_a: DVec2, b: ElementId, pick_b: DVec2) -> Result<(), EditError> {
        let (ca, cb) = (detail_curve(self.project(), a), detail_curve(self.project(), b));
        if ca.is_some() || cb.is_some() {
            // Detail lines meet detail lines only (a model element is not trimmed to a drawing).
            let (Some(ca), Some(cb)) = (ca, cb) else { return Err(EditError::NotLinear) };
            let (na, nb) = sketch_edit::trim_corner(&ca, pick_a, &cb, pick_b).ok_or(EditError::Degenerate)?;
            self.set_detail_curve(a, na);
            self.set_detail_curve(b, nb);
            return Ok(());
        }
        let (pa, pb) = (path(self.project(), a)?, path(self.project(), b)?);
        let crossings = pa.crossings(&pb);
        if crossings.is_empty() {
            return Err(EditError::Parallel);
        }
        let (_, na, nb) = crossings
            .iter()
            .filter_map(|x| {
                let (a, b) = (pa.trimmed(pick_a, *x)?, pb.trimmed(pick_b, *x)?);
                Some((rank(*x, &pa, &pb, a.change + b.change), a, b))
            })
            .min_by(|a, b| better(a.0, b.0))
            .ok_or(EditError::Degenerate)?;
        self.apply_trimmed(a, na);
        self.apply_trimmed(b, nb);
        Ok(())
    }

    /// Trim or extend `id` to the `reference` path (a line, or an arc's
    /// circle), keeping the picked part.
    pub fn trim_to(&mut self, id: ElementId, pick: DVec2, reference: Path) -> Result<(), EditError> {
        if detail_curve(self.project(), id).is_some() {
            let boundary = match reference {
                Path::Line(a, b) => SketchCurve::Line { a, b },
                Path::Arc(c) => SketchCurve::Arc {
                    a: c.point(0.0),
                    b: c.point(c.length()),
                    bulge: c.bulge_between(0.0, c.length()),
                },
            };
            return self.trim_detail(id, pick, &boundary);
        }
        let p = path(self.project(), id)?;
        let crossings = p.crossings(&reference);
        if crossings.is_empty() {
            return Err(EditError::Parallel);
        }
        let (_, n) = crossings
            .iter()
            .filter_map(|x| p.trimmed(pick, *x).map(|t| (rank(*x, &p, &reference, t.change), t)))
            .min_by(|a, b| better(a.0, b.0))
            .ok_or(EditError::Degenerate)?;
        self.apply_trimmed(id, n);
        Ok(())
    }

    /// Split a wall, beam or grid at the projection of `at`; returns the
    /// new element (the part after the split). Openings past the split go
    /// to the new wall.
    pub fn split(&mut self, id: ElementId, at: DVec2) -> Result<ElementId, EditError> {
        if let Some(c) = detail_curve(self.project(), id) {
            let (first, second) = sketch_edit::split(&c, at).ok_or(EditError::Degenerate)?;
            self.set_detail_curve(id, first);
            return self.add_detail_copy(id, second);
        }
        let (a, b) = line(self.project(), id)?;
        let dir = (b - a).normalize_or_zero();
        let arc = self.project().element(id).and_then(arc_of);
        let len = arc.map_or(a.distance(b), |c| c.length());
        // Arc walls and arc beams split on the arc, each part keeping its share of the sweep.
        let t = arc.map_or((at - a).dot(dir), |c| c.locate(at).0);
        if t < 1e-3 || t > len - 1e-3 {
            return Err(EditError::Degenerate);
        }
        let p = arc.map_or(a + dir * t, |c| c.point(t));
        let Some(original) = self.project().element(id).cloned() else { return Err(EditError::NotLinear) };
        let mut second = original.clone();
        second.id = ElementId(self.alloc_id());
        second.uid = Default::default();
        if let ElementKind::Grid(g) = &mut second.kind {
            g.name = format!("{}'", g.name);
        }
        set_line(&mut second, p, b);
        if let (Some(c), Some(bulge)) = (arc, bulge_mut(&mut second)) {
            *bulge = c.bulge_between(t, len);
        }
        // A sloped beam keeps its slope: the split point's elevation joins the parts.
        let split_z = match &original.kind {
            ElementKind::Beam(x) if x.end_offset.is_some() => Some(x.top_offset + x.rise() * t / len),
            _ => None,
        };
        if let (Some(z), ElementKind::Beam(x)) = (split_z, &mut second.kind) {
            x.top_offset = z;
        }
        if let Some(el) = self.element_mut(id) {
            set_line(el, a, p);
            if let (Some(c), Some(bulge)) = (arc, bulge_mut(el)) {
                *bulge = c.bulge_between(0.0, t);
            }
            if let (Some(z), ElementKind::Beam(x)) = (split_z, &mut el.kind) {
                x.end_offset = Some(z);
            }
        }
        let new = self.add_element(second);
        let hosted: Vec<(ElementId, f64)> =
            self.project().hosted_by(id).filter_map(|h| h.as_hosted().map(|x| (h.id, x.offset))).collect();
        for (h, offset) in hosted {
            if offset > t
                && let Some(x) = self.element_mut(h).and_then(|e| e.as_hosted_mut())
            {
                x.host = new;
                x.offset = offset - t;
            }
        }
        Ok(new)
    }

    /// Move one end of a wall, beam or grid to `to` (lengthening or
    /// shortening it). Walls whose ends met that end move with it, so
    /// corners stay closed; openings keep their place.
    pub fn move_endpoint(&mut self, id: ElementId, at_start: bool, to: DVec2) -> Result<(), EditError> {
        let (a, b) = line(self.project(), id)?;
        let from = if at_start { a } else { b };
        let new = if at_start { (to, b) } else { (a, to) };
        if new.0.distance(new.1) < 0.01 {
            return Err(EditError::Degenerate);
        }
        let connected = connected_ends(self.project(), id, from);
        self.apply_line(id, new);
        for (other, other_start) in connected {
            let Ok((s, e)) = line(self.project(), other) else { continue };
            let moved = if other_start { (to, e) } else { (s, to) };
            if moved.0.distance(moved.1) >= 0.01 {
                self.apply_line(other, moved);
            }
        }
        Ok(())
    }

    /// A copy of `id` moved `distance` perpendicular to its line (positive
    /// to the left of start→end). Openings are not copied.
    pub fn offset_copy(&mut self, id: ElementId, distance: f64) -> Result<ElementId, EditError> {
        if let Some(c) = detail_curve(self.project(), id) {
            if distance.abs() < 1e-6 {
                return Err(EditError::Degenerate);
            }
            let moved = sketch_edit::offset_left(&c, distance).ok_or(EditError::Degenerate)?;
            return self.add_detail_copy(id, moved);
        }
        let (a, b) = line(self.project(), id)?;
        let n = (b - a).normalize_or_zero().perp();
        if n == DVec2::ZERO || distance.abs() < 1e-6 {
            return Err(EditError::Degenerate);
        }
        let Some(mut copy) = self.project().element(id).cloned() else { return Err(EditError::NotLinear) };
        copy.id = ElementId(self.alloc_id());
        copy.uid = Default::default();
        match &mut copy.kind {
            // An arc wall's copy is concentric (with the same sweep).
            ElementKind::Wall(w) if w.arc().is_some() => {
                let arc = w.arc().ok_or(EditError::NotLinear)?;
                if arc.radius_at(distance) < 1e-3 {
                    return Err(EditError::Degenerate);
                }
                (w.start, w.end) = w.offset_line(distance);
            }
            // An arc beam's too: the same bulge on a concentric circle.
            ElementKind::Beam(b) if b.arc().is_some() => {
                let arc = b.arc().ok_or(EditError::NotLinear)?;
                if arc.radius_at(distance) < 1e-3 {
                    return Err(EditError::Degenerate);
                }
                (b.start, b.end) = (arc.at(0.0, distance), arc.at(arc.length(), distance));
            }
            _ => copy.translate(n * distance),
        }
        Ok(self.add_element(copy))
    }

    /// Move `id` (with its openings) onto the `reference` path: a line onto
    /// a parallel line, an arc wall onto the circle of a concentric arc (its
    /// radius changes, its sweep stays; openings keep their angles).
    pub fn align_to(&mut self, id: ElementId, reference: Path) -> Result<(), EditError> {
        match (path(self.project(), id)?, reference) {
            (Path::Line(a, b), Path::Line(r0, r1)) => {
                let (u, v) = ((b - a).normalize_or_zero(), (r1 - r0).normalize_or_zero());
                if u.perp_dot(v).abs() > 0.02 {
                    return Err(EditError::Parallel);
                }
                let n = v.perp();
                let shift = n * (r0 - a).dot(n);
                self.transform_elements(&[id], &crate::PlanTransform::translate(shift));
                Ok(())
            }
            (Path::Arc(c), Path::Arc(r)) => {
                if c.center.distance(r.center) > 1e-3 {
                    return Err(EditError::Parallel);
                }
                let (Some((a, b)), true) = (self.project().element(id).and_then(line_of), r.radius > 1e-3) else {
                    return Err(EditError::Degenerate);
                };
                let onto = |p: DVec2| c.center + (p - c.center).normalize_or_zero() * r.radius;
                if let Some(el) = self.element_mut(id) {
                    set_line(el, onto(a), onto(b));
                }
                let scale = r.radius / c.radius.max(1e-12);
                let hosted: Vec<ElementId> = self.project().hosted_by(id).map(|h| h.id).collect();
                for h in hosted {
                    if let Some(x) = self.element_mut(h).and_then(|e| e.as_hosted_mut()) {
                        x.offset *= scale;
                    }
                }
                Ok(())
            }
            _ => Err(EditError::Curved),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Document, Hosted, Wall};

    fn doc_with_walls(lines: &[(DVec2, DVec2)]) -> (Document, Vec<ElementId>) {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let ty = doc.project.default_type(Category::Wall).unwrap();
        let ids = doc.edit("walls", |e| {
            lines
                .iter()
                .map(|(s, t)| {
                    let id = ElementId(e.alloc_id());
                    let kind = ElementKind::Wall(Wall { start: *s, end: *t, ..Default::default() });
                    e.add_element(Element {
                        id,
                        level,
                        type_id: ty,
                        mark: String::new(),
                        comments: String::new(),
                        kind,
                        uid: Default::default(),
                        phase_created: Default::default(),
                        phase_demolished: None,
                        design_option: None,
                        paints: Vec::new(),
                    })
                })
                .collect()
        });
        (doc, ids)
    }

    fn wall(doc: &Document, id: ElementId) -> (DVec2, DVec2) {
        doc.project.element(id).and_then(line_of).unwrap()
    }

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    #[test]
    fn corner_extends_and_trims_keeping_the_picked_parts() {
        // A horizontal wall short of a vertical one that overshoots.
        let (mut doc, ids) = doc_with_walls(&[(v(0.0, 0.0), v(4.0, 0.0)), (v(5.0, -2.0), v(5.0, 3.0))]);
        doc.edit("corner", |e| e.trim_corner(ids[0], v(1.0, 0.0), ids[1], v(5.0, 2.0))).unwrap();
        assert_eq!(wall(&doc, ids[0]), (v(0.0, 0.0), v(5.0, 0.0)));
        assert_eq!(wall(&doc, ids[1]), (v(5.0, 0.0), v(5.0, 3.0)));
        // Parallel walls cannot make a corner.
        let (mut doc, ids) = doc_with_walls(&[(v(0.0, 0.0), v(4.0, 0.0)), (v(0.0, 1.0), v(4.0, 1.0))]);
        assert_eq!(
            doc.edit("x", |e| e.trim_corner(ids[0], v(1.0, 0.0), ids[1], v(1.0, 1.0))),
            Err(EditError::Parallel)
        );
    }

    #[test]
    fn trim_to_reference_and_split_keep_openings() {
        let (mut doc, ids) = doc_with_walls(&[(v(0.0, 0.0), v(10.0, 0.0))]);
        let level = doc.project.levels[0].id;
        let door_t = doc.project.default_type(Category::Door).unwrap();
        let door = doc.edit("door", |e| {
            let id = ElementId(e.alloc_id());
            let hosted = Hosted { host: ids[0], offset: 8.0, sill_height: 0.0, flip_facing: false, flip_hand: false };
            e.add_element(Element {
                id,
                level,
                type_id: door_t,
                mark: "1".into(),
                comments: String::new(),
                kind: ElementKind::Door(hosted),
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: Vec::new(),
            })
        });
        // Split at x = 6: the door (at 8) moves to the second wall at 2.
        let second = doc.edit("split", |e| e.split(ids[0], v(6.0, 0.3))).unwrap();
        assert_eq!(wall(&doc, ids[0]), (v(0.0, 0.0), v(6.0, 0.0)));
        assert_eq!(wall(&doc, second), (v(6.0, 0.0), v(10.0, 0.0)));
        let h = doc.project.element(door).and_then(|e| e.as_hosted()).cloned().unwrap();
        assert_eq!((h.host, h.offset), (second, 2.0));
        // Trim the second wall's start back to x = 7 (keeping the end): the door stays put.
        doc.edit("trim", |e| e.trim_to(second, v(9.0, 0.0), Path::Line(v(7.0, -1.0), v(7.0, 1.0)))).unwrap();
        assert_eq!(wall(&doc, second), (v(7.0, 0.0), v(10.0, 0.0)));
        let h = doc.project.element(door).and_then(|e| e.as_hosted()).cloned().unwrap();
        assert!((h.offset - 1.0).abs() < 1e-12);
        assert_eq!(doc.edit("split", |e| e.split(second, v(7.0, 0.0))), Err(EditError::Degenerate));
        assert_eq!(doc.project.repair(), 0);
    }

    #[test]
    fn offset_and_align() {
        let (mut doc, ids) = doc_with_walls(&[(v(0.0, 0.0), v(4.0, 0.0)), (v(0.0, 3.0), v(4.0, 3.2))]);
        let copy = doc.edit("offset", |e| e.offset_copy(ids[0], 1.5)).unwrap();
        assert_eq!(wall(&doc, copy), (v(0.0, 1.5), v(4.0, 1.5)));
        doc.edit("align", |e| e.align_to(copy, Path::Line(v(-1.0, 2.0), v(9.0, 2.0)))).unwrap();
        assert_eq!(wall(&doc, copy), (v(0.0, 2.0), v(4.0, 2.0)));
        // Not parallel enough.
        let upright = Path::Line(v(0.0, 0.0), v(0.0, 5.0));
        assert_eq!(doc.edit("align", |e| e.align_to(ids[1], upright)), Err(EditError::Parallel));
    }

    #[test]
    fn arc_walls_split_on_the_arc_and_offset_concentric() {
        let (mut doc, ids) = doc_with_walls(&[(v(4.0, 0.0), v(-4.0, 0.0))]);
        doc.edit("arc", |e| {
            if let Some(ElementKind::Wall(w)) = e.element_mut(ids[0]).map(|x| &mut x.kind) {
                w.bulge = 1.0; // a half circle of radius 4 through (0, 4)
            }
        });
        let length = |doc: &Document, id| doc.project.element(id).and_then(|e| e.as_wall()).unwrap().length();
        let second = doc.edit("split", |e| e.split(ids[0], v(0.0, 5.0))).unwrap();
        assert!(wall(&doc, ids[0]).1.distance(v(0.0, 4.0)) < 1e-9);
        assert!((length(&doc, ids[0]) - length(&doc, second)).abs() < 1e-9);
        assert!((length(&doc, ids[0]) - std::f64::consts::PI * 2.0).abs() < 1e-9);
        let copy = doc.edit("offset", |e| e.offset_copy(second, 1.0)).unwrap();
        // The copy runs on the circle of radius 3, from (0, 3) to (-3, 0).
        let (s, t) = wall(&doc, copy);
        assert!(s.distance(v(0.0, 3.0)) < 1e-9 && t.distance(v(-3.0, 0.0)) < 1e-9, "{s} {t}");
        assert_eq!(doc.edit("offset", |e| e.offset_copy(copy, 5.0)), Err(EditError::Degenerate));
    }

    fn arc_wall(doc: &mut Document, id: ElementId, bulge: f64) {
        doc.edit("arc", |e| {
            if let Some(ElementKind::Wall(w)) = e.element_mut(id).map(|x| &mut x.kind) {
                w.bulge = bulge;
            }
        });
    }

    fn arc_of(doc: &Document, id: ElementId) -> Arc {
        doc.project.element(id).and_then(|e| e.as_wall()).and_then(|w| w.arc()).unwrap()
    }

    /// Arc walls trim and extend on their circle, to lines and to other arcs.
    #[test]
    fn arc_walls_trim_and_extend_on_their_circle() {
        // A quarter circle of radius 4 around the origin, from (4, 0) to (0, 4),
        // and a vertical wall x = 2 crossing the circle at y = ±√12.
        let (mut doc, ids) = doc_with_walls(&[(v(4.0, 0.0), v(0.0, 4.0)), (v(2.0, -1.0), v(2.0, 1.0))]);
        let bulge = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
        arc_wall(&mut doc, ids[0], bulge);
        let r12 = 12f64.sqrt();
        // Trim the arc to the line, keeping its end near (0, 4): it starts at (2, √12).
        let line = Path::Line(v(2.0, -1.0), v(2.0, 1.0));
        doc.edit("trim", |e| e.trim_to(ids[0], v(0.0, 4.0), line)).unwrap();
        let (s, t) = wall(&doc, ids[0]);
        assert!(s.distance(v(2.0, r12)) < 1e-9 && t.distance(v(0.0, 4.0)) < 1e-9, "{s} {t}");
        let c = arc_of(&doc, ids[0]);
        assert!(c.center.length() < 1e-9 && (c.radius - 4.0).abs() < 1e-9, "the circle stays");
        // Extend the line to the arc's circle: it reaches the nearer crossing, (2, √12).
        let circle = path(&doc.project, ids[0]).unwrap();
        doc.edit("extend", |e| e.trim_to(ids[1], v(2.0, 0.0), circle)).unwrap();
        let (s, t) = wall(&doc, ids[1]);
        assert!(s.distance(v(2.0, -1.0)) < 1e-9 && t.distance(v(2.0, r12)) < 1e-9, "{s} {t}");
        // Extend the arc back past its start to a horizontal line y = 0: it reaches (4, 0).
        let floor = Path::Line(v(-5.0, 0.0), v(5.0, 0.0));
        doc.edit("extend", |e| e.trim_to(ids[0], v(0.0, 4.0), floor)).unwrap();
        let (s, _) = wall(&doc, ids[0]);
        assert!(s.distance(v(4.0, 0.0)) < 1e-9, "{s}");
        // A line that misses the circle cannot be reached.
        let far = Path::Line(v(9.0, 0.0), v(9.0, 1.0));
        assert_eq!(doc.edit("x", |e| e.trim_to(ids[0], v(0.0, 4.0), far)), Err(EditError::Parallel));
        assert_eq!(doc.project.repair(), 0);
    }

    /// A corner between an arc wall and a line, and between two arcs.
    #[test]
    fn arc_walls_make_corners() {
        // Arc of radius 4 from (4, 0) to (0, 4); a horizontal wall y = 2 from x = 1 to 3.
        let (mut doc, ids) = doc_with_walls(&[(v(4.0, 0.0), v(0.0, 4.0)), (v(1.0, 2.0), v(3.0, 2.0))]);
        arc_wall(&mut doc, ids[0], (std::f64::consts::FRAC_PI_2 / 4.0).tan());
        let x = 12f64.sqrt();
        // Keep the arc's top and the line: the line reaches the circle at (√12, 2).
        doc.edit("corner", |e| e.trim_corner(ids[0], v(0.0, 4.0), ids[1], v(2.0, 2.0))).unwrap();
        let (s, t) = wall(&doc, ids[0]);
        assert!(s.distance(v(x, 2.0)) < 1e-9 && t.distance(v(0.0, 4.0)) < 1e-9, "{s} {t}");
        let (s, t) = wall(&doc, ids[1]);
        assert!(s.distance(v(1.0, 2.0)) < 1e-9 && t.distance(v(x, 2.0)) < 1e-9, "{s} {t}");

        // Two arcs of radius 2 centered at (0, 0) and (2, 0) meet at (1, ±√3).
        let (mut doc, ids) = doc_with_walls(&[(v(2.0, 0.0), v(-2.0, 0.0)), (v(0.0, 0.0), v(4.0, 0.0))]);
        arc_wall(&mut doc, ids[0], 1.0); // upper half of the first circle
        arc_wall(&mut doc, ids[1], 1.0); // lower half of the second, turning counter-clockwise
        doc.edit("corner", |e| e.trim_corner(ids[0], v(-2.0, 0.0), ids[1], v(4.0, 0.0))).unwrap();
        let r3 = 3f64.sqrt();
        // Both keep their picked ends and start at the same crossing.
        let (s0, t0) = wall(&doc, ids[0]);
        let (s1, t1) = wall(&doc, ids[1]);
        assert!(t0.distance(v(-2.0, 0.0)) < 1e-9 && t1.distance(v(4.0, 0.0)) < 1e-9, "{t0} {t1}");
        assert!(s0.distance(s1) < 1e-9, "{s0} {s1}");
        assert!((s0.x - 1.0).abs() < 1e-9 && (s0.y.abs() - r3).abs() < 1e-9, "{s0}");
        assert_eq!(doc.project.repair(), 0);
    }

    /// An arc wall aligns onto a concentric arc, its openings keeping their angles.
    #[test]
    fn arc_walls_align_onto_concentric_arcs() {
        let (mut doc, ids) = doc_with_walls(&[(v(4.0, 0.0), v(-4.0, 0.0)), (v(6.0, 0.0), v(0.0, 6.0))]);
        arc_wall(&mut doc, ids[0], 1.0);
        arc_wall(&mut doc, ids[1], (std::f64::consts::FRAC_PI_2 / 4.0).tan());
        let level = doc.project.levels[0].id;
        let door_t = doc.project.default_type(Category::Door).unwrap();
        let door = doc.edit("door", |e| {
            let id = ElementId(e.alloc_id());
            let hosted = Hosted { host: ids[0], offset: 2.0, sill_height: 0.0, flip_facing: false, flip_hand: false };
            e.add_element(Element {
                id,
                level,
                type_id: door_t,
                mark: "1".into(),
                comments: String::new(),
                kind: ElementKind::Door(hosted),
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: Vec::new(),
            })
        });
        let outer = path(&doc.project, ids[1]).unwrap();
        doc.edit("align", |e| e.align_to(ids[0], outer)).unwrap();
        let (s, t) = wall(&doc, ids[0]);
        assert!(s.distance(v(6.0, 0.0)) < 1e-9 && t.distance(v(-6.0, 0.0)) < 1e-9, "{s} {t}");
        let h = doc.project.element(door).and_then(|e| e.as_hosted()).cloned().unwrap();
        assert!((h.offset - 3.0).abs() < 1e-9, "same angle on the larger circle: {}", h.offset);
        // Not concentric, or a line onto an arc: refused.
        let elsewhere = Path::Arc(Arc::from_bulge(v(10.0, 0.0), v(12.0, 0.0), 1.0).unwrap());
        assert_eq!(doc.edit("x", |e| e.align_to(ids[0], elsewhere)), Err(EditError::Parallel));
        let line = Path::Line(v(0.0, 0.0), v(1.0, 0.0));
        assert_eq!(doc.edit("x", |e| e.align_to(ids[0], line)), Err(EditError::Curved));
    }

    #[test]
    fn dragging_an_end_keeps_the_corner_closed() {
        let (mut doc, ids) =
            doc_with_walls(&[(v(0.0, 0.0), v(4.0, 0.0)), (v(4.0, 0.0), v(4.0, 3.0)), (v(9.0, 9.0), v(9.0, 12.0))]);
        doc.edit("grip", |e| e.move_endpoint(ids[0], false, v(5.0, 0.0))).unwrap();
        assert_eq!(wall(&doc, ids[0]), (v(0.0, 0.0), v(5.0, 0.0)));
        // The wall that met that end follows; unrelated walls do not.
        assert_eq!(wall(&doc, ids[1]), (v(5.0, 0.0), v(4.0, 3.0)));
        assert_eq!(wall(&doc, ids[2]), (v(9.0, 9.0), v(9.0, 12.0)));
        assert_eq!(doc.edit("grip", |e| e.move_endpoint(ids[2], true, v(9.0, 12.0))), Err(EditError::Degenerate));
    }
}
