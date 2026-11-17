//! Reader tests on small synthetic files (analytic coordinates).

use super::*;
use cc_core::cad::{CadHAlign, CadPrim, CadUnit, CadVAlign};
use glam::DVec2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Group pairs as ASCII DXF text.
fn pairs(groups: &[(i32, &str)]) -> String {
    groups.iter().map(|(c, v)| format!("{c:>3}\n{v}\n")).collect()
}

/// A DXF file with `$INSUNITS`, layers (name, color), blocks and entities (pair text).
fn file(units: Option<i32>, layers: &[(&str, i32)], blocks: &str, entities: &str) -> String {
    let mut s = String::new();
    s += &pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1015")]);
    if let Some(u) = units {
        s += &pairs(&[(9, "$INSUNITS"), (70, &u.to_string())]);
    }
    s += &pairs(&[(0, "ENDSEC"), (0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "LAYER")]);
    for (name, color) in layers {
        s += &pairs(&[(0, "LAYER"), (2, name), (70, "0"), (62, &color.to_string()), (6, "CONTINUOUS")]);
    }
    s += &pairs(&[(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "BLOCKS")]);
    s += blocks;
    s += &pairs(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    s += entities;
    s += &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    s
}

fn line(layer: &str, a: (f64, f64), b: (f64, f64)) -> String {
    pairs(&[
        (0, "LINE"),
        (8, layer),
        (10, &a.0.to_string()),
        (20, &a.1.to_string()),
        (30, "0"),
        (11, &b.0.to_string()),
        (21, &b.1.to_string()),
        (31, "0"),
    ])
}

fn read_str(s: &str) -> CadRead {
    read(s.as_bytes(), &Limits::default()).unwrap()
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn detects_formats() {
    assert_eq!(detect(b"AC1032\0\0\0"), Some(Format::Dwg));
    assert_eq!(detect(b"  0\nSECTION\n"), Some(Format::Dxf));
    assert_eq!(detect(tokens::BINARY_SENTINEL), Some(Format::Dxf));
    assert_eq!(detect(b"%PDF-1.4"), None);
    assert_eq!(detect(b""), None);
    assert!(matches!(read(b"hello", &Limits::default()), Err(CadError::UnknownFormat)));
}

#[test]
fn lines_arcs_layers_and_units() {
    let mut ents = line("Walls", (0.0, 0.0), (1000.0, 0.0));
    ents += &pairs(&[(0, "CIRCLE"), (8, "Walls"), (62, "1"), (10, "500"), (20, "500"), (30, "0"), (40, "250")]);
    ents += &pairs(&[(0, "ARC"), (8, "Hidden"), (10, "0"), (20, "0"), (30, "0"), (40, "100"), (50, "0"), (51, "90")]);
    let r = read_str(&file(Some(4), &[("0", 7), ("Walls", 3), ("Hidden", -2)], "", &ents));
    let d = &r.drawing;
    assert_eq!(r.entities, 3);
    assert_eq!(d.units, Some(CadUnit::Millimeters));
    assert_eq!(d.prims.len(), 3);
    let names: Vec<&str> = d.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["0", "Walls", "Hidden"]);
    assert_eq!(d.layers[1].color, [0, 255, 0]);
    assert!(!d.layers[2].visible, "a negative color turns the layer off");
    match &d.prims[0] {
        CadPrim::Polyline { layer, pts, closed, .. } => {
            assert_eq!(*layer, 1);
            assert!(!closed);
            assert!(close(pts[0], DVec2::ZERO) && close(pts[1], DVec2::new(1000.0, 0.0)));
        }
        p => panic!("{p:?}"),
    }
    match &d.prims[1] {
        CadPrim::Arc { center, radius, sweep, color, .. } => {
            assert!(close(*center, DVec2::new(500.0, 500.0)));
            assert_eq!(*radius, 250.0);
            assert!((sweep - TAU).abs() < 1e-12);
            assert_eq!(*color, Some([255, 0, 0]));
        }
        p => panic!("{p:?}"),
    }
    match &d.prims[2] {
        CadPrim::Arc { start, sweep, layer, .. } => {
            assert_eq!(*layer, 2);
            assert!(start.abs() < 1e-12 && (sweep - FRAC_PI_2).abs() < 1e-12);
        }
        p => panic!("{p:?}"),
    }
    let (lo, hi) = d.extents.unwrap();
    assert!(close(lo, DVec2::new(-100.0, -100.0)) && close(hi, DVec2::new(1000.0, 750.0)), "{lo} {hi}");
}

#[test]
fn bulged_lightweight_polyline() {
    // A 2 × 1 slot: two straight sides and two half circles.
    let ents = pairs(&[
        (0, "LWPOLYLINE"),
        (8, "0"),
        (90, "4"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (10, "2"),
        (20, "0"),
        (42, "1"),
        (10, "2"),
        (20, "1"),
        (10, "0"),
        (20, "1"),
        (42, "1"),
    ]);
    let r = read_str(&file(Some(6), &[], "", &ents));
    match &r.drawing.prims[0] {
        CadPrim::Polyline { pts, bulges, closed, .. } => {
            assert!(*closed);
            assert_eq!(pts.len(), 4);
            assert_eq!(bulges, &[0.0, 1.0, 0.0, 1.0]);
        }
        p => panic!("{p:?}"),
    }
    let (lo, hi) = r.drawing.extents.unwrap();
    assert!(close(lo, DVec2::new(-0.5, 0.0)) && close(hi, DVec2::new(2.5, 1.0)), "{lo} {hi}");
}

/// Block `DOOR` (base 10,0) holds a line on layer 0 and an arc in BYBLOCK
/// color; block `TWO` inserts `DOOR` twice. The model inserts `TWO` rotated
/// 90° and scaled by 2.
fn nested_blocks() -> String {
    let mut b = pairs(&[(0, "BLOCK"), (8, "0"), (2, "DOOR"), (70, "0"), (10, "10"), (20, "0"), (30, "0"), (3, "DOOR")]);
    b += &line("0", (10.0, 0.0), (11.0, 0.0));
    b += &pairs(&[
        (0, "ARC"),
        (8, "Detail"),
        (62, "0"),
        (10, "10"),
        (20, "0"),
        (30, "0"),
        (40, "1"),
        (50, "0"),
        (51, "90"),
    ]);
    b += &pairs(&[(0, "ENDBLK"), (8, "0")]);
    b += &pairs(&[(0, "BLOCK"), (8, "0"), (2, "TWO"), (70, "0"), (10, "0"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "INSERT"), (8, "0"), (2, "DOOR"), (10, "0"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "INSERT"), (8, "0"), (2, "DOOR"), (10, "5"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "ENDBLK"), (8, "0")]);
    b
}

#[test]
fn nested_inserts_with_rotation_scale_layers_and_colors() {
    let ents = pairs(&[
        (0, "INSERT"),
        (8, "Doors"),
        (62, "5"),
        (2, "TWO"),
        (10, "100"),
        (20, "0"),
        (30, "0"),
        (41, "2"),
        (42, "2"),
        (43, "2"),
        (50, "90"),
    ]);
    let r = read_str(&file(None, &[("0", 7), ("Doors", 1), ("Detail", 2)], &nested_blocks(), &ents));
    let d = &r.drawing;
    assert_eq!(r.blocks, 2);
    assert_eq!(d.prims.len(), 4);
    let doors = d.layers.iter().position(|l| l.name == "Doors").unwrap() as u32;
    // First DOOR: its line from block point (10,0)-(11,0), less the base (10,0),
    // is (0,0)-(1,0); scaled 2, turned 90° and moved to (100,0): (100,0)-(100,2).
    match &d.prims[0] {
        CadPrim::Polyline { pts, layer, color, .. } => {
            assert!(close(pts[0], DVec2::new(100.0, 0.0)) && close(pts[1], DVec2::new(100.0, 2.0)), "{pts:?}");
            assert_eq!(*layer, doors, "layer 0 takes the insert's layer");
            assert_eq!(*color, None);
        }
        p => panic!("{p:?}"),
    }
    match &d.prims[1] {
        CadPrim::Arc { center, radius, start, sweep, color, layer } => {
            assert!(close(*center, DVec2::new(100.0, 0.0)));
            assert!((radius - 2.0).abs() < 1e-12);
            assert!((start - FRAC_PI_2).abs() < 1e-12 && (sweep - FRAC_PI_2).abs() < 1e-12);
            // BYBLOCK takes the color of the insert holding it: the inner DOOR
            // insert, BYLAYER on layer 0, so the color of the outer layer.
            assert_eq!(*color, Some([255, 0, 0]));
            assert_ne!(*layer, doors, "other layers stay");
        }
        p => panic!("{p:?}"),
    }
    // Second DOOR, 5 along TWO's X: 10 along the turned X, i.e. up.
    match &d.prims[2] {
        CadPrim::Polyline { pts, .. } => assert!(close(pts[0], DVec2::new(100.0, 10.0)), "{pts:?}"),
        p => panic!("{p:?}"),
    }
}

#[test]
fn kept_blocks_become_references_and_others_stay_flat() {
    let mut b = nested_blocks();
    // A block that inserts itself through another one.
    b += &pairs(&[(0, "BLOCK"), (2, "P"), (10, "0"), (20, "0"), (30, "0")]);
    b += &line("0", (0.0, 0.0), (1.0, 0.0));
    b += &pairs(&[(0, "INSERT"), (8, "0"), (2, "Q"), (10, "0"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "ENDBLK")]);
    b += &pairs(&[(0, "BLOCK"), (2, "Q"), (10, "0"), (20, "0"), (30, "0")]);
    b += &line("0", (0.0, 0.0), (0.0, 1.0));
    b += &pairs(&[(0, "INSERT"), (8, "0"), (2, "P"), (10, "0"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "ENDBLK")]);
    let mut ents = pairs(&[(0, "INSERT"), (8, "Doors"), (2, "TWO"), (10, "100"), (20, "0"), (30, "0")]);
    ents += &pairs(&[(41, "2"), (42, "-2"), (43, "1"), (50, "90")]);
    // Stretched: flattened as before.
    ents += &pairs(&[(0, "INSERT"), (8, "0"), (2, "DOOR"), (10, "0"), (20, "0"), (30, "0"), (41, "2"), (42, "1")]);
    ents += &pairs(&[(0, "INSERT"), (8, "0"), (2, "P"), (10, "0"), (20, "50"), (30, "0")]);
    ents += &line("0", (0.0, 0.0), (3.0, 0.0));
    let text = file(None, &[("0", 7), ("Doors", 1), ("Detail", 2)], &b, &ents);
    let r = read(text.as_bytes(), &Limits { keep_blocks: true, ..Default::default() }).unwrap();
    assert_eq!(r.inserts.len(), 2, "{:?}", r.inserts);
    let two = &r.inserts[0];
    assert_eq!(two.block, "TWO");
    assert!(close(two.position, DVec2::new(100.0, 0.0)) && (two.scale - 2.0).abs() < 1e-12 && two.mirrored);
    assert!((two.rotation - FRAC_PI_2).abs() < 1e-12);
    // The stretched door (line and arc) and the loose line stay in the drawing.
    assert_eq!(r.drawing.prims.len(), 3);
    let names: Vec<&str> = r.kept.iter().map(|b| b.name.as_str()).collect();
    for n in ["TWO", "DOOR", "P", "Q"] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let block = |n: &str| r.kept.iter().find(|b| b.name == n).unwrap();
    // TWO holds two DOOR references, one 5 along its X; DOOR its line from its base.
    assert_eq!(block("TWO").inserts.len(), 2);
    assert!(close(block("TWO").inserts[1].position, DVec2::new(5.0, 0.0)));
    assert!(block("TWO").prims.is_empty());
    match &block("DOOR").prims[0] {
        CadPrim::Polyline { pts, .. } => assert!(close(pts[0], DVec2::ZERO) && close(pts[1], DVec2::X), "{pts:?}"),
        p => panic!("{p:?}"),
    }
    // P and Q insert each other: one of the two references is dropped.
    assert_eq!(block("P").inserts.len() + block("Q").inserts.len(), 1);
    // Without the option nothing changes.
    let flat = read_str(&text);
    assert!(flat.kept.is_empty() && flat.inserts.is_empty() && flat.drawing.prims.len() > 3);
}

#[test]
fn stretched_blocks_tessellate_arcs_and_mirrors_flip_them() {
    let mut b = pairs(&[(0, "BLOCK"), (2, "C"), (10, "0"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "CIRCLE"), (8, "0"), (10, "0"), (20, "0"), (30, "0"), (40, "1")]);
    b += &pairs(&[(0, "ARC"), (8, "0"), (10, "0"), (20, "0"), (30, "0"), (40, "1"), (50, "0"), (51, "90")]);
    b += &pairs(&[(0, "ENDBLK")]);
    let mut ents = pairs(&[(0, "INSERT"), (8, "0"), (2, "C"), (10, "0"), (20, "0"), (30, "0"), (41, "2"), (42, "1")]);
    ents += &pairs(&[(0, "INSERT"), (8, "0"), (2, "C"), (10, "10"), (20, "0"), (30, "0"), (41, "-1"), (42, "1")]);
    let r = read_str(&file(None, &[], &b, &ents));
    let d = &r.drawing;
    // The stretched circle is an ellipse 4 wide.
    match &d.prims[0] {
        CadPrim::Polyline { pts, closed, .. } => {
            assert!(*closed);
            let max_x = pts.iter().map(|p| p.x).fold(f64::MIN, f64::max);
            assert!((max_x - 2.0).abs() < 1e-9);
        }
        p => panic!("{p:?}"),
    }
    // The mirrored quarter arc turns clockwise from π.
    match &d.prims[3] {
        CadPrim::Arc { center, start, sweep, .. } => {
            assert!(close(*center, DVec2::new(10.0, 0.0)));
            assert!((start.abs() - PI).abs() < 1e-12 && (sweep + FRAC_PI_2).abs() < 1e-12, "{start} {sweep}");
        }
        p => panic!("{p:?}"),
    }
}

#[test]
fn downward_extrusion_mirrors_x() {
    let ents = pairs(&[
        (0, "CIRCLE"),
        (8, "0"),
        (10, "5"),
        (20, "3"),
        (30, "0"),
        (40, "1"),
        (210, "0"),
        (220, "0"),
        (230, "-1"),
    ]);
    let r = read_str(&file(None, &[], "", &ents));
    match &r.drawing.prims[0] {
        CadPrim::Arc { center, .. } => assert!(close(*center, DVec2::new(-5.0, 3.0)), "{center}"),
        p => panic!("{p:?}"),
    }
}

#[test]
fn texts_and_mtext() {
    let mut ents =
        pairs(&[(0, "TEXT"), (8, "0"), (10, "1"), (20, "2"), (30, "0"), (40, "0.5"), (1, "Sala %%c10"), (50, "90")]);
    ents += &pairs(&[
        (0, "TEXT"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (11, "4"),
        (21, "4"),
        (31, "0"),
        (40, "1"),
        (1, "C"),
        (72, "1"),
        (73, "2"),
    ]);
    ents += &pairs(&[
        (0, "MTEXT"),
        (8, "0"),
        (10, "0"),
        (20, "10"),
        (30, "0"),
        (40, "3"),
        (71, "1"),
        (1, "{\\fArial;Linha 1}\\PLinha 2"),
    ]);
    let r = read_str(&file(None, &[], "", &ents));
    let texts: Vec<_> = r
        .drawing
        .prims
        .iter()
        .filter_map(|p| match p {
            CadPrim::Text { pos, height, rotation, text, h, v, .. } => {
                Some((*pos, *height, *rotation, text.clone(), *h, *v))
            }
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 4);
    assert_eq!(texts[0].3, "Sala \u{2300}10");
    assert!((texts[0].2 - FRAC_PI_2).abs() < 1e-12 && close(texts[0].0, DVec2::new(1.0, 2.0)));
    assert!(close(texts[1].0, DVec2::new(4.0, 4.0)), "justified text sits at its second point");
    assert_eq!((texts[1].4, texts[1].5), (CadHAlign::Center, CadVAlign::Middle));
    // MTEXT attached top left: the first baseline one height below, the next 5/3 heights lower.
    assert_eq!(texts[2].3, "Linha 1");
    assert!(close(texts[2].0, DVec2::new(0.0, 7.0)), "{:?}", texts[2].0);
    assert!(close(texts[3].0, DVec2::new(0.0, 2.0)), "{:?}", texts[3].0);
}

#[test]
fn heavy_polylines_ellipses_splines_points_and_solids() {
    let mut ents = pairs(&[(0, "POLYLINE"), (8, "0"), (66, "1"), (10, "0"), (20, "0"), (30, "0"), (70, "0")]);
    for (x, y, b) in [(0.0, 0.0, 1.0), (2.0, 0.0, 0.0)] {
        ents += &pairs(&[
            (0, "VERTEX"),
            (8, "0"),
            (10, &x.to_string()),
            (20, &y.to_string()),
            (30, "0"),
            (42, &b.to_string()),
        ]);
    }
    ents += &pairs(&[(0, "SEQEND"), (8, "0")]);
    ents += &pairs(&[
        (0, "ELLIPSE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (11, "2"),
        (21, "0"),
        (31, "0"),
        (40, "0.5"),
        (41, "0"),
        (42, &TAU.to_string()),
    ]);
    ents += &pairs(&[
        (0, "SPLINE"),
        (8, "0"),
        (70, "8"),
        (71, "2"),
        (72, "6"),
        (73, "3"),
        (40, "0"),
        (40, "0"),
        (40, "0"),
        (40, "1"),
        (40, "1"),
        (40, "1"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (10, "1"),
        (20, "2"),
        (30, "0"),
        (10, "2"),
        (20, "0"),
        (30, "0"),
    ]);
    ents += &pairs(&[(0, "POINT"), (8, "0"), (10, "7"), (20, "8"), (30, "0")]);
    ents += &pairs(&[
        (0, "SOLID"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (11, "1"),
        (21, "0"),
        (31, "0"),
        (12, "0"),
        (22, "1"),
        (32, "0"),
        (13, "1"),
        (23, "1"),
        (33, "0"),
    ]);
    ents += &pairs(&[(0, "XLINE"), (8, "0")]);
    let r = read_str(&file(None, &[], "", &ents));
    let d = &r.drawing;
    assert_eq!(r.skipped.get("XLINE"), Some(&1));
    match &d.prims[0] {
        CadPrim::Polyline { pts, bulges, .. } => {
            assert_eq!(pts.len(), 2);
            assert_eq!(bulges, &[1.0, 0.0]);
        }
        p => panic!("{p:?}"),
    }
    match &d.prims[1] {
        CadPrim::Polyline { pts, closed, .. } => {
            assert!(*closed);
            let max_y = pts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            assert!((max_y - 1.0).abs() < 1e-3, "{max_y}");
        }
        p => panic!("{p:?}"),
    }
    match &d.prims[2] {
        CadPrim::Polyline { pts, .. } => {
            assert!(close(pts[0], DVec2::ZERO) && close(pts[pts.len() - 1], DVec2::new(2.0, 0.0)));
            assert!(pts.iter().any(|p| close(*p, DVec2::new(1.0, 1.0))), "the spline's middle point");
        }
        p => panic!("{p:?}"),
    }
    assert!(matches!(&d.prims[3], CadPrim::Polyline { pts, .. } if close(pts[0], DVec2::new(7.0, 8.0))));
    // The solid's corners go 1, 2, 4, 3: a square, filled and outlined.
    match &d.prims[4] {
        CadPrim::Fill { pts, .. } => {
            assert_eq!(pts, &[DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y]);
        }
        p => panic!("{p:?}"),
    }
    assert!(matches!(&d.prims[5], CadPrim::Polyline { closed: true, .. }));
}

#[test]
fn hatch_boundaries() {
    // A solid hatch: one polyline loop (a square), and a pattern hatch with
    // an edge loop (two lines and a half circle).
    let mut ents = pairs(&[
        (0, "HATCH"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (2, "SOLID"),
        (70, "1"),
        (71, "0"),
        (91, "1"),
        (92, "2"),
        (72, "0"),
        (73, "1"),
        (93, "4"),
        (10, "0"),
        (20, "0"),
        (10, "1"),
        (20, "0"),
        (10, "1"),
        (20, "1"),
        (10, "0"),
        (20, "1"),
        (97, "0"),
        (75, "0"),
        (76, "1"),
        (98, "0"),
    ]);
    ents += &pairs(&[
        (0, "HATCH"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (2, "ANSI31"),
        (70, "0"),
        (71, "0"),
        (91, "1"),
        (92, "1"),
        (93, "3"),
        (72, "1"),
        (10, "0"),
        (20, "0"),
        (11, "2"),
        (21, "0"),
        (72, "2"),
        (10, "1"),
        (20, "0"),
        (40, "1"),
        (50, "0"),
        (51, "180"),
        (73, "1"),
        (72, "1"),
        (10, "0"),
        (20, "0"),
        (11, "0"),
        (21, "0"),
        (97, "0"),
        (75, "0"),
        (76, "1"),
        (52, "0"),
        (41, "1"),
        (77, "0"),
        (78, "0"),
        (98, "1"),
        (10, "0.5"),
        (20, "0.5"),
    ]);
    let r = read_str(&file(None, &[], "", &ents));
    let d = &r.drawing;
    assert_eq!(d.prims.len(), 3, "{:?}", d.prims);
    assert!(matches!(&d.prims[0], CadPrim::Fill { pts, .. } if pts.len() == 4));
    assert!(matches!(&d.prims[1], CadPrim::Polyline { closed: true, pts, .. } if pts.len() == 4));
    match &d.prims[2] {
        CadPrim::Polyline { pts, .. } => {
            let top = pts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            assert!((top - 1.0).abs() < 1e-9, "the half circle reaches y = 1");
        }
        p => panic!("{p:?}"),
    }
}

#[test]
fn block_cycles_and_array_bombs_are_bounded() {
    let mut b = pairs(&[(0, "BLOCK"), (2, "A"), (10, "0"), (20, "0"), (30, "0")]);
    b += &line("0", (0.0, 0.0), (1.0, 0.0));
    b += &pairs(&[(0, "INSERT"), (8, "0"), (2, "A"), (10, "1"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "ENDBLK")]);
    b += &pairs(&[(0, "BLOCK"), (2, "BOMB"), (10, "0"), (20, "0"), (30, "0")]);
    b += &pairs(&[(0, "INSERT"), (8, "0"), (2, "A"), (70, "1000"), (71, "1000"), (44, "1"), (45, "1")]);
    b += &pairs(&[(0, "ENDBLK")]);
    let mut ents = pairs(&[(0, "INSERT"), (8, "0"), (2, "A"), (10, "0"), (20, "0"), (30, "0")]);
    ents += &pairs(&[(0, "INSERT"), (8, "0"), (2, "BOMB"), (70, "1000"), (71, "1000"), (44, "1"), (45, "1")]);
    let limits = Limits { max_prims: 50_000, max_work: 200_000, ..Default::default() };
    let r = read(file(None, &[], &b, &ents).as_bytes(), &limits).unwrap();
    assert!(r.truncated);
    assert!(r.drawing.prims.len() <= 50_000);
    // The self-insert stops at once: the first insert draws one line.
    assert!(matches!(&r.drawing.prims[0], CadPrim::Polyline { pts, .. } if close(pts[1], DVec2::X)));
}

/// The points of all polylines and the characters of all texts.
fn points_read(r: &CadRead) -> usize {
    r.drawing
        .prims
        .iter()
        .map(|p| match p {
            CadPrim::Polyline { pts, .. } | CadPrim::Fill { pts, .. } => pts.len(),
            CadPrim::Text { text, .. } => text.len(),
            _ => 1,
        })
        .sum()
}

#[test]
fn large_entities_in_arrayed_blocks_are_bounded_by_points() {
    // A block of one 2,000-vertex polyline, inserted 100 × 100 times: few
    // primitives and little work, but 20 million points.
    let mut b = pairs(&[(0, "BLOCK"), (2, "P"), (10, "0"), (20, "0"), (30, "0"), (0, "LWPOLYLINE"), (8, "0")]);
    b += &pairs(&[(90, "2000"), (70, "0")]);
    for i in 0..2000 {
        b += &pairs(&[(10, &i.to_string()), (20, &(i % 2).to_string())]);
    }
    b += &pairs(&[(0, "ENDBLK")]);
    // And a block of one long multi-line text.
    b += &pairs(&[(0, "BLOCK"), (2, "T"), (10, "0"), (20, "0"), (30, "0"), (0, "MTEXT"), (8, "0"), (40, "1")]);
    b += &pairs(&[(10, "0"), (20, "0"), (30, "0"), (1, &"abcdefghij\\P".repeat(500))]);
    b += &pairs(&[(0, "ENDBLK")]);
    let array =
        |name: &str| pairs(&[(0, "INSERT"), (8, "0"), (2, name), (70, "100"), (71, "100"), (44, "1"), (45, "1")]);
    let limits = Limits { max_points: 300_000, ..Default::default() };
    for name in ["P", "T"] {
        let r = read(file(None, &[], &b, &array(name)).as_bytes(), &limits).unwrap();
        assert!(r.truncated, "{name}");
        assert!(points_read(&r) <= 300_000, "{name}: {}", points_read(&r));
        assert!(!r.drawing.prims.is_empty(), "{name}");
    }
}

#[test]
fn code_page_text_and_binary_dxf() {
    let mut s = pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$DWGCODEPAGE"), (3, "ANSI_1252"), (0, "ENDSEC")]);
    s += &pairs(&[(0, "SECTION"), (2, "ENTITIES")]);
    let mut bytes = s.into_bytes();
    bytes.extend(pairs(&[(0, "TEXT"), (8, "0"), (10, "0"), (20, "0"), (30, "0"), (40, "1"), (1, "")]).as_bytes());
    // Overwrite the empty text with a Latin-1 encoded word.
    bytes.truncate(bytes.len() - 1);
    bytes.extend(b"Cora\xe7\xe3o\n");
    bytes.extend(pairs(&[(0, "ENDSEC"), (0, "EOF")]).as_bytes());
    let r = read(&bytes, &Limits::default()).unwrap();
    assert!(matches!(&r.drawing.prims[0], CadPrim::Text { text, .. } if text == "Coração"));

    // The same line in binary DXF.
    let mut b = tokens::BINARY_SENTINEL.to_vec();
    let s = |b: &mut Vec<u8>, code: i16, v: &str| {
        b.extend(code.to_le_bytes());
        b.extend(v.as_bytes());
        b.push(0);
    };
    let f = |b: &mut Vec<u8>, code: i16, v: f64| {
        b.extend(code.to_le_bytes());
        b.extend(v.to_le_bytes());
    };
    s(&mut b, 0, "SECTION");
    s(&mut b, 2, "ENTITIES");
    s(&mut b, 0, "LINE");
    s(&mut b, 8, "0");
    for (code, v) in [(10, 1.0), (20, 2.0), (30, 0.0), (11, 3.0), (21, 4.0), (31, 0.0)] {
        f(&mut b, code, v);
    }
    s(&mut b, 0, "ENDSEC");
    s(&mut b, 0, "EOF");
    let r = read(&b, &Limits::default()).unwrap();
    assert!(r.binary);
    assert!(matches!(&r.drawing.prims[0], CadPrim::Polyline { pts, .. } if close(pts[1], DVec2::new(3.0, 4.0))));
}

/// Truncated, corrupted and random data never panics.
#[test]
fn hostile_input_does_not_panic() {
    let mut ents = line("0", (0.0, 0.0), (1.0, 0.0));
    ents += &pairs(&[(0, "INSERT"), (8, "0"), (2, "TWO"), (10, "1"), (20, "2"), (30, "0"), (66, "1")]);
    ents += &pairs(&[(0, "ATTRIB"), (8, "0"), (10, "0"), (20, "0"), (30, "0"), (40, "1"), (1, "A")]);
    ents += &pairs(&[(0, "SEQEND")]);
    let good = file(Some(4), &[("0", 7)], &nested_blocks(), &ents).into_bytes();
    for end in (0..good.len()).step_by(7) {
        let _ = read(&good[..end], &Limits::default());
    }
    // Byte flips with a simple deterministic generator.
    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..300 {
        let mut bad = good.clone();
        for _ in 0..4 {
            let i = (next() as usize) % bad.len();
            bad[i] = (next() & 0xff) as u8;
        }
        let _ = read(&bad, &Limits::default());
    }
    for n in [1usize, 10, 100, 1000] {
        let mut noise: Vec<u8> = (0..n).map(|_| (next() & 0xff) as u8).collect();
        let _ = read(&noise, &Limits::default());
        noise.splice(0..0, tokens::BINARY_SENTINEL.iter().copied());
        let _ = read(&noise, &Limits::default());
    }
    // Binary DXF with a huge binary chunk length.
    let mut b = tokens::BINARY_SENTINEL.to_vec();
    b.extend(310i16.to_le_bytes());
    b.push(255);
    let _ = read(&b, &Limits::default());
    // A DWG header followed by garbage is an error or an empty drawing, not a panic.
    for version in [&b"AC1015"[..], b"AC1018", b"AC1032", b"AC1009"] {
        let mut dwg = version.to_vec();
        dwg.extend((0..4000).map(|_| (next() & 0xff) as u8));
        if let Ok(r) = read(&dwg, &Limits::default()) {
            assert!(r.drawing.prims.len() < 100);
        }
    }
}

#[test]
fn file_size_limit() {
    let limits = Limits { max_dxf_bytes: 10, ..Default::default() };
    assert_eq!(read(b"  0\nSECTION\n  2\nENTITIES\n", &limits).err(), Some(CadError::TooLarge(0)));
}

/// A DWG written by acadrust (in the test) is read back through the same path.
#[test]
fn reads_dwg_written_by_acadrust() {
    use acadrust::entities::{Circle, EntityType, Line};
    use acadrust::types::Vector3;
    let mut doc = acadrust::CadDocument::new();
    doc.header.insertion_units = 6;
    let line = Line::from_points(Vector3::new(0.0, 0.0, 0.0), Vector3::new(3.0, 4.0, 0.0));
    doc.add_entity(EntityType::Line(line)).unwrap();
    let circle = Circle::from_center_radius(Vector3::new(1.0, 1.0, 0.0), 0.5);
    doc.add_entity(EntityType::Circle(circle)).unwrap();
    let bytes = acadrust::DwgWriter::write_to_vec(&doc).unwrap();
    assert_eq!(detect(&bytes), Some(Format::Dwg));
    let r = read(&bytes, &Limits::default()).unwrap();
    assert_eq!(r.format, Format::Dwg);
    assert_eq!(r.drawing.units, Some(CadUnit::Meters));
    let line = r.drawing.prims.iter().find_map(|p| match p {
        CadPrim::Polyline { pts, .. } => Some(pts.clone()),
        _ => None,
    });
    let line = line.unwrap();
    assert!(close(line[0], DVec2::ZERO) && close(line[1], DVec2::new(3.0, 4.0)), "{line:?}");
    let circle = r.drawing.prims.iter().find_map(|p| match p {
        CadPrim::Arc { center, radius, .. } => Some((*center, *radius)),
        _ => None,
    });
    assert_eq!(circle, Some((DVec2::ONE, 0.5)));
}
