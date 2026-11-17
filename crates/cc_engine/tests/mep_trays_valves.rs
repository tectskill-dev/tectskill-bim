//! Cable trays and valves (issue #33): tray runs alongside conduits, joined
//! into their own network and carrying circuits, and valves placed on
//! pipes, following them.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::ElementKind;
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

#[test]
fn cable_trays_run_carry_circuits_and_join_each_other() {
    let mut s = Session::default();
    assert!(s.project().default_type(cc_core::Category::CableTray).is_none(), "the template has no tray type");
    // The first tray brings its type, in one undo step.
    let depth = s.doc.undo_depth();
    let a = run(&mut s, "cable_tray_create", json!({"start": [0, 0], "end": [6, 0]}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    assert_eq!(a["derived"]["style"], "ladder");
    assert!((num(&a["derived"]["width"]) - 0.2).abs() < 1e-12 && (num(&a["derived"]["height"]) - 0.1).abs() < 1e-12);
    assert!((num(&a["derived"]["length"]) - 6.0).abs() < 1e-9);
    // Under the level above, clear of its slab.
    let level_height = s.project().level_height(s.project().levels[0].id, 2.8);
    assert!((num(&a["params"]["start_offset"]) - (level_height - 0.3)).abs() < 1e-9);
    // A wider solid-bottom type; a run from the first's end starts at its height.
    let wide = run(&mut s, "cable_tray_type_create", json!({"width": 0.3, "height": 0.1, "style": "solid_bottom"}));
    run(&mut s, "system_create", json!({"kind": "power", "code": "C1"}));
    let b = run(
        &mut s,
        "cable_tray_create",
        json!({"start": [6, 0], "end": [6, 4], "type": id(&wide), "circuits": ["C1"]}),
    );
    assert_eq!(b["params"]["start_offset"], a["params"]["start_offset"]);
    assert_eq!(b["derived"]["systems"][0]["code"], "C1");
    // They meet at an elbow of the tray network (between widths), apart from conduits.
    run(&mut s, "conduit_create", json!({"start": [6, 0], "end": [9, 0], "offset": num(&a["params"]["start_offset"])}));
    let fittings = run(&mut s, "mep_fittings", json!({}));
    let trays: Vec<&Value> =
        fittings["fittings"].as_array().unwrap().iter().filter(|f| f["category"] == "cabletray").collect();
    assert_eq!(trays.len(), 1, "{fittings}");
    assert_eq!(trays[0]["kind"], "elbow");
    assert_eq!(trays[0]["diameters_mm"], json!([300, 200]));
    // Circuits only: a plumbing system is refused.
    let (tray_a, tray_b) = (id(&a), id(&b));
    assert_eq!(
        fails(&mut s, "system_assign", json!({"elements": [tray_a], "system": "AF"})),
        ErrorCode::InvalidArguments
    );
    run(&mut s, "system_assign", json!({"elements": [tray_a], "system": "C1"}));
    let systems = run(&mut s, "systems_list", json!({}));
    let c1 = systems.as_array().unwrap().iter().find(|x| x["code"] == "C1").unwrap();
    assert!((num(&c1["length"]) - 10.0).abs() < 1e-9, "{c1}");
    // Deleting the circuit leaves the trays outside it.
    run(&mut s, "system_delete", json!({"system": "C1"}));
    assert_eq!(run(&mut s, "element_get", json!({"id": tray_b}))["derived"]["systems"], json!([]));
    // Edited through the generic parameters, like conduits.
    let got = run(&mut s, "element_set", json!({"id": tray_b, "params": {"end_offset": 2.0}}));
    assert!((num(&got["params"]["end_offset"]) - 2.0).abs() < 1e-12);
    // Sizes are checked.
    assert_eq!(fails(&mut s, "cable_tray_type_create", json!({"width": 5.0})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "cable_tray_type_create", json!({"style": "basket"})), ErrorCode::InvalidArguments);
    // Listed by category, scheduled by length.
    let listed = run(&mut s, "elements_list", json!({"category": "cabletray"}));
    assert_eq!(listed.as_array().unwrap().len(), 2);
}

#[test]
fn valves_sit_on_pipes_and_follow_them() {
    let mut s = Session::default();
    let pipe_type = s
        .project()
        .types_of(cc_core::Category::Pipe)
        .find(|t| t.diameter().is_some_and(|d| (d - 0.025).abs() < 1e-9))
        .unwrap()
        .id
        .0;
    let pipe = id(&run(&mut s, "pipe_create", json!({"start": [0, 0], "end": [4, 0], "type": pipe_type})));
    // On the pipe at a clicked point: its diameter, a gate valve type made for it.
    let v = run(&mut s, "valve_place", json!({"pipe": pipe, "point": [1.0, 0.1]}));
    let valve = id(&v);
    assert_eq!(v["derived"]["kind"], "gate");
    assert!((num(&v["derived"]["diameter"]) - 0.025).abs() < 1e-12);
    assert!((num(&v["params"]["at"]) - 1.0).abs() < 1e-9);
    assert!((num(&v["derived"]["point"][0]) - 1.0).abs() < 1e-9);
    // A check valve by kind (its type created), at a distance along the pipe.
    let check = run(&mut s, "valve_place", json!({"pipe": pipe, "distance": 3.0, "kind": "check"}));
    assert_eq!(check["derived"]["kind"], "check");
    assert_eq!(s.project().types_of(cc_core::Category::Valve).count(), 2);
    // Both are nodes of the pipe's network: three edges along it, no fittings.
    let net = &s.geometry().mep;
    assert_eq!(net.valves.len(), 2);
    assert_eq!(net.edges.iter().filter(|e| e.run.0 == pipe).count(), 3);
    assert!(net.fittings.is_empty());
    // Refused: off the pipe, past its end, on something else.
    assert_eq!(fails(&mut s, "valve_place", json!({"pipe": pipe, "point": [1.0, 2.0]})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "valve_place", json!({"pipe": pipe, "distance": 5.0})), ErrorCode::InvalidArguments);
    let conduit = id(&run(&mut s, "conduit_create", json!({"start": [0, 2], "end": [4, 2]})));
    assert_eq!(fails(&mut s, "valve_place", json!({"pipe": conduit})), ErrorCode::InvalidArguments);
    // Moving the pipe moves its valves; copying it copies them.
    run(&mut s, "elements_move", json!({"ids": [pipe], "delta": [0, 1]}));
    let moved = run(&mut s, "element_get", json!({"id": valve}));
    assert!((num(&moved["derived"]["point"][1]) - 1.0).abs() < 1e-9, "{moved}");
    let copies = run(&mut s, "elements_copy", json!({"ids": [pipe], "delta": [0, 3]}));
    let copied_pipe =
        copies["created"].as_array().map_or_else(|| copies[0].as_u64().unwrap(), |c| c[0].as_u64().unwrap());
    let hosted_by_copy = s
        .project()
        .elements
        .values()
        .filter(|e| matches!(&e.kind, ElementKind::Valve(v) if v.host.0 == copied_pipe))
        .count();
    assert_eq!(hosted_by_copy, 2, "{copies}");
    // Deleting a pipe deletes its valves.
    run(&mut s, "elements_delete", json!({"ids": [pipe]}));
    assert!(s.project().element(cc_core::ElementId(valve)).is_none());
    // Types of every kind at once.
    let types = run(&mut s, "valve_type_create", json!({"kinds": ["ball", "globe"]}));
    assert_eq!(types.as_array().unwrap().len(), 2);
    assert_eq!(fails(&mut s, "valve_type_create", json!({"kinds": ["butterfly"]})), ErrorCode::InvalidArguments);
}

#[test]
fn valves_off_pipes_are_dropped_on_load() {
    let mut s = Session::default();
    let pipe = id(&run(&mut s, "pipe_create", json!({"start": [0, 0], "end": [4, 0]})));
    let valve = id(&run(&mut s, "valve_place", json!({"pipe": pipe})));
    let conduit = id(&run(&mut s, "conduit_create", json!({"start": [0, 2], "end": [4, 2]})));
    // A crafted file puts the valve on a conduit, and another at a bad distance.
    let mut project = s.project().clone();
    if let Some(ElementKind::Valve(v)) = project.elements.get_mut(&cc_core::ElementId(valve)).map(|e| &mut e.kind) {
        v.host = cc_core::ElementId(conduit);
    }
    let text = cc_core::to_ron(&project).unwrap();
    let (loaded, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert!(repairs >= 1);
    assert!(loaded.element(cc_core::ElementId(valve)).is_none());
    let mut bad = s.project().clone();
    if let Some(ElementKind::Valve(v)) = bad.elements.get_mut(&cc_core::ElementId(valve)).map(|e| &mut e.kind) {
        v.at = f64::NAN;
    }
    assert!(bad.repair() >= 1);
    assert!(bad.element(cc_core::ElementId(valve)).is_none());
    // A huge tray section in a file is cut down.
    run(&mut s, "cable_tray_create", json!({"start": [0, 5], "end": [4, 5]}));
    let mut huge = s.project().clone();
    for t in huge.types.values_mut() {
        if let cc_core::TypeData::CableTray { width, .. } = &mut t.data {
            *width = 1e9;
        }
    }
    assert!(huge.repair() >= 1);
    let width = huge.types.values().find_map(|t| t.tray_section()).unwrap().0;
    assert!(width <= cc_core::mep::MAX_TRAY_WIDTH);
}
