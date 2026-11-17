//! Detail components (issue #151): reusable 2D components made of pieces
//! or of a view's detail elements, placed in 2D views with position,
//! rotation, mirror and scale, nested in each other (cycles refused),
//! saved to and loaded from .ccomp files, brought in from DXF blocks and
//! exported to DXF as blocks and inserts.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::Prim;
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-detail-components-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn drafting(s: &mut Session) -> u64 {
    run(s, "view_create", json!({"kind": "drafting"}))["id"].as_u64().unwrap()
}

/// The lines element `id` draws in `view`.
fn lines_of(s: &mut Session, view: u64, id: u64) -> Vec<(DVec2, DVec2)> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { a, b, element: Some(e), .. } if e.0 == id => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn has_line(lines: &[(DVec2, DVec2)], a: DVec2, b: DVec2) -> bool {
    lines
        .iter()
        .any(|(p, q)| (p.distance(a) < 1e-6 && q.distance(b) < 1e-6) || (p.distance(b) < 1e-6 && q.distance(a) < 1e-6))
}

fn line_piece(a: [f64; 2], b: [f64; 2]) -> Value {
    json!({"kind": "stroke", "points": [{"u": a[0], "v": a[1]}, {"u": b[0], "v": b[1]}]})
}

#[test]
fn detail_components_are_made_placed_nested_and_edited() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    // A brick: a 0.2 x 0.1 rectangle; a wall of two bricks nests it twice.
    let brick = run(
        &mut s,
        "detail_component_create",
        json!({"name": "Brick", "pieces": [
            {"kind": "stroke", "closed": true, "fill": [200, 80, 60], "points": [
                {"u": 0, "v": 0}, {"u": 0.2, "v": 0}, {"u": 0.2, "v": 0.1}, {"u": 0, "v": 0.1}]},
            {"kind": "text", "x": 0.05, "y": 0.02, "text": "B", "size": 0.05}
        ]}),
    );
    assert_eq!(brick["category"], "detailcomponent");
    let wall = run(
        &mut s,
        "detail_component_create",
        json!({"name": "Wall", "pieces": [
            line_piece([0.0, -0.05], [0.5, -0.05]),
            {"kind": "nested", "component": "Brick", "x": 0, "y": 0},
            {"kind": "nested", "component": "Brick", "x": 0.4, "y": 0, "rotation": 90}
        ]}),
    );
    let wall_type = wall["id"].as_u64().unwrap();
    // Names are unique; a nested piece must name a detail component, and no cycle.
    assert_eq!(
        fails(
            &mut s,
            "detail_component_create",
            json!({"name": "Brick", "pieces": [line_piece([0.0, 0.0], [1.0, 0.0])]})
        ),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(
            &mut s,
            "component_sheet_piece_add",
            json!({"type": "Brick", "piece": {"kind": "nested", "component": "Wall", "x": 0, "y": 0}})
        ),
        ErrorCode::InvalidArguments,
        "Wall holds Brick"
    );
    assert_eq!(
        fails(
            &mut s,
            "component_sheet_piece_add",
            json!({"type": "Brick", "piece": {"kind": "nested", "component": "Nope", "x": 0, "y": 0}})
        ),
        ErrorCode::InvalidArguments
    );

    let placed = run(
        &mut s,
        "detail_component_place",
        json!({"type": wall_type, "view": view, "position": [10, 5], "rotation": 90, "scale": 2}),
    );
    let id = placed["id"].as_u64().unwrap();
    assert_eq!(placed["derived"]["component"], "Wall");
    let lines = lines_of(&mut s, view, id);
    // The wall's own line (0,-0.05)-(0.5,-0.05), doubled and turned 90°: (10.1,5)-(10.1,6).
    assert!(has_line(&lines, DVec2::new(10.1, 5.0), DVec2::new(10.1, 6.0)), "{lines:?}");
    // The first brick's bottom edge (0,0)-(0.2,0): (10,5)-(10,5.4).
    assert!(has_line(&lines, DVec2::new(10.0, 5.0), DVec2::new(10.0, 5.4)));
    // 1 own line + 4 edges for each of the two bricks.
    assert_eq!(lines.len(), 9);
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let texts = d.prims.iter().filter(|p| matches!(p, Prim::Text { element: Some(e), .. } if e.0 == id)).count();
    assert_eq!(texts, 2);

    // Mirror through element_set; a bad scale or another view is refused.
    run(&mut s, "element_set", json!({"id": id, "params": {"mirrored": true, "scale": 1}}));
    let lines = lines_of(&mut s, view, id);
    // Mirrored across its X axis then turned: (0,-0.05) maps to (9.95, 5).
    assert!(has_line(&lines, DVec2::new(9.95, 5.0), DVec2::new(9.95, 5.5)), "{lines:?}");
    assert_eq!(fails(&mut s, "element_set", json!({"id": id, "params": {"scale": 0}})), ErrorCode::InvalidArguments);
    let other = drafting(&mut s);
    assert_eq!(fails(&mut s, "element_set", json!({"id": id, "params": {"view": other}})), ErrorCode::InvalidArguments);
    // Not in a 3D view; not a component of another kind.
    let three_d = run(&mut s, "views_list", json!({"category": "three_d"}))[0]["id"].clone();
    assert_eq!(
        fails(&mut s, "detail_component_place", json!({"type": wall_type, "view": three_d, "position": [0, 0]})),
        ErrorCode::InvalidArguments
    );

    // Moved with the generic commands, and gone with its view.
    run(&mut s, "elements_move", json!({"ids": [id], "delta": [1, 0]}));
    assert_eq!(run(&mut s, "element_get", json!({"id": id}))["params"]["position"], json!([11.0, 5.0]));
    run(&mut s, "view_delete", json!({"view": view}));
    assert_eq!(fails(&mut s, "element_get", json!({"id": id})), ErrorCode::NotFound);
}

