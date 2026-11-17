//! Column and beam types by structural material, and the multi-storey grid frame.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementTypeId, StructuralMaterial};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn run_err(s: &mut Session, name: &str, args: Value) -> cc_engine::EngineError {
    s.execute(name, args).expect_err(name)
}

fn type_for(s: &mut Session, category: &str, material: &str) -> Value {
    run(s, "structural_type_for", json!({"category": category, "material": material}))
}

#[test]
fn an_existing_type_is_returned_without_an_undo_step() {
    let mut s = Session::default();
    let depth = s.doc.undo_depth();
    for (category, material) in [("column", "steel"), ("beam", "steel"), ("column", "concrete")] {
        let out = type_for(&mut s, category, material);
        assert_eq!(out["created"], false, "{category} {material}: the template has one");
        let id = ElementTypeId(out["id"].as_u64().unwrap());
        let data = &s.project().element_type(id).unwrap().data;
        assert_eq!(s.project().structural_material(data), StructuralMaterial::from_code(material));
    }
    assert_eq!(s.doc.undo_depth(), depth);
}

#[test]
fn a_missing_material_gets_its_catalog_default_once() {
    let mut s = Session::default();
    for category in ["column", "beam"] {
        let depth = s.doc.undo_depth();
        let out = type_for(&mut s, category, "timber");
        assert_eq!(out["created"], true);
        assert_eq!(out["name"], "MLC 9x32");
        assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
        let id = ElementTypeId(out["id"].as_u64().unwrap());
        let data = &s.project().element_type(id).unwrap().data;
        assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Timber));
        let again = type_for(&mut s, category, "timber");
        assert_eq!((again["id"].clone(), again["created"].clone()), (out["id"].clone(), json!(false)));
    }
    assert_eq!(s.project().profiles.values().filter(|p| p.name == "MLC 9x32").count(), 1, "one profile shared");
    run(&mut s, "undo", json!({}));
    assert!(s.project().types_of_material(cc_core::Category::Beam, StructuralMaterial::Timber).next().is_none());
}

#[test]
fn steel_and_concrete_defaults_are_made_when_missing() {
    let mut s = Session::default();
    // Delete every steel column type so steel is missing (concrete ones remain).
    let steel: Vec<u64> =
        s.project().types_of_material(cc_core::Category::Column, StructuralMaterial::Steel).map(|t| t.id.0).collect();
    for id in steel {
        run(&mut s, "type_delete", json!({"type": id}));
    }
    let out = type_for(&mut s, "column", "steel");
    assert_eq!((out["created"].clone(), out["name"].clone()), (json!(true), json!("W200x26,6")));
    // Concrete: delete them all but the new steel one, then ask for concrete.
    let concrete_ids: Vec<u64> = s
        .project()
        .types_of_material(cc_core::Category::Column, StructuralMaterial::Concrete)
        .map(|t| t.id.0)
        .collect();
    for id in concrete_ids {
        run(&mut s, "type_delete", json!({"type": id}));
    }
    let made = type_for(&mut s, "column", "concrete");
    assert_eq!(made["created"], true);
    let id = ElementTypeId(made["id"].as_u64().unwrap());
    let def = s.project().type_profile_def(&s.project().element_type(id).unwrap().data).unwrap();
    assert_eq!(def.shape, cc_core::ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 });
    assert_eq!(def.material, cc_core::ProfileMaterial::Concrete);
}

#[test]
fn bad_arguments_are_refused() {
    let mut s = Session::default();
    for (category, material) in [("wall", "steel"), ("column", "gold")] {
        let err = run_err(&mut s, "structural_type_for", json!({"category": category, "material": material}));
        assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments, "{category} {material}");
    }
}

/// Three storeys (levels at 0, 3, 6 and 9 m) over a 3 x 3 grid 6 m apart.
fn three_storeys() -> (Session, Vec<u64>, [u64; 4]) {
    let mut s = Session::default();
    let mut grids = Vec::new();
    for k in 0..3 {
        let x = 6.0 * k as f64;
        grids.push(run(&mut s, "grid_create", json!({"start": [x, -2.0], "end": [x, 14.0]}))["id"].as_u64().unwrap());
        grids.push(run(&mut s, "grid_create", json!({"start": [-2.0, x], "end": [14.0, x]}))["id"].as_u64().unwrap());
    }
    run(&mut s, "level_create", json!({"name": "L2", "elevation": 6.0}));
    run(&mut s, "level_create", json!({"name": "L3", "elevation": 9.0}));
    let mut levels: Vec<(f64, u64)> = s.project().levels.iter().map(|l| (l.elevation, l.id.0)).collect();
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let ids = [levels[0].1, levels[1].1, levels[2].1, levels[3].1];
    (s, grids, ids)
}

