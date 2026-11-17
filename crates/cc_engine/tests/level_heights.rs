//! Levels edited by storey height (issue #297): a new height moves the
//! levels above, a level added above or below another pushes the levels
//! beyond it away, and neither changes the order of the levels.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// Levels A to D at 0, 3, 6 and 9 m (the project's own levels renamed or removed).
fn setup() -> Session {
    let mut s = Session::default();
    let old: Vec<u64> = s.project().levels.iter().map(|l| l.id.0).collect();
    for (name, z) in [("A", 100.0), ("B", 103.0), ("C", 106.0), ("D", 109.0)] {
        run(&mut s, "level_create", json!({ "name": name, "elevation": z, "plans": false }));
    }
    for id in old {
        run(&mut s, "level_delete", json!({ "level": id }));
    }
    for (name, z) in [("A", 0.0), ("B", 3.0), ("C", 6.0), ("D", 9.0)] {
        run(&mut s, "level_set", json!({ "level": name, "elevation": z }));
    }
    s
}

/// Names and elevations, lowest first.
fn levels(s: &Session) -> Vec<(String, f64)> {
    s.project().levels.iter().map(|l| (l.name.clone(), (l.elevation * 1000.0).round() / 1000.0)).collect()
}

fn named(list: &[(&str, f64)]) -> Vec<(String, f64)> {
    list.iter().map(|(n, z)| ((*n).to_string(), *z)).collect()
}

#[test]
fn a_new_height_moves_the_levels_above_and_keeps_the_order() {
    let mut s = setup();
    // The value that reordered the levels when typed as an elevation.
    let out = run(&mut s, "level_set", json!({ "level": "B", "height": 7.4 }));
    assert_eq!(out["height"], json!(7.4));
    assert_eq!(levels(&s), named(&[("A", 0.0), ("B", 3.0), ("C", 10.4), ("D", 13.4)]));
    run(&mut s, "level_set", json!({ "level": "A", "height": 2.5 }));
    assert_eq!(levels(&s), named(&[("A", 0.0), ("B", 2.5), ("C", 9.9), ("D", 12.9)]));
    s.doc.undo();
    s.doc.undo();
    assert_eq!(levels(&s), named(&[("A", 0.0), ("B", 3.0), ("C", 6.0), ("D", 9.0)]));
}

#[test]
fn a_height_is_refused_where_it_means_nothing() {
    let mut s = setup();
    let before = levels(&s);
    for args in [
        json!({ "level": "D", "height": 3.0 }),
        json!({ "level": "B", "height": 0.0 }),
        json!({ "level": "B", "height": -1.0 }),
        json!({ "level": "B", "height": 4.0, "elevation": 2.0 }),
    ] {
        assert!(s.execute("level_set", args.clone()).is_err(), "{args}");
    }
    for args in [
        json!({ "above": "B", "below": "C" }),
        json!({ "above": "B", "elevation": 4.0 }),
        json!({ "height": 3.0 }),
        json!({ "above": "B", "height": 0.0 }),
        json!({ "above": "nowhere" }),
    ] {
        assert!(s.execute("level_create", args.clone()).is_err(), "{args}");
    }
    assert_eq!(levels(&s), before);
}

#[test]
fn a_level_added_above_pushes_the_upper_levels_up() {
    let mut s = setup();
    run(&mut s, "level_set", json!({ "level": "B", "height": 4.0 }));
    // The new storey takes the height of the one it is added over.
    let out = run(&mut s, "level_create", json!({ "name": "N", "above": "B" }));
    assert_eq!(out["elevation"], json!(7.0));
    assert_eq!(levels(&s), named(&[("A", 0.0), ("B", 3.0), ("N", 7.0), ("C", 11.0), ("D", 14.0)]));
    s.doc.undo();
    run(&mut s, "level_create", json!({ "name": "N", "above": "B", "height": 1.5 }));
    assert_eq!(levels(&s), named(&[("A", 0.0), ("B", 3.0), ("N", 4.5), ("C", 8.5), ("D", 11.5)]));
    // Over the highest level: the height of the storey below it.
    run(&mut s, "level_create", json!({ "name": "Top", "above": "D" }));
    assert_eq!(levels(&s).last().cloned(), Some(("Top".to_string(), 14.5)));
}

#[test]
fn a_level_added_below_pushes_the_lower_levels_down() {
    let mut s = setup();
    run(&mut s, "level_create", json!({ "name": "N", "below": "C", "height": 2.0 }));
    assert_eq!(levels(&s), named(&[("A", -2.0), ("B", 1.0), ("N", 4.0), ("C", 6.0), ("D", 9.0)]));
    // Under the lowest level: the height of the storey above it; one undo step.
    run(&mut s, "level_create", json!({ "name": "Base", "below": "A" }));
    assert_eq!(levels(&s).first().cloned(), Some(("Base".to_string(), -5.0)));
    s.doc.undo();
    assert_eq!(levels(&s).first().cloned(), Some(("A".to_string(), -2.0)));
}

#[test]
fn what_stands_on_a_moved_level_moves_with_it() {
    let mut s = setup();
    let wall = run(&mut s, "wall_create", json!({ "level": "C", "start": [0.0, 0.0], "end": [4.0, 0.0] }));
    let id = cc_core::ElementId(wall["id"].as_u64().unwrap());
    run(&mut s, "level_set", json!({ "level": "A", "height": 5.0 }));
    let p = s.project();
    let e = p.element(id).unwrap();
    let cc_core::ElementKind::Wall(w) = &e.kind else { panic!("not a wall") };
    assert!((p.wall_base_z(e, w) - 8.0).abs() < 1e-9);
}
