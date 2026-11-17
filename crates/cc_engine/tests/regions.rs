//! Line styles, filled regions and masking regions (issue #150): project
//! line styles used by detail lines and region boundaries, regions sketched
//! with openings, drawn in their view over the model and under its
//! annotations, edited with the generic commands and `sketch_set`, exported
//! as fills and pattern lines, and repaired when a file breaks them.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::Prim;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn first_plan(s: &mut Session) -> u64 {
    run(s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap()
}

fn drafting(s: &mut Session) -> u64 {
    run(s, "view_create", json!({"kind": "drafting"}))["id"].as_u64().unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!({"kind": "rectangle", "corner": [x0, y0], "opposite": [x1, y1]})
}

/// Index of the first primitive of element `id` in a view's drawing.
fn first(s: &mut Session, view: u64, id: u64) -> Option<usize> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims.iter().position(|p| p.element().map(|e| e.0) == Some(id))
}

#[test]
fn line_styles_are_created_used_changed_and_deleted() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    let style = run(
        &mut s,
        "line_style_create",
        json!({"name": "Red dashed", "weight": 0.35, "color": [200, 0, 0], "pattern": "dash"}),
    );
    let id = style["id"].as_u64().unwrap();
    assert_eq!(style["pattern"], "dash");
    // Names are unique and differ from the built-in codes; weights are bounded.
    for bad in [
        json!({"name": "red DASHED"}),
        json!({"name": "thin"}),
        json!({"name": " "}),
        json!({"name": "X", "weight": 50}),
        json!({"name": "X", "pattern": "wavy"}),
    ] {
        assert_eq!(fails(&mut s, "line_style_create", bad), ErrorCode::InvalidArguments);
    }
    let list = run(&mut s, "line_styles_list", json!({}));
    assert!(list.as_array().unwrap().iter().any(|v| v["name"] == "thin" && v["built_in"] == true));

    // Detail lines take it by name; the drawing uses its pen.
    let made = run(
        &mut s,
        "detail_line_create",
        json!({"view": view, "style": "Red dashed", "curves": [{"kind": "line", "start": [0, 0], "end": [1, 0]}]}),
    );
    let line = made["created"][0].as_u64().unwrap();
    let got = run(&mut s, "element_get", json!({"id": line}));
    assert_eq!(got["params"]["style"], json!({"Project": id}));
    assert_eq!(got["derived"]["style_name"], "Red dashed");
    let pen = |s: &mut Session| {
        let d = s.drawing(cc_core::ViewId(view)).unwrap();
        d.prims
            .iter()
            .find_map(|p| match p {
                Prim::Line { element: Some(e), pen: Some(pen), .. } if e.0 == line => Some(*pen),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!((pen(&mut s).color, pen(&mut s).weight), (Some([200, 0, 0]), Some(0.35)));

    // Changing the style changes the line; undo brings it back.
    run(&mut s, "line_style_set", json!({"style": id, "color": [0, 0, 255], "name": "Blue"}));
    assert_eq!(pen(&mut s).color, Some([0, 0, 255]));
    assert_eq!(run(&mut s, "line_styles_list", json!({}))[7]["used_by"], 1);
    run(&mut s, "undo", json!({}));
    assert_eq!(pen(&mut s).color, Some([200, 0, 0]));

    // Deleting it moves its users to the replacement in the same step.
    let out = run(&mut s, "line_style_delete", json!({"style": "Red dashed", "replace_with": "wide"}));
    assert_eq!(out["moved"], 1);
    assert_eq!(run(&mut s, "element_get", json!({"id": line}))["params"]["style"], json!({"BuiltIn": "Wide"}));
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "element_get", json!({"id": line}))["params"]["style"], json!({"Project": id}));
    assert_eq!(
        fails(&mut s, "line_style_delete", json!({"style": id, "replace_with": "Red dashed"})),
        ErrorCode::InvalidArguments
    );
    run(&mut s, "line_style_delete", json!({"style": id}));
    assert_eq!(run(&mut s, "element_get", json!({"id": line}))["params"]["style"], json!({"BuiltIn": "Thin"}));
    assert!(s.project().line_styles.is_empty());
}

