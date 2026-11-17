//! The render settings of a 3D view through `view_set`/`view_get`.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::RenderSettings;
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// The command fails and leaves no undo step.
fn refused(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    let depth = s.doc.undo_depth();
    let code = s.execute(name, args.clone()).map(|v| panic!("{name} {args} should fail, gave {v}")).unwrap_err().code;
    assert_eq!(s.doc.undo_depth(), depth, "{name} {args} left an undo step");
    code
}

fn three_d(s: &mut Session) -> u64 {
    run(s, "view_create", json!({"kind": "three_d"}))["id"].as_u64().unwrap()
}

fn render_of(s: &mut Session, view: u64) -> Value {
    run(s, "view_get", json!({"view": view}))["render"].clone()
}

fn set_render(s: &mut Session, view: u64, render: Value) -> Value {
    run(s, "view_set", json!({"view": view, "render": render}))["render"].clone()
}

fn stored(s: &Session, view: u64) -> Option<RenderSettings> {
    s.project().views[&cc_core::ViewId(view)].render.clone()
}

#[test]
fn set_read_merge_and_clear() {
    let mut s = Session::default();
    let v = three_d(&mut s);
    assert_eq!(render_of(&mut s, v), Value::Null);

    // A view without settings starts from the defaults.
    let got = set_render(&mut s, v, json!({"samples": 32}));
    assert_eq!(got["samples"], 32);
    assert_eq!(got["max_depth"], 8);
    assert_eq!(got["width"], 1920);
    assert_eq!(got["tone_map"], "Filmic");
    assert_eq!(stored(&s, v), Some(RenderSettings { samples: 32, ..Default::default() }));
    assert_eq!(render_of(&mut s, v), got);

    // A patch merges over what the view has, nested objects too.
    let got = set_render(
        &mut s,
        v,
        json!({"width": 800, "height": 600, "tone_map": "standard", "environment": {"ground_plane": false}}),
    );
    assert_eq!(got["samples"], 32);
    assert_eq!(got["width"], 800);
    assert_eq!(got["height"], 600);
    assert_eq!(got["tone_map"], "Standard");
    assert_eq!(got["environment"]["ground_plane"], false);
    assert_eq!(got["environment"]["sky_strength"], 1.0);

    // Null removes them.
    assert_eq!(set_render(&mut s, v, Value::Null), Value::Null);
    assert_eq!(stored(&s, v), None);
}

#[test]
fn refused_where_it_does_not_apply_or_out_of_range() {
    let mut s = Session::default();
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap();
    assert_eq!(
        refused(&mut s, "view_set", json!({"view": plan, "render": {"samples": 4}})),
        ErrorCode::InvalidArguments
    );
    refused(&mut s, "view_set", json!({"view": plan, "render": null}));

    let v = three_d(&mut s);
    for bad in [
        json!({"samples": 0}),
        json!({"samples": 20000}),
        json!({"max_depth": 65}),
        json!({"width": 8}),
        json!({"width": 8192, "height": 8192}),
        json!({"fov_deg": 5.0}),
        json!({"exposure": 11.0}),
        json!({"clamp": -1.0}),
        json!({"environment": {"sun_strength": 101.0}}),
        json!({"samples": -1}),
        json!({"tone_map": "neon"}),
        json!({"sampels": 4}),
        json!({"environment": {"sky": 1.0}}),
        json!({"environment": 3}),
    ] {
        let code = refused(&mut s, "view_set", json!({"view": v, "render": bad}));
        assert_eq!(code, ErrorCode::InvalidArguments, "{bad}");
    }
    assert_eq!(stored(&s, v), None);
}

#[test]
fn undo_restores_and_files_keep_it() {
    let mut s = Session::default();
    let v = three_d(&mut s);
    set_render(&mut s, v, json!({"samples": 64}));
    set_render(&mut s, v, json!({"clay": true, "exposure": 1.5}));
    run(&mut s, "undo", json!({}));
    assert_eq!(stored(&s, v), Some(RenderSettings { samples: 64, ..Default::default() }));
    run(&mut s, "redo", json!({}));
    let settings = RenderSettings { samples: 64, clay: true, exposure: 1.5, ..Default::default() };
    assert_eq!(stored(&s, v), Some(settings.clone()));

    // A copy of the view takes them along.
    let copy = run(&mut s, "view_duplicate", json!({"view": v}))["id"].as_u64().unwrap();
    assert_eq!(stored(&s, copy), Some(settings.clone()));

    let text = cc_core::to_ron(s.project()).unwrap();
    let back = cc_core::from_ron(&text).unwrap();
    assert_eq!(back.views[&cc_core::ViewId(v)].render, Some(settings));
    // Views without settings write nothing for them.
    let plan = back.views.values().find(|x| x.render.is_none()).unwrap();
    assert!(plan.render.is_none());
    assert!(!text.contains("render: None"));
}

#[test]
fn repair_drops_settings_where_they_do_not_belong() {
    let mut s = Session::default();
    let v = three_d(&mut s);
    set_render(&mut s, v, json!({"samples": 64}));
    let mut project = s.project().clone();
    let plan = project.views.values().find(|x| matches!(x.kind, cc_core::ViewKind::FloorPlan { .. })).unwrap().id;
    project.views.get_mut(&plan).unwrap().render = Some(RenderSettings::default());
    project.views.get_mut(&cc_core::ViewId(v)).unwrap().render =
        Some(RenderSettings { samples: 64, fov_deg: f64::NAN, width: 9000, ..Default::default() });
    let text = cc_core::to_ron(&project).unwrap();
    let (back, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert!(repairs >= 2, "{repairs}");
    assert_eq!(back.views[&plan].render, None);
    let fixed = back.views[&cc_core::ViewId(v)].render.clone().unwrap();
    assert!(fixed.is_valid());
    assert_eq!(fixed.samples, 64);
}
