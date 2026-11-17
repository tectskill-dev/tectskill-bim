//! Construction phases through commands: element status per view phase,
//! phase filters and graphics, demolition (with hosted elements) and undo,
//! rooms and geometry per phase, schedules, deleting and reordering
//! phases, view templates, and repair of files.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementId, LinePattern, ViewId};
use cc_engine::{ErrorCode, Session};
use cc_views::{LineStyle, Prim};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

/// The two template phases: existing, new construction.
fn phases(s: &mut Session) -> (u64, u64) {
    let list = run(s, "phases_list", json!({}));
    assert_eq!(list.as_array().unwrap().len(), 2);
    (id(&list[0]), id(&list[1]))
}

fn filter(s: &mut Session, index: usize) -> u64 {
    id(&run(s, "phase_filters_list", json!({}))[index])
}

/// A session with walls A (existing), B (existing, demolished in the new
/// phase), C (new) and D (temporary: built and demolished in the new
/// phase); returns the ground plan (new phase), a copy of it in the
/// existing phase, and the walls.
fn renovation() -> (Session, u64, u64, [u64; 4]) {
    let mut s = Session::default();
    let (existing, new) = phases(&mut s);
    let plan = id(&run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]);
    let old_plan = id(&run(&mut s, "view_duplicate", json!({"view": plan})));
    run(&mut s, "view_set", json!({"view": old_plan, "phase": existing}));
    let mut walls = [0; 4];
    for (i, w) in walls.iter_mut().enumerate() {
        let y = i as f64 * 3.0;
        *w = id(&run(&mut s, "wall_create", json!({"start": [0.0, y], "end": [4.0, y]})));
    }
    let [a, b, c, d] = walls;
    run(&mut s, "elements_phase_set", json!({"ids": [a, b], "created": existing}));
    run(&mut s, "elements_demolish", json!({"ids": [b], "phase": new}));
    run(&mut s, "elements_demolish", json!({"ids": [d], "view": plan}));
    let _ = c;
    (s, plan, old_plan, walls)
}

fn drawn(s: &mut Session, view: u64) -> BTreeSet<u64> {
    s.drawing(ViewId(view)).unwrap().prims.iter().filter_map(|p| p.element()).map(|e| e.0).collect()
}

/// Dash patterns and halftone of the cut lines of `element`.
fn cut_pens(s: &mut Session, view: u64, element: u64) -> BTreeSet<(Option<LinePattern>, bool)> {
    s.drawing(ViewId(view))
        .unwrap()
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { style: LineStyle::Cut, element: Some(e), pen, .. } if e.0 == element => {
                Some(pen.map_or((None, false), |p| (p.pattern, p.halftone)))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn template_phases_filters_and_new_elements() {
    let mut s = Session::default();
    let (existing, new) = phases(&mut s);
    let filters = run(&mut s, "phase_filters_list", json!({}));
    assert_eq!(filters.as_array().unwrap().len(), 7);
    let all = &filters[0];
    assert_eq!(
        (&all["existing"], &all["demolished"], &all["new"], &all["temporary"]),
        (&json!("overridden"), &json!("overridden"), &json!("by_category"), &json!("overridden"))
    );
    // Views show the last phase with the "show all" filter.
    let plan = id(&run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]);
    let v = run(&mut s, "view_get", json!({"view": plan}));
    assert_eq!(v["phase"]["id"], json!(new));
    assert_eq!(v["phase_filter"]["id"], all["id"]);
    // New elements are built in the creation phase (the last by default).
    let w = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [3, 0]})));
    assert_eq!(run(&mut s, "element_get", json!({"id": w}))["phase_created"]["id"], json!(new));
    run(&mut s, "phase_creation_set", json!({"phase": existing}));
    let w2 = id(&run(&mut s, "wall_create", json!({"start": [0, 2], "end": [3, 2]})));
    assert_eq!(run(&mut s, "element_get", json!({"id": w2}))["phase_created"]["id"], json!(existing));
    // Copies are built in the creation phase too, standing.
    run(&mut s, "elements_demolish", json!({"ids": [w2], "phase": new}));
    run(&mut s, "phase_creation_set", json!({}));
    let copy = run(&mut s, "elements_copy", json!({"ids": [w2], "delta": [0, 5]}));
    let copied = run(&mut s, "element_get", json!({"id": copy["created"][0]}));
    assert_eq!(copied["phase_created"]["id"], json!(new));
    assert_eq!(copied["phase_demolished"], Value::Null);
    // New views take the last phase and the default filter.
    let section = id(&run(&mut s, "view_create", json!({"kind": "section", "start": [0, -1], "end": [3, -1]})));
    assert_eq!(run(&mut s, "view_get", json!({"view": section}))["phase"]["id"], json!(new));
    // Annotations have no phase.
    let grid = id(&run(&mut s, "grid_create", json!({"start": [0, -3], "end": [5, -3]})));
    assert_eq!(run(&mut s, "element_get", json!({"id": grid}))["phase_created"], Value::Null);
    assert_eq!(fails(&mut s, "elements_demolish", json!({"ids": [grid]})), ErrorCode::Rejected);
}

