//! Light steel and wood framing of walls: `wall_framing_create` makes one
//! host per wall, which keeps its studs, tracks, headers and sills in step
//! with its parameters, its wall and the wall's doors and windows.

use cc_core::wall_framing::{FramingRoleKind as K, WallFraming};
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

fn wall(s: &mut Session, start: [f64; 2], end: [f64; 2]) -> u64 {
    id(&run(s, "wall_create", json!({"start": start, "end": end, "height": 3.0})))
}

fn framing(s: &Session, host: u64) -> WallFraming {
    match s.project().element(ElementId(host)).map(|e| &e.kind) {
        Some(ElementKind::WallFraming(f)) => (**f).clone(),
        _ => panic!("not a wall framing"),
    }
}

fn count(f: &WallFraming, kind: K) -> usize {
    f.members.iter().filter(|m| m.role.kind == kind).count()
}

fn framings(s: &Session) -> usize {
    s.project().elements.values().filter(|e| matches!(e.kind, ElementKind::WallFraming(_))).count()
}

fn first_framing(out: &Value) -> u64 {
    id(&out["framings"][0])
}

#[test]
fn light_steel_defaults_make_the_catalog_types_once() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let b = wall(&mut s, [0.0, 5.0], [3.6, 5.0]);
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "wall_framing_create", json!({"walls": [a, b]}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let list = out["framings"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["wall"], json!(a));
    assert_eq!(list[0]["members"].as_array().unwrap().len(), 12, "10 studs and two tracks");
    let f = framing(&s, first_framing(&out));
    assert_eq!((count(&f, K::Stud), count(&f, K::TopTrack)), (10, 1));
    assert!((f.spacing - 0.4).abs() < 1e-9 && !f.double_top && !f.blocking);
    assert_eq!(types_named(&s, "Ue 90x40x12x0,95"), 1, "the studs' column type");
    assert_eq!(types_named(&s, "U 92x38x0,95"), 1, "the tracks' beam type");
    assert_eq!(types_named(&s, "Ue 140x40x12x0,95"), 1, "the headers' beam type");
    run(&mut s, "undo", json!({}));
    assert_eq!(framings(&s), 0);
    assert_eq!(types_named(&s, "Ue 90x40x12x0,95"), 0, "undone with the framings");
}

#[test]
fn wood_defaults_double_the_top_plate() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let out = run(&mut s, "wall_framing_create", json!({"walls": [a], "system": "wood", "blocking": true}));
    let f = framing(&s, first_framing(&out));
    assert!((f.spacing - 0.6).abs() < 1e-9 && f.double_top);
    assert_eq!((count(&f, K::TopTrack), count(&f, K::Stud), count(&f, K::Blocking)), (2, 7, 6));
    assert_eq!(types_named(&s, "Peça 6x8"), 2, "the studs' column type and the plates' beam type");
    assert_eq!(types_named(&s, "Peça 6x16"), 1, "the headers' beam type");
}

#[test]
fn studs_turn_their_depth_across_the_wall() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [0.0, 3.6]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let f = framing(&s, host);
    let stud = f.members.iter().find(|m| m.role.kind == K::Stud).unwrap().id;
    let e = s.project().element(stud).unwrap();
    let ElementKind::Column(c) = &e.kind else { panic!("a column") };
    assert!((c.rotation_deg - 90.0).abs() < 1e-9, "turned with the wall");
    // The section's width (local x) runs along the wall, its depth (local y) across it.
    let (width, depth) = cc_core::steel_building::section_box(s.project(), e.type_id);
    assert!(width < depth, "{width} x {depth}");
    assert!(c.position.x.abs() < 1e-9 && (c.position.y - width / 2.0).abs() < 1e-9, "flush with the wall start");
}

