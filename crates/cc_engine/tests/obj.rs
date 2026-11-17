//! OBJ import: synthetic .obj files (and their .mtl libraries) become
//! component types and elements of the sizes, positions and colors they give.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ComponentShape, ElementId, ElementKind, ElementTypeId, TypeData};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-obj-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A unit cube from `(x, y, z)` (Y up) as OBJ statements, its faces wound
/// outwards; `base` is the number of vertices written before it.
fn cube(x: f64, y: f64, z: f64, size: f64, base: usize) -> String {
    let mut out = String::new();
    for i in 0..8 {
        let (dx, dy, dz) = ((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
        out += &format!("v {} {} {}\n", x + dx * size, y + dy * size, z + dz * size);
    }
    // Corners (1-based) of each face, counter-clockwise seen from outside.
    for f in [[1, 5, 7, 3], [2, 4, 8, 6], [1, 2, 6, 5], [3, 7, 8, 4], [1, 3, 4, 2], [5, 6, 8, 7]] {
        let f: Vec<String> = f.iter().map(|i| (i + base).to_string()).collect();
        out += &format!("f {}\n", f.join(" "));
    }
    out
}

fn component(s: &Session, id: u64) -> (ElementTypeId, cc_core::Component) {
    let e = s.project().elements.get(&ElementId(id)).unwrap();
    let ElementKind::Component(c) = &e.kind else { panic!("not a component: {:?}", e.kind) };
    (e.type_id, c.clone())
}

/// Bounds and signed volume of a type's mesh pieces, and how many there are.
fn mesh_of(s: &Session, ty: ElementTypeId) -> ([f64; 3], [f64; 3], f64, usize) {
    let TypeData::Component { def, .. } = &s.project().element_type(ty).unwrap().data else { panic!() };
    let (mut lo, mut hi, mut volume) = ([f64::MAX; 3], [f64::MIN; 3], 0.0);
    for item in &def.items {
        let ComponentShape::Mesh { vertices, faces } = &item.shape else { panic!() };
        for v in vertices.iter() {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        for f in faces.iter() {
            let p = |i: usize| glam::DVec3::from_array(vertices[f[i] as usize]);
            for i in 1..f.len() - 1 {
                volume += p(0).dot(p(i).cross(p(i + 1))) / 6.0;
            }
        }
    }
    (lo, hi, volume, def.items.len())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn obj_objects_become_components() {
    let path = scratch("two.obj");
    let mtl = "newmtl Brick\nKd 0.8 0.2 0.1\n";
    std::fs::write(scratch("two.mtl"), mtl).unwrap();
    let text = format!(
        "mtllib two.mtl\no Left\nusemtl Brick\n{}o Right\n{}",
        cube(0.0, 0.0, 0.0, 1.0, 0),
        cube(4.0, 1.0, -2.0, 2.0, 8)
    );
    std::fs::write(&path, text).unwrap();
    let mut s = Session::default();
    let before = s.project().elements.len();
    let out = run(&mut s, "obj_import", json!({ "path": path }));
    let elements: Vec<u64> = out["elements"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(elements.len(), 2, "{out}");
    assert_eq!(out["faces"], 12);
    assert!(s.project().materials.iter().any(|m| m.name == "Brick" && m.color == [204, 51, 26]));

    // Y up: the file's (x, y, z) is the model's (x, -z, y); inserted at the footprint's center and bottom.
    let (left_type, left) = component(&s, elements[0]);
    assert_eq!(s.project().element_type(left_type).unwrap().name, "Left");
    assert!(close(left.position.x, 0.5) && close(left.position.y, -0.5) && close(left.base_offset, 0.0), "{left:?}");
    let (lo, hi, volume, pieces) = mesh_of(&s, left_type);
    assert_eq!(pieces, 1);
    assert!(close(lo[0], -0.5) && close(hi[0], 0.5) && close(lo[2], 0.0) && close(hi[2], 1.0), "{lo:?} {hi:?}");
    assert!(close(volume, 1.0), "{volume}");

    let (right_type, right) = component(&s, elements[1]);
    assert!(close(right.position.x, 5.0) && close(right.position.y, 1.0) && close(right.base_offset, 1.0), "{right:?}");
    let (_, _, volume, _) = mesh_of(&s, right_type);
    assert!(close(volume, 8.0), "{volume}");

    // One undo step.
    run(&mut s, "undo", json!({}));
    assert_eq!(s.project().elements.len(), before);
}

#[test]
fn obj_units_axes_and_merge() {
    let path = scratch("merged.obj");
    let text = format!("g A\n{}g B\n{}", cube(0.0, 0.0, 0.0, 100.0, 0), cube(200.0, 0.0, 0.0, 100.0, 8));
    std::fs::write(&path, text).unwrap();
    let mut s = Session::default();
    let out = run(&mut s, "obj_import", json!({ "path": path, "units": "cm", "z_up": true, "merge": true }));
    let elements = out["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 1, "{out}");
    let (ty, c) = component(&s, elements[0].as_u64().unwrap());
    assert_eq!(s.project().element_type(ty).unwrap().name, "merged");
    assert!(c.position.length() < 1e-12 && close(c.base_offset, 0.0));
    let (lo, hi, volume, _) = mesh_of(&s, ty);
    // Z up, centimeters: from the origin to 3 m along x, 1 m deep and high.
    assert!(close(lo[0], 0.0) && close(hi[0], 3.0) && close(hi[1], 1.0) && close(hi[2], 1.0), "{lo:?} {hi:?}");
    assert!(close(volume, 2.0), "{volume}");
}

#[test]
fn bad_obj_files_are_errors() {
    let mut s = Session::default();
    let missing = scratch("missing.obj");
    assert_eq!(s.execute("obj_import", json!({ "path": missing })).unwrap_err().code, ErrorCode::Io);
    // Bad indices, bad numbers, lines and points only: nothing to import.
    let empty = scratch("empty.obj");
    std::fs::write(&empty, "v 0 0 0\nv 1 0 nan\nv x y z\nf 1 2 3\nf 1 9 2\nf 0 1 2\nl 1 2\np 1\n\u{0}\u{ff}").unwrap();
    let err = s.execute("obj_import", json!({ "path": empty })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    // A library named with a path does not reach outside the file's folder.
    let outside = scratch("outside.obj");
    std::fs::write(&outside, format!("mtllib ../../../etc/passwd\nusemtl X\n{}", cube(0.0, 0.0, 0.0, 1.0, 0))).unwrap();
    let out = run(&mut s, "obj_import", json!({ "path": outside }));
    assert_eq!(out["elements"].as_array().unwrap().len(), 1);
}

/// The material library's picture becomes the texture, `Kd` tints it and
/// `d` makes it partly clear; the repeat comes from `vt`.
#[test]
fn obj_textures_and_opacity() {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255]));
    img.save(scratch("leaf texture.png")).unwrap();
    std::fs::write(scratch("leaf.mtl"), "newmtl Leaf\nKd 1 1 1\nd 0.25\nmap_Kd -s 1 1 1 leaf texture.png\n").unwrap();
    // A 3 x 1.5 m quad lying flat (Z up), the texture repeated three times across and once along.
    let text = "mtllib leaf.mtl\nusemtl Leaf\nv 0 0 0\nv 3 0 0\nv 3 1.5 0\nv 0 1.5 0\nvt 0 0\nvt 3 0\nvt 3 1\nvt 0 1\nf 1/1 2/2 3/3 4/4\n";
    let path = scratch("leaf.obj");
    std::fs::write(&path, text).unwrap();
    let mut s = Session::default();
    run(&mut s, "obj_import", json!({ "path": path, "z_up": true }));
    let m = s.project().materials.iter().find(|m| m.name == "Leaf").unwrap();
    assert_eq!(m.color, [0, 255, 0]);
    assert!((m.graphics.transparency - 0.75).abs() < 1e-12);
    let texture = m.appearance.texture.as_ref().expect("map_Kd is the texture");
    assert!(close(texture.width, 1.0) && close(texture.height, 1.5), "{} {}", texture.width, texture.height);
}
