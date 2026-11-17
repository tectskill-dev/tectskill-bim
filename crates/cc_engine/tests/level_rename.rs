//! A level and its views are named apart (issue #278): the plans made with
//! a level only start with its name. Renaming the level leaves them alone,
//! and renaming a plan leaves the level and its other plans alone.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{LevelId, ViewId};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

/// The names of the level's plans, sorted.
fn plan_names(s: &Session, level: u64) -> Vec<String> {
    let mut names: Vec<_> =
        s.project().views.values().filter(|v| v.level() == Some(LevelId(level))).map(|v| v.name.clone()).collect();
    names.sort();
    names
}

fn plans(s: &Session, level: u64) -> Vec<u64> {
    s.project().views.values().filter(|v| v.level() == Some(LevelId(level))).map(|v| v.id.0).collect()
}

/// A level with its floor and ceiling plans, a third plan of its name
/// (an MEP plan) and one named otherwise.
fn setup() -> (Session, u64, u64) {
    let mut s = Session::default();
    let level = id(&run(&mut s, "level_create", json!({ "name": "Mezanino", "elevation": 4.5 })));
    run(&mut s, "view_create", json!({ "kind": "electrical_plan", "level": level, "name": "Mezanino"}));
    let other = id(&run(&mut s, "view_create", json!({ "kind": "floor_plan", "level": level, "name": "Layout" })));
    (s, level, other)
}

fn level_name(s: &Session, level: u64) -> String {
    s.project().level_name(LevelId(level)).to_string()
}

#[test]
fn renaming_a_level_leaves_its_plans_names() {
    let (mut s, level, _) = setup();
    assert_eq!(plan_names(&s, level), ["Layout", "Mezanino", "Mezanino", "Mezanino"]);
    run(&mut s, "level_set", json!({ "level": level, "name": "  Sobreloja " }));
    assert_eq!(level_name(&s, level), "Sobreloja");
    assert_eq!(plan_names(&s, level), ["Layout", "Mezanino", "Mezanino", "Mezanino"]);
    // The plans are still the level's, which answers to its new name.
    assert_eq!(plans(&s, level).len(), 4);
    let got = run(&mut s, "level_set", json!({ "level": "Sobreloja", "elevation": 5.0 }));
    assert_eq!(id(&got), level);

    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(level_name(&s, level), "Mezanino");
}

#[test]
fn renaming_a_plan_leaves_its_level_and_the_other_plans() {
    let (mut s, level, other) = setup();
    // A plan that has the level's name.
    let plan = *plans(&s, level).iter().find(|p| s.project().view(ViewId(**p)).unwrap().name == "Mezanino").unwrap();
    run(&mut s, "view_set", json!({ "view": plan, "name": "Paredes" }));
    assert_eq!(level_name(&s, level), "Mezanino");
    assert_eq!(plan_names(&s, level), ["Layout", "Mezanino", "Mezanino", "Paredes"]);
    // And one that does not.
    run(&mut s, "view_set", json!({ "view": other, "name": "Layout 2" }));
    assert_eq!(level_name(&s, level), "Mezanino");
    assert_eq!(plan_names(&s, level), ["Layout 2", "Mezanino", "Mezanino", "Paredes"]);

    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(plan_names(&s, level), ["Layout", "Mezanino", "Mezanino", "Mezanino"]);
}

/// Typing a name key by key, as the levels dialog and the properties
/// palette do (merged steps), renames only what is being renamed.
#[test]
fn renaming_key_by_key_renames_only_the_level() {
    let (mut s, level, _) = setup();
    for name in ["M", "Me", "Mez"] {
        run(&mut s, "level_set", json!({ "level": level, "name": name }));
    }
    assert_eq!(level_name(&s, level), "Mez");
    assert_eq!(plan_names(&s, level), ["Layout", "Mezanino", "Mezanino", "Mezanino"]);
}

/// Deleting a plan keeps its level; deleting the level takes its plans.
#[test]
fn a_level_outlives_its_plans_but_not_the_other_way() {
    let (mut s, level, other) = setup();
    run(&mut s, "view_delete", json!({ "view": other }));
    assert_eq!(level_name(&s, level), "Mezanino");
    assert_eq!(plans(&s, level).len(), 3);
    run(&mut s, "level_delete", json!({ "level": level }));
    assert!(s.project().level(LevelId(level)).is_none());
    assert!(plans(&s, level).is_empty());
}
