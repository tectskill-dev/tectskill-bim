//! Material textures and painted faces (issue #36): a material holds an
//! image laid on its faces in the textures style; an element's faces can be
//! painted with another material, and the paint follows its face.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::{Drawing2D, Prim};
use glam::{DVec2, DVec3};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

/// A PNG 100 x 50 pixels.
fn sample_png() -> Vec<u8> {
    let mut d = Drawing2D::default();
    d.prims.push(Prim::Fill {
        pts: vec![DVec2::new(2.0, 2.0), DVec2::new(10.0, 2.0), DVec2::new(10.0, 10.0), DVec2::new(2.0, 10.0)],
        color: [200, 30, 30, 255],
        element: None,
        cut: false,
    });
    cc_views::export::to_png(&d, DVec2::new(25.4, 12.7), 100.0).unwrap()
}

#[test]
fn materials_take_textures() {
    let mut s = Session::default();
    let m = run(&mut s, "material_create", json!({"name": "Tijolo"}));
    let name = m["name"].as_str().unwrap().to_string();
    let data = cc_core::raster::base64_encode(&sample_png());
    // A width alone keeps the image's proportions (100 x 50 pixels).
    let out = run(&mut s, "material_texture_set", json!({"material": name, "data": data, "width": 2.0}));
    let t = &out["appearance"]["texture"];
    assert_eq!((t["width"].as_f64(), t["height"].as_f64()), (Some(2.0), Some(1.0)), "{t}");
    // The image shows as a summary, never its bytes.
    assert_eq!(t["image"]["width"], 100);
    assert_eq!(t["image"]["height"], 50);
    assert!(t["image"]["bytes"].as_u64().unwrap() > 0 && t["image"].get("data").is_none());
    let got = run(&mut s, "material_get", json!({"material": name}));
    assert_eq!(got["appearance"]["texture"], *t);

    // The layout changes without a new image; a file replaces the image and keeps the layout.
    run(&mut s, "material_texture_set", json!({"material": name, "rotation": 30.0, "offset": [0.5, 0.25]}));
    let path = std::env::temp_dir().join(format!("ccad-texture-{}.png", std::process::id()));
    std::fs::write(&path, sample_png()).unwrap();
    let out = run(&mut s, "material_texture_set", json!({"material": name, "path": path}));
    let t = &out["appearance"]["texture"];
    assert_eq!(t["rotation_deg"], 30.0);
    assert_eq!(t["offset"], json!([0.5, 0.25]));
    assert_eq!(t["width"], 2.0);

    // Other fields change around it.
    run(&mut s, "material_set", json!({"material": name, "values": {"appearance": {"color": [1, 2, 3]}}}));
    let m = s.project().material(&name).unwrap();
    assert_eq!(m.appearance.color, [1, 2, 3]);
    assert!(m.appearance.texture.as_ref().is_some_and(|t| t.image.width == 100));
    // Patching back what material_get shows keeps the image.
    let shown = run(&mut s, "material_get", json!({"material": name}));
    run(&mut s, "material_set", json!({"material": name, "values": {"appearance": shown["appearance"].clone()}}));
    assert!(s.project().material(&name).unwrap().appearance.texture.as_ref().is_some_and(|t| t.image_ok()));

    // Saved and read back.
    let text = cc_core::to_ron(s.project()).unwrap();
    let back = cc_core::from_ron(&text).unwrap();
    assert_eq!(back.material(&name), s.project().material(&name));

    // Bad input is refused.
    assert_eq!(
        fails(&mut s, "material_texture_set", json!({"material": name, "data": "not base64!"})),
        ErrorCode::InvalidArguments
    );
    let text_file = cc_core::raster::base64_encode(b"plain text");
    assert_eq!(
        fails(&mut s, "material_texture_set", json!({"material": name, "data": text_file})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "material_texture_set", json!({"material": name, "width": 0.0})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "material_texture_set", json!({"material": name, "path": path, "data": "AAAA"})),
        ErrorCode::InvalidArguments
    );

    // Cleared, and undone.
    let out = run(&mut s, "material_texture_clear", json!({"material": name}));
    assert!(out["appearance"].get("texture").is_none_or(Value::is_null));
    assert_eq!(fails(&mut s, "material_texture_clear", json!({"material": name})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "material_texture_set", json!({"material": name, "rotation": 1.0})),
        ErrorCode::InvalidArguments
    );
    run(&mut s, "undo", json!({}));
    assert!(s.project().material(&name).unwrap().appearance.texture.is_some());
}

