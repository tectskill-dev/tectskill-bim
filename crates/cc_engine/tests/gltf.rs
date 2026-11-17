//! glTF import: synthetic .glb and .gltf files become component types and
//! elements of the sizes, positions and colors they give, through the node
//! hierarchy and its transforms.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ComponentShape, ElementId, ElementKind, ElementTypeId, TypeData};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-gltf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A unit cube from the origin (glTF axes: Y up) as positions (f32) and
/// 16-bit indices, its triangles wound outwards: the bytes and the JSON
/// accessors, buffer views and buffer they need (buffer 0).
fn cube_buffer() -> (Vec<u8>, Value) {
    let mut bytes = Vec::new();
    for i in 0..8u32 {
        for k in 0..3 {
            bytes.extend_from_slice(&(((i >> k) & 1) as f32).to_le_bytes());
        }
    }
    // Quads (corners counter-clockwise from outside) split in two.
    let quads = [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]];
    let mut indices = Vec::new();
    for q in quads {
        indices.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
    }
    for i in &indices {
        bytes.extend_from_slice(&(*i as u16).to_le_bytes());
    }
    let json = json!({
        "buffers": [{ "byteLength": bytes.len() }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 96 },
            { "buffer": 0, "byteOffset": 96, "byteLength": indices.len() * 2 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3" },
            { "bufferView": 1, "componentType": 5123, "count": indices.len(), "type": "SCALAR" }
        ]
    });
    (bytes, json)
}

/// A scene: "Box" (a cube scaled 2 x 1 x 3, moved to (4, 0, -1)) and
/// "Group", holding a red cube turned a quarter about Y and a child cube
/// standing on it, the mesh shared.
fn scene() -> (Vec<u8>, Value) {
    let (bytes, mut json) = cube_buffer();
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let extra = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0, 1] }],
        "nodes": [
            { "name": "Box", "mesh": 0, "translation": [4.0, 0.0, -1.0], "scale": [2.0, 1.0, 3.0] },
            { "name": "Group", "children": [2] },
            { "mesh": 1, "rotation": [0.0, half, 0.0, half], "children": [3] },
            { "mesh": 1, "translation": [0.0, 1.0, 0.0] }
        ],
        "meshes": [
            { "name": "Plain", "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1 }] },
            { "name": "Red", "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1, "material": 0 }] }
        ],
        "materials": [{ "name": "Paint", "pbrMetallicRoughness": { "baseColorFactor": [1.0, 0.0, 0.0, 1.0] } }]
    });
    for (k, v) in extra.as_object().unwrap() {
        json[k] = v.clone();
    }
    (bytes, json)
}

