//! Steel and timber connections: placed on member ends by
//! `connections_create`, edited through `element_set`, removed with their
//! members and scheduled by plate mass and bolts.

use cc_core::connection::MemberEnd;
use cc_core::{Category, ElementId, ElementKind, ScheduleField};
use cc_engine::Session;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn created(v: &Value) -> Vec<u64> {
    v["created"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect()
}

/// Two columns 5 m apart with a beam between them (`main`), a beam from
/// the right column out to (5, 4) (`out`, its far end free) and one from
/// the middle of `main` to (2.5, -3) (`tee`, its start on `main`).
fn frame(s: &mut Session) -> ([u64; 2], u64, u64, u64) {
    let a = id(&run(s, "column_create", json!({"position": [0, 0]})));
    let b = id(&run(s, "column_create", json!({"position": [5, 0]})));
    let main = id(&run(s, "beam_create", json!({"start": [0, 0], "end": [5, 0]})));
    let out = id(&run(s, "beam_create", json!({"start": [5, 0], "end": [5, 4]})));
    let tee = id(&run(s, "beam_create", json!({"start": [2.5, 0], "end": [2.5, -3]})));
    ([a, b], main, out, tee)
}

/// The member and end of every connection.
fn placed(s: &Session) -> BTreeSet<(u64, MemberEnd)> {
    s.project()
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            ElementKind::Connection(c) => Some((c.member.0, c.end)),
            _ => None,
        })
        .collect()
}

#[test]
fn base_plates_go_on_every_column_once() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let columns = s.project().elements_of(Category::Column).count();
    let depth = s.doc.undo_depth();
    let first = run(&mut s, "connections_create", json!({"kind": "base_plate"}));
    assert_eq!(created(&first).len(), columns);
    assert_eq!(first["skipped"], json!(0));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    assert!(cols.iter().all(|c| placed(&s).contains(&(*c, MemberEnd::Base))));
    assert_eq!(s.project().types_of(Category::Connection).count(), 1, "the type comes with the first");
    let again = run(&mut s, "connections_create", json!({"kind": "base_plate"}));
    assert!(created(&again).is_empty());
    assert_eq!(again["skipped"], json!(columns));
    assert_eq!(s.doc.undo_depth(), depth + 1, "nothing to do is no undo step");
    run(&mut s, "undo", json!({}));
    assert!(placed(&s).is_empty());
    assert_eq!(s.project().types_of(Category::Connection).count(), 0, "undone with them");
}

#[test]
fn end_plates_go_only_on_beam_ends_at_columns() {
    let mut s = Session::default();
    let (_, main, out, _) = frame(&mut s);
    let r = run(&mut s, "connections_create", json!({"kind": "end_plate"}));
    assert_eq!(created(&r).len(), 3);
    let want = BTreeSet::from([(main, MemberEnd::Start), (main, MemberEnd::End), (out, MemberEnd::Start)]);
    assert_eq!(placed(&s), want);
}

#[test]
fn each_kind_takes_the_ends_it_suits() {
    for (kind, want) in [("fin_plate", 4), ("timber_bolts", 3), ("hanger", 1)] {
        let mut s = Session::default();
        let (_, _, _, tee) = frame(&mut s);
        let r = run(&mut s, "connections_create", json!({"kind": kind}));
        assert_eq!(created(&r).len(), want, "{kind}");
        if kind == "hanger" {
            assert_eq!(placed(&s), BTreeSet::from([(tee, MemberEnd::Start)]));
        }
    }
}

#[test]
fn bad_arguments_are_refused() {
    let mut s = Session::default();
    let (cols, main, ..) = frame(&mut s);
    for args in [
        json!({"kind": "weld"}),
        json!({"kind": "base_plate", "params": {"rows": 0}}),
        json!({"kind": "base_plate", "params": {"bolt_diameter": 3.0}}),
        json!({"kind": "base_plate", "params": {"bolts": 2}}),
        json!({"kind": "base_plate", "members": [main]}),
        json!({"kind": "end_plate", "members": [cols[0]]}),
    ] {
        assert!(s.execute("connections_create", args.clone()).is_err(), "{args}");
    }
    assert!(placed(&s).is_empty());
    let r =
        run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]], "params": {"rows": 3}}));
    let c = created(&r)[0];
    assert_eq!(run(&mut s, "element_get", json!({"id": c}))["derived"]["bolt_count"], json!(6));
}