fn kind_count(s: &Session, f: impl Fn(&cc_core::ElementKind) -> bool) -> usize {
    s.project().elements.values().filter(|e| f(&e.kind)).count()
}

#[test]
fn grid_frame_fills_every_storey_in_one_step() {
    let (mut s, _, levels) = three_storeys();
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "grid_frame_create", json!({"base_level": levels[0], "top_level": levels[3]}));
    assert_eq!(out["columns"].as_array().unwrap().len(), 27, "9 per storey");
    // Each upper level: 3 grids x 2 spans in each direction = 12 beams.
    assert_eq!(out["beams"].as_array().unwrap().len(), 36);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    for id in out["columns"].as_array().unwrap() {
        let e = s.project().element(cc_core::ElementId(id.as_u64().unwrap())).unwrap();
        let cc_core::ElementKind::Column(c) = &e.kind else { panic!() };
        let i = levels.iter().position(|l| *l == e.level.0).unwrap();
        assert_eq!(c.top, cc_core::WallTop::Level { level: cc_core::LevelId(levels[i + 1]), offset: 0.0 });
        let data = &s.project().element_type(e.type_id).unwrap().data;
        assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Steel), "steel by default");
    }
    for id in out["beams"].as_array().unwrap() {
        let e = s.project().element(cc_core::ElementId(id.as_u64().unwrap())).unwrap();
        assert_ne!(e.level.0, levels[0], "beams sit on the upper levels");
        let cc_core::ElementKind::Beam(b) = &e.kind else { panic!() };
        assert!((s.project().beam_z(e, b).1 - s.project().level_elevation(e.level)).abs() < 1e-9, "top at the level");
    }
    // Again: nothing new.
    let again = run(&mut s, "grid_frame_create", json!({"base_level": levels[0], "top_level": levels[3]}));
    assert!(again["columns"].as_array().unwrap().is_empty() && again["beams"].as_array().unwrap().is_empty());
    assert_eq!(again["skipped"], 27 + 36);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(kind_count(&s, |k| matches!(k, cc_core::ElementKind::Column(_))), 0);
}

#[test]
fn grid_frame_takes_given_types_and_refuses_bad_ranges() {
    let (mut s, _, levels) = three_storeys();
    let concrete = type_for(&mut s, "column", "concrete")["id"].clone();
    let out = run(
        &mut s,
        "grid_frame_create",
        json!({"base_level": levels[1], "top_level": levels[2], "column_type": concrete, "material": "timber"}),
    );
    let col = cc_core::ElementId(out["columns"][0].as_u64().unwrap());
    assert_eq!(s.project().element(col).unwrap().type_id.0, concrete.as_u64().unwrap(), "the given type wins");
    let beam = cc_core::ElementId(out["beams"][0].as_u64().unwrap());
    let data = &s.project().element_type(s.project().element(beam).unwrap().type_id).unwrap().data;
    assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Timber));
    let depth = s.doc.undo_depth();
    let err = run_err(&mut s, "grid_frame_create", json!({"base_level": levels[2], "top_level": levels[1]}));
    assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    assert_eq!(s.doc.undo_depth(), depth, "a refused run leaves no undo step");
}

#[test]
fn grid_frame_is_bounded() {
    let mut s = Session::default();
    // 40 x 40 grids: 1600 columns and 3120 beams for one storey, over the limit.
    for k in 0..40 {
        let x = k as f64;
        run(&mut s, "grid_create", json!({"start": [x, -1.0], "end": [x, 41.0]}));
        run(&mut s, "grid_create", json!({"start": [-1.0, x], "end": [41.0, x]}));
    }
    let depth = s.doc.undo_depth();
    let err = run_err(&mut s, "grid_frame_create", json!({}));
    assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    assert_eq!(s.doc.undo_depth(), depth, "refused before any edit, the type default included");
}

