//! Rendering a 3D view to an image file with `view_render`.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};
use std::path::PathBuf;

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

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-render-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// The example house, opened and saved state (not dirty, no undo steps).
fn house() -> Session {
    let mut s = Session::default();
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/casa_simples.ccad");
    run(&mut s, "project_open", json!({"path": sample}));
    s
}

fn small() -> Value {
    json!({"samples": 2, "width": 48, "height": 32})
}

#[test]
fn renders_a_3d_view_to_png_without_changing_the_model() {
    let mut s = house();
    let before = s.project().views.values().find(|v| v.name == "{3D}").unwrap().clone();
    let (depth, dirty) = (s.doc.undo_depth(), s.doc.is_dirty());
    let path = scratch("house.png");
    let _ = std::fs::remove_file(&path);
    let out = run(&mut s, "view_render", json!({"view": "{3D}", "path": path, "settings": small()}));
    assert_eq!(out["width"], 48);
    assert_eq!(out["height"], 32);
    assert_eq!(out["samples"], 2);
    assert!(out["triangles"].as_u64().unwrap() > 0, "{out}");
    assert!(out["seconds"].as_f64().unwrap() >= 0.0, "{out}");
    assert_eq!(out["path"], json!(path));
    let img = image::open(&path).unwrap();
    assert_eq!((img.width(), img.height()), (48, 32));
    // Not all one color: the house is in the picture.
    let rgb = img.to_rgb8();
    let first = rgb.get_pixel(0, 0);
    assert!(rgb.pixels().any(|p| p != first));
    // The view's settings and camera are as they were, and nothing to undo or save.
    let after = s.project().views.values().find(|v| v.name == "{3D}").unwrap();
    assert_eq!(after.render, before.render);
    assert_eq!(after.kind, before.kind);
    assert_eq!((s.doc.undo_depth(), s.doc.is_dirty()), (depth, dirty));
}

#[test]
fn renders_a_region_only() {
    let mut s = house();
    let path = scratch("region.png");
    let out = run(
        &mut s,
        "view_render",
        json!({"view": "{3D}", "path": path, "settings": small(), "region": [8, 4, 16, 12]}),
    );
    assert_eq!((out["width"].as_u64(), out["height"].as_u64()), (Some(48), Some(32)));
    let rgb = image::open(&path).unwrap().to_rgb8();
    assert_eq!(rgb.dimensions(), (48, 32));
    // Outside the region the image stays black; inside it is not.
    assert_eq!(rgb.get_pixel(0, 0).0, [0, 0, 0]);
    assert_eq!(rgb.get_pixel(47, 31).0, [0, 0, 0]);
    assert!((8..24).any(|x| (4..16).any(|y| rgb.get_pixel(x, y).0 != [0, 0, 0])));
}

#[test]
fn refuses_what_it_cannot_render() {
    let mut s = house();
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].clone();
    let png = scratch("refused.png");
    // Not a 3D view.
    let code = refused(&mut s, "view_render", json!({"view": plan, "path": png, "settings": small()}));
    assert_eq!(code, ErrorCode::InvalidArguments);
    // An image format that is not written.
    let bmp = scratch("refused.bmp");
    let code = refused(&mut s, "view_render", json!({"view": "{3D}", "path": bmp, "settings": small()}));
    assert_eq!(code, ErrorCode::InvalidArguments);
    assert!(!bmp.exists());
    // Settings out of range, or not settings at all.
    for settings in [json!({"samples": 0}), json!({"width": 2}), json!({"color": 3})] {
        let code = refused(&mut s, "view_render", json!({"view": "{3D}", "path": png, "settings": settings}));
        assert_eq!(code, ErrorCode::InvalidArguments);
    }
    // A region outside the image (or empty).
    for region in [[40, 0, 16, 8], [0, 30, 8, 8], [0, 0, 0, 8], [u32::MAX, 0, 2, 2]] {
        let code =
            refused(&mut s, "view_render", json!({"view": "{3D}", "path": png, "settings": small(), "region": region}));
        assert_eq!(code, ErrorCode::InvalidArguments, "{region:?}");
    }
    // No such view.
    let code = refused(&mut s, "view_render", json!({"view": "nowhere", "path": png}));
    assert_eq!(code, ErrorCode::NotFound);
}