#[test]
fn refused_before_anything_is_made() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let curved = id(&run(&mut s, "wall_create", json!({"start": [0, 5], "end": [4, 5], "bulge": 0.3, "height": 3.0})));
    let depth = s.doc.undo_depth();
    for args in [
        json!({"walls": []}),
        json!({"walls": [curved]}),
        json!({"walls": [a, a]}),
        json!({"walls": [a], "spacing": 0.1}),
        json!({"walls": [a], "spacing": 1.3}),
        json!({"walls": [a], "system": "steel"}),
        json!({"walls": [a], "stud_type": "no such type"}),
        json!({"walls": [999_999]}),
    ] {
        assert!(s.execute("wall_framing_create", args.clone()).is_err(), "{args}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(framings(&s), 0);
    assert_eq!(types_named(&s, "Ue 90x40x12x0,95"), 0);
    run(&mut s, "wall_framing_create", json!({"walls": [a]}));
    let depth = s.doc.undo_depth();
    assert!(s.execute("wall_framing_create", json!({"walls": [a]})).is_err(), "one framing per wall");
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(framings(&s), 1);
}

#[test]
fn element_set_of_spacing_regenerates_in_one_step() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let before = framing(&s, host);
    let start = before.members.iter().find(|m| m.role.kind == K::Stud).unwrap().id;
    let depth = s.doc.undo_depth();
    run(&mut s, "element_set", json!({"id": host, "params": {"spacing": 0.6}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let after = framing(&s, host);
    assert_eq!(count(&after, K::Stud), 7);
    assert_eq!(after.members.iter().find(|m| m.role.kind == K::Stud).unwrap().id, start, "same id");
    let d = run(&mut s, "element_get", json!({"id": host}))["derived"].clone();
    assert_eq!(d["members"]["studs"].as_array().unwrap().len(), 7);
    assert_eq!(d["detached"], json!(0));
    for bad in [json!({"spacing": 2.0}), json!({"members": []}), json!({"wall": 0}), json!({"stud_type": 0})] {
        assert!(s.execute("element_set", json!({"id": host, "params": bad})).is_err(), "{bad}");
    }
    assert_eq!(s.doc.undo_depth(), depth + 1);
    run(&mut s, "undo", json!({}));
    assert_eq!(framing(&s, host), before);
}

#[test]
fn a_window_frames_its_opening_and_deleting_the_wall_removes_everything() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    run(&mut s, "window_create", json!({"wall": a, "offset": 1.8}));
    let f = framing(&s, host);
    assert_eq!((count(&f, K::KingStud), count(&f, K::JackStud), count(&f, K::Header)), (2, 2, 1));
    assert!(count(&f, K::Stud) < 10, "studs inside the opening went");
    let members: Vec<ElementId> = f.members.iter().map(|m| m.id).collect();
    run(&mut s, "elements_delete", json!({"ids": [a]}));
    assert_eq!(framings(&s), 0);
    assert!(members.iter().all(|m| s.project().element(*m).is_none()));
    run(&mut s, "undo", json!({}));
    assert_eq!(framings(&s), 1);
    assert!(members.iter().all(|m| s.project().element(*m).is_some()));
}

#[test]
fn ifc_export_leaves_the_framing_out() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let uid = cc_ifc::guid::encode(s.project().element(ElementId(host)).unwrap().uid.0);
    assert!(!text.contains(&uid), "the framing itself is not exported");
}

/// The beam of a member and the size of its section as drawn: (across, up).
fn beam_of(s: &Session, id: ElementId) -> (cc_core::Beam, (f64, f64)) {
    let e = s.project().element(id).unwrap();
    let ElementKind::Beam(b) = &e.kind else { panic!("a beam") };
    (b.clone(), cc_core::steel_building::section_box(s.project(), e.type_id))
}

fn column_of(s: &Session, id: ElementId) -> cc_core::Column {
    match &s.project().element(id).unwrap().kind {
        ElementKind::Column(c) => c.clone(),
        _ => panic!("a column"),
    }
}

fn first(f: &WallFraming, kind: K) -> ElementId {
    f.members.iter().find(|m| m.role.kind == kind).unwrap().id
}

/// The framing and its members.
fn with_members(s: &Session, host: u64) -> Vec<ElementId> {
    let mut ids = vec![ElementId(host)];
    ids.extend(framing(s, host).members.iter().map(|m| m.id));
    ids
}

#[test]
fn light_steel_tracks_lie_flat_on_the_catalog_row() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let f = frame(&mut s, json!({"walls": [a]}));
    // `U 92x38x0,95` turned on its side: 92 mm across the wall, 38 mm high.
    let (bottom, (across, up)) = beam_of(&s, first(&f, K::BottomTrack));
    assert!((across - 0.092).abs() < 1e-6 && (up - 0.038).abs() < 1e-6, "{across} x {up}");
    assert!((bottom.top_offset - 0.038).abs() < 1e-9, "the bottom track's top at the base plus 38 mm");
    let (top, _) = beam_of(&s, first(&f, K::TopTrack));
    assert!((top.top_offset - 3.0).abs() < 1e-9, "the top track's top at the wall top");
    let stud = column_of(&s, first(&f, K::Stud));
    assert!((stud.base_offset - 0.038).abs() < 1e-9, "the studs stand on the track");
    assert!((stud.height - (3.0 - 2.0 * 0.038)).abs() < 1e-9, "and reach the top track: {}", stud.height);
    // The type is the catalog row turned 90 degrees, made once.
    let track_type = s.project().element(first(&f, K::BottomTrack)).unwrap().type_id;
    let data = &s.project().element_type(track_type).unwrap().data;
    assert!(
        matches!(data, cc_core::TypeData::Beam { profile_rotation_deg, .. } if (*profile_rotation_deg - 90.0).abs() < 1e-9)
    );
    let b = wall(&mut s, [0.0, 5.0], [3.6, 5.0]);
    let g = frame(&mut s, json!({"walls": [b]}));
    assert_eq!(s.project().element(first(&g, K::BottomTrack)).unwrap().type_id, track_type, "reused");
    // A header stands on edge: the 140 mm depth is its height.
    let w = wall(&mut s, [0.0, 10.0], [3.6, 10.0]);
    run(&mut s, "window_create", json!({"wall": w, "offset": 1.8}));
    let h = frame(&mut s, json!({"walls": [w]}));
    let (_, (across, up)) = beam_of(&s, first(&h, K::Header));
    assert!((up - 0.14).abs() < 1e-6 && across < up, "{across} x {up}");
}