/// The JSON and binary chunk as a .glb file.
fn glb(json: &Value, bin: &[u8]) -> Vec<u8> {
    let mut text = serde_json::to_vec(json).unwrap();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let mut bin = bin.to_vec();
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

fn component(s: &Session, id: u64) -> (ElementTypeId, cc_core::Component) {
    let e = s.project().elements.get(&ElementId(id)).unwrap();
    let ElementKind::Component(c) = &e.kind else { panic!("not a component: {:?}", e.kind) };
    (e.type_id, c.clone())
}

/// Bounds and signed volume of a type's mesh pieces, and their materials.
fn mesh_of(s: &Session, ty: ElementTypeId) -> ([f64; 3], [f64; 3], f64, Vec<String>) {
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
    (lo, hi, volume, def.items.iter().map(|i| i.material.clone()).collect())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn elements(out: &Value) -> Vec<u64> {
    out["elements"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}

#[test]
fn glb_nodes_become_components() {
    let (bin, json) = scene();
    let path = scratch("scene.glb");
    std::fs::write(&path, glb(&json, &bin)).unwrap();
    let mut s = Session::default();
    let before = s.project().elements.len();
    let out = run(&mut s, "gltf_import", json!({ "path": path }));
    let ids = elements(&out);
    assert_eq!(ids.len(), 2, "{out}");
    assert_eq!(out["faces"], 36);
    assert!(s.project().materials.iter().any(|m| m.name == "Paint" && m.color == [255, 0, 0]));

    // Box: glTF (x, y, z) is the model's (x, -z, y); scaled, then moved.
    let (box_type, b) = component(&s, ids[0]);
    assert_eq!(s.project().element_type(box_type).unwrap().name, "Box");
    // x 4..6, y (model) -(-1..2) = -2..1, z 0..1: footprint center (5, -0.5).
    assert!(close(b.position.x, 5.0) && close(b.position.y, -0.5) && close(b.base_offset, 0.0), "{b:?}");
    let (lo, hi, volume, materials) = mesh_of(&s, box_type);
    assert!(close(hi[0] - lo[0], 2.0) && close(hi[1] - lo[1], 3.0) && close(hi[2] - lo[2], 1.0), "{lo:?} {hi:?}");
    assert!(close(volume, 6.0), "{volume}");
    assert_eq!(materials, vec![String::new()]);

    // Group: two cubes stacked (the child one up), turned a quarter: x 0..1, z -1..0 in glTF.
    let (group_type, g) = component(&s, ids[1]);
    assert_eq!(s.project().element_type(group_type).unwrap().name, "Group");
    assert!(close(g.position.x, 0.5) && close(g.position.y, 0.5) && close(g.base_offset, 0.0), "{g:?}");
    let (lo, hi, volume, materials) = mesh_of(&s, group_type);
    assert!(close(hi[2] - lo[2], 2.0) && close(hi[0] - lo[0], 1.0), "{lo:?} {hi:?}");
    assert!(close(volume, 2.0), "{volume}");
    assert_eq!(materials, vec!["Paint".to_string()]);

    // One undo step.
    run(&mut s, "undo", json!({}));
    assert_eq!(s.project().elements.len(), before);
}

#[test]
fn gltf_buffers_embedded_or_beside_and_merge() {
    let (bin, json) = scene();
    // A data URI.
    let mut embedded = json.clone();
    embedded["buffers"][0]["uri"] =
        json!(format!("data:application/octet-stream;base64,{}", cc_core::raster::base64_encode(&bin)));
    let path = scratch("embedded.gltf");
    std::fs::write(&path, serde_json::to_vec(&embedded).unwrap()).unwrap();
    let mut s = Session::default();
    let out = run(&mut s, "gltf_import", json!({ "path": path, "merge": true, "position": [10.0, 0.0] }));
    let ids = elements(&out);
    assert_eq!(ids.len(), 1, "{out}");
    let (ty, c) = component(&s, ids[0]);
    assert_eq!(s.project().element_type(ty).unwrap().name, "embedded");
    // Merged: at the file's origin, moved by the position.
    assert!(close(c.position.x, 10.0) && close(c.position.y, 0.0));
    let (_, _, volume, _) = mesh_of(&s, ty);
    assert!(close(volume, 8.0), "{volume}");

    // A file beside it, named with a space; a path does not reach outside the folder.
    std::fs::write(scratch("cube data.bin"), &bin).unwrap();
    let mut beside = json.clone();
    beside["buffers"][0]["uri"] = json!("cube%20data.bin");
    let path = scratch("beside.gltf");
    std::fs::write(&path, serde_json::to_vec(&beside).unwrap()).unwrap();
    assert_eq!(elements(&run(&mut s, "gltf_import", json!({ "path": path }))).len(), 2);
    beside["buffers"][0]["uri"] = json!("../../cube%20data.bin/../elsewhere.bin");
    std::fs::write(&path, serde_json::to_vec(&beside).unwrap()).unwrap();
    let err = s.execute("gltf_import", json!({ "path": path })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
}

#[test]
fn bad_gltf_files_are_errors() {
    let mut s = Session::default();
    let rejected = |s: &mut Session, name: &str, bytes: Vec<u8>| {
        let path = scratch(name);
        std::fs::write(&path, bytes).unwrap();
        let err = s.execute("gltf_import", json!({ "path": path })).unwrap_err();
        assert_eq!(err.code, ErrorCode::Rejected, "{name}: {err:?}");
    };
    assert_eq!(s.execute("gltf_import", json!({ "path": scratch("missing.glb") })).unwrap_err().code, ErrorCode::Io);
    rejected(&mut s, "garbage.glb", b"glTF\x02\0\0\0\xff\xff\xff\xff\0\0\0\0JSON".to_vec());
    rejected(&mut s, "text.gltf", b"\x00\xff not json".to_vec());
    let (bin, json) = scene();
    // Version 1.
    let mut old = json.clone();
    old["asset"]["version"] = json!("1.0");
    rejected(&mut s, "old.glb", glb(&old, &bin));
    // Compressed meshes.
    let mut draco = json.clone();
    draco["extensionsRequired"] = json!(["KHR_draco_mesh_compression"]);
    rejected(&mut s, "draco.glb", glb(&draco, &bin));
    // Offsets, counts and indices out of range, a node cycle, a bad matrix: nothing to import, no panic.
    let mut broken = json.clone();
    broken["accessors"][0]["count"] = json!(1_000_000);
    broken["accessors"][1]["byteOffset"] = json!(u64::MAX);
    broken["bufferViews"][1]["byteStride"] = json!(1);
    broken["nodes"][3]["children"] = json!([1, 2, 3, 99]);
    broken["nodes"][0]["matrix"] = json!([1, 2, 3]);
    broken["scenes"][0]["nodes"] = json!([0, 1, 7, -1, "x"]);
    rejected(&mut s, "broken.glb", glb(&broken, &bin));
    // Indices past the positions are skipped, the rest kept.
    let mut indices = json.clone();
    indices["accessors"][0]["count"] = json!(7);
    let short = scratch("short.glb");
    std::fs::write(&short, glb(&indices, &bin)).unwrap();
    let out = run(&mut s, "gltf_import", json!({ "path": short }));
    assert!(out["faces"].as_u64().unwrap() < 36, "{out}");
}

/// A 2 x 2 PNG, two red pixels over two blue ones.
fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(2, 2, |_, y| {
        if y == 0 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A textured, half-clear 2 x 4 m quad standing up (glTF x, y), its texture
/// repeated twice across and once up, the picture in the binary chunk.
#[test]
fn textures_colors_and_opacity_come_in() {
    let mut bin = Vec::new();
    for p in [[0.0f32, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 4.0, 0.0], [0.0, 4.0, 0.0]] {
        p.iter().for_each(|v| bin.extend_from_slice(&v.to_le_bytes()));
    }
    for t in [[0.0f32, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]] {
        t.iter().for_each(|v| bin.extend_from_slice(&v.to_le_bytes()));
    }
    for i in [0u16, 1, 2, 0, 2, 3] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    let image_at = bin.len();
    let picture = png();
    bin.extend_from_slice(&picture);
    let json = json!({
        "asset": { "version": "2.0" },
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "name": "Panel", "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0, "TEXCOORD_0": 1 }, "indices": 2, "material": 0 }] }],
        "materials": [{
            "name": "Tiles",
            "alphaMode": "BLEND",
            "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 0.5], "baseColorTexture": { "index": 0 } }
        }],
        "textures": [{ "source": 0 }],
        "images": [{ "bufferView": 3, "mimeType": "image/png" }],
        "buffers": [{ "byteLength": bin.len() }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 48, "byteLength": 32 },
            { "buffer": 0, "byteOffset": 80, "byteLength": 12 },
            { "buffer": 0, "byteOffset": image_at, "byteLength": picture.len() }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3" },
            { "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC2" },
            { "bufferView": 2, "componentType": 5123, "count": 6, "type": "SCALAR" }
        ]
    });
    let path = scratch("textured.glb");
    std::fs::write(&path, glb(&json, &bin)).unwrap();
    let mut s = Session::default();
    run(&mut s, "gltf_import", json!({ "path": path }));
    let m = s.project().materials.iter().find(|m| m.name == "Tiles").unwrap();
    // The color is the picture's average (half red, half blue, in linear light).
    let half = cc_engine::import_material::to_srgb(0.5);
    assert_eq!(m.color, [half, 0, half]);
    assert!((m.graphics.transparency - 0.5).abs() < 1e-12);
    let texture = m.appearance.texture.as_ref().expect("the picture is the texture");
    assert_eq!((texture.image.width, texture.image.height, texture.image.data.clone()), (2, 2, picture));
    // Twice across 2 m, once up 4 m.
    assert!(close(texture.width, 1.0) && close(texture.height, 4.0), "{} {}", texture.width, texture.height);
}
