//! The levels a level mark writes: measured from the floors under its point
//! unless the mark sets them.
//!
//! In a plan the finished level is the top of the highest floor of the
//! plan's level under the point (the level itself where there is none); in
//! elevations and sections it is the height of the point. The raw level is
//! the top of that floor's structure (below its finish layers): in sections
//! from the floor whose top the point lies on, on the cut line.

use crate::polygon;
use cc_core::{ElementKind, LevelId, Project, View, ViewKind, level_mark::LevelMark};
use glam::DVec2;

/// Levels to write, as elevations of the project (the type's datum not added).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkLevels {
    pub finished: f64,
    /// The raw level, when the mark writes one.
    pub raw: Option<f64>,
}

/// Floors closer than this to a section point's height are the ones it lies on.
const ON_FLOOR: f64 = 1e-3;

/// The levels `mark` writes in `view` (none in views without levels: 3D,
/// legends, diagrams…).
pub fn levels(project: &Project, view: &View, mark: &LevelMark) -> Option<MarkLevels> {
    let (finished, raw) = match &view.kind {
        ViewKind::FloorPlan { level, .. } | ViewKind::CeilingPlan { level, .. } => {
            match top_floor(project, *level, mark.position) {
                Some((top, raw)) => (top, raw),
                None => (project.level_elevation(*level), None),
            }
        }
        ViewKind::Section { .. } | ViewKind::Elevation { .. } => {
            let z = mark.position.y;
            let raw = match (&view.kind, view.projection()) {
                (ViewKind::Section { .. }, Some(proj)) => {
                    let at = proj.origin + proj.right * mark.position.x;
                    floors_at(project, None, at).find(|(top, _)| (top - z).abs() < ON_FLOOR).and_then(|(_, raw)| raw)
                }
                _ => None,
            };
            (z, raw)
        }
        _ => return None,
    };
    let finished = mark.finished.unwrap_or(finished);
    let raw = if mark.show_raw { mark.raw.or(raw) } else { None };
    Some(MarkLevels { finished, raw })
}

/// The top and raw level of the highest floor of `level` under `p`.
fn top_floor(project: &Project, level: LevelId, p: DVec2) -> Option<(f64, Option<f64>)> {
    floors_at(project, Some(level), p).max_by(|a, b| a.0.total_cmp(&b.0))
}

/// Top and raw level (the top of the structure, when finish layers lie on
/// it) of each floor under `p`, of `level` or of any level.
fn floors_at(project: &Project, level: Option<LevelId>, p: DVec2) -> impl Iterator<Item = (f64, Option<f64>)> + '_ {
    project.elements.values().filter_map(move |e| {
        let ElementKind::Floor(f) = &e.kind else { return None };
        if level.is_some_and(|l| l != e.level) {
            return None;
        }
        if !polygon::contains(&f.boundary, p) || f.all_openings().any(|o| polygon::contains(o, p)) {
            return None;
        }
        let top = project.level_elevation(e.level) + f.height_offset;
        let finish = project.element_type(e.type_id).map_or(0.0, |t| t.core_range().0);
        top.is_finite().then_some((top, (finish > 1e-9).then_some(top - finish)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Category, Element, ElementId, Floor, ViewId};

    /// The template with its 12 cm slab (3 cm screed over the concrete), 15 cm
    /// above the ground level over a 4 x 4 square, a plan of that level and a
    /// section across the floor looking north along y = 2.
    fn project() -> (Project, LevelId, View, View) {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let type_id = p.default_type(Category::Floor).unwrap();
        let square = vec![DVec2::ZERO, DVec2::new(4.0, 0.0), DVec2::new(4.0, 4.0), DVec2::new(0.0, 4.0)];
        let id = ElementId(9999);
        let e = Element {
            id,
            level,
            type_id,
            mark: String::new(),
            comments: String::new(),
            uid: Default::default(),
            kind: ElementKind::Floor(Floor::new(square, 0.15)),
            phase_created: Default::default(),
            phase_demolished: None,
            design_option: None,
            paints: Vec::new(),
        };
        p.elements.insert(id, e);
        let plan = View::new(ViewId(9998), "P", 50, ViewKind::floor_plan(level));
        let kind =
            ViewKind::Section { start: DVec2::new(-1.0, 2.0), end: DVec2::new(6.0, 2.0), depth: 10.0, segments: None };
        let section = View::new(ViewId(9997), "S", 50, kind);
        (p, level, plan, section)
    }

    #[test]
    fn plans_measure_the_floor_under_the_point() {
        let (p, level, view, _) = project();
        let z = p.level_elevation(level);
        let mark = LevelMark::new(view.id, DVec2::new(1.0, 1.0));
        let m = levels(&p, &view, &mark).unwrap();
        assert!((m.finished - (z + 0.15)).abs() < 1e-9, "{m:?}");
        // The 3 cm screed over the slab's concrete.
        assert!(m.raw.is_some_and(|r| (r - (z + 0.12)).abs() < 1e-9), "{m:?}");
        // Off the floor: the level, no raw level.
        let off = levels(&p, &view, &LevelMark::new(view.id, DVec2::new(9.0, 1.0))).unwrap();
        assert_eq!(off, MarkLevels { finished: z, raw: None });
        // Set values win; the raw level can be hidden.
        let set = LevelMark { finished: Some(3.0), show_raw: false, ..mark.clone() };
        assert_eq!(levels(&p, &view, &set).unwrap(), MarkLevels { finished: 3.0, raw: None });
        let set = LevelMark { raw: Some(2.9), ..mark };
        assert_eq!(levels(&p, &view, &set).unwrap().raw, Some(2.9));
        // No levels in legends.
        let legend = View::new(ViewId(1), "L", 50, ViewKind::Legend { types: Vec::new() });
        assert!(levels(&p, &legend, &LevelMark::new(legend.id, DVec2::ZERO)).is_none());
    }

    #[test]
    fn sections_measure_the_height_of_the_point() {
        let (p, level, _, section) = project();
        let z = p.level_elevation(level);
        let x = section.projection().unwrap().project(DVec2::new(1.0, 2.0), 0.0).x;
        let on = levels(&p, &section, &LevelMark::new(section.id, DVec2::new(x, z + 0.15))).unwrap();
        assert!((on.finished - (z + 0.15)).abs() < 1e-9);
        assert!(on.raw.is_some_and(|r| (r - (z + 0.12)).abs() < 1e-9), "{on:?}");
        // Above the floor, or beside it: the height only.
        let above = levels(&p, &section, &LevelMark::new(section.id, DVec2::new(x, z + 1.0))).unwrap();
        assert_eq!(above, MarkLevels { finished: z + 1.0, raw: None });
        let x = section.projection().unwrap().project(DVec2::new(5.0, 2.0), 0.0).x;
        let beside = levels(&p, &section, &LevelMark::new(section.id, DVec2::new(x, z + 0.15))).unwrap();
        assert_eq!(beside.raw, None);
    }
}
