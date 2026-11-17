//! French doors and double doors (#249): door types of one or two leaves,
//! glazed or not, their swings in plans and elevations and `door_type_create`.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::Session;
use cc_views::{LineStyle, Prim};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn plan(s: &mut Session) -> u64 {
    run(s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap()
}

fn door_in_wall(s: &mut Session, y: f64, ty: Value) -> u64 {
    let wall = run(s, "wall_create", json!({"start": [0, y], "end": [5, y]}));
    run(s, "door_create", json!({"wall": wall["id"], "point": [2.5, y], "type": ty}))["id"].as_u64().unwrap()
}

/// Leaves (projection lines) and arcs (thin polylines of 16 chords) of
/// element `id` in plan view `view`.
fn swings(s: &mut Session, view: u64, id: u64) -> (Vec<(f64, f64)>, usize) {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let mut leaves = Vec::new();
    let mut chords = 0;
    for p in &d.prims {
        match p {
            Prim::Line { a, b, style: LineStyle::Projection, element: Some(e), .. } if e.0 == id => {
                leaves.push((a.x, b.x));
            }
            Prim::Line { style: LineStyle::Thin, element: Some(e), .. } if e.0 == id => chords += 1,
            _ => {}
        }
    }
    assert_eq!(chords % 16, 0, "whole arcs only");
    (leaves, chords / 16)
}

#[test]
fn the_template_has_french_doors_of_one_and_two_leaves() {
    let mut s = Session::default();
    let single = run(&mut s, "type_get", json!({"type": "Porta-janela 80x220"}));
    let def = &single["params"]["def"];
    assert_eq!(def["kind"], "Door");
    assert!(def["opening"].get("door_operation").is_none(), "one leaf hinged on the left: the default");
    assert!(def["params"].as_array().unwrap().iter().any(|p| p["name"] == "Rodapé"), "the bottom rail parameter");
    let double = run(&mut s, "type_get", json!({"type": "Porta-janela 2 folhas 140x220"}));
    assert_eq!(double["params"]["def"]["opening"]["door_operation"], "DoubleDoor");
    // The plain doors stay as they were.
    let plain = run(&mut s, "type_get", json!({"type": "Porta 80x210"}));
    assert!(plain["params"]["def"]["opening"].get("door_operation").is_none());
}

#[test]
fn a_double_door_draws_two_swings_meeting_in_the_middle() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let single = door_in_wall(&mut s, 0.0, json!("Porta-janela 80x220"));
    let double = door_in_wall(&mut s, 3.0, json!("Porta-janela 2 folhas 140x220"));
    let (leaves, arcs) = swings(&mut s, view, single);
    assert_eq!((leaves.len(), arcs), (1, 1), "one leaf and its arc");
    let (leaves, arcs) = swings(&mut s, view, double);
    assert_eq!((leaves.len(), arcs), (2, 2), "two leaves and their arcs");
    // Hinged on either jamb of the 1.40 m opening around x = 2.5.
    let mut xs: Vec<f64> = leaves.iter().map(|(a, _)| *a).collect();
    xs.sort_by(f64::total_cmp);
    assert!((xs[0] - 1.8).abs() < 1e-6 && (xs[1] - 3.2).abs() < 1e-6, "{xs:?}");
    // A flipped hand keeps a double door symmetric.
    run(&mut s, "element_set", json!({"id": double, "params": {"flip_hand": true}}));
    assert_eq!(swings(&mut s, view, double).0.len(), 2);
}

#[test]
fn door_type_create_makes_french_and_double_doors() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let french = run(
        &mut s,
        "door_type_create",
        json!({"style": "french", "leaves": 2, "width": 1.6, "height": 2.4, "bottom_rail": 0.6}),
    );
    assert_eq!(french["name"], "Porta-janela 2 folhas 160x240", "{french}");
    let def = &french["params"]["def"];
    assert_eq!(def["opening"]["door_operation"], "DoubleDoor");
    let rail = def["params"].as_array().unwrap().iter().find(|p| p["name"] == "Rodapé").unwrap();
    assert!((rail["value"].as_f64().unwrap() - 0.6).abs() < 1e-12);
    let door = door_in_wall(&mut s, 0.0, french["id"].clone());
    assert_eq!(swings(&mut s, view, door).1, 2);
    let got = run(&mut s, "element_get", json!({"id": door}));
    assert!(got["params"]["sill_height"].as_f64().unwrap().abs() < 1e-12, "{got}");

    // A plain double door, under a name of its own.
    let double = run(&mut s, "door_type_create", json!({"leaves": 2, "name": "Double 140"}));
    assert_eq!(double["params"]["def"]["opening"]["door_operation"], "DoubleDoor");
    let door = door_in_wall(&mut s, 3.0, double["id"].clone());
    assert_eq!(swings(&mut s, view, door).1, 2);
    // The same default name twice gets a number.
    let again = run(
        &mut s,
        "door_type_create",
        json!({"style": "french", "leaves": 2, "width": 1.6, "height": 2.4, "bottom_rail": 0.6}),
    );
    assert_eq!(again["name"], "Porta-janela 2 folhas 160x240 (2)");

    // Refused: a taken name, three leaves, a rail leaving no glass, a rail on a solid door.
    for bad in [
        json!({"name": "Double 140"}),
        json!({"leaves": 3}),
        json!({"style": "french", "height": 2.2, "bottom_rail": 2.0}),
        json!({"bottom_rail": 0.4}),
        json!({"style": "sliding"}),
        json!({"width": f64::MAX}),
    ] {
        assert!(s.execute("door_type_create", bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn a_double_door_shows_two_triangles_in_elevation() {
    let mut s = Session::default();
    let views = run(&mut s, "views_list", json!({"category": "elevation"}));
    let south = views.as_array().unwrap().iter().find(|v| v["name"] == "Sul").unwrap()["id"].as_u64().unwrap();
    let double = door_in_wall(&mut s, 0.0, json!("Porta-janela 2 folhas 140x220"));
    let d = s.drawing(cc_core::ViewId(south)).unwrap();
    let dashed = d
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hidden, element: Some(e), .. } if e.0 == double))
        .count();
    assert_eq!(dashed, 4, "a triangle per leaf");
}