#[test]
fn status_decides_what_each_filter_shows() {
    let (mut s, plan, old_plan, [a, b, c, d]) = renovation();
    // Show all: everything of the phase, existing faint, demolished dashed,
    // new as drawn, temporary faint and short-dashed.
    let shown = drawn(&mut s, plan);
    assert!([a, b, c, d].iter().all(|w| shown.contains(w)));
    assert_eq!(cut_pens(&mut s, plan, a), BTreeSet::from([(None, true)]));
    assert_eq!(cut_pens(&mut s, plan, b), BTreeSet::from([(Some(LinePattern::Dash), false)]));
    assert_eq!(cut_pens(&mut s, plan, c), BTreeSet::from([(None, false)]));
    assert_eq!(cut_pens(&mut s, plan, d), BTreeSet::from([(Some(LinePattern::ShortDash), true)]));
    // The existing phase shows the walls built then, as new, and nothing else.
    let shown = drawn(&mut s, old_plan);
    assert!(shown.contains(&a) && shown.contains(&b) && !shown.contains(&c) && !shown.contains(&d));
    assert_eq!(cut_pens(&mut s, old_plan, b), BTreeSet::from([(None, false)]));

    let expect = |s: &mut Session, index: usize, wanted: &[u64]| {
        let f = filter(s, index);
        run(s, "view_set", json!({"view": plan, "phase_filter": f}));
        let shown = drawn(s, plan);
        let got: Vec<u64> = [a, b, c, d].into_iter().filter(|w| shown.contains(w)).collect();
        assert_eq!(got, wanted, "filter {index}");
    };
    expect(&mut s, 1, &[a, c]); // complete
    expect(&mut s, 2, &[c]); // new
    expect(&mut s, 3, &[a, b]); // previous + demolition
    expect(&mut s, 4, &[a, c]); // previous + new
    expect(&mut s, 5, &[a]); // previous phase
    expect(&mut s, 6, &[b, c, d]); // demolition + new
    // "Complete" shows existing walls by category: no override.
    let complete = filter(&mut s, 1);
    run(&mut s, "view_set", json!({"view": plan, "phase_filter": complete}));
    assert_eq!(cut_pens(&mut s, plan, a), BTreeSet::from([(None, false)]));
    // No filter: every status by category.
    run(&mut s, "view_set", json!({"view": plan, "phase_filter": null}));
    let shown = drawn(&mut s, plan);
    assert!([a, b, c, d].iter().all(|w| shown.contains(w)));
    assert_eq!(cut_pens(&mut s, plan, b), BTreeSet::from([(None, false)]));

    // Project graphics of a status change every view that overrides it.
    let all = filter(&mut s, 0);
    run(&mut s, "view_set", json!({"view": plan, "phase_filter": all}));
    run(
        &mut s,
        "phase_graphics_set",
        json!({"status": "demolished", "overrides": {"cut_lines": {"color": [200, 0, 0], "pattern": "Dot"}}}),
    );
    let colors: BTreeSet<_> = s
        .drawing(ViewId(plan))
        .unwrap()
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { style: LineStyle::Cut, element: Some(e), pen, .. } if e.0 == b => {
                pen.map(|p| (p.color, p.pattern))
            }
            _ => None,
        })
        .collect();
    assert_eq!(colors, BTreeSet::from([(Some([200, 0, 0]), Some(LinePattern::Dot))]));
    // A custom filter.
    let f = id(&run(
        &mut s,
        "phase_filter_create",
        json!({"name": "Demo only", "demolished": "overridden", "existing": "not_displayed", "new": "not_displayed", "temporary": "not_displayed"}),
    ));
    run(&mut s, "view_set", json!({"view": plan, "phase_filter": f}));
    assert!(drawn(&mut s, plan).contains(&b) && !drawn(&mut s, plan).contains(&a));
    run(&mut s, "phase_filter_set", json!({"filter": f, "existing": "by_category"}));
    assert!(drawn(&mut s, plan).contains(&a));
    run(&mut s, "phase_filter_delete", json!({"filter": "Demo only"}));
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["phase_filter"], Value::Null);
}