#[test]
fn wood_plates_lie_flat_on_the_catalog_row() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let f = frame(&mut s, json!({"walls": [a], "system": "wood"}));
    // `Peça 6x8` on its side: 8 cm across the wall, 6 cm high.
    let (bottom, (across, up)) = beam_of(&s, first(&f, K::BottomTrack));
    assert!((across - 0.08).abs() < 1e-6 && (up - 0.06).abs() < 1e-6, "{across} x {up}");
    assert!((bottom.top_offset - 0.06).abs() < 1e-9);
    let stud = column_of(&s, first(&f, K::Stud));
    assert!((stud.base_offset - 0.06).abs() < 1e-9);
    assert!((stud.height - (3.0 - 3.0 * 0.06)).abs() < 1e-9, "under two top plates: {}", stud.height);
}

#[test]
fn a_framing_follows_its_wall_when_demolished() {
    let mut s = Session::default();
    let phases = run(&mut s, "phases_list", json!({}));
    let (existing, new) = (id(&phases[0]), id(&phases[1]));
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    run(&mut s, "elements_phase_set", json!({"ids": [a], "created": existing}));
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let phase_of = |s: &Session, id: ElementId| {
        let e = s.project().element(id).unwrap();
        (e.phase_created.0, e.phase_demolished.map(|p| p.0))
    };
    let before: Vec<_> = with_members(&s, host).iter().map(|m| phase_of(&s, *m)).collect();
    let depth = s.doc.undo_depth();
    run(&mut s, "elements_demolish", json!({"ids": [a], "phase": new}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    for m in with_members(&s, host) {
        assert_eq!(phase_of(&s, m).1, Some(new), "{m:?} demolished with the wall");
    }
    run(&mut s, "undo", json!({}));
    let after: Vec<_> = with_members(&s, host).iter().map(|m| phase_of(&s, *m)).collect();
    assert_eq!(after, before);
    assert!(after.iter().all(|p| p.1.is_none()));
}

#[test]
fn a_framing_follows_its_walls_design_option() {
    let mut s = Session::default();
    let set = run(&mut s, "option_set_create", json!({"name": "Layout", "options": ["Open", "Split"]}));
    let split = id(&set["options"][1]);
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    run(&mut s, "elements_option_set", json!({"ids": [a], "option": "Layout: Split"}));
    let option = |s: &Session, id: ElementId| s.project().element(id).unwrap().design_option.map(|o| o.0);
    // Framed from the main model: the framing and its members go into the wall's option.
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    assert!(with_members(&s, host).iter().all(|m| option(&s, *m) == Some(split)));
    // Moving the wall to the main model takes its framing along, in one step.
    let depth = s.doc.undo_depth();
    run(&mut s, "elements_option_set", json!({"ids": [a], "option": null}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    assert!(with_members(&s, host).iter().all(|m| option(&s, *m).is_none()));
    run(&mut s, "undo", json!({}));
    assert!(with_members(&s, host).iter().all(|m| option(&s, *m) == Some(split)));
    // A main-model wall framed while editing an option stays in the main model.
    let b = wall(&mut s, [0.0, 5.0], [3.6, 5.0]);
    run(&mut s, "option_active_set", json!({"option": null}));
    let b_option = option(&s, ElementId(b));
    run(&mut s, "option_active_set", json!({"option": split}));
    let other = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [b]})));
    assert!(with_members(&s, other).iter().all(|m| option(&s, *m) == b_option));
    // Members a regeneration adds keep the framing's option too.
    run(&mut s, "element_set", json!({"id": other, "params": {"spacing": 0.2}}));
    assert!(with_members(&s, other).iter().all(|m| option(&s, *m) == b_option));
}

