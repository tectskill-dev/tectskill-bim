//! *Edit Profile*: the profile of a sweep type drawn in a sketch, in the
//! profile's own plane (x across the path, to its left; y up from the
//! path's height), which takes the place of the views while it is open.
//! The sketch is the same as for outlines in plans (its ribbon, tools,
//! grips, properties and undo); *Finish* runs `sweep_profile_set` with its
//! curves, so every sweep of the type follows in one undo step.
//!
//! *Edit sketch* in the Profile Manager opens a sketched structural profile
//! the same way, around the axes x = 0 and y = 0 of its sketch, with no
//! level: *Finish* writes its outline and openings with `profile_set`,
//! keeping the expressions of the points that did not move, and both
//! *Finish* and *Cancel* go back to the manager.

use crate::paint::{PREVIEW, Xf};
use crate::sketch_mode::{Session, Target};
use crate::state::AppState;
use crate::t;
use crate::theme::PALETTE;
use crate::tools::{Snap, Tool};
use cc_core::profile::{self, ProfilePoint, Segment};
use cc_core::sketch::{Sketch, SketchCurve};
use cc_core::structural_profile::ParamSketch;
use cc_core::{Category, ElementTypeId, ParamKind, ProfileId, ProfileShape, Project, TypeData};
use egui::{Sense, Stroke, Ui};
use glam::DVec2;

/// Text on the paper, which stays white in both themes.
const INK: egui::Color32 = egui::Color32::from_gray(40);
/// Pick distance on screen, points.
const PICK_PX: f64 = 6.0;
/// Points snap to this grid (meters) when nothing nearer catches them.
const GRID: f64 = 0.001;

/// The sweep type *Edit Profile* would open: that of the one selected sweep.
pub fn target(state: &AppState) -> Option<ElementTypeId> {
    let mut sel = state.selection.iter();
    let (Some(id), None) = (sel.next(), sel.next()) else { return None };
    state.project().element(*id).filter(|e| e.category() == Category::Sweep).map(|e| e.type_id)
}

/// The curves of a sweep type's profile: those it was drawn with, or lines
/// along its points.
pub fn curves_of(state: &AppState, type_id: ElementTypeId) -> Option<Vec<SketchCurve>> {
    match &state.project().element_type(type_id)?.data {
        TypeData::Sweep { sketch: Some(s), .. } => Some(s.curves.clone()),
        TypeData::Sweep { profile, .. } => Some(Sketch::from_polygons(profile, &[]).curves),
        _ => None,
    }
}

/// Open the profile of sweep type `type_id` in a sketch.
pub fn open(state: &mut AppState, type_id: ElementTypeId) {
    let Some(curves) = curves_of(state, type_id) else { return };
    state.set_tool(Tool::Select);
    let mut session = Session::new(Target::Profile { type_id }, curves);
    session.draw = crate::sketch_mode::Draw::Select;
    state.sketch = Some(session);
    state.message = t!("hint-sweep-profile-sketch");
}

/// *Edit Profile* from the ribbon: the selected sweep's type.
pub fn edit_selected(state: &mut AppState) {
    if let Some(type_id) = target(state) {
        open(state, type_id);
    }
}

/// Whether the open sketch is a sweep's or a structural profile (which then
/// takes the central area).
pub fn is_open(state: &AppState) -> bool {
    state.sketch.as_ref().is_some_and(|s| matches!(s.target, Target::Profile { .. } | Target::Section { .. }))
}

/// The sketch of a sketched structural profile.
fn param_sketch(project: &Project, id: ProfileId) -> Option<&ParamSketch> {
    match &project.profile(id)?.shape {
        ProfileShape::Sketch(s) => Some(s),
        _ => None,
    }
}

/// The value of a sketch's numeric parameter, as its points read it.
fn param_value(sketch: &ParamSketch, name: &str) -> Option<f64> {
    let numeric = |p: &&cc_core::ComponentParam| {
        p.name == name && !matches!(p.kind, ParamKind::Text | ParamKind::Choice | ParamKind::Material)
    };
    sketch.params.iter().find(numeric).map(|p| p.value)
}

/// The loops of a structural profile's sketch (outline first, then the
/// openings), each as its vertices in the sketch's own coordinates.
fn section_loops(sketch: &ParamSketch) -> Option<Vec<Vec<(DVec2, Segment)>>> {
    let lookup = |name: &str| param_value(sketch, name);
    std::iter::once(&sketch.outline).chain(&sketch.openings).map(|l| profile::vertices(l, &lookup).ok()).collect()
}