#[test]
fn element_set_refuses_bad_parameters() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    let params = &run(&mut s, "element_get", json!({"id": c}))["params"];
    assert_eq!(params["kind"], json!("BasePlate"));
    assert_eq!(params["params"]["rows"], json!(2));
    let depth = s.doc.undo_depth();
    for patch in [
        json!({"params": {"rows": 0}}),
        json!({"params": {"columns": 11}}),
        json!({"params": {"bolt_diameter": 3.0}}),
        json!({"params": {"width": 0.0005}}),
        json!({"params": {"thickness": -0.01}}),
        json!({"member": cols[1]}),
        json!({"end": "Start"}),
        json!({"kind": "EndPlate"}),
    ] {
        assert!(s.execute("element_set", json!({"id": c, "params": patch})).is_err(), "{patch}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
    run(&mut s, "element_set", json!({"id": c, "params": {"params": {"columns": 3, "width": 0.6}}}));
    let d = &run(&mut s, "element_get", json!({"id": c}))["derived"];
    assert_eq!(d["bolt_count"], json!(6));
}

#[test]
fn deleting_a_column_removes_its_base_plate() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    run(&mut s, "elements_delete", json!({"ids": [cols[0]]}));
    assert!(s.project().element(ElementId(c)).is_none());
}

#[test]
fn repair_drops_a_second_connection_on_one_end() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    let mut p = s.project().clone();
    let mut dup = p.element(ElementId(c)).unwrap().clone();
    dup.id = ElementId(p.alloc_id());
    dup.uid = cc_core::Uid::generate();
    p.elements.insert(dup.id, dup);
    assert_eq!(p.repair(), 1);
    assert_eq!(p.elements_of(Category::Connection).count(), 1);
    assert!(p.element(ElementId(c)).is_some(), "the first one stays");
}

/// `derived` and the schedule fields agree: plate mass is volume x 7850.
#[test]
fn schedules_weigh_the_plates() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    let d = run(&mut s, "element_get", json!({"id": c}))["derived"].clone();
    let volume = d["plate_volume"].as_f64().unwrap();
    assert!(volume > 0.0);
    assert!((d["plate_mass"].as_f64().unwrap() - volume * 7850.0).abs() < 1e-9);
    assert_eq!((d["bolt_count"].clone(), d["bolt_diameter"].clone()), (json!(4), json!(0.02)));
    assert_eq!(d["warning"], Value::Null);
    let (p, geom) = s.project_and_geometry();
    let e = p.element(ElementId(c)).unwrap();
    let number = |f| cc_views::field_value(p, geom, e, f).and_then(|(_, n)| n);
    assert!((number(ScheduleField::PlateMass).unwrap() - volume * 7850.0).abs() < 1e-6);
    assert_eq!(number(ScheduleField::BoltCount), Some(4.0));
    assert_eq!(number(ScheduleField::BoltDiameter), Some(0.02));
    let column = p.element(ElementId(cols[1])).unwrap();
    assert!(cc_views::field_value(p, geom, column, ScheduleField::PlateMass).is_none(), "connections only");
    let view = run(&mut s, "view_create", json!({"kind": "schedule", "category": "connection"}));
    let table = run(&mut s, "schedule_table", json!({"view": view["id"]}));
    assert_eq!(table["rows"].as_array().unwrap().len(), 1);
}

#[test]
fn a_beam_outside_the_shown_geometry_counts_as_skipped() {
    let mut s = Session::default();
    let (_, _, _, tee) = frame(&mut s);
    run(&mut s, "option_set_create", json!({"name": "Layout", "options": ["Open", "Split"]}));
    run(&mut s, "elements_option_set", json!({"ids": [tee], "option": "Layout: Split"}));
    let r = run(&mut s, "connections_create", json!({"kind": "fin_plate", "members": [tee]}));
    assert!(created(&r).is_empty());
    assert_eq!(r["skipped"], json!(1), "{r}");
}

#[test]
fn a_connection_follows_its_member_to_another_level() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let r = run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]}));
    let c = ElementId(created(&r)[0]);
    let up = id(&run(&mut s, "level_create", json!({"name": "Upper", "elevation": 3.0})));
    run(&mut s, "element_set", json!({"id": cols[0], "level": up}));
    let level = |s: &Session, e: ElementId| s.project().element(e).unwrap().level;
    let col_level = level(&s, ElementId(cols[0]));
    assert_eq!(col_level.0, up);
    assert_eq!(level(&s, c), col_level, "the connection follows its column");
}