/// A wall along X from the origin, and the centroid of its face turned towards `normal`.
fn wall(s: &mut Session, length: f64) -> u64 {
    run(s, "wall_create", json!({"start": [0, 0], "end": [length, 0], "height": 3.0}))["id"].as_u64().unwrap()
}

fn face_point(s: &mut Session, id: u64, normal: DVec3) -> DVec3 {
    let geom = s.geometry();
    let face = geom
        .body_of(cc_core::ElementId(id))
        .flat_map(|s| s.faces.iter())
        .filter(|f| f.normal.dot(normal) > 0.999)
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap();
    face.centroid()
}

fn painted_area(s: &mut Session, id: u64) -> f64 {
    let list = run(s, "paints_list", json!({"element": id}));
    list.as_array().unwrap().iter().map(|p| p["area"].as_f64().unwrap()).sum()
}

#[test]
fn faces_are_painted_and_the_paint_follows_them() {
    let mut s = Session::default();
    let green = run(&mut s, "material_create", json!({"name": "Verde", "color": [20, 160, 40]}))["name"].clone();
    let blue = run(&mut s, "material_create", json!({"name": "Azul", "color": [20, 40, 200]}))["name"].clone();
    let id = wall(&mut s, 5.0);
    let south = face_point(&mut s, id, -DVec3::Y);
    let out = run(
        &mut s,
        "paint",
        json!({"element": id, "point": south.to_array(), "normal": [0, -1, 0], "material": green}),
    );
    assert!(out["faces"].as_u64().unwrap() >= 1);
    let area = out["area"].as_f64().unwrap();
    assert!((area - 15.0).abs() < 1.0, "{out}");
    let e = s.project().element(cc_core::ElementId(id)).unwrap();
    assert_eq!(e.paints.len(), 1);
    assert!(e.paints[0].normal.distance(-DVec3::Y) < 1e-9);

    // The geometry carries the paint on that face only.
    let geom = s.geometry();
    let painted: Vec<(DVec3, Option<String>)> = geom
        .body_of(cc_core::ElementId(id))
        .flat_map(|sol| sol.painted.iter().map(|(fi, look)| (sol.faces[*fi].normal, look.material.clone())))
        .collect();
    assert!(!painted.is_empty());
    assert!(painted.iter().all(|(n, m)| n.y < -0.99 && m.as_deref() == green.as_str()));

    // Painting the same surface again replaces the paint.
    run(&mut s, "paint", json!({"element": id, "point": south.to_array(), "material": blue}));
    let list = run(&mut s, "paints_list", json!({}));
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["material"], blue);

    // The paint follows its face: the wall moved, turned and made longer.
    run(&mut s, "elements_move", json!({"ids": [id], "delta": [2.0, 3.0]}));
    assert!((painted_area(&mut s, id) - area).abs() < 1e-6);
    run(&mut s, "elements_rotate", json!({"ids": [id], "angle": 90.0, "center": [0, 0]}));
    assert!((painted_area(&mut s, id) - area).abs() < 1e-6);
    let paint = s.project().element(cc_core::ElementId(id)).unwrap().paints[0].clone();
    assert!(paint.normal.distance(DVec3::X) < 1e-9, "{:?}", paint.normal);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    run(&mut s, "element_set", json!({"id": id, "params": {"end": [8.0, 0.0]}}));
    assert!((painted_area(&mut s, id) - area * 8.0 / 5.0).abs() < 1.0, "{}", painted_area(&mut s, id));

    // Renaming the material keeps the paint; deleting it is refused while it is used.
    run(&mut s, "material_rename", json!({"material": blue, "name": "Azul claro"}));
    assert_eq!(s.project().element(cc_core::ElementId(id)).unwrap().paints[0].material, "Azul claro");
    assert_eq!(fails(&mut s, "material_delete", json!({"material": "Azul claro"})), ErrorCode::Rejected);

    // Points off the element, unknown materials and bare elements are refused.
    let far = json!({"element": id, "point": [50.0, 50.0, 1.0], "material": green});
    assert_eq!(fails(&mut s, "paint", far), ErrorCode::InvalidArguments);
    let unknown = json!({"element": id, "point": south.to_array(), "material": "Nada"});
    assert_eq!(fails(&mut s, "paint", unknown), ErrorCode::NotFound);

    // Taking the paint off.
    let south = face_point(&mut s, id, -DVec3::Y);
    let north = face_point(&mut s, id, DVec3::Y);
    run(&mut s, "paint", json!({"element": id, "point": north.to_array(), "material": green}));
    assert_eq!(
        fails(&mut s, "paint_remove", json!({"element": id, "point": [4.0, 0.0, 10.0]})),
        ErrorCode::InvalidArguments
    );
    let out = run(&mut s, "paint_remove", json!({"element": id, "point": south.to_array(), "normal": [0, -1, 0]}));
    assert_eq!(out["removed"], 1);
    assert_eq!(s.project().element(cc_core::ElementId(id)).unwrap().paints[0].material, green.as_str().unwrap());
    run(&mut s, "paint_remove", json!({"element": id}));
    assert!(s.project().element(cc_core::ElementId(id)).unwrap().paints.is_empty());
    assert_eq!(fails(&mut s, "paint_remove", json!({"element": id})), ErrorCode::InvalidArguments);
    run(&mut s, "undo", json!({}));
    assert_eq!(s.project().element(cc_core::ElementId(id)).unwrap().paints.len(), 1);

    // Saved and read back; copies carry their paints.
    let text = cc_core::to_ron(s.project()).unwrap();
    let back = cc_core::from_ron(&text).unwrap();
    assert_eq!(back.element(cc_core::ElementId(id)).unwrap().paints.len(), 1);
}