/// The curves that draw a sketched structural profile: its outline's, then
/// each opening's.
pub fn section_curves(project: &Project, id: ProfileId) -> Option<Vec<SketchCurve>> {
    let loops = section_loops(param_sketch(project, id)?)?;
    Some(loops.iter().flat_map(|v| profile::to_curves(v, true)).collect())
}

/// Open a sketched structural profile in a sketch (*Edit sketch*);
/// Whether it opened (a profile whose points do not evaluate does not).
pub fn open_section(state: &mut AppState, id: ProfileId) -> bool {
    let Some(curves) = section_curves(state.project(), id) else {
        state.message = t!("profiles-sketch-unreadable");
        return false;
    };
    state.set_tool(Tool::Select);
    let mut session = Session::new(Target::Section { profile: id }, curves);
    session.draw = crate::sketch_mode::Draw::Select;
    state.sketch = Some(session);
    state.message = t!("hint-section-sketch");
    true
}

/// The profile points of one loop of the sketch: a point that did not move
/// (at the same place, with the same segment) keeps what it was written as;
/// others are numbers rounded to the micrometer.
fn loop_points(vertices: &[(DVec2, Segment)], old: &[(ProfilePoint, DVec2, Segment)]) -> Vec<ProfilePoint> {
    use crate::component_sketch::um;
    vertices
        .iter()
        .map(|&(p, s)| {
            if let Some((kept, _, _)) =
                old.iter().find(|(_, q, t)| q.distance(p) < 1e-6 && crate::component_sketch::same(*t, s))
            {
                return kept.clone();
            }
            match s {
                Segment::Line => ProfilePoint::new(um(p.x), um(p.y)),
                Segment::Arc { radius } => {
                    // Rounded up, so it still spans the rounded ends.
                    let r = (radius.abs() * 1e6).ceil() / 1e6;
                    ProfilePoint::arc(um(p.x), um(p.y), um(r * radius.signum()))
                }
                Segment::Ellipse { .. } => {
                    profile::written(&[(p, s)]).pop().unwrap_or_else(|| ProfilePoint::new(um(p.x), um(p.y)))
                }
            }
        })
        .collect()
}

/// The outline and openings `curves` draw for `sketch`, or why not: the
/// largest loop is the outline; loops keep the order they are drawn in.
fn section_points(
    sketch: &ParamSketch,
    curves: &[SketchCurve],
) -> Result<(Vec<ProfilePoint>, Vec<Vec<ProfilePoint>>), String> {
    let as_sketch = Sketch { curves: curves.to_vec() };
    as_sketch.profile().map_err(|e| crate::sketch_mode::error_text(&e))?;
    // The curves of each loop, from the first curve not yet in one.
    let mut used = vec![false; curves.len()];
    let mut loops: Vec<Vec<SketchCurve>> = Vec::new();
    for i in 0..curves.len() {
        if used.get(i).copied().unwrap_or(true) {
            continue;
        }
        let (links, _) = cc_core::sketch_edit::chain(curves, i);
        let mut group = Vec::with_capacity(links.len());
        for (k, _) in links {
            if let (Some(u), Some(c)) = (used.get_mut(k), curves.get(k)) {
                *u = true;
                group.push(*c);
            }
        }
        loops.push(group);
    }
    let area = |group: &[SketchCurve]| {
        Sketch { curves: group.to_vec() }.outline().map_or(0.0, |o| cc_geom::polygon::area(&o).abs())
    };
    let outer = (0..loops.len()).max_by(|&a, &b| area(&loops[a]).total_cmp(&area(&loops[b])));
    let outer = outer.ok_or_else(|| crate::sketch_mode::error_text(&cc_core::sketch::SketchError::Empty))?;
    let old: Vec<(ProfilePoint, DVec2, Segment)> = section_loops(sketch)
        .map(|loops| {
            let points = std::iter::once(&sketch.outline).chain(&sketch.openings).flatten();
            points.cloned().zip(loops.into_iter().flatten()).map(|(p, (at, s))| (p, at, s)).collect()
        })
        .unwrap_or_default();
    let mut outline = Vec::new();
    let mut openings = Vec::new();
    for (i, group) in loops.iter().enumerate() {
        let vertices = profile::from_curves(group, true).map_err(|e| crate::sketch_mode::error_text(&e))?;
        let points = loop_points(&vertices, &old);
        if i == outer {
            outline = points;
        } else {
            openings.push(points);
        }
    }
    Ok((outline, openings))
}