#[test]
fn too_many_members_are_refused_and_a_grown_wall_is_at_the_limit() {
    let mut s = Session::default();
    let long = wall(&mut s, [0.0, 0.0], [1000.0, 0.0]);
    let depth = s.doc.undo_depth();
    assert!(s.execute("wall_framing_create", json!({"walls": [long], "spacing": 0.2})).is_err());
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(framings(&s), 0);
    let a = wall(&mut s, [0.0, 5.0], [3.6, 5.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a], "spacing": 0.2})));
    let at_limit = |s: &mut Session| run(s, "element_get", json!({"id": host}))["derived"]["at_limit"].clone();
    assert_eq!(at_limit(&mut s), json!(false));
    run(&mut s, "element_set", json!({"id": a, "params": {"end": [1000.0, 5.0]}}));
    assert_eq!(at_limit(&mut s), json!(true));
    assert!(framing(&s, host).members.is_empty(), "no members past the limit");
}

#[test]
fn a_framing_stays_on_its_walls_level() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let upper = id(&run(&mut s, "level_create", json!({"name": "Upper", "elevation": 3.0})));
    run(&mut s, "element_set", json!({"id": a, "level": upper}));
    let level = |s: &Session, id: ElementId| s.project().element(id).unwrap().level.0;
    assert_eq!(level(&s, ElementId(a)), upper, "the wall moved");
    assert_eq!(level(&s, ElementId(host)), upper);
    assert!(framing(&s, host).members.iter().all(|m| level(&s, m.id) == upper));
}

fn frame(s: &mut Session, args: Value) -> WallFraming {
    let host = first_framing(&run(s, "wall_framing_create", args));
    framing(s, host)
}
