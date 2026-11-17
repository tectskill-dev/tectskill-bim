//! Custom fill patterns (issue #226): `.pat` files imported into the
//! project, used by materials, filled region types and view overrides,
//! hatched in plans and sections (drafting patterns at paper size, model
//! patterns at world size) and exported, changed, deleted with a
//! replacement and undone.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::{LineStyle, Prim};
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

/// Our own patterns: 30° stripes (drafting, inches), a dashed model grid.
const PAT: &str = ";; test patterns\r\n\
    ;%UNITS=INCH\r\n\
    *STRIPES30, Thirty degree stripes\r\n\
    30, 0,0, 0,.1\r\n\
    *TILES, Square tiles\r\n\
    ;%TYPE=MODEL\r\n\
    0, 0,0, 0,12, 10,-2\r\n\
    90, 0,0, 0,12\r\n\
    *BROKEN\r\n\
    0, 0,0\r\n";

/// The hatch lines of element `id` in a view's drawing.
fn hatch_lines(s: &mut Session, view: u64, id: u64) -> Vec<(DVec2, DVec2)> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { a, b, style: LineStyle::Hatch, element: Some(e), .. } if e.0 == id => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

/// Lines at `deg` (modulo 180°).
fn at_angle(lines: &[(DVec2, DVec2)], deg: f64) -> usize {
    lines
        .iter()
        .filter(|(a, b)| {
            let d = (*b - *a).to_angle().to_degrees().rem_euclid(180.0);
            (d - deg).abs() < 0.5 && a.distance(*b) > 1e-9
        })
        .count()
}