#[test]
fn demolishing_takes_hosted_elements_and_undoes() {
    let (mut s, plan, old_plan, [a, _, c, _]) = renovation();
    let (existing, new) = phases(&mut s);
    // A door in the existing wall, built with it.
    let door = id(&run(&mut s, "door_create", json!({"wall": a, "offset": 1.0})));
    run(&mut s, "elements_phase_set", json!({"ids": [door], "created": existing}));
    let window = id(&run(&mut s, "window_create", json!({"wall": a, "offset": 3.0})));
    run(&mut s, "elements_phase_set", json!({"ids": [window], "created": existing}));
    // The door alone is demolished in the existing phase (temporary there).
    run(&mut s, "elements_demolish", json!({"ids": [door], "view": old_plan}));
    let out = run(&mut s, "elements_demolish", json!({"ids": [a], "view": plan}));
    // The wall takes its window along; the door keeps its earlier demolition.
    assert_eq!(out["demolished"], json!([a, window]));
    let get = |s: &mut Session, e: u64| run(s, "element_get", json!({"id": e}));
    assert_eq!(get(&mut s, window)["phase_demolished"]["id"], json!(new));
    assert_eq!(get(&mut s, door)["phase_demolished"]["id"], json!(existing));
    // One undo step brings both back.
    assert!(s.doc.undo());
    assert_eq!(get(&mut s, a)["phase_demolished"], Value::Null);
    assert_eq!(get(&mut s, window)["phase_demolished"], Value::Null);
    // A hosted element is never older than its host: a door put in an
    // existing phase in a new wall is built with the wall.
    let new_door = id(&run(&mut s, "door_create", json!({"wall": c, "offset": 1.0})));
    run(&mut s, "elements_phase_set", json!({"ids": [new_door], "created": existing}));
    assert_eq!(get(&mut s, new_door)["phase_created"]["id"], json!(new));
    // Nothing is demolished before it is built, or twice.
    assert_eq!(fails(&mut s, "elements_demolish", json!({"ids": [c], "phase": existing})), ErrorCode::Rejected);
    assert_eq!(fails(&mut s, "elements_phase_set", json!({"ids": [c], "demolished": existing})), ErrorCode::Rejected);
    // `null` stands it again.
    run(&mut s, "elements_demolish", json!({"ids": [c], "phase": new}));
    run(&mut s, "elements_phase_set", json!({"ids": [c], "demolished": null}));
    assert_eq!(get(&mut s, c)["phase_demolished"], Value::Null);
}

#[test]
fn rooms_and_openings_follow_the_phase() {
    let mut s = Session::default();
    let (existing, new) = phases(&mut s);
    let plan = id(&run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]);
    let old_plan = id(&run(&mut s, "view_duplicate", json!({"view": plan})));
    run(&mut s, "view_set", json!({"view": old_plan, "phase": existing}));
    // An 8 x 4 box split in two by a partition demolished in the new phase.
    let pts = [[0.0, 0.0], [8.0, 0.0], [8.0, 4.0], [0.0, 4.0]];
    let mut walls = Vec::new();
    for i in 0..4 {
        walls.push(id(&run(&mut s, "wall_create", json!({"start": pts[i], "end": pts[(i + 1) % 4]}))));
    }
    let partition = id(&run(&mut s, "wall_create", json!({"start": [4.0, 0.0], "end": [4.0, 4.0]})));
    walls.push(partition);
    run(&mut s, "elements_phase_set", json!({"ids": walls, "created": existing}));
    run(&mut s, "elements_demolish", json!({"ids": [partition], "phase": new}));
    // A room of the existing phase is bounded by the partition; one of the
    // new phase spans the whole box.
    run(&mut s, "phase_creation_set", json!({"view": old_plan}));
    let old_room = id(&run(&mut s, "room_create", json!({"point": [2.0, 2.0]})));
    run(&mut s, "phase_creation_set", json!({"view": plan}));
    let new_room = id(&run(&mut s, "room_create", json!({"point": [2.0, 2.0]})));
    let area = |s: &mut Session, r: u64| run(s, "element_get", json!({"id": r}))["derived"]["area"].as_f64().unwrap();
    let (old_area, new_area) = (area(&mut s, old_room), area(&mut s, new_room));
    assert!(old_area < new_area * 0.6, "{old_area} vs {new_area}");
    assert!(new_area > 25.0, "{new_area}");
    // The existing room is demolished with the partition's removal.
    run(&mut s, "elements_demolish", json!({"ids": [old_room], "phase": new}));
    assert!(drawn(&mut s, plan).contains(&new_room));
    assert!(!drawn(&mut s, old_plan).contains(&new_room));

    // A door of the new phase does not open the wall in the existing phase.
    let door = id(&run(&mut s, "door_create", json!({"wall": walls[0], "offset": 2.0})));
    assert_eq!(s.view_geometry(ViewId(plan)).walls[&ElementId(walls[0])].openings.len(), 1);
    assert!(s.view_geometry(ViewId(old_plan)).walls[&ElementId(walls[0])].openings.is_empty());
    assert!(!drawn(&mut s, old_plan).contains(&door));
    // The partition is still drawn (dashed) in the phase it is demolished in.
    assert!(drawn(&mut s, plan).contains(&partition));
}

