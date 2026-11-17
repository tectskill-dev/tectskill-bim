//! Parametric steel buildings: the host element keeps its columns, rafters,
//! purlins and girts in step with its parameters.

use cc_core::{ElementId, ElementKind};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn types_named(s: &Session, name: &str) -> usize {
    s.project().types.values().filter(|t| t.name == name).count()
}

fn derived(s: &mut Session, host: u64) -> Value {
    run(s, "element_get", json!({"id": host}))["derived"].clone()
}

fn column_x(s: &Session, id: u64) -> f64 {
    match s.project().element(ElementId(id)).map(|e| &e.kind) {
        Some(ElementKind::Column(c)) => c.position.x,
        _ => panic!("not a column"),
    }
}

fn buildings(s: &Session) -> usize {
    s.project().elements.values().filter(|e| matches!(e.kind, ElementKind::SteelBuilding(_))).count()
}

#[test]
fn defaults_make_the_catalog_types_once() {
    let mut s = Session::default();
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "steel_building_create", json!({}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    // 6 frames of 2 columns and 2 rafters, plus purlins and girts.
    assert!(out["members"].as_array().unwrap().len() > 24);
    assert_eq!(types_named(&s, "W310x38,7"), 2, "a column type and a beam type");
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 1, "purlins and girts share one");
    run(&mut s, "steel_building_create", json!({"origin": [40, 0]}));
    assert_eq!(types_named(&s, "W310x38,7"), 2);
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 1);
    assert_eq!(buildings(&s), 2);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(types_named(&s, "W310x38,7"), 0, "undone with the first building");
    assert_eq!(buildings(&s), 0);
}

#[test]
fn element_get_lists_the_members_by_role() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"bays": 2, "purlins": null, "girts": null})));
    let d = derived(&mut s, host);
    let len = |k: &str| d["members"][k].as_array().map_or(0, Vec::len);
    assert_eq!([len("columns"), len("rafters"), len("purlins"), len("girts")], [6, 6, 0, 0]);
    assert_eq!(d["detached"], json!(0));
    assert_eq!(d["at_limit"], json!(false));
    let params = &run(&mut s, "element_get", json!({"id": host}))["params"];
    assert_eq!(params["span"], json!(20.0));
    assert_eq!(params["roof"], json!("Gable"));
}

#[test]
fn element_set_regenerates_in_one_step() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"purlins": null, "girts": null})));
    let before = derived(&mut s, host)["members"].clone();
    let right = before["columns"][1].as_u64().unwrap();
    let depth = s.doc.undo_depth();
    run(&mut s, "element_set", json!({"id": host, "params": {"span": 24.0}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let after = derived(&mut s, host)["members"].clone();
    assert_eq!(after["columns"], before["columns"], "same ids");
    assert!((column_x(&s, right) - 24.0).abs() < 1e-9);
    run(&mut s, "undo", json!({}));
    assert!((column_x(&s, right) - 20.0).abs() < 1e-9);
}

#[test]
fn a_member_moved_by_command_is_detached() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"purlins": null, "girts": null})));
    let col = derived(&mut s, host)["members"]["columns"][0].as_u64().unwrap();
    run(&mut s, "elements_move", json!({"ids": [col], "delta": [0.5, 0]}));
    let d = derived(&mut s, host);
    assert_eq!(d["detached"], json!(1));
    assert_eq!(d["members"]["columns"].as_array().unwrap().len(), 11);
    run(&mut s, "element_set", json!({"id": host, "params": {"span": 30.0}}));
    assert!((column_x(&s, col) - 0.5).abs() < 1e-9, "left alone");
}

#[test]
fn out_of_range_values_are_refused() {
    let mut s = Session::default();
    for args in [
        json!({"span": 150}),
        json!({"bays": 0}),
        json!({"slope_percent": -1}),
        json!({"eave_height": 0.5}),
        json!({"purlins": {"spacing": 0.1}}),
        json!({"roof": "dome"}),
        json!({"column_type": "no such type"}),
    ] {
        assert!(s.execute("steel_building_create", args.clone()).is_err(), "{args}");
    }
    assert_eq!(buildings(&s), 0);
    assert_eq!(types_named(&s, "W310x38,7"), 0, "nothing was made before the refusal");
    let host = id(&run(&mut s, "steel_building_create", json!({})));
    let depth = s.doc.undo_depth();
    for params in [json!({"eave_height": 80}), json!({"members": []}), json!({"bay_spacing": 0})] {
        assert!(s.execute("element_set", json!({"id": host, "params": params})).is_err(), "{params}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
}

#[test]
fn ifc_export_writes_the_members_and_not_the_building() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"bays": 2})));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let p = s.project();
    let count = |f: fn(&ElementKind) -> bool| p.elements.values().filter(|e| f(&e.kind)).count();
    assert_eq!(text.matches("IFCCOLUMN(").count(), count(|k| matches!(k, ElementKind::Column(_))));
    assert_eq!(text.matches("IFCBEAM(").count(), count(|k| matches!(k, ElementKind::Beam(_))));
    let uid = cc_ifc::guid::encode(p.element(ElementId(host)).unwrap().uid.0);
    assert!(!text.contains(&uid), "the building itself is not exported");
}

#[test]
fn a_crafted_paste_does_not_adopt_foreign_elements() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"purlins": null, "girts": null})));
    let user = id(&run(&mut s, "column_create", json!({"position": [7.0, 7.0]})));
    let mut set = run(&mut s, "elements_export", json!({"ids": [host]}))["elements"].clone();
    // Point every listed member at the user's column, as a file from
    // another project whose ids collide might.
    let members = set[0]["kind"]["SteelBuilding"]["members"].as_array_mut().expect("a building with members");
    for m in members.iter_mut() {
        m["id"] = json!(user);
    }
    let created = run(&mut s, "elements_paste", json!({"elements": set, "delta": [60.0, 0.0]}))["created"].clone();
    let pasted = created[0].as_u64().unwrap();
    assert!((column_x(&s, user) - 7.0).abs() < 1e-9, "the user's column stays put");
    let d = derived(&mut s, pasted);
    let columns = d["members"]["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 12);
    assert!(columns.iter().all(|c| c.as_u64() != Some(user)));
    let original = derived(&mut s, host);
    assert_eq!(original["members"]["columns"].as_array().unwrap().len(), 12);
}

#[test]
fn type_delete_names_the_steel_building_using_it() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"girts": null})));
    // Every purlin deleted by hand: no element uses the type, the building still does.
    let purlins = derived(&mut s, host)["members"]["purlins"].clone();
    run(&mut s, "elements_delete", json!({"ids": purlins}));
    let err = s.execute("type_delete", json!({"type": "Ue 150x60x20x2,00"})).expect_err("refused");
    assert!(err.message.contains("steel buildings"), "{}", err.message);
}
