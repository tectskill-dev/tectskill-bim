//! Copying a host's reinforcement to others (`rebar_copy`): hosts of the same
//! kind and section, the sets chosen, replacing, and what is refused.

use cc_core::{ElementId, ElementKind, Rebar};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).expect_err("should fail").code
}

fn id(v: &Value) -> ElementId {
    ElementId(v["id"].as_u64().unwrap())
}

fn ids(v: &Value) -> Vec<ElementId> {
    v.as_array().unwrap().iter().map(|x| ElementId(x.as_u64().unwrap())).collect()
}

fn set(s: &Session, id: ElementId) -> Rebar {
    match &s.project().element(id).unwrap().kind {
        ElementKind::Rebar(r) => r.clone(),
        _ => panic!("a rebar set"),
    }
}

fn sets_of(s: &Session, host: ElementId) -> Vec<ElementId> {
    s.project().rebar_of(host).map(|(e, _)| e.id).collect()
}

/// A column type of the square outline `side` wide, under `name`.
fn square_type(s: &mut Session, name: &str, side: f64) -> u64 {
    let pts = [[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]];
    let curves: Vec<Value> =
        (0..4).map(|i| json!({"kind": "line", "start": pts[i], "end": pts[(i + 1) % 4]})).collect();
    run(s, "profile_create", json!({"name": name, "material": "concrete", "curves": curves}));
    run(s, "profile_type_create", json!({"profile": name, "category": "column"}))["id"].as_u64().unwrap()
}

fn column(s: &mut Session, ty: u64, x: f64) -> ElementId {
    id(&run(s, "column_create", json!({"position": [x, 0], "type": ty})))
}

/// A column with bars by face, cut in lifts, in a partition, and stirrups.
fn reinforced(s: &mut Session, ty: u64) -> (ElementId, ElementId, ElementId) {
    let c = column(s, ty, 0.0);
    let bars = id(&run(
        s,
        "rebar_create",
        json!({"host": c.0, "layout": "perimeter", "faces": [3, 2, 3, 2], "lifts": [1.2], "partition": "PILAR"}),
    ));
    let stirrups = id(&run(s, "rebar_create", json!({"host": c.0, "layout": "stirrups", "spacing": 0.1})));
    (c, bars, stirrups)
}

