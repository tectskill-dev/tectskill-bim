//! Project browser organization (issue #278): named schemes group the
//! browser's views by type, level, discipline or phase; they are project
//! settings (undone, saved) and `browser_tree` gives the tree.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).expect_err(name).code
}

/// A project with a second level and, on the lowest, an electrical plan and
/// a structural plan next to the plans the levels came with.
fn setup() -> (Session, u64, u64) {
    let mut s = Session::default();
    let ground = s.project().levels.first().map(|l| l.id.0).unwrap();
    let upper = run(&mut s, "level_create", json!({ "name": "Upper", "elevation": 30.0 }))["id"].as_u64().unwrap();
    run(&mut s, "view_create", json!({ "kind": "electrical_plan", "level": ground, "name": "Power" }));
    run(&mut s, "view_create", json!({ "kind": "structural_plan", "level": ground, "name": "Formwork" }));
    (s, ground, upper)
}

/// The group rows of `nodes` as (criterion, value).
fn groups(nodes: &Value) -> Vec<(String, Value)> {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n.get("group").is_some())
        .map(|n| (n["group"].as_str().unwrap().to_string(), n["value"].clone()))
        .collect()
}

fn group<'a>(nodes: &'a Value, criterion: &str, value: Value) -> &'a Value {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["group"] == json!(criterion) && n["value"] == value)
        .unwrap_or_else(|| panic!("no {criterion} group {value} in {nodes}"))
}

/// The names of the view rows right under `nodes`.
fn views(nodes: &Value) -> Vec<String> {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n.get("view").is_some())
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_default_tree_lists_the_views_by_type() {
    let (mut s, _, _) = setup();
    let list = run(&mut s, "browser_schemes_list", json!({}));
    assert_eq!(list["in_use"], Value::Null);
    assert_eq!(list["schemes"], json!([]));
    assert_eq!(list["criteria"], json!(["view_type", "level", "discipline", "phase"]));

    let out = run(&mut s, "browser_tree", json!({}));
    assert_eq!(out["group_by"], json!(["view_type"]));
    let tree = &out["tree"];
    assert!(groups(tree).iter().all(|(c, _)| c == "view_type"));
    let plans = group(tree, "view_type", json!("FloorPlan"));
    assert_eq!(plans["views"].as_u64(), Some(views(&plans["children"]).len() as u64));
    // Plans come in the order of their levels: the upper level's last.
    assert_eq!(views(&plans["children"]).last().map(String::as_str), Some("Upper"));
    assert!(views(&plans["children"]).contains(&"Power".to_string()));
    let total: u64 = tree.as_array().unwrap().iter().map(|n| n["views"].as_u64().unwrap()).sum();
    assert_eq!(total as usize, s.project().views.len());
}

#[test]
fn a_scheme_nests_the_plans_under_their_levels() {
    let (mut s, ground, upper) = setup();
    let scheme = run(&mut s, "browser_scheme_create", json!({ "name": "By level" }));
    assert_eq!(scheme["group_by"], json!(["view_type", "level"]));
    assert_eq!(scheme["in_use"], json!(true));

    let out = run(&mut s, "browser_tree", json!({}));
    let plans = &group(&out["tree"], "view_type", json!("FloorPlan"))["children"];
    // A group per level, lowest first.
    let levels = groups(plans);
    assert_eq!(levels.len(), s.project().levels.len());
    assert_eq!(levels.first(), Some(&("level".to_string(), json!(ground))));
    assert_eq!(levels.last(), Some(&("level".to_string(), json!(upper))));
    assert!(views(plans).is_empty(), "every plan has a level");
    let on_ground = views(&group(plans, "level", json!(ground))["children"]);
    assert!(on_ground.contains(&"Power".to_string()) && on_ground.contains(&"Formwork".to_string()));
    assert_eq!(views(&group(plans, "level", json!(upper))["children"]), ["Upper"]);
    // Views without a level stay right under their type.
    let three_d = &group(&out["tree"], "view_type", json!("ThreeD"))["children"];
    assert!(groups(three_d).is_empty());

    // One undo step each: the scheme and its use go together.
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "browser_schemes_list", json!({}))["schemes"], json!([]));
    assert_eq!(run(&mut s, "browser_tree", json!({}))["group_by"], json!(["view_type"]));
}

