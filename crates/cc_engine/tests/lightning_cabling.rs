//! Lightning protection and structured cabling (issue #34): conductors,
//! air terminals, earth electrodes and the coverage check; racks, data
//! outlets and the cabling per rack.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

/// Four walls around a 10 x 8 m box, 3 m high, with a flat roof on top.
fn building(s: &mut Session) {
    let corners = [[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]];
    for i in 0..4 {
        run(s, "wall_create", json!({"start": corners[i], "end": corners[(i + 1) % 4], "height": 3.0}));
    }
    let outline = json!([[0, 0], [10, 0], [10, 8], [0, 8]]);
    run(s, "roof_create", json!({"boundary": outline, "offset": 3.0, "overhang": 0.0}));
}

#[test]
fn lightning_protection_conductors_terminals_and_electrodes() {
    let mut s = Session::default();
    building(&mut s);
    assert!(s.project().default_type(cc_core::Category::LightningConductor).is_none());
    // The first conductor brings its type and a lightning protection system, in one undo step.
    let depth = s.doc.undo_depth();
    let roof = run(&mut s, "lps_conductor_create", json!({"start": [0, 4], "end": [10, 4]}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    assert_eq!(roof["derived"]["role"], "air");
    let on_roof = num(&roof["params"]["run"]["start_offset"]);
    assert!(on_roof > 3.0 && on_roof < 3.6, "on the roof surface: {on_roof}");
    assert!((num(&roof["derived"]["length"]) - 10.0).abs() < 1e-9);
    assert!((num(&roof["derived"]["section_mm2"]) - 50.27).abs() < 0.01, "an 8 mm round conductor");
    let systems = run(&mut s, "systems_list", json!({}));
    let lps: Vec<&Value> = systems.as_array().unwrap().iter().filter(|x| x["kind"] == "lightning").collect();
    assert_eq!(lps.len(), 1);
    // A down conductor from the roof edge to the earthing, 0.5 m below the level.
    let down = run(&mut s, "lps_conductor_create", json!({"start": [0, 4]}));
    assert_eq!(down["derived"]["role"], "down");
    assert!((num(&down["params"]["run"]["start_offset"]) - on_roof).abs() < 1e-9);
    assert!((num(&down["params"]["run"]["end_offset"]) + 0.5).abs() < 1e-9);
    // The earth ring around the walls, 1 m out from their outer faces.
    let ring = run(&mut s, "earth_ring_create", json!({}));
    assert_eq!(ring["created"].as_array().unwrap().len(), 4);
    assert!(num(&ring["length"]) > 2.0 * (10.0 + 8.0) + 8.0 * 0.9, "{ring}");
    // An air terminal on the roof, its base on the roof surface; an earth electrode.
    let rod = run(&mut s, "air_terminal_place", json!({"position": [5, 4], "height": 2.0}));
    assert!((num(&rod["params"]["base_offset"]) - on_roof).abs() < 1e-6);
    let electrode = run(&mut s, "earth_electrode_place", json!({"position": [-1, -1]}));
    assert!((num(&electrode["params"]["base_offset"]) + 0.5).abs() < 1e-9);
    let all = run(&mut s, "systems_list", json!({}));
    let lps = all.as_array().unwrap().iter().find(|x| x["kind"] == "lightning").unwrap();
    assert_eq!(lps["air_terminals"], 1);
    assert_eq!(lps["earth_electrodes"], 1);
    assert!((num(&lps["conductor_length"]["air"]) - 10.0).abs() < 1e-9);
    assert!(num(&lps["conductor_length"]["earth"]) > 30.0);
    // Conductors meet at clamps: the roof conductor, the down conductor and the earth ring.
    let fittings = run(&mut s, "mep_fittings", json!({}));
    let clamps: f64 = fittings["fittings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["category"] == "lightningconductor")
        .map(|f| num(&f["count"]))
        .sum();
    assert_eq!(clamps, 5.0, "the ring's corners and the roof conductor's bend down: {fittings}");
    // The role can be changed through element_set; a pipe system is refused.
    let changed = run(&mut s, "element_set", json!({"id": id(&roof), "params": {"role": "Earth"}}));
    assert_eq!(changed["derived"]["role"], "earth");
    run(&mut s, "system_create", json!({"kind": "cold_water", "code": "AF"}));
    let code = fails(&mut s, "system_assign", json!({"elements": [id(&roof)], "system": "AF"}));
    assert_eq!(code, ErrorCode::InvalidArguments);
    // A schedule of conductors lists their role and length.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "lightningconductor"}));
    let table = run(&mut s, "schedule_table", json!({"view": id(&schedule)}));
    assert_eq!(table["rows"].as_array().unwrap().len(), 6, "{table}");
    // A lightning protection plan of the level draws them.
    let plan = run(&mut s, "view_create", json!({"kind": "lightning_plan"}));
    assert_eq!(plan["params"]["mep"], "Lightning", "{plan}");
}

#[test]
fn the_check_finds_what_the_air_termination_leaves_unprotected() {
    let mut s = Session::default();
    building(&mut s);
    let bare = run(&mut s, "lps_check", json!({}));
    assert_eq!(bare["class"], "III");
    assert!(num(&bare["checked"]) > 50.0 && bare["protected"] == 0, "{bare}");
    assert_eq!(bare["ok"], false);
    // A 3 m rod in the middle covers the 10 x 8 m roof for class III (reach sqrt(3 * 87) ≈ 16 m).
    run(&mut s, "air_terminal_place", json!({"position": [5, 4], "height": 3.0}));
    let one = run(&mut s, "lps_check", json!({"class": "III"}));
    assert_eq!(one["ok"], true, "{one}");
    assert_eq!(one["air_points"], 1);
    // Class I with a short rod leaves the corners out.
    run(&mut s, "undo", json!({}));
    run(&mut s, "air_terminal_place", json!({"position": [5, 4], "height": 0.3}));
    let strict = run(&mut s, "lps_check", json!({"class": "I"}));
    assert_eq!(strict["ok"], false);
    assert!(num(&strict["protected"]) > 0.0 && !strict["unprotected"].as_array().unwrap().is_empty());
    assert_eq!(fails(&mut s, "lps_check", json!({"class": "V"})), ErrorCode::InvalidArguments);
}

#[test]
fn structured_cabling_racks_outlets_and_cable_lengths() {
    let mut s = Session::default();
    let level_height = s.project().level_height(s.project().levels[0].id, 2.8);
    // A rack brings its type and its cabling, coded after its mark.
    let rack = run(&mut s, "rack_place", json!({"position": [0, 0]}));
    assert_eq!(rack["mark"], "R1");
    assert!((num(&rack["params"]["base_offset"]) - 1.4).abs() < 1e-9);
    let system = rack["system"].as_u64().unwrap();
    // Outlets default to the only rack; two jacks each.
    let a = run(&mut s, "data_outlet_place", json!({"position": [10, 5]}));
    assert_eq!(a["params"]["system"], system);
    assert_eq!(a["derived"]["cables"], 2);
    // Up 2.5 m to the ceiling, 15 m across, down to the rack, plus 3 m of slack.
    let expected = (level_height - 0.3) + 15.0 + (level_height - 1.4) + 3.0;
    assert!((num(&a["derived"]["cable_length"]) - expected).abs() < 1e-9, "{a}");
    // A one-jack outlet gets a type of its own; a second rack takes the next code.
    let b = run(&mut s, "data_outlet_place", json!({"position": [2, 2], "ports": 1}));
    assert_ne!(a["type"]["id"], b["type"]["id"]);
    let rack2 = run(&mut s, "rack_place", json!({"position": [50, 0]}));
    assert_eq!(rack2["mark"], "R2");
    let far = run(&mut s, "data_outlet_place", json!({"position": [150, 0], "rack": id(&rack2)}));
    assert_eq!(far["params"]["system"], rack2["system"]);
    // Without a rack named, two racks leave the outlet unassigned.
    let loose = run(&mut s, "data_outlet_place", json!({"position": [1, 1]}));
    assert!(loose["params"]["system"].is_null());
    run(&mut s, "system_assign", json!({"elements": [id(&loose)], "system": "R1"}));
    // Data outlets do not go on power circuits, nor racks feed circuits.
    run(&mut s, "system_create", json!({"kind": "power", "code": "C1"}));
    assert_eq!(
        fails(&mut s, "system_assign", json!({"elements": [id(&a)], "system": "C1"})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "system_create", json!({"kind": "power", "panel": id(&rack)})),
        ErrorCode::InvalidArguments
    );
    // Conduits and cable trays carry the cabling.
    let tray = run(&mut s, "cable_tray_create", json!({"start": [0, 0], "end": [10, 0], "circuits": ["R1", "C1"]}));
    assert_eq!(tray["derived"]["systems"].as_array().unwrap().len(), 2);
    // The cabling per rack.
    let list = run(&mut s, "cabling_list", json!({}));
    let racks = list["racks"].as_array().unwrap();
    assert_eq!(racks.len(), 2);
    let r1 = racks.iter().find(|r| r["code"] == "R1").unwrap();
    assert_eq!(r1["outlets"], 3);
    assert_eq!(r1["cables"], 2 + 1 + 2);
    assert_eq!(r1["over_limit"], 0);
    let r2 = racks.iter().find(|r| r["code"] == "R2").unwrap();
    assert_eq!(r2["over_limit"], 1, "a 100 m run is over the 90 m of a permanent link");
    let only = run(&mut s, "cabling_list", json!({"rack": id(&rack2)}));
    assert_eq!(only["racks"].as_array().unwrap().len(), 1);
    // A schedule of components shows the cable length of each outlet.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "component"}));
    let fields = json!({"def": {"fields": ["Mark", "Type", "System", "CableLength"]}});
    run(&mut s, "view_set", json!({"view": id(&schedule), "params": fields}));
    let table = run(&mut s, "schedule_table", json!({"view": id(&schedule)}));
    assert!(table.to_string().contains("R1"), "{table}");
    // A communications plan.
    let plan = run(&mut s, "view_create", json!({"kind": "communications_plan"}));
    assert_eq!(plan["params"]["mep"], "Communications", "{plan}");
    // The built-in types on demand.
    let t = run(&mut s, "device_type_create", json!({"device": "earth_electrode"}));
    let again = run(&mut s, "device_type_create", json!({"device": "earth_electrode"}));
    assert_eq!(t["id"], again["id"]);
    assert_eq!(fails(&mut s, "device_type_create", json!({"device": "toaster"})), ErrorCode::InvalidArguments);
}