/// *Finish* on a structural profile's sketch: its outline and openings
/// written with `profile_set` (nothing when they draw the same loops, from
/// any start and either way round), then
/// back to the Profile Manager; a sketch that makes no profile stays open
/// with the reason.
pub fn finish_section(state: &mut AppState) {
    let Some(Target::Section { profile: id }) = state.sketch.as_ref().map(|s| s.target) else { return };
    let curves = state.sketch.as_ref().map(|s| s.curves.clone()).unwrap_or_default();
    let Some(sketch) = param_sketch(state.project(), id).cloned() else {
        leave_section(state, id);
        return;
    };
    let (outline, openings) = match section_points(&sketch, &curves) {
        Ok(points) => points,
        Err(message) => {
            state.message = message;
            return;
        }
    };
    let changed = outline != sketch.outline || openings != sketch.openings;
    if changed && !same_shape(&sketch, &curves) {
        let command = cc_engine::commands::profiles::ProfileSet {
            profile: cc_engine::Ref::Id(id.0),
            sketch: Some(serde_json::json!({ "outline": outline, "openings": openings })),
            ..Default::default()
        };
        if crate::actions::run_command(state, command).is_none() {
            return;
        }
    }
    leave_section(state, id);
}

/// Whether `curves` draw the loops `sketch` already makes: the same points
/// from any start and either way round.
fn same_shape(sketch: &ParamSketch, curves: &[SketchCurve]) -> bool {
    let Some(loops) = section_loops(sketch) else { return false };
    let old: Vec<SketchCurve> = loops.iter().flat_map(|v| profile::to_curves(v, true)).collect();
    let Ok(old) = Sketch { curves: old }.profile() else { return false };
    Sketch { curves: curves.to_vec() }.makes(&old.outline, &old.openings)
}

/// Close a structural profile's sketch and go back to the Profile Manager on
/// it, with the manager's edits from before the sketch.
pub fn leave_section(state: &mut AppState, id: ProfileId) {
    // Taken first, so changing the tool below does not settle them.
    let edits = state.dialogs.profiles_resume.take();
    state.sketch = None;
    state.set_tool(Tool::Select);
    crate::profiles_dialog::resume(state, id, edits);
}

/// The view of the profile: where the camera looks and how near.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Camera {
    center: DVec2,
    scale: f64,
}

/// The camera that shows the whole profile (or 20 cm around the path).
fn fit(curves: &[SketchCurve], rect: egui::Rect) -> Camera {
    let pts: Vec<DVec2> = curves.iter().flat_map(|c| c.points()).chain([DVec2::ZERO]).collect();
    let (lo, hi) = pts
        .iter()
        .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let size = (hi - lo).max(DVec2::splat(0.2)) * 1.5;
    let scale = (f64::from(rect.width()) / size.x).min(f64::from(rect.height()) / size.y).clamp(50.0, 1e6);
    Camera { center: (lo + hi) / 2.0, scale }
}

/// Snap `raw`: to the sketch's own points, the path's point (the origin),
/// the axes, then the millimeter grid; Shift keeps the line from the last
/// pick horizontal or vertical.
fn snap(s: &Session, raw: DVec2, tol: f64, shift: bool) -> Snap {
    let near = s
        .snap_points()
        .into_iter()
        .chain([DVec2::ZERO])
        .filter(|q| q.distance(raw) <= tol)
        .min_by(|a, b| a.distance(raw).total_cmp(&b.distance(raw)));
    if let Some(q) = near {
        return Snap::plain(q, true);
    }
    let mut p = raw;
    if shift && let Some(a) = s.picks.last() {
        let d = raw - *a;
        if d.x.abs() >= d.y.abs() {
            p.y = a.y;
        } else {
            p.x = a.x;
        }
    }
    let axis = |v: f64| if v.abs() <= tol { Some(0.0) } else { None };
    let x = axis(p.x).unwrap_or((p.x / GRID).round() * GRID);
    let y = axis(p.y).unwrap_or((p.y / GRID).round() * GRID);
    Snap::plain(DVec2::new(x, y), false)
}

