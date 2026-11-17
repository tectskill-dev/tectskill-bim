//! Recorded macros play anywhere: on the level active when they run, with
//! the elements earlier steps created (issue #275).

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementId, ElementKind, LevelId};
use cc_engine::Session;
use cc_engine::script::{self, Step};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn levels(s: &Session) -> (LevelId, LevelId) {
    let levels = &s.project().levels;
    assert!(levels.len() >= 2, "the template has two levels");
    (levels[0].id, levels[1].id)
}

/// Walls, then a window and a door in two of them, drawn on `level` (as the
/// tools do: the level and the host wall by id).
fn record_house(s: &mut Session, level: LevelId) -> Vec<Step> {
    s.set_active_level(Some(level));
    s.start_recording();
    let a = run(s, "wall_create", json!({"level": level.0, "start": [0, 0], "end": [6, 0], "height": 3.0}));
    let b = run(s, "wall_create", json!({"level": level.0, "start": [6, 0], "end": [6, 4], "height": 3.0}));
    run(s, "wall_create", json!({"level": level.0, "start": [6, 4], "end": [0, 4], "height": 3.0}));
    run(s, "window_create", json!({"wall": a["id"], "offset": 2.0, "sill": 1.0}));
    run(s, "door_create", json!({"wall": b["id"], "offset": 1.5, "sill": 0.0}));
    s.stop_recording()
}

/// Whether `id` is a door or window hosted by a wall on `level`.
fn hosted_on(s: &Session, id: ElementId, level: LevelId) -> bool {
    let p = s.project();
    let host = match &p.elements[&id].kind {
        ElementKind::Door(h) | ElementKind::Window(h) => h.host,
        other => panic!("not an opening: {other:?}"),
    };
    matches!(p.elements[&host].kind, ElementKind::Wall(_)) && p.elements[&host].level == level
}

#[test]
fn a_macro_recorded_on_one_level_plays_on_the_active_level() {
    let mut s = Session::default();
    let (ground, upper) = levels(&s);
    let steps = record_house(&mut s, ground);
    let text = script::to_text(&steps);
    assert!(text.contains(r#""level":{"$level":"active"}"#), "{text}");
    assert!(text.contains(r#""wall":{"$step":1}"#) && text.contains(r#""wall":{"$step":2}"#), "{text}");

    // The same project, with the upper level active.
    let before: Vec<ElementId> = s.project().elements.keys().copied().collect();
    s.set_active_level(Some(upper));
    let results =
        script::run(&mut s, &script::parse(&text).unwrap()).unwrap_or_else(|(_, e)| panic!("{}: {}", e.index, e.error));
    let new: Vec<ElementId> = results.iter().map(|r| ElementId(r["id"].as_u64().unwrap())).collect();
    assert!(new.iter().all(|id| !before.contains(id)));
    for wall in &new[..3] {
        assert_eq!(s.project().elements[wall].level, upper);
    }
    assert!(hosted_on(&s, new[3], upper) && hosted_on(&s, new[4], upper));
    let hosts: Vec<_> = new[3..]
        .iter()
        .map(|id| match &s.project().elements[id].kind {
            ElementKind::Door(h) | ElementKind::Window(h) => h.host,
            other => panic!("not an opening: {other:?}"),
        })
        .collect();
    assert_eq!(hosts, [new[0], new[1]]);

    // Another project: no active level plays on the lowest one.
    let mut other = Session::default();
    let (lowest, _) = levels(&other);
    script::run(&mut other, &steps).unwrap_or_else(|(_, e)| panic!("{}: {}", e.index, e.error));
    assert_eq!(other.project().walls().filter(|(e, _)| e.level == lowest).count(), 3);
    assert_eq!(other.project().elements.values().filter(|e| matches!(e.kind, ElementKind::Door(_))).count(), 1);
}

#[test]
fn levels_other_than_the_active_one_stay_as_recorded() {
    let mut s = Session::default();
    let (ground, upper) = levels(&s);
    s.set_active_level(Some(ground));
    s.start_recording();
    run(&mut s, "wall_create", json!({"level": upper.0, "start": [0, 0], "end": [6, 0]}));
    let steps = s.stop_recording();
    assert_eq!(steps[0].args["level"], json!(upper.0));
}

#[test]
fn scripts_with_literal_ids_still_run() {
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}));
    let text = format!(
        "{}\n{}\n",
        json!({"command": "window_create", "args": {"wall": wall["id"], "offset": 2.0}}),
        json!({"command": "wall_create", "args": {"level": s.project().levels[1].id.0, "start": [0, 5], "end": [6, 5]}})
    );
    let results = script::run(&mut s, &script::parse(&text).unwrap()).unwrap();
    assert_eq!(results.len(), 2);
    let made = ElementId(results[1]["id"].as_u64().unwrap());
    assert_eq!(s.project().elements[&made].level, s.project().levels[1].id);
}

#[test]
fn step_references_are_checked() {
    let mut s = Session::default();
    let bad = [
        // Not an earlier step.
        r#"{"command": "window_create", "args": {"wall": {"$step": 1}, "offset": 2}}"#.to_string(),
        r#"{"command": "window_create", "args": {"wall": {"$step": 18446744073709551615}}}"#.to_string(),
        // A path the result does not have.
        format!(
            "{}\n{}",
            r#"{"command": "wall_create", "args": {"start": [0, 0], "end": [6, 0]}}"#,
            r#"{"command": "window_create", "args": {"wall": {"$step": 1, "path": "/nope"}}}"#
        ),
        r#"{"command": "wall_create", "args": {"level": {"$level": "other"}, "start": [0, 0], "end": [6, 0]}}"#
            .to_string(),
    ];
    for text in bad {
        let steps = script::parse(&text).unwrap();
        let (_, e) = script::run(&mut s, &steps).expect_err(&text);
        assert_eq!(e.error.code, cc_engine::ErrorCode::InvalidArguments, "{text}");
    }
    // Deeply nested arguments are refused, not overflowed.
    let mut deep = json!(1);
    for _ in 0..200 {
        deep = json!([deep]);
    }
    let steps = [Step { command: "wall_create".into(), args: json!({"start": deep}) }];
    assert!(script::run(&mut s, &steps).is_err());
}

#[test]
fn a_reference_to_an_element_in_a_list_keeps_its_path() {
    let mut s = Session::default();
    let (ground, _) = levels(&s);
    s.set_active_level(Some(ground));
    s.start_recording();
    let a = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}));
    let copied = run(&mut s, "elements_copy", json!({"ids": [a["id"]], "delta": [0, 5]}));
    let copy = copied.pointer("/created/0").cloned().unwrap_or_else(|| panic!("{copied}"));
    run(&mut s, "window_create", json!({"wall": copy, "offset": 2.0}));
    let steps = s.stop_recording();
    let wall = &steps[2].args["wall"];
    assert_eq!(wall["$step"], 2, "{wall}");
    assert!(wall["path"].is_string(), "{wall}");
    script::run(&mut s, &steps).unwrap_or_else(|(_, e)| panic!("{}: {}", e.index, e.error));
}