#[test]
fn elevations_show_painted_faces() {
    let mut s = Session::default();
    let red = run(&mut s, "material_create", json!({"name": "Vermelho", "color": [220, 0, 0]}))["name"].clone();
    let id = wall(&mut s, 5.0);
    let south_view = run(&mut s, "views_list", json!({"category": "elevation"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"].as_str().is_some_and(|n| n.contains("Sul") || n.contains("South")))
        .map(|v| v["id"].as_u64().unwrap())
        .unwrap();
    let fills = |s: &mut Session| -> Vec<[u8; 4]> {
        let d = s.drawing(cc_core::ViewId(south_view)).unwrap();
        d.prims
            .iter()
            .filter_map(|p| match p {
                Prim::Fill { color, element: Some(_), .. } if color[3] > 0 => Some(*color),
                _ => None,
            })
            .collect()
    };
    let before = fills(&mut s);
    let south = face_point(&mut s, id, -DVec3::Y);
    run(&mut s, "paint", json!({"element": id, "point": south.to_array(), "material": red}));
    let after = fills(&mut s);
    assert_ne!(before, after);
    // The face seen is now reddish.
    assert!(after.iter().any(|c| c[0] > c[1] + 40 && c[0] > c[2] + 40), "{after:?}");
}

#[test]
fn files_with_bad_paints_are_repaired() {
    let mut s = Session::default();
    let m = run(&mut s, "material_create", json!({"name": "Tinta"}))["name"].clone();
    let id = wall(&mut s, 4.0);
    let p = face_point(&mut s, id, DVec3::Y);
    run(&mut s, "paint", json!({"element": id, "point": p.to_array(), "material": m}));
    let text = cc_core::to_ron(s.project()).unwrap().replace("material: \"Tinta\"", "material: \"Sumiu\"");
    let (project, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert!(repairs >= 1);
    assert!(project.element(cc_core::ElementId(id)).unwrap().paints.is_empty());
}