/// The profile's plane in the central area: the path's point and the axes,
/// the sketch and its tools; middle or right drag pans, the wheel zooms, a
/// middle double click fits.
pub fn central(ui: &mut Ui, state: &mut AppState) {
    // A sweep type's profile (with its path) or a structural profile, and its name.
    let (id, sweep, name) = match state.sketch.as_ref().map(|s| s.target) {
        Some(Target::Profile { type_id }) => (
            egui::Id::new(("sweep-profile-camera", type_id.0)),
            true,
            state.project().element_type(type_id).map(|t| t.name.clone()),
        ),
        Some(Target::Section { profile }) => (
            egui::Id::new(("section-profile-camera", profile.0)),
            false,
            state.project().profile(profile).map(|p| p.name.clone()),
        ),
        _ => return,
    };
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PALETTE.paper);
    let curves = state.sketch.as_ref().map(|s| s.curves.clone()).unwrap_or_default();
    let mut cam = ui.ctx().data(|d| d.get_temp::<Camera>(id)).unwrap_or_else(|| fit(&curves, rect));
    if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
        let d = resp.drag_delta();
        cam.center -= DVec2::new(f64::from(d.x), -f64::from(d.y)) / cam.scale;
    }
    if let Some(hover) = resp.hover_pos() {
        let gesture = crate::gestures::read(ui);
        let d = gesture.pan;
        cam.center -= DVec2::new(f64::from(d.x), -f64::from(d.y)) / cam.scale;
        let factor = gesture.zoom(0.002);
        if (factor - 1.0).abs() > 1e-6 {
            let xf = Xf { screen: rect.center(), origin: cam.center, scale: cam.scale };
            let before = xf.to_world(hover);
            cam.scale = (cam.scale * factor).clamp(50.0, 1e6);
            let after = Xf { scale: cam.scale, ..xf }.to_world(hover);
            cam.center += before - after;
        }
    }
    if resp.double_clicked_by(egui::PointerButton::Middle) {
        cam = fit(&curves, rect);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, cam));
    let xf = Xf { screen: rect.center(), origin: cam.center, scale: cam.scale };

    // The axes through the origin: for a sweep, the path's height (x) and the path (y).
    let axis = Stroke::new(1.0, egui::Color32::from_gray(160));
    let o = xf.to_screen(DVec2::ZERO);
    painter.line_segment([egui::pos2(rect.left(), o.y), egui::pos2(rect.right(), o.y)], axis);
    painter.line_segment([egui::pos2(o.x, rect.top()), egui::pos2(o.x, rect.bottom())], axis);
    painter.circle_stroke(o, 5.0, Stroke::new(1.5, PREVIEW));
    let font = egui::FontId::proportional(12.0);
    let note =
        |at: egui::Pos2, align, text: String| painter.text(at, align, text, font.clone(), INK.gamma_multiply(0.7));
    if sweep {
        note(o + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, t!("profile-sketch-path"));
        note(egui::pos2(rect.right() - 8.0, o.y - 4.0), egui::Align2::RIGHT_BOTTOM, t!("profile-sketch-across"));
        note(egui::pos2(o.x + 6.0, rect.top() + 8.0), egui::Align2::LEFT_TOP, t!("profile-sketch-up"));
    }
    if let Some(name) = name {
        painter.text(rect.left_top() + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, name, font, INK);
    }

    let Some(mut s) = state.sketch.take() else { return };
    let tol = PICK_PX / cam.scale;
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let shift = ui.input(|i| i.modifiers.shift);
    let snapped = raw.map(|r| snap(&s, r, tol, shift));
    let message = crate::sketch_mode::interact(ui, &resp, &painter, &mut s, &state.modify, raw, snapped, &xf);
    state.sketch = Some(s);
    if let Some(m) = message {
        state.message = m;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Document;

    /// Issue #166: the selected sweep's profile opens in a sketch, and
    /// *Finish* redraws its type with the curves (the arc kept as an arc).
    #[test]
    fn a_sweep_profile_is_edited_in_a_sketch() {
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let out = state
            .session
            .run(cc_engine::commands::sweep::SweepCreate { path: vec![[0.0, 0.0], [4.0, 0.0]], ..Default::default() })
            .unwrap();
        let sweep = cc_core::ElementId(out["id"].as_u64().unwrap());
        let type_id = state.project().element(sweep).unwrap().type_id;
        assert!(crate::commands::UiCommand::EditProfile.unavailable(&state).is_some(), "nothing selected");
        state.selection = [sweep].into_iter().collect();
        assert_eq!(crate::commands::UiCommand::EditProfile.unavailable(&state), None);
        crate::commands::run(&mut state, &egui::Context::default(), crate::commands::UiCommand::EditProfile);
        assert!(is_open(&state));
        // The baseboard's four sides.
        assert_eq!(state.sketch.as_ref().unwrap().curves.len(), 4);

        // A cove 6 cm: two lines and a quarter arc.
        let cove = vec![
            SketchCurve::Line { a: DVec2::ZERO, b: DVec2::new(0.06, 0.0) },
            SketchCurve::Arc { a: DVec2::new(0.06, 0.0), b: DVec2::new(0.0, 0.06), bulge: -0.414_213_562_37 },
            SketchCurve::Line { a: DVec2::new(0.0, 0.06), b: DVec2::ZERO },
        ];
        // An open outline is refused and the sketch stays.
        state.sketch.as_mut().unwrap().curves = cove[..2].to_vec();
        crate::sketch_mode::finish(&mut state);
        assert!(is_open(&state), "an open profile keeps the sketch open");
        let depth = state.session.doc.undo_depth();
        state.sketch.as_mut().unwrap().curves = cove.clone();
        crate::sketch_mode::finish(&mut state);
        assert!(state.sketch.is_none());
        assert_eq!(state.session.doc.undo_depth(), depth + 1, "one undo step");
        assert_eq!(curves_of(&state, type_id), Some(cove.clone()), "the arc is kept");
        assert_eq!(state.selection, [sweep].into_iter().collect(), "the sweep stays selected");
        // Opened again, the sketch shows the curves it was drawn with; Cancel changes nothing.
        open(&mut state, type_id);
        assert_eq!(state.sketch.as_ref().unwrap().curves, cove);
        crate::sketch_mode::cancel(&mut state);
        assert!(state.sketch.is_none());
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
    }

    /// A structural profile's sketch: points that stay keep their
    /// expressions, a moved one is written as numbers, and a loop inside
    /// the outline is an opening.
    #[test]
    fn a_section_sketch_keeps_the_expressions_of_points_that_stay() {
        let sketch = ParamSketch::square(0.1, 0.2);
        let loops = section_loops(&sketch).unwrap();
        let mut curves = profile::to_curves(&loops[0], true);
        // Unchanged: the same points.
        let (outline, openings) = section_points(&sketch, &curves).unwrap();
        assert_eq!(outline, sketch.outline);
        assert!(openings.is_empty());
        // The top right corner (b/2, h/2) moved 10 mm right; a hole drawn inside.
        let corner = DVec2::new(0.05, 0.1);
        let moved = DVec2::new(0.06, 0.1);
        for c in &mut curves {
            if let SketchCurve::Line { a, b } = c {
                for p in [a, b] {
                    if p.distance(corner) < 1e-9 {
                        *p = moved;
                    }
                }
            }
        }
        let hole = [DVec2::new(-0.01, -0.01), DVec2::new(0.01, -0.01), DVec2::new(0.01, 0.01), DVec2::new(-0.01, 0.01)];
        curves.extend(Sketch::from_polygons(&hole, &[]).curves);
        let (outline, openings) = section_points(&sketch, &curves).unwrap();
        assert_eq!(outline.len(), 4);
        assert!(outline.contains(&ProfilePoint::new("0.06", "0.1")), "{outline:?}");
        let kept = outline.iter().filter(|p| sketch.outline.contains(p)).count();
        assert_eq!(kept, 3, "the other corners keep b/2 and h/2");
        assert_eq!(openings.len(), 1);
        assert_eq!(openings[0].len(), 4);
        // An open sketch is refused with the reason.
        curves.truncate(3);
        assert!(section_points(&sketch, &curves).is_err());
    }

    /// The same loop drawn from another corner and the other way round is
    /// no change; a moved corner is.
    #[test]
    fn a_loop_from_another_start_or_way_round_is_the_same_shape() {
        let sketch = ParamSketch::square(0.1, 0.2);
        let mut v = section_loops(&sketch).unwrap().remove(0);
        v.rotate_left(1);
        v.reverse();
        assert!(same_shape(&sketch, &profile::to_curves(&v, true)));
        v[0].0.x += 0.01;
        assert!(!same_shape(&sketch, &profile::to_curves(&v, true)));
    }

    #[test]
    fn points_snap_to_the_path_the_axes_and_millimeters() {
        let s = Session::new(Target::Profile { type_id: ElementTypeId(1) }, Vec::new());
        assert_eq!(snap(&s, DVec2::new(0.0004, -0.0003), 0.002, false).point, DVec2::ZERO);
        assert_eq!(snap(&s, DVec2::new(0.0008, 0.05012), 0.001, false).point, DVec2::new(0.0, 0.05));
        let p = snap(&s, DVec2::new(0.03004, 0.04996), 0.001, false).point;
        assert!(p.distance(DVec2::new(0.03, 0.05)) < 1e-12, "{p}");
    }
}