#[test]
fn filled_regions_have_openings_a_type_and_a_boundary_style() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    let region = run(
        &mut s,
        "filled_region_create",
        json!({"view": view, "boundary": "wide", "curves": [rect(0.0, 0.0, 4.0, 3.0), rect(1.0, 1.0, 2.0, 2.0)]}),
    )["id"]
        .as_u64()
        .unwrap();
    let got = run(&mut s, "element_get", json!({"id": region}));
    assert_eq!(got["category"], "filledregion");
    assert!((got["derived"]["area"].as_f64().unwrap() - 11.0).abs() < 1e-9);
    assert_eq!(got["derived"]["boundary_name"], "wide");
    assert_eq!(got["params"]["openings"].as_array().unwrap().len(), 1);
    // The first region made the project's filled region type.
    let ty = got["type"]["id"].as_u64().unwrap();
    assert!(s.project().element_type(cc_core::ElementTypeId(ty)).is_some());

    // Drawn as one fill (a keyhole ring), pattern lines and eight boundary edges.
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let mine = |p: &&Prim| p.element().map(|e| e.0) == Some(region);
    let fills: Vec<_> = d.prims.iter().filter(mine).filter(|p| matches!(p, Prim::Fill { .. })).collect();
    assert_eq!(fills.len(), 1);
    assert_eq!(d.hit_test(glam::DVec2::new(3.0, 2.5), 1e-3).map(|e| e.0), Some(region));
    assert_ne!(d.hit_test(glam::DVec2::new(1.5, 1.5), 1e-3).map(|e| e.0), Some(region), "not in the opening");
    let lines = d.prims.iter().filter(mine).filter(|p| matches!(p, Prim::Line { .. })).count();
    assert!(lines > 8);

    // A solid red type: the fill takes its color.
    let red = run(
        &mut s,
        "filled_region_type_create",
        json!({"name": "Red", "foreground": "solid", "foreground_color": [255, 0, 0]}),
    )["id"]
        .as_u64()
        .unwrap();
    run(&mut s, "element_set", json!({"id": region, "type": red, "params": {"boundary": "none"}}));
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let prims: Vec<_> = d.prims.iter().filter(mine).collect();
    assert_eq!(prims.len(), 1, "a solid fill and no boundary");
    assert!(matches!(prims[0], Prim::Fill { color: [255, 0, 0, 255], .. }));
    assert_eq!(
        fails(&mut s, "filled_region_type_create", json!({"foreground": "zigzag"})),
        ErrorCode::InvalidArguments
    );

    // sketch_get gives back the curves; sketch_set redraws it without the opening.
    let curves = run(&mut s, "sketch_get", json!({"id": region}))["curves"].clone();
    assert_eq!(curves.as_array().unwrap().len(), 8);
    run(&mut s, "sketch_set", json!({"id": region, "curves": [rect(0.0, 0.0, 2.0, 2.0)]}));
    let got = run(&mut s, "element_get", json!({"id": region}));
    assert!((got["derived"]["area"].as_f64().unwrap() - 4.0).abs() < 1e-9);
    // Open loops, another view, or an opening outside the outline are refused.
    let open = json!([{"kind": "line", "start": [0, 0], "end": [1, 0]}]);
    assert_eq!(
        fails(&mut s, "filled_region_create", json!({"view": view, "curves": open})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "element_set", json!({"id": region, "params": {"view": 1}})), ErrorCode::InvalidArguments);
    let three_d = run(&mut s, "views_list", json!({"category": "three_d"}))[0]["id"].clone();
    assert_eq!(
        fails(&mut s, "filled_region_create", json!({"view": three_d, "curves": [rect(0.0, 0.0, 1.0, 1.0)]})),
        ErrorCode::InvalidArguments
    );

    // Moved with the generic commands, together with its sketch.
    run(&mut s, "elements_move", json!({"ids": [region], "delta": [10, 0]}));
    let curves = run(&mut s, "sketch_get", json!({"id": region}));
    assert_eq!(curves["drawn"], true);
    assert_eq!(curves["curves"][0]["start"], json!([10.0, 0.0]));

    // Deleting the view deletes the region.
    run(&mut s, "view_delete", json!({"view": view}));
    assert!(s.project().element(cc_core::ElementId(region)).is_none());
}