#[test]
fn sets_are_copied_to_columns_of_the_same_section_in_one_step() {
    let mut s = Session::default();
    let ty = square_type(&mut s, "P40", 0.4);
    let (c, bars, stirrups) = reinforced(&mut s, ty);
    // Another type of the same profile: the same section.
    let twin = run(&mut s, "profile_type_create", json!({"profile": "P40", "category": "column"}));
    let twin = twin["id"].as_u64().unwrap();
    let (a, b) = (column(&mut s, ty, 3.0), column(&mut s, twin, 6.0));
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "rebar_copy", json!({"from": c.0, "to": [a.0, b.0, a.0]}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let created = ids(&out["created"]);
    assert_eq!(created.len(), 4, "two sets in each of two columns, a repeated target once");
    assert!(ids(&out["replaced"]).is_empty());
    let geom = s.geometry().clone();
    for target in [a, b] {
        let copies = sets_of(&s, target);
        assert_eq!(copies.len(), 2);
        for (copy, original) in copies.iter().zip([bars, stirrups]) {
            let (mut got, want) = (set(&s, *copy), set(&s, original));
            assert_eq!(got.host, target);
            got.host = want.host;
            assert_eq!(got, want, "layout, cover, faces, lifts and partition go along");
            let (e, o) = (s.project().element(*copy).unwrap(), s.project().element(original).unwrap());
            assert_eq!(e.type_id, o.type_id, "the bar type");
            assert_eq!(e.level, s.project().element(target).unwrap().level);
            // The copies are laid out like the originals.
            assert_eq!(geom.rebar[copy].count(), geom.rebar[&original].count());
            assert!((geom.rebar[copy].total_length() - geom.rebar[&original].total_length()).abs() < 1e-9);
        }
    }
    s.doc.undo();
    assert!(sets_of(&s, a).is_empty() && sets_of(&s, b).is_empty());
}

#[test]
fn chosen_sets_are_copied_and_replace_removes_the_targets_own() {
    let mut s = Session::default();
    let ty = square_type(&mut s, "P40", 0.4);
    let (c, _bars, stirrups) = reinforced(&mut s, ty);
    let a = column(&mut s, ty, 3.0);
    let old = id(&run(&mut s, "rebar_create", json!({"host": a.0, "layout": "perimeter", "count": 4})));
    // Only the stirrups, beside what the target has.
    let out = run(&mut s, "rebar_copy", json!({"from": c.0, "to": [a.0], "sets": [stirrups.0]}));
    assert_eq!(ids(&out["created"]).len(), 1);
    assert_eq!(sets_of(&s, a).len(), 2);
    assert!(sets_of(&s, a).contains(&old));
    // Everything, in place of what the target has.
    let out = run(&mut s, "rebar_copy", json!({"from": c.0, "to": [a.0], "replace": true}));
    assert_eq!(ids(&out["replaced"]).len(), 2);
    assert_eq!(sets_of(&s, a), ids(&out["created"]));
    assert!(s.project().element(old).is_none());
    assert_eq!(sets_of(&s, c).len(), 2, "the source keeps its sets");
}

#[test]
fn walls_of_one_type_and_beams_of_one_section_take_copies() {
    let mut s = Session::default();
    let wall = |s: &mut Session, y: f64, length: f64| {
        id(&run(s, "wall_create", json!({"start": [0, y], "end": [length, y], "height": 3.0})))
    };
    let (w1, w2) = (wall(&mut s, 0.0, 6.0), wall(&mut s, 5.0, 4.0));
    let made = run(&mut s, "wall_rebar_create", json!({"walls": [w1.0]}));
    let n = ids(&made["created"]).len();
    assert!(n > 0);
    let out = run(&mut s, "rebar_copy", json!({"from": w1.0, "to": [w2.0]}));
    assert_eq!(ids(&out["created"]).len(), n);
    let geom = s.geometry().clone();
    assert!(sets_of(&s, w2).iter().all(|id| geom.rebar[id].count() > 0), "laid out in the shorter wall");

    let b1 = id(&run(&mut s, "beam_create", json!({"start": [0, 10], "end": [5, 10]})));
    let b2 = id(&run(&mut s, "beam_create", json!({"start": [0, 12], "end": [3, 12]})));
    run(&mut s, "rebar_create", json!({"host": b1.0, "layout": "bottom", "count": 3}));
    run(&mut s, "rebar_create", json!({"host": b1.0, "layout": "stirrups"}));
    let out = run(&mut s, "rebar_copy", json!({"from": b1.0, "to": [b2.0]}));
    assert_eq!(ids(&out["created"]).len(), 2);
    // A beam's sets do not go in a wall.
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": b1.0, "to": [w2.0]})), ErrorCode::InvalidArguments);
}

#[test]
fn hosts_that_do_not_match_are_refused_before_any_edit() {
    let mut s = Session::default();
    let ty = square_type(&mut s, "P40", 0.4);
    let (c, bars, _) = reinforced(&mut s, ty);
    let same = column(&mut s, ty, 3.0);
    let wide = square_type(&mut s, "P60", 0.6);
    let other = column(&mut s, wide, 6.0);
    let bare = column(&mut s, ty, 9.0);
    let floor = id(&run(&mut s, "floor_create", json!({"boundary": [[0, 20], [4, 20], [4, 24], [0, 24]]})));
    let foreign = id(&run(&mut s, "rebar_create", json!({"host": other.0, "layout": "stirrups"})));
    let depth = s.doc.undo_depth();
    let invalid = ErrorCode::InvalidArguments;
    // Another section, even with a good target beside it.
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": c.0, "to": [same.0, other.0]})), invalid);
    assert!(sets_of(&s, same).is_empty(), "nothing copied");
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": c.0, "to": [c.0]})), invalid);
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": c.0, "to": []})), invalid);
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": bare.0, "to": [same.0]})), invalid, "nothing to copy");
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": floor.0, "to": [same.0]})), invalid, "slabs");
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": bars.0, "to": [same.0]})), invalid, "a set is no host");
    // A set of another host.
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": c.0, "to": [same.0], "sets": [foreign.0]})), invalid);
    assert_eq!(fails(&mut s, "rebar_copy", json!({"from": c.0, "to": [999_999]})), ErrorCode::NotFound);
    assert_eq!(s.doc.undo_depth(), depth, "no undo step for a refused copy");
}