#[test]
fn schedules_follow_the_phase_and_show_phases() {
    let (mut s, _plan, _old, [a, b, c, d]) = renovation();
    let (existing, _new) = phases(&mut s);
    let sched = id(&run(&mut s, "view_create", json!({"kind": "schedule", "category": "wall"})));
    run(
        &mut s,
        "view_set",
        json!({"view": sched, "params": {"def": {"fields": ["Mark", "PhaseCreated", "PhaseDemolished"]}}}),
    );
    let rows =
        |s: &mut Session| -> Vec<u64> { s.schedule(ViewId(sched)).unwrap().rows.iter().map(|r| r.element.0).collect() };
    assert_eq!(rows(&mut s), vec![a, b, c, d]);
    let new_only = filter(&mut s, 2);
    run(&mut s, "view_set", json!({"view": sched, "phase_filter": new_only}));
    assert_eq!(rows(&mut s), vec![c]);
    run(&mut s, "view_set", json!({"view": sched, "phase": existing, "phase_filter": null}));
    assert_eq!(rows(&mut s), vec![a, b]);
    let names: Vec<String> = run(&mut s, "phases_list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    let table = s.schedule(ViewId(sched)).unwrap();
    // Cached until the document changes.
    assert!(std::rc::Rc::ptr_eq(&table, &s.schedule(ViewId(sched)).unwrap()));
    assert_eq!(table.headers, vec!["col-mark", "col-phase-created", "col-phase-demolished"]);
    assert_eq!(table.rows[1].cells[1].raw(), names[0]);
    assert_eq!(table.rows[1].cells[2].raw(), names[1]);
    // View filters can test the phase fields.
    let f = id(&run(
        &mut s,
        "filter_create",
        json!({"name": "Demolished", "rules": [{"param": "phase_demolished", "op": "has_value"}]}),
    ));
    let hits = run(&mut s, "filter_elements", json!({"filter": f}));
    assert_eq!(hits["ids"], json!([b, d]));
}

#[test]
fn phases_are_created_renamed_reordered_and_deleted() {
    let (mut s, plan, _old, [_, _, c, _]) = renovation();
    let (existing, new) = phases(&mut s);
    let demo = id(&run(&mut s, "phase_create", json!({"name": "Demolition", "before": new})));
    let list = run(&mut s, "phases_list", json!({}));
    assert_eq!(list.as_array().unwrap().iter().map(id).collect::<Vec<_>>(), vec![existing, demo, new]);
    assert_eq!(list[2]["created"], json!(2));
    assert_eq!(fails(&mut s, "phase_create", json!({"name": "demolition"})), ErrorCode::Rejected);
    run(&mut s, "phase_set", json!({"phase": demo, "name": "Demo"}));
    // Moving the existing phase after the new one would demolish walls
    // before building them.
    assert_eq!(fails(&mut s, "phase_set", json!({"phase": existing, "index": 2})), ErrorCode::Rejected);
    // The unused phase moves freely and goes without a merge.
    run(&mut s, "phase_set", json!({"phase": demo, "index": 0}));
    assert_eq!(id(&run(&mut s, "phases_list", json!({}))[0]), demo);
    run(&mut s, "elements_phase_set", json!({"ids": [c], "created": demo}));
    assert_eq!(fails(&mut s, "phase_delete", json!({"phase": demo})), ErrorCode::Rejected);
    run(&mut s, "phase_delete", json!({"phase": "Demo", "merge_into": existing}));
    assert_eq!(run(&mut s, "element_get", json!({"id": c}))["phase_created"]["id"], json!(existing));
    // Deleting the new phase merges its demolitions and views into the existing one.
    run(&mut s, "phase_delete", json!({"phase": new, "merge_into": existing}));
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["phase"]["id"], json!(existing));
    assert_eq!(fails(&mut s, "phase_delete", json!({"phase": existing})), ErrorCode::Rejected);
    // All of it undoes.
    for _ in 0..4 {
        assert!(s.doc.undo());
    }
    assert_eq!(run(&mut s, "phases_list", json!({})).as_array().unwrap().len(), 3);
}