#[test]
fn masking_regions_hide_the_model_and_stay_under_annotations() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}))["id"].as_u64().unwrap();
    let mask = run(&mut s, "masking_region_create", json!({"view": plan, "curves": [rect(1.0, -1.0, 3.0, 1.0)]}))["id"]
        .as_u64()
        .unwrap();
    let line = run(
        &mut s,
        "detail_line_create",
        json!({"view": plan, "curves": [{"kind": "line", "start": [0, 0.5], "end": [6, 0.5]}]}),
    )["created"][0]
        .as_u64()
        .unwrap();
    let text =
        run(&mut s, "text_create", json!({"view": plan, "position": [2, 0], "text": "Note"}))["id"].as_u64().unwrap();
    let (w, m, l, t) = (
        first(&mut s, plan, wall).unwrap(),
        first(&mut s, plan, mask).unwrap(),
        first(&mut s, plan, line).unwrap(),
        first(&mut s, plan, text).unwrap(),
    );
    assert!(w < m && m < l && m < t, "over the model, under detail lines and texts: {w} {m} {l} {t}");
    let d = s.drawing(cc_core::ViewId(plan)).unwrap();
    let fill = d.prims.iter().find(|p| p.element().map(|e| e.0) == Some(mask)).unwrap();
    assert!(matches!(fill, Prim::Fill { color: [255, 255, 255, 255], .. }));
    // No boundary by default; one when asked.
    assert!(d.prims.iter().all(|p| !(matches!(p, Prim::Line { .. }) && p.element().map(|e| e.0) == Some(mask))));
    run(&mut s, "element_set", json!({"id": mask, "params": {"boundary": "medium"}}));
    let d = s.drawing(cc_core::ViewId(plan)).unwrap();
    assert_eq!(d.prims.iter().filter(|p| p.element().map(|e| e.0) == Some(mask)).count(), 5);
    // In a drafting view too, and in elevations and sections.
    let elevation = run(&mut s, "views_list", json!({"category": "elevation"}))[0]["id"].as_u64().unwrap();
    let on_elevation =
        run(&mut s, "masking_region_create", json!({"view": elevation, "curves": [rect(0.0, 0.0, 1.0, 1.0)]}))["id"]
            .as_u64()
            .unwrap();
    assert!(first(&mut s, elevation, on_elevation).is_some());
    assert!(first(&mut s, plan, on_elevation).is_none(), "only in its own view");
}

#[test]
fn exports_write_region_fills_and_dxf_puts_them_on_the_style_layer() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    run(&mut s, "line_style_create", json!({"name": "Edge", "weight": 0.5, "color": [0, 128, 0]}));
    run(
        &mut s,
        "filled_region_create",
        json!({"view": view, "boundary": "Edge", "curves": [rect(0.0, 0.0, 2.0, 1.0)]}),
    );
    run(&mut s, "masking_region_create", json!({"view": view, "curves": [rect(3.0, 0.0, 4.0, 1.0)]}));
    let dir = std::env::temp_dir().join(format!("ccad-regions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dxf = dir.join("regions.dxf");
    run(&mut s, "view_export", json!({"view": view, "path": dxf}));
    let text = std::fs::read_to_string(&dxf).unwrap();
    assert!(text.contains("\nEDGE\n"), "a layer named after the boundary style");
    assert!(text.contains("\nSOLID\n"), "fills as solids");
    assert!(text.contains("\nMASKINGREGION\n"), "a region without a boundary on its category's layer");
    let svg = dir.join("regions.svg");
    run(&mut s, "view_export", json!({"view": view, "path": svg}));
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains("<polygon") && text.contains("rgb(255,255,255)"));
    let ifc = dir.join("regions.ifc");
    run(&mut s, "ifc_export", json!({"path": ifc}));
    let text = std::fs::read_to_string(&ifc).unwrap();
    assert!(!text.contains("IFCANNOTATIONFILLAREA") && !text.contains("Filled region"));
}

#[test]
fn files_with_bad_regions_and_styles_are_repaired() {
    let mut s = Session::default();
    let view = drafting(&mut s);
    run(&mut s, "line_style_create", json!({"name": "Mine"}));
    let a = run(
        &mut s,
        "filled_region_create",
        json!({"view": view, "boundary": "Mine", "curves": [rect(0.0, 0.0, 1.0, 1.0)]}),
    )["id"]
        .as_u64()
        .unwrap();
    let b = run(&mut s, "masking_region_create", json!({"view": view, "curves": [rect(2.0, 0.0, 3.0, 1.0)]}))["id"]
        .as_u64()
        .unwrap();
    let mut project = s.project().clone();
    project.line_styles.clear();
    if let Some(cc_core::ElementKind::MaskingRegion(r)) =
        project.elements.get_mut(&cc_core::ElementId(b)).map(|e| &mut e.kind)
    {
        r.outline[0].x = f64::INFINITY;
    }
    let text = cc_core::to_ron(&project).unwrap();
    let (loaded, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert!(repairs >= 2);
    assert!(loaded.element(cc_core::ElementId(b)).is_none());
    let Some(cc_core::ElementKind::FilledRegion(r)) = loaded.element(cc_core::ElementId(a)).map(|e| &e.kind) else {
        panic!("the region stays")
    };
    assert_eq!(r.boundary, Some(cc_core::LineStyleRef::default()), "the style that is gone falls back to thin");
}
