//! Exploding an imported drawing into detail lines, filled regions and text
//! notes (`cad_explode`): placement, layers to line styles, hidden layers,
//! types by color and size, pictures kept, one undo step and the limits.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::cad::CadImage;
use cc_core::detail::LineStyleRef;
use cc_core::sketch::SketchCurve;
use cc_core::{ElementId, ElementKind, Pattern, TypeData};
use cc_engine::{ErrorCode, Session};
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-explode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn pairs(groups: &[(i32, &str)]) -> String {
    groups.iter().map(|(c, v)| format!("{c:>3}\n{v}\n")).collect()
}

/// A millimeter drawing: on `Walls` (red) a 4 m line, a quarter arc of
/// 1 m, a polyline with a half-circle bulge and a circle; on `0` a text
/// 250 mm tall and a blue solid; on the frozen layer `Notes` a line.
fn sample_dxf() -> String {
    let mut s = pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]);
    s += &pairs(&[(0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "LAYER")]);
    s += &pairs(&[(0, "LAYER"), (2, "0"), (70, "0"), (62, "7")]);
    s += &pairs(&[(0, "LAYER"), (2, "Walls"), (70, "0"), (62, "1")]);
    s += &pairs(&[(0, "LAYER"), (2, "Notes"), (70, "1"), (62, "3")]);
    s += &pairs(&[(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    s += &pairs(&[(0, "LINE"), (8, "Walls"), (10, "0"), (20, "0"), (30, "0"), (11, "4000"), (21, "0"), (31, "0")]);
    s += &pairs(&[(0, "ARC"), (8, "Walls"), (10, "0"), (20, "0"), (30, "0"), (40, "1000"), (50, "0"), (51, "90")]);
    s += &pairs(&[
        (0, "LWPOLYLINE"),
        (8, "Walls"),
        (90, "3"),
        (70, "0"),
        (10, "0"),
        (20, "3000"),
        (42, "1"),
        (10, "2000"),
        (20, "3000"),
        (10, "2000"),
        (20, "5000"),
    ]);
    s += &pairs(&[(0, "CIRCLE"), (8, "Walls"), (10, "8000"), (20, "0"), (30, "0"), (40, "500")]);
    s += &pairs(&[(0, "TEXT"), (8, "0"), (10, "100"), (20, "200"), (30, "0"), (40, "250"), (1, "Sala")]);
    s += &pairs(&[
        (0, "SOLID"),
        (8, "0"),
        (62, "5"),
        (10, "5000"),
        (20, "0"),
        (11, "6000"),
        (21, "0"),
        (12, "5000"),
        (22, "1000"),
        (13, "6000"),
        (23, "1000"),
    ]);
    s += &pairs(&[(0, "LINE"), (8, "Notes"), (10, "0"), (20, "-500"), (30, "0"), (11, "100"), (21, "-500"), (31, "0")]);
    s += &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    s
}

fn plan(s: &mut Session) -> u64 {
    let views = run(s, "views_list", json!({"category": "floor_plan"}));
    views[0]["id"].as_u64().unwrap()
}

fn import(s: &mut Session, args: Value) -> u64 {
    // A file of its own per call: the tests run in parallel and a shared one
    // could be read while another test rewrites it.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = scratch(&format!("explode-{n}.dxf"));
    std::fs::write(&path, sample_dxf()).unwrap();
    let mut args = args;
    args["path"] = json!(path);
    run(s, "cad_import", args)["id"].as_u64().unwrap()
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

fn curves(s: &Session, ids: &[u64]) -> Vec<(SketchCurve, LineStyleRef)> {
    ids.iter()
        .filter_map(|id| match &s.doc.project.element(ElementId(*id))?.kind {
            ElementKind::DetailLine(l) => Some((l.curve, l.style)),
            _ => None,
        })
        .collect()
}

fn ids(v: &Value) -> Vec<u64> {
    v["created"].as_array().unwrap().iter().map(|i| i.as_u64().unwrap()).collect()
}

#[test]
fn explodes_into_native_elements_in_one_step() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let id = import(&mut s, json!({"view": view, "position": [10.0, 20.0]}));
    let styles_before = s.doc.project.line_styles.len();
    let elements_before = s.doc.project.elements.len();

    let out = run(&mut s, "cad_explode", json!({"id": id}));
    // Line, arc, two polyline segments, circle, the solid's outline (4 sides); not the frozen layer.
    assert_eq!(out["detail_lines"], 9, "{out}");
    assert_eq!(out["filled_regions"], 1);
    assert_eq!(out["texts"], 1);
    assert_eq!(out["kept"], Value::Null, "no pictures: the element goes");
    assert!(s.doc.project.element(ElementId(id)).is_none());
    let created = ids(&out);
    assert_eq!(created.len(), 11);

    // Layers became project line styles by name and color.
    let p = &s.doc.project;
    assert_eq!(p.line_styles.len(), styles_before + 2);
    let walls = p.line_styles.values().find(|st| st.name == "Walls").unwrap();
    assert_eq!(walls.color, [255, 0, 0]);
    assert!(p.line_styles.values().any(|st| st.name == "0"));

    // Placement applied: millimeters to meters at (10, 20).
    let cs = curves(&s, &created);
    let on_walls = LineStyleRef::Project(walls.id);
    assert!(cs.iter().any(|(c, st)| *st == on_walls
        && matches!(c, SketchCurve::Line { a, b } if close(*a, DVec2::new(10.0, 20.0)) && close(*b, DVec2::new(14.0, 20.0)))));
    // The quarter arc from (11, 20) to (10, 21), counter-clockwise.
    let q = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
    assert!(cs.iter().any(|(c, _)| matches!(c, SketchCurve::Arc { a, b, bulge }
        if close(*a, DVec2::new(11.0, 20.0)) && close(*b, DVec2::new(10.0, 21.0)) && (bulge - q).abs() < 1e-9)));
    // The bulged polyline segment stays an arc, the next one a line.
    assert!(cs.iter().any(|(c, _)| matches!(c, SketchCurve::Arc { a, b, bulge }
        if close(*a, DVec2::new(10.0, 23.0)) && close(*b, DVec2::new(12.0, 23.0)) && (bulge - 1.0).abs() < 1e-9)));
    assert!(cs.iter().any(|(c, _)| matches!(c, SketchCurve::Line { a, b }
        if close(*a, DVec2::new(12.0, 23.0)) && close(*b, DVec2::new(12.0, 25.0)))));
    assert!(cs.iter().any(|(c, _)| matches!(c, SketchCurve::Circle { center, radius }
        if close(*center, DVec2::new(18.0, 20.0)) && (radius - 0.5).abs() < 1e-9)));
    assert!(!cs.iter().any(|(c, _)| matches!(c, SketchCurve::Line { a, .. } if (a.y - 19.5).abs() < 1e-9)));

    // The solid: a filled region of a solid blue type, no boundary.
    let region = created
        .iter()
        .find_map(|i| {
            let e = p.element(ElementId(*i))?;
            match &e.kind {
                ElementKind::FilledRegion(r) => Some((r.clone(), e.type_id)),
                _ => None,
            }
        })
        .unwrap();
    assert!((region.0.area() - 1.0).abs() < 1e-9);
    assert_eq!(region.0.boundary, None);
    assert!(matches!(&p.element_type(region.1).unwrap().data,
        TypeData::FilledRegion { fill } if fill.foreground == Pattern::Solid && fill.foreground_color == [0, 0, 255]));

    // The text: a note of 0.25 m on paper at the view's scale, its baseline where the text was.
    let scale = f64::from(p.view(cc_core::ViewId(view)).unwrap().scale);
    let (note, type_id) = created
        .iter()
        .find_map(|i| {
            let e = p.element(ElementId(*i))?;
            match &e.kind {
                ElementKind::Text(n) => Some(((**n).clone(), e.type_id)),
                _ => None,
            }
        })
        .unwrap();
    assert_eq!(note.text, "Sala");
    let size = match &p.element_type(type_id).unwrap().data {
        TypeData::Text { size, .. } => *size,
        other => panic!("{other:?}"),
    };
    assert!((size - (250.0 / scale * 10.0).round() / 10.0).abs() < 1e-9, "{size}");
    let layout = cc_views::notes::text_layout(&note, size, scale as u32);
    assert!(close(layout.lines[0].0, DVec2::new(10.1, 20.2)), "{:?}", layout.lines[0].0);

    // One undo step brings the drawing back and removes everything made.
    run(&mut s, "undo", json!({}));
    assert!(s.doc.project.element(ElementId(id)).is_some());
    assert_eq!(s.doc.project.elements.len(), elements_before);
    assert_eq!(s.doc.project.line_styles.len(), styles_before);

    // A second explode reuses the styles and types made by the first (redo, then a new import).
    run(&mut s, "redo", json!({}));
    let types = s.doc.project.types.len();
    let styles = s.doc.project.line_styles.len();
    let again = import(&mut s, json!({"view": view}));
    let out = run(&mut s, "cad_explode", json!({"id": again}));
    assert_eq!(out["line_styles"], json!([]));
    assert_eq!(out["types"], json!([]), "{out}");
    assert_eq!(s.doc.project.types.len(), types, "the imported drawing type stays from the first import");
    assert_eq!(s.doc.project.line_styles.len(), styles);
}

#[test]
fn mirrored_and_turned_placement_and_hidden_layers() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let id = import(&mut s, json!({"view": view}));
    run(&mut s, "element_set", json!({"id": id, "params": {"rotation_deg": 90.0, "mirrored": true}}));
    // The layer 0 hidden in the view: no text, no fill.
    run(&mut s, "cad_layers_set", json!({"id": id, "layers": ["0"], "visible": false}));
    let out = run(&mut s, "cad_explode", json!({"id": id}));
    assert_eq!((out["texts"].clone(), out["filled_regions"].clone()), (json!(0), json!(0)), "{out}");
    let cs = curves(&s, &ids(&out));
    assert_eq!(cs.len(), 5);
    // Mirror across X then turn 90°: (x, y) -> (y, x). The 4 m line runs up.
    assert!(cs.iter().any(|(c, _)| matches!(c, SketchCurve::Line { a, b }
        if close(*a, DVec2::ZERO) && close(*b, DVec2::new(0.0, 4.0)))));
    // The quarter arc turns the other way: (1, 0) -> (0, 1) becomes (0, 1) -> (1, 0), clockwise.
    let q = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
    let arc = cs.iter().find_map(|(c, _)| match c {
        SketchCurve::Arc { a, b, bulge } if close(*a, DVec2::new(0.0, 1.0)) => Some((*b, *bulge)),
        _ => None,
    });
    let (b, bulge) = arc.unwrap();
    assert!(close(b, DVec2::new(1.0, 0.0)) && (bulge + q).abs() < 1e-9, "{b} {bulge}");
    // The arc still passes through the placed midpoint of the original.
    let mid = cc_core::arc::Arc::from_bulge(DVec2::new(0.0, 1.0), b, bulge).unwrap();
    let m = mid.center + DVec2::from_angle(mid.start_angle + mid.sweep * 0.5) * mid.radius;
    let s2 = std::f64::consts::FRAC_1_SQRT_2;
    assert!(close(m, DVec2::new(s2, s2)), "{m}");
}

#[test]
fn pictures_stay_and_bad_targets_are_refused() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let id = import(&mut s, json!({"view": view}));
    // A picture in the drawing (on layer 0), as an SVG import would hold.
    let picture = CadImage {
        layer: 0,
        origin: DVec2::ZERO,
        u: DVec2::new(1000.0, 0.0),
        v: DVec2::new(0.0, 1000.0),
        image: std::sync::Arc::new(cc_core::raster::RasterImage {
            format: cc_core::raster::RasterFormat::Png,
            data: vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
            width: 1,
            height: 1,
        }),
    };
    s.doc.edit("picture", |e| {
        if let Some(ElementKind::CadImport(c)) = e.element_mut(ElementId(id)).map(|x| &mut x.kind) {
            let mut d = (*c.drawing).clone();
            d.images.push(picture);
            c.drawing = std::sync::Arc::new(d);
        }
    });
    let out = run(&mut s, "cad_explode", json!({"id": id}));
    assert_eq!(out["kept"], json!(id));
    let kept = match &s.doc.project.element(ElementId(id)).unwrap().kind {
        ElementKind::CadImport(c) => c.clone(),
        other => panic!("{other:?}"),
    };
    assert!(kept.drawing.prims.is_empty());
    assert_eq!(kept.drawing.images.len(), 1);
    // Only pictures left: nothing to explode.
    assert_eq!(fails(&mut s, "cad_explode", json!({"id": id})), ErrorCode::InvalidArguments);
    // Not an imported drawing.
    let wall = s.doc.project.walls().next().map(|(e, _)| e.id.0);
    if let Some(wall) = wall {
        assert_eq!(fails(&mut s, "cad_explode", json!({"id": wall})), ErrorCode::InvalidArguments);
    }
    assert_eq!(fails(&mut s, "cad_explode", json!({"id": 999_999})), ErrorCode::NotFound);
}

#[test]
fn too_many_elements_are_refused_without_a_step() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let id = import(&mut s, json!({"view": view}));
    // One polyline of more segments than one explode makes.
    let n = cc_engine::commands::cad_explode::MAX_ELEMENTS + 10;
    s.doc.edit("big", |e| {
        if let Some(ElementKind::CadImport(c)) = e.element_mut(ElementId(id)).map(|x| &mut x.kind) {
            let mut d = (*c.drawing).clone();
            for k in 0..(n / 50_000 + 1) {
                let pts = (0..50_001).map(|i| DVec2::new(i as f64 * 10.0, k as f64 * 10.0)).collect();
                d.prims.push(cc_core::cad::CadPrim::Polyline {
                    layer: 1,
                    color: None,
                    closed: false,
                    pts,
                    bulges: Vec::new(),
                });
            }
            c.drawing = std::sync::Arc::new(d);
        }
    });
    let revision = s.doc.revision();
    assert_eq!(fails(&mut s, "cad_explode", json!({"id": id})), ErrorCode::Rejected);
    assert_eq!(s.doc.revision(), revision);
    assert!(s.doc.project.element(ElementId(id)).is_some());
}