#[test]
fn templates_control_the_phase_filter_not_the_phase() {
    let (mut s, plan, _old, _) = renovation();
    let (existing, _) = phases(&mut s);
    let t = id(&run(&mut s, "template_create", json!({"name": "Demolition plan", "view": plan})));
    run(&mut s, "view_template_assign", json!({"views": [plan], "template": t}));
    let previous_demo = filter(&mut s, 3);
    assert_eq!(fails(&mut s, "view_set", json!({"view": plan, "phase_filter": previous_demo})), ErrorCode::Rejected);
    run(&mut s, "template_set", json!({"template": t, "phase_filter": previous_demo}));
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["phase_filter"]["id"], json!(previous_demo));
    // The phase stays the view's own.
    run(&mut s, "view_set", json!({"view": plan, "phase": existing}));
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["phase"]["id"], json!(existing));
}

#[test]
fn files_repair_phases_and_old_files_get_them() {
    let (s, _, _, [a, b, _, _]) = renovation();
    let mut p = s.project().clone();
    // Dangling and inconsistent references.
    p.elements.get_mut(&ElementId(a)).unwrap().phase_created = cc_core::PhaseId(999);
    let last = p.last_phase().unwrap();
    let first = p.phases[0].id;
    {
        let e = p.elements.get_mut(&ElementId(b)).unwrap();
        e.phase_created = last;
        e.phase_demolished = Some(first);
    }
    let view = *p.views.keys().next().unwrap();
    p.views.get_mut(&view).unwrap().phase = cc_core::PhaseId(998);
    p.views.get_mut(&view).unwrap().phase_filter = Some(cc_core::PhaseFilterId(997));
    let text = cc_core::to_ron(&p).unwrap();
    let (back, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert_eq!(repairs, 4);
    assert_eq!(back.element(ElementId(a)).unwrap().phase_created, last);
    assert_eq!(back.element(ElementId(b)).unwrap().phase_demolished, None);
    assert_eq!(back.view(view).unwrap().phase, last);
    assert_eq!(back.view(view).unwrap().phase_filter, None);

    // A file from before phases: everything in the last of the template's
    // phases, views with the "show all" filter, nothing to repair.
    let mut old = s.project().clone();
    old.phases.clear();
    old.phase_filters.clear();
    for e in old.elements.values_mut() {
        e.phase_created = cc_core::PhaseId(0);
        e.phase_demolished = None;
    }
    for v in old.views.values_mut() {
        v.phase = cc_core::PhaseId(0);
        v.phase_filter = None;
    }
    let text = cc_core::to_ron(&old).unwrap().replacen(
        &format!("format_version: {}", cc_core::FORMAT_VERSION),
        "format_version: 10",
        1,
    );
    assert!(!text.contains("phase_created: 999"));
    let (back, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert_eq!(repairs, 0);
    assert_eq!(back.phases.len(), 2);
    assert_eq!(back.phase_filters.len(), 7);
    let last = back.last_phase().unwrap();
    assert!(back.elements.values().filter(|e| e.category().is_phased()).all(|e| e.phase_created == last));
    let all = back.default_phase_filter();
    assert!(all.is_some() && back.views.values().all(|v| v.phase == last && v.phase_filter == all));
    // And it round-trips.
    let again = cc_core::from_ron(&cc_core::to_ron(&back).unwrap()).unwrap();
    assert_eq!(again, back);
}