#[test]
fn pat_files_import_and_hatch_section_cuts() {
    let mut s = Session::default();
    // Preview: what the file holds, nothing imported.
    let preview = run(&mut s, "fill_pattern_import", json!({"text": PAT, "preview": true}));
    assert_eq!(preview["patterns"].as_array().unwrap().len(), 2);
    assert_eq!(preview["errors"][0]["pattern"], "BROKEN");
    assert_eq!(preview["errors"][0]["line"], 10);
    assert!(run(&mut s, "fill_patterns_list", json!({})).as_array().unwrap().iter().all(|p| p["built_in"] == true));

    let out = run(&mut s, "fill_pattern_import", json!({"text": PAT}));
    assert_eq!(out["imported"].as_array().unwrap().len(), 2);
    assert_eq!(out["errors"].as_array().unwrap().len(), 1);
    let stripes = run(&mut s, "fill_pattern_get", json!({"pattern": "stripes30"}));
    assert_eq!(stripes["kind"], "drafting");
    assert!((stripes["lines"][0]["offset"][1].as_f64().unwrap() - 2.54).abs() < 1e-9, "inches to mm");
    let tiles = run(&mut s, "fill_pattern_get", json!({"pattern": "TILES"}));
    assert_eq!(tiles["kind"], "model");
    let id = stripes["id"].as_u64().unwrap();

    // A wall cut by a section, its materials cut with the stripes.
    let wall = run(&mut s, "wall_create", json!({"start": [0.0, 0.0], "end": [10.0, 0.0]}))["id"].as_u64().unwrap();
    let section = run(&mut s, "view_create", json!({"kind": "section", "start": [5.0, -2.0], "end": [5.0, 2.0]}))["id"]
        .as_u64()
        .unwrap();
    assert_eq!(at_angle(&hatch_lines(&mut s, section, wall), 30.0), 0);
    let materials: Vec<String> = run(&mut s, "materials_list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect();
    for m in &materials {
        run(
            &mut s,
            "material_set",
            json!({"material": m, "values": {"graphics": {"cut": {"foreground": {"Custom": id}, "background": "None"}}}}),
        );
    }
    let lines = hatch_lines(&mut s, section, wall);
    let striped = at_angle(&lines, 30.0);
    assert!(striped > 0, "{lines:?}");
    // Drafting: 2.54 mm on paper. At 1:50 the stripes are 0.127 m apart; at 1:20, 2.5 times closer.
    assert_eq!(run(&mut s, "view_get", json!({"view": section}))["scale"], 50);
    run(&mut s, "view_set", json!({"view": section, "scale": 20}));
    let fine = at_angle(&hatch_lines(&mut s, section, wall), 30.0);
    assert!(fine > 2 * striped, "{striped} {fine}");
    assert!(run(&mut s, "fill_pattern_get", json!({"pattern": id}))["used_by"].as_u64().unwrap() >= 1);

    // The model pattern keeps its world size: 12 in = 0.3048 m at any scale.
    let tiles_id = tiles["id"].as_u64().unwrap();
    for m in &materials {
        run(
            &mut s,
            "material_set",
            json!({"material": m, "values": {"graphics": {"cut": {"foreground": {"Custom": tiles_id}}}}}),
        );
    }
    let at_20 = hatch_lines(&mut s, section, wall).len();
    run(&mut s, "view_set", json!({"view": section, "scale": 50}));
    assert_eq!(hatch_lines(&mut s, section, wall).len(), at_20);

    // A pattern that does not exist is refused in model values.
    assert_eq!(
        fails(
            &mut s,
            "material_set",
            json!({"material": materials[0], "values": {"graphics": {"cut": {"foreground": {"Custom": 999_999}}}}})
        ),
        ErrorCode::Rejected
    );
}

#[test]
fn patterns_are_created_changed_exported_and_deleted_with_a_replacement() {
    let mut s = Session::default();
    let made = run(
        &mut s,
        "fill_pattern_create",
        json!({"name": "Herring", "kind": "model", "lines": [
            {"angle": 45, "offset": [0, 100], "dashes": [100, -100]},
            {"angle": 135, "origin": [70.71, 0], "offset": [0, 100], "dashes": [100, -100]}
        ]}),
    );
    let id = made["id"].as_u64().unwrap();
    assert_eq!(made["value"], json!({"Custom": id}));
    for bad in [
        json!({"name": "herring", "lines": [{"angle": 0, "offset": [0, 1]}]}),
        json!({"name": "brick", "lines": [{"angle": 0, "offset": [0, 1]}]}),
        json!({"name": "X", "lines": []}),
        json!({"name": "X", "lines": [{"angle": 0, "offset": [0, 0]}]}),
        json!({"name": "X", "scale": 0, "lines": [{"angle": 0, "offset": [0, 1]}]}),
        json!({"name": "X", "kind": "paper", "lines": [{"angle": 0, "offset": [0, 1]}]}),
    ] {
        assert_eq!(fails(&mut s, "fill_pattern_create", bad), ErrorCode::InvalidArguments);
    }
    run(&mut s, "fill_pattern_set", json!({"pattern": id, "scale": 2.0, "name": "Herringbone"}));
    let copy = run(&mut s, "fill_pattern_duplicate", json!({"pattern": "Herringbone"}));
    assert_eq!(copy["name"], "Herringbone 2");

    // A filled region type uses it by name.
    let ty = run(&mut s, "filled_region_type_create", json!({"name": "Paving", "foreground": "herringbone"}))["id"]
        .as_u64()
        .unwrap();
    let view = run(&mut s, "view_create", json!({"kind": "drafting"}))["id"].as_u64().unwrap();
    let region = run(
        &mut s,
        "filled_region_create",
        json!({"view": view, "type": ty, "curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [4, 3]}]}),
    )["id"]
        .as_u64()
        .unwrap();
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let hatched = d
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hatch, element: Some(e), .. } if e.0 == region))
        .count();
    assert!(hatched > 10, "{hatched}");

    // Export as .pat text, which reads back.
    let text =
        run(&mut s, "fill_pattern_export", json!({"patterns": ["Herringbone"]}))["text"].as_str().unwrap().to_string();
    assert!(text.contains("*Herringbone") && text.contains(";%TYPE=MODEL"), "{text}");
    // Same name again: renamed, replaced or skipped.
    let again = run(&mut s, "fill_pattern_import", json!({"text": text}));
    assert_eq!(again["imported"][0]["name"], "Herringbone 3");
    let replaced = run(&mut s, "fill_pattern_import", json!({"text": text, "on_clash": "replace"}));
    assert_eq!(replaced["replaced"][0]["id"], id);
    let skipped = run(&mut s, "fill_pattern_import", json!({"text": text, "on_clash": "skip"}));
    assert_eq!(skipped["skipped"][0], "Herringbone");
    assert_eq!(fails(&mut s, "fill_pattern_import", json!({"text": text, "names": ["Nope"]})), ErrorCode::NotFound);

    // In use: deleting needs a replacement, which takes its place in one undo step.
    assert_eq!(fails(&mut s, "fill_pattern_delete", json!({"pattern": id})), ErrorCode::Rejected);
    let out = run(&mut s, "fill_pattern_delete", json!({"pattern": id, "replace_with": "diagonal"}));
    assert_eq!(out["moved"], 1);
    let ty_now = run(&mut s, "type_get", json!({"type": ty}));
    assert!(ty_now.to_string().contains("Diagonal"), "{ty_now}");
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "fill_pattern_get", json!({"pattern": id}))["used_by"], 1);
    // Unused patterns delete without one.
    run(&mut s, "fill_pattern_delete", json!({"pattern": "Herringbone 2"}));
    let list = run(&mut s, "fill_patterns_list", json!({}));
    assert_eq!(list.as_array().unwrap().iter().filter(|p| p["built_in"] == false).count(), 2);
}