#[test]
fn detail_components_are_made_from_a_views_elements() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    let line = run(
        &mut s,
        "detail_line_create",
        json!({"view": view, "style": "wide", "curves": [{"kind": "line", "start": [2, 1], "end": [4, 1]}]}),
    )["created"][0]
        .as_u64()
        .unwrap();
    let region = run(
        &mut s,
        "filled_region_create",
        json!({"view": view, "curves": [{"kind": "rectangle", "corner": [2, 1], "opposite": [3, 2]}]}),
    )["id"]
        .as_u64()
        .unwrap();
    let text =
        run(&mut s, "text_create", json!({"view": view, "position": [2, 3], "text": "Detail"}))["id"].as_u64().unwrap();
    let out =
        run(&mut s, "detail_component_create", json!({"name": "Sill", "view": view, "elements": [line, region, text]}));
    let placed = out["placed"].as_u64().unwrap();
    // The sources are replaced by the component, its origin at their lower left corner.
    for gone in [line, region, text] {
        assert_eq!(fails(&mut s, "element_get", json!({"id": gone})), ErrorCode::NotFound);
    }
    let got = run(&mut s, "element_get", json!({"id": placed}));
    assert_eq!(got["params"]["position"], json!([2.0, 1.0]));
    let def = run(&mut s, "component_get", json!({"type": "Sill"}));
    let sheet = def["def"]["sheet"].as_array().unwrap();
    assert_eq!(sheet.len(), 3);
    // The wide line keeps its pen; it runs from the origin.
    let stroke = &sheet[0]["shape"]["Stroke"];
    assert_eq!(stroke["weight"], "0.5");
    assert_eq!(stroke["points"][0]["u"], "0");
    let lines = lines_of(&mut s, view, placed);
    assert!(has_line(&lines, DVec2::new(2.0, 1.0), DVec2::new(4.0, 1.0)), "drawn where it was");
    // One undo step brings the elements back.
    run(&mut s, "undo", json!({}));
    assert!(s.project().element(cc_core::ElementId(line)).is_some());
    assert!(s.project().element(cc_core::ElementId(placed)).is_none());
    // Elements of another kind, or of another view, are refused.
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap();
    assert_eq!(
        fails(&mut s, "detail_component_create", json!({"name": "X", "view": plan, "elements": [line]})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn detail_components_round_trip_through_component_files_with_what_they_nest() {
    let mut s = Session::default();
    run(&mut s, "detail_component_create", json!({"name": "Leaf", "pieces": [line_piece([0.0, 0.0], [1.0, 0.0])]}));
    run(
        &mut s,
        "detail_component_create",
        json!({"name": "Holder", "pieces": [
            line_piece([0.0, 0.0], [0.0, 1.0]),
            {"kind": "nested", "component": "Leaf", "x": 0, "y": 1}
        ]}),
    );
    let path = scratch("holder.ccomp");
    run(&mut s, "component_save", json!({"component": "Holder", "path": path}));
    let listed = run(&mut s, "component_library_list", json!({"folders": [path.parent().unwrap()]}));
    let entry = listed["components"].as_array().unwrap().iter().find(|c| c["component"] == "Holder").unwrap();
    assert_eq!(entry["kind"], "Detail");
    assert_eq!(entry["category"], "detail_component");

    let mut other = Session::default();
    run(&mut other, "component_load", json!({"path": path}));
    let p = other.project();
    assert!(p.detail_def("Holder").is_some() && p.detail_def("Leaf").is_some(), "the nested one comes along");
    let view = drafting(&mut other);
    let id =
        run(&mut other, "detail_component_place", json!({"type": "Holder", "view": view, "position": [0, 0]}))["id"]
            .as_u64()
            .unwrap();
    let lines = lines_of(&mut other, view, id);
    assert!(has_line(&lines, DVec2::new(0.0, 1.0), DVec2::new(1.0, 1.0)), "{lines:?}");

    // A crafted file with a bad piece is refused.
    let text = std::fs::read_to_string(&path).unwrap().replace("component: \"Leaf\"", "component: \"\"");
    let bad = scratch("bad.ccomp");
    std::fs::write(&bad, text).unwrap();
    assert!(other.execute("component_load", json!({"path": bad})).is_err());
}

/// A millimeter DXF: block `DOOR` (a 1 m line), block `PAIR` (two DOORs, the
/// second 2 m along), PAIR inserted turned 90° at (10 m, 0), DOOR inserted
/// stretched (flattened), and a loose line.
fn blocks_dxf() -> String {
    let pairs = |groups: &[(i32, &str)]| -> String { groups.iter().map(|(c, v)| format!("{c:>3}\n{v}\n")).collect() };
    let line = |x0: &str, y0: &str, x1: &str, y1: &str| {
        pairs(&[(0, "LINE"), (8, "0"), (10, x0), (20, y0), (30, "0"), (11, x1), (21, y1), (31, "0")])
    };
    let insert = |name: &str, x: &str, y: &str, rot: &str, sx: &str, sy: &str| {
        pairs(&[(0, "INSERT"), (8, "0"), (2, name), (10, x), (20, y), (30, "0"), (41, sx), (42, sy), (50, rot)])
    };
    let mut s = pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]);
    s += &pairs(&[(0, "SECTION"), (2, "BLOCKS")]);
    s += &pairs(&[(0, "BLOCK"), (2, "DOOR"), (70, "0"), (10, "0"), (20, "0"), (30, "0")]);
    s += &line("0", "0", "1000", "0");
    s += &pairs(&[(0, "ENDBLK"), (0, "BLOCK"), (2, "PAIR"), (70, "0"), (10, "0"), (20, "0"), (30, "0")]);
    s += &insert("DOOR", "0", "0", "0", "1", "1");
    s += &insert("DOOR", "2000", "0", "0", "1", "1");
    s += &pairs(&[(0, "ENDBLK"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    s += &insert("PAIR", "10000", "0", "90", "1", "1");
    s += &insert("DOOR", "0", "5000", "0", "2", "1");
    s += &line("0", "0", "0", "3000");
    s += &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    s
}

#[test]
fn dxf_blocks_come_in_as_detail_components_and_go_out_as_blocks() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    let path = scratch("blocks.dxf");
    std::fs::write(&path, blocks_dxf()).unwrap();

    // By default blocks are flattened into the drawing, as before.
    let flat = run(&mut s, "cad_import", json!({"path": path, "view": view}));
    assert_eq!(flat["primitives"], 4, "two doors of the pair, the stretched door and the loose line");
    assert!(flat["components"].is_null());
    assert!(s.project().detail_def("DOOR").is_none());

    let kept = run(&mut s, "cad_import", json!({"path": path, "view": view, "blocks": true}));
    assert_eq!(kept["primitives"], 2, "the stretched door and the loose line stay in the drawing");
    assert_eq!(kept["components"]["types"].as_array().unwrap().len(), 2);
    let elements = kept["components"]["elements"].as_array().unwrap().clone();
    assert_eq!(elements.len(), 1, "one PAIR insert");
    let pair = elements[0].as_u64().unwrap();
    let got = run(&mut s, "element_get", json!({"id": pair}));
    assert_eq!(got["derived"]["component"], "PAIR");
    assert_eq!(got["params"]["position"], json!([10.0, 0.0]));
    // Both doors, turned 90° with the pair: (10,0)-(10,1) and (10,2)-(10,3).
    let lines = lines_of(&mut s, view, pair);
    assert!(has_line(&lines, DVec2::new(10.0, 0.0), DVec2::new(10.0, 1.0)), "{lines:?}");
    assert!(has_line(&lines, DVec2::new(10.0, 2.0), DVec2::new(10.0, 3.0)), "{lines:?}");
    // Importing again makes new, unique names.
    let again = run(&mut s, "cad_import", json!({"path": path, "view": view, "blocks": true}));
    let names: Vec<String> = again["components"]["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| s.project().type_name(cc_core::ElementTypeId(t.as_u64().unwrap())).to_string())
        .collect();
    assert!(names.iter().all(|n| n.ends_with("(2)")), "{names:?}");
    let undo_to = run(&mut s, "undo", json!({}));
    assert!(undo_to.is_object());

    // DXF export: the pair is a block with the door's inserts, inserted once.
    let out = scratch("detail.dxf");
    run(&mut s, "view_export", json!({"view": view, "path": out}));
    let text = std::fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let entities = lines.iter().position(|l| *l == "ENTITIES").unwrap();
    let count =
        |from: usize, to: usize, what: &str| lines[from..to].windows(2).filter(|w| w[0] == "0" && w[1] == what).count();
    assert_eq!(count(0, entities, "BLOCK"), 2);
    assert_eq!(count(0, entities, "INSERT"), 2, "PAIR holds two DOOR inserts");
    assert_eq!(count(entities, lines.len(), "INSERT"), 1);
    assert!(lines.contains(&"PAIR") && lines.contains(&"DOOR"));
}