#[test]
fn secondary_beams_divide_each_bay() {
    let (mut s, grids, levels) = three_storeys();
    // grids[0] is the vertical grid x = 0; bays are 6 m wide, spacing 2.5 m: ceil(6 / 2.5) - 1 = 2 per bay.
    let out = run(
        &mut s,
        "grid_frame_create",
        json!({"base_level": levels[0], "top_level": levels[1], "secondary": {"parallel_to": grids[0], "spacing": 2.5}}),
    );
    // 4 bays x 2 beams on the one upper level.
    let sec = out["secondary"].as_array().unwrap();
    assert_eq!(sec.len(), 8);
    for id in sec {
        let e = s.project().element(cc_core::ElementId(id.as_u64().unwrap())).unwrap();
        let cc_core::ElementKind::Beam(b) = &e.kind else { panic!() };
        assert!((b.start.x - b.end.x).abs() < 1e-9, "parallel to the x = 0 grid");
        assert!(((b.start.y - b.end.y).abs() - 6.0).abs() < 1e-9);
        let frac = b.start.x.rem_euclid(6.0);
        assert!((frac - 2.0).abs() < 1e-9 || (frac - 4.0).abs() < 1e-9, "evenly spaced: {}", b.start.x);
    }
    // An exact multiple: 6 / 3 = 2 gives 1 beam per bay.
    let again = run(
        &mut s,
        "grid_frame_create",
        json!({"base_level": levels[1], "top_level": levels[2], "secondary": {"parallel_to": grids[0], "spacing": 3.0}}),
    );
    assert_eq!(again["secondary"].as_array().unwrap().len(), 4);
}

#[test]
fn skewed_bays_are_skipped_and_bad_secondary_refused() {
    let mut s = Session::default();
    let a = run(&mut s, "grid_create", json!({"start": [0.0, -1.0], "end": [0.0, 7.0]}))["id"].as_u64().unwrap();
    run(&mut s, "grid_create", json!({"start": [6.0, -1.0], "end": [6.0, 7.0]}));
    run(&mut s, "grid_create", json!({"start": [-1.0, 0.0], "end": [7.0, 0.0]}));
    // The fourth grid leans: the bay's crossing grids are not parallel.
    run(&mut s, "grid_create", json!({"start": [-1.0, 5.0], "end": [7.0, 7.0]}));
    let out = run(&mut s, "grid_frame_create", json!({"secondary": {"parallel_to": a, "spacing": 2.0}}));
    assert_eq!(out["secondary"].as_array().unwrap().len(), 0);
    assert_eq!(out["skipped_bays"], 1);
    for sec in [json!({"parallel_to": a, "spacing": 0.1}), json!({"parallel_to": 999_999, "spacing": 2.0})] {
        let err = run_err(&mut s, "grid_frame_create", json!({"secondary": sec}));
        assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    }
}

#[test]
fn a_huge_secondary_request_is_refused_quickly_and_before_any_edit() {
    let mut s = Session::default();
    let a = run(&mut s, "grid_create", json!({"start": [0.0, -1.0], "end": [0.0, 1001.0]}))["id"].as_u64().unwrap();
    run(&mut s, "grid_create", json!({"start": [5000.0, -1.0], "end": [5000.0, 1001.0]}));
    run(&mut s, "grid_create", json!({"start": [-1.0, 0.0], "end": [5001.0, 0.0]}));
    run(&mut s, "grid_create", json!({"start": [-1.0, 1000.0], "end": [5001.0, 1000.0]}));
    let depth = s.doc.undo_depth();
    let started = std::time::Instant::now();
    // 5000 m between the grids at 0.3 m: some 16,000 beams in the one bay.
    let err = run_err(&mut s, "grid_frame_create", json!({"secondary": {"parallel_to": a, "spacing": 0.3}}));
    assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "refused early: {:?}", started.elapsed());
    assert_eq!(s.doc.undo_depth(), depth);
}

#[test]
fn a_catalog_name_of_another_material_is_not_reused() {
    let mut s = Session::default();
    // A concrete profile that happens to carry the steel default's name.
    run(
        &mut s,
        "profile_create",
        json!({"name": "W200x26,6", "material": "concrete", "shape": "rect", "params": {"b": 0.2, "h": 0.4}}),
    );
    // No steel type exists: delete them all so the default is made.
    let steel: Vec<u64> =
        s.project().types_of_material(cc_core::Category::Column, StructuralMaterial::Steel).map(|t| t.id.0).collect();
    for id in steel {
        run(&mut s, "type_delete", json!({"type": id}));
    }
    let made = type_for(&mut s, "column", "steel");
    let id = ElementTypeId(made["id"].as_u64().unwrap());
    let data = &s.project().element_type(id).unwrap().data;
    assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Steel), "made type is steel");
    let again = type_for(&mut s, "column", "steel");
    assert_eq!(again["created"], false, "found next time, not another type per call");
    assert_eq!(again["id"], made["id"]);
}
