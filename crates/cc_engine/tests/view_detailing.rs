//! Reusing what is drawn in a view in other views (issue #159): pasting
//! detail lines, regions, notes and dimensions into another view, and
//! duplicating a view with its detailing.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn list(v: &Value) -> Vec<u64> {
    v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect()
}

/// The elements a view's drawing shows.
fn drawn(s: &mut Session, view: u64) -> BTreeSet<u64> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims.iter().filter_map(|p| p.element()).map(|e| e.0).collect()
}

fn owner(s: &Session, element: u64) -> Option<u64> {
    s.project().element(cc_core::ElementId(element)).and_then(|e| e.owner_view()).map(|v| v.0)
}

struct Site {
    plan: u64,
    level: u64,
    wall: u64,
    region: u64,
    lines: Vec<u64>,
    note: u64,
    dim: u64,
}

/// A plan with a wall, the plot boundary as detail lines, the buildable area
/// as a filled region, a note and a dimension measuring the wall.
fn site(s: &mut Session) -> Site {
    let level = id(&run(s, "levels_list", json!({}))[0]);
    let plan = id(&run(s, "views_list", json!({"category": "floor_plan"}))[0]);
    let wall = id(&run(s, "wall_create", json!({"level": level, "start": [0, 0], "end": [6, 0]})));
    let lines = list(
        &run(
            s,
            "detail_line_create",
            json!({"view": plan, "curves": [{"kind": "rectangle", "corner": [-2, -2], "opposite": [10, 8]}]}),
        )["created"],
    );
    let region = id(&run(
        s,
        "filled_region_create",
        json!({"view": plan, "curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [6, 4]}]}),
    ));
    let note = id(&run(s, "text_create", json!({"view": plan, "position": [1, 5], "text": "Buildable area"})));
    let dim = id(&run(s, "dimension_create", json!({"view": plan, "a": [0, 0], "b": [6, 0]})));
    let refs = &run(s, "element_get", json!({"id": dim}))["params"]["refs"];
    assert!(refs[0].is_object() && refs[1].is_object(), "the dimension measures the wall: {refs}");
    Site { plan, level, wall, region, lines, note, dim }
}

fn detailing(site: &Site) -> Vec<u64> {
    let mut ids = site.lines.clone();
    ids.extend([site.region, site.note, site.dim]);
    ids
}

#[test]
fn view_elements_paste_into_another_plan_in_place() {
    let mut s = Session::default();
    let site = site(&mut s);
    let other = id(&run(&mut s, "view_create", json!({"kind": "floor_plan", "level": site.level})));
    let set = run(&mut s, "elements_export", json!({"ids": detailing(&site)}));

    let pasted = list(&run(&mut s, "elements_paste", json!({"elements": set["elements"], "view": other}))["created"]);
    assert_eq!(pasted.len(), detailing(&site).len());
    assert!(pasted.iter().all(|e| owner(&s, *e) == Some(other)), "the copies belong to the target view");
    let shown = drawn(&mut s, other);
    assert!(pasted.iter().all(|e| shown.contains(e)), "and are drawn there");
    assert!(pasted.iter().all(|e| !drawn(&mut s, site.plan).contains(e)), "not in the first plan");

    // At the same drawing coordinates.
    let region =
        pasted.iter().copied().find(|e| run(&mut s, "element_get", json!({"id": e}))["category"] == "filledregion");
    let got = run(&mut s, "element_get", json!({"id": region.unwrap()}));
    let orig = run(&mut s, "element_get", json!({"id": site.region}));
    assert_eq!(got["params"]["outline"], orig["params"]["outline"]);

    // The pasted dimension still measures the wall, so it follows it.
    let dim = pasted.iter().copied().find(|e| run(&mut s, "element_get", json!({"id": e}))["category"] == "dimension");
    let dim = dim.unwrap();
    let refs = &run(&mut s, "element_get", json!({"id": dim}))["params"]["refs"];
    assert_eq!(refs[0]["target"]["Element"], json!(site.wall), "{refs}");
    run(&mut s, "element_set", json!({"id": site.wall, "params": {"end": [8, 0]}}));
    let b = &run(&mut s, "element_get", json!({"id": dim}))["params"]["b"];
    assert!((b[0].as_f64().unwrap() - 8.0).abs() < 1e-9, "{b}");

    // One undo step.
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert!(pasted.iter().all(|e| s.project().element(cc_core::ElementId(*e)).is_none()));
}

#[test]
fn view_elements_paste_onto_the_target_views_level() {
    let mut s = Session::default();
    let site = site(&mut s);
    let upper = run(&mut s, "level_create", json!({"elevation": 3.0}));
    let upper_plan = id(&run(&mut s, "view_create", json!({"kind": "floor_plan", "level": id(&upper)})));
    let set = run(&mut s, "elements_export", json!({"ids": [site.region, site.dim]}));
    let pasted =
        list(&run(&mut s, "elements_paste", json!({"elements": set["elements"], "view": upper_plan}))["created"]);
    for e in &pasted {
        let level = s.project().element(cc_core::ElementId(*e)).unwrap().level.0;
        assert_eq!(level, id(&upper), "a pasted element takes the target plan's level");
    }
    assert!(pasted.iter().all(|e| drawn(&mut s, upper_plan).contains(e)));
}

#[test]
fn view_elements_paste_only_where_the_view_takes_them() {
    let mut s = Session::default();
    let site = site(&mut s);
    let set = run(&mut s, "elements_export", json!({"ids": [site.region]}));
    let three_d = id(&run(&mut s, "view_create", json!({"kind": "three_d"})));
    assert_eq!(
        fails(&mut s, "elements_paste", json!({"elements": set["elements"], "view": three_d})),
        ErrorCode::InvalidArguments
    );
    // Into a drafting view the dimension stops measuring the model (another frame).
    let drafting = id(&run(&mut s, "view_create", json!({"kind": "drafting"})));
    let set = run(&mut s, "elements_export", json!({"ids": [site.dim, site.region]}));
    let pasted =
        list(&run(&mut s, "elements_paste", json!({"elements": set["elements"], "view": drafting}))["created"]);
    assert_eq!(pasted.len(), 2);
    let dim = pasted.iter().copied().find(|e| run(&mut s, "element_get", json!({"id": e}))["category"] == "dimension");
    let p = run(&mut s, "element_get", json!({"id": dim.unwrap()}))["params"].clone();
    assert!(p["refs"][0].is_null() && p["refs"][1].is_null(), "{p}");
    assert_eq!(p["a"], json!([0.0, 0.0]));
    // Model elements in the set ignore the view.
    let set = run(&mut s, "elements_export", json!({"ids": [site.wall]}));
    let wall = list(
        &run(&mut s, "elements_paste", json!({"elements": set["elements"], "view": three_d, "delta": [0, 10]}))["created"],
    );
    assert_eq!(wall.len(), 1);
    assert_eq!(owner(&s, wall[0]), None);
}

#[test]
fn view_duplicate_with_detailing_copies_what_is_drawn_in_it() {
    let mut s = Session::default();
    let site = site(&mut s);

    // Without detailing, only the view.
    let bare = run(&mut s, "view_duplicate", json!({"view": site.plan}));
    assert!(bare.get("copied").is_none());
    assert!(drawn(&mut s, id(&bare)).is_disjoint(&detailing(&site).into_iter().collect()));
    assert!(s.project().elements.values().all(|e| e.owner_view() != Some(cc_core::ViewId(id(&bare)))));

    let copy = run(&mut s, "view_duplicate", json!({"view": site.plan, "detailing": true}));
    let view = id(&copy);
    let copied = list(&copy["copied"]);
    assert_eq!(copied.len(), detailing(&site).len());
    assert!(copied.iter().all(|e| owner(&s, *e) == Some(view)));
    let shown = drawn(&mut s, view);
    assert!(copied.iter().all(|e| shown.contains(e)));
    assert!(shown.contains(&site.wall), "the model shows as in any plan");
    // The originals stay in the first plan.
    assert!(detailing(&site).iter().all(|e| owner(&s, *e) == Some(site.plan)));
    // The copied dimension measures the same wall.
    let dim = copied.iter().copied().find(|e| run(&mut s, "element_get", json!({"id": e}))["category"] == "dimension");
    let refs = &run(&mut s, "element_get", json!({"id": dim.unwrap()}))["params"]["refs"];
    assert_eq!(refs[1]["target"]["Element"], json!(site.wall), "{refs}");

    // View and copies in one undo step.
    run(&mut s, "undo", json!({}));
    assert!(s.project().view(cc_core::ViewId(view)).is_none());
    assert!(copied.iter().all(|e| s.project().element(cc_core::ElementId(*e)).is_none()));
}

#[test]
fn a_drafting_view_duplicates_with_its_detailing() {
    let mut s = Session::default();
    let drafting = id(&run(&mut s, "view_create", json!({"kind": "drafting"})));
    run(
        &mut s,
        "detail_line_create",
        json!({"view": drafting, "curves": [{"kind": "line", "start": [0, 0], "end": [1, 0]}]}),
    );
    let copy = run(&mut s, "view_duplicate", json!({"view": drafting, "detailing": true}));
    assert_eq!(list(&copy["copied"]).len(), 1);
    assert_eq!(copy["category"], run(&mut s, "view_get", json!({"view": drafting}))["category"]);
}