#[test]
fn schemes_group_by_level_then_discipline_and_are_renamed_used_and_deleted() {
    let (mut s, ground, _) = setup();
    let args = json!({ "name": "Levels", "group_by": ["level", "discipline"], "use_it": false });
    let id = run(&mut s, "browser_scheme_create", args)["id"].as_u64().unwrap();
    assert_eq!(run(&mut s, "browser_schemes_list", json!({}))["in_use"], Value::Null);

    // Asked for by name, without being in use.
    let out = run(&mut s, "browser_tree", json!({ "scheme": "levels" }));
    let level = &group(&out["tree"], "level", json!(ground))["children"];
    assert_eq!(views(&group(level, "discipline", json!("electrical"))["children"]), ["Power"]);
    assert_eq!(views(&group(level, "discipline", json!("structure"))["children"]), ["Formwork"]);
    assert!(group(level, "discipline", json!("architecture"))["views"].as_u64().unwrap() >= 2);
    // Sections, 3D views and sheets have neither: they follow the levels.
    assert!(!views(&out["tree"]).is_empty());

    let used = run(&mut s, "browser_scheme_use", json!({ "scheme": id }));
    assert_eq!(used["group_by"], json!(["level", "discipline"]));
    let set = run(&mut s, "browser_scheme_set", json!({ "scheme": id, "name": "Storeys", "group_by": ["phase"] }));
    assert_eq!((set["name"].clone(), set["group_by"].clone()), (json!("Storeys"), json!(["phase"])));
    assert_eq!(run(&mut s, "browser_tree", json!({}))["group_by"], json!(["phase"]));

    // The same criteria without a scheme.
    let direct = run(&mut s, "browser_tree", json!({ "group_by": ["level", "discipline"] }));
    assert_eq!(direct["tree"], out["tree"]);

    run(&mut s, "browser_scheme_use", json!({}));
    assert_eq!(run(&mut s, "browser_tree", json!({}))["group_by"], json!(["view_type"]));
    run(&mut s, "browser_scheme_use", json!({ "scheme": "Storeys" }));
    run(&mut s, "browser_scheme_delete", json!({ "scheme": "Storeys" }));
    let list = run(&mut s, "browser_schemes_list", json!({}));
    assert_eq!((list["in_use"].clone(), list["schemes"].clone()), (Value::Null, json!([])));
}

#[test]
fn bad_schemes_are_refused() {
    let (mut s, _, _) = setup();
    run(&mut s, "browser_scheme_create", json!({ "name": "Mine" }));
    for args in [
        json!({ "name": " " }),
        json!({ "name": "mine" }),
        json!({ "name": "Other", "group_by": ["level", "color"] }),
        json!({ "name": "Other", "group_by": ["level", "Level"] }),
    ] {
        assert_eq!(fails(&mut s, "browser_scheme_create", args.clone()), ErrorCode::InvalidArguments, "{args}");
    }
    assert_eq!(fails(&mut s, "browser_scheme_use", json!({ "scheme": "nope" })), ErrorCode::NotFound);
    assert_eq!(fails(&mut s, "browser_scheme_delete", json!({ "scheme": 999_999 })), ErrorCode::NotFound);
    let both = json!({ "scheme": "Mine", "group_by": ["level"] });
    assert_eq!(fails(&mut s, "browser_tree", both), ErrorCode::InvalidArguments);
    assert_eq!(run(&mut s, "browser_schemes_list", json!({}))["schemes"].as_array().map(Vec::len), Some(1));
}

#[test]
fn schemes_are_saved_with_the_project() {
    let (mut s, _, _) = setup();
    let id = run(&mut s, "browser_scheme_create", json!({ "name": "By level" }))["id"].as_u64().unwrap();
    let ron = cc_core::to_ron(s.project()).unwrap();
    let (project, repairs) = cc_core::from_ron_with_repairs(&ron).unwrap();
    assert_eq!(repairs, 0);
    assert_eq!(project.browser, s.project().browser);
    assert_eq!(project.browser.in_use().map(|sc| sc.id.0), Some(id));
    // New ids do not collide with the scheme's.
    let mut project = project;
    assert!(project.alloc_id() > id);
}
