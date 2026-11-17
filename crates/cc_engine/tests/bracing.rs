//! Parametric bracing: `bracing_create` makes a host that keeps the braces
//! of a bay in step with its pattern, storeys and brace type.

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

fn bracings(s: &Session) -> usize {
    s.project().elements.values().filter(|e| matches!(e.kind, ElementKind::Bracing(_))).count()
}

fn members(s: &mut Session, host: u64) -> Vec<u64> {
    let d = run(s, "element_get", json!({"id": host}))["derived"].clone();
    d["members"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}

#[test]
fn every_pattern_makes_its_braces_per_storey() {
    let mut s = Session::default();
    for (pattern, per_story) in [("x", 2), ("diagonal", 1), ("v", 2), ("chevron", 2), ("k", 4)] {
        let args = json!({"start": [0, 0], "end": [6, 0], "pattern": pattern, "height": 3.0, "stories": 3});
        let depth = s.doc.undo_depth();
        let out = run(&mut s, "bracing_create", args);
        assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
        let made = out["members"].as_array().unwrap();
        assert_eq!(made.len(), 3 * per_story, "{pattern}");
        for m in made {
            let e = s.project().element(ElementId(m.as_u64().unwrap())).unwrap();
            assert!(matches!(e.kind, ElementKind::Beam(_)), "braces are beams");
        }
    }
    assert_eq!(types_named(&s, "SHS 100x100x4"), 1, "the catalog brace type, made once");
    assert_eq!(bracings(&s), 5);
}

#[test]
fn a_bay_between_two_columns_takes_their_base_and_height() {
    let mut s = Session::default();
    let column = |s: &mut Session, x: f64| {
        let c = id(&run(s, "column_create", json!({"position": [x, 2.0], "height": 6.0})));
        run(s, "element_set", json!({"id": c, "params": {"base_offset": 0.5, "top": "Unconnected"}}));
        c
    };
    let (a, b) = (column(&mut s, 1.0), column(&mut s, 5.0));
    let out = run(&mut s, "bracing_create", json!({"columns": [a, b], "pattern": "chevron", "stories": 2}));
    let params = run(&mut s, "element_get", json!({"id": id(&out)}))["params"].clone();
    assert_eq!(params["start"], json!([1.0, 2.0]));
    assert_eq!(params["end"], json!([5.0, 2.0]));
    assert_eq!(params["height"], json!(3.0), "the columns' height over the storeys");
    assert_eq!(params["base_offset"], json!(0.5));
    assert_eq!(out["members"].as_array().unwrap().len(), 4);
    for bad in [json!({"columns": [a]}), json!({"columns": [a, b], "start": [0, 0], "end": [1, 0]})] {
        assert!(s.execute("bracing_create", bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn timber_gets_its_row_and_concrete_needs_a_type() {
    let mut s = Session::default();
    run(&mut s, "bracing_create", json!({"start": [0, 0], "end": [4, 0], "material": "timber"}));
    assert_eq!(types_named(&s, "Peça 6x12"), 1);
    let depth = s.doc.undo_depth();
    let concrete = json!({"start": [0, 5], "end": [4, 5], "material": "concrete"});
    assert!(s.execute("bracing_create", concrete).is_err());
    assert_eq!(s.doc.undo_depth(), depth);
    let beam = s.project().default_type(cc_core::Category::Beam).unwrap().0;
    run(&mut s, "bracing_create", json!({"start": [0, 5], "end": [4, 5], "material": "concrete", "type": beam}));
    assert_eq!(bracings(&s), 2);
}

#[test]
fn bad_values_are_refused_before_anything_is_made() {
    let mut s = Session::default();
    for extra in [
        json!({"pattern": "w"}),
        json!({"stories": 0}),
        json!({"stories": 101}),
        json!({"height": 0.1}),
        json!({"base_offset": 1e300}),
        json!({"material": "glass"}),
        json!({"type": "no such type"}),
        json!({"end": [0.1, 0]}),
    ] {
        let mut args = json!({"start": [0, 0], "end": [6, 0]});
        args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        assert!(s.execute("bracing_create", args.clone()).is_err(), "{args}");
    }
    assert!(s.execute("bracing_create", json!({"start": [0, 0]})).is_err());
    assert_eq!(bracings(&s), 0);
    assert_eq!(types_named(&s, "SHS 100x100x4"), 0);
}

#[test]
fn element_set_regenerates_in_one_step_and_a_deleted_brace_is_detached() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "bracing_create", json!({"start": [0, 0], "end": [6, 0], "height": 3.0})));
    let before = members(&mut s, host);
    assert_eq!(before.len(), 2);
    let depth = s.doc.undo_depth();
    run(&mut s, "element_set", json!({"id": host, "params": {"pattern": "K", "stories": 2}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let after = members(&mut s, host);
    assert_eq!(after.len(), 8);
    assert_eq!(after[0], before[0], "same id for the first brace");
    for bad in [json!({"members": []}), json!({"stories": 0}), json!({"height": 0})] {
        assert!(s.execute("element_set", json!({"id": host, "params": bad})).is_err(), "{bad}");
    }
    run(&mut s, "undo", json!({}));
    assert_eq!(members(&mut s, host), before);
    run(&mut s, "elements_delete", json!({"ids": [before[1]]}));
    let d = run(&mut s, "element_get", json!({"id": host}))["derived"].clone();
    assert_eq!(d["detached"], json!(1));
    run(&mut s, "element_set", json!({"id": host, "params": {"height": 4.0}}));
    assert_eq!(members(&mut s, host), [before[0]], "not made again");
    assert_eq!(run(&mut s, "element_get", json!({"id": host}))["derived"]["total_height"], json!(4.0));
}

#[test]
fn ifc_export_writes_the_braces_and_not_the_bracing() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "bracing_create", json!({"start": [0, 0], "end": [6, 0], "pattern": "k"})));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let p = s.project();
    let beams = p.elements.values().filter(|e| matches!(e.kind, ElementKind::Beam(_))).count();
    assert_eq!(text.matches("IFCBEAM(").count(), beams);
    let uid = cc_ifc::guid::encode(p.element(ElementId(host)).unwrap().uid.0);
    assert!(!text.contains(&uid), "the bracing itself is not exported");
}
