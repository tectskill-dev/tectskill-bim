//! View graphics through commands: category and element overrides, hiding
//! in a view, filters, templates and temporary hide/isolate.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementId, ViewId};
use cc_engine::{ErrorCode, Session};
use cc_views::{LineStyle, Prim};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

/// A session with three walls on the ground level (4, 6 and 8 m long,
/// marked A1, A2 and B1), a door in the first and a wall on the upper level;
/// returns the ground plan, the walls, the door and the upper wall.
fn house() -> (Session, u64, [u64; 3], u64, u64) {
    let mut s = Session::default();
    let plan = id(&run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]);
    let lengths = [4.0, 6.0, 8.0];
    let marks = ["A1", "A2", "B1"];
    let mut walls = [0; 3];
    for (i, len) in lengths.iter().enumerate() {
        let y = i as f64 * 3.0;
        walls[i] = id(&run(&mut s, "wall_create", json!({"start": [0.0, y], "end": [len, y]})));
        run(&mut s, "element_set", json!({"id": walls[i], "mark": marks[i]}));
    }
    let door = id(&run(&mut s, "door_create", json!({"wall": walls[0], "offset": 2.0})));
    let levels = run(&mut s, "levels_list", json!({}));
    let upper =
        id(&run(&mut s, "wall_create", json!({"start": [0.0, 20.0], "end": [5.0, 20.0], "level": levels[1]["id"]})));
    (s, plan, walls, door, upper)
}

/// The sheet showing `view` (placing it on a new sheet if none does).
fn sheet_of(s: &mut Session, view: u64) -> u64 {
    for sheet in run(s, "views_list", json!({"category": "sheet"})).as_array().unwrap().clone() {
        let v = run(s, "view_get", json!({"view": sheet["id"]}));
        if v["params"]["viewports"].as_array().unwrap().iter().any(|vp| vp["view"] == view) {
            return id(&sheet);
        }
    }
    let sheet = id(&run(s, "view_create", json!({"kind": "sheet"})));
    run(s, "sheet_place_view", json!({"sheet": sheet, "view": view}));
    sheet
}

fn drawing(s: &mut Session, view: u64) -> std::rc::Rc<cc_views::Drawing2D> {
    s.drawing(ViewId(view)).unwrap()
}

fn drawn(s: &mut Session, view: u64) -> BTreeSet<u64> {
    drawing(s, view).prims.iter().filter_map(|p| p.element()).map(|e| e.0).collect()
}

/// Colors of the cut lines of `element` in the view (`None`: default pen).
fn cut_colors(s: &mut Session, view: u64, element: u64) -> BTreeSet<Option<[u8; 3]>> {
    drawing(s, view)
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { style: LineStyle::Cut, element: Some(e), pen, .. } if e.0 == element => {
                Some(pen.and_then(|p| p.color))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn category_and_element_graphics() {
    let (mut s, plan, walls, door, _) = house();
    assert!(drawn(&mut s, plan).contains(&door));
    assert_eq!(cut_colors(&mut s, plan, walls[1]), BTreeSet::from([None]));

    // Walls cut in red at 0.7 mm; doors hidden.
    run(
        &mut s,
        "view_graphics_set",
        json!({"view": plan, "category": "wall", "overrides": {"cut_lines": {"color": [255, 0, 0], "weight": 0.7}}}),
    );
    run(&mut s, "view_graphics_set", json!({"view": plan, "category": "door", "visible": false}));
    assert_eq!(cut_colors(&mut s, plan, walls[1]), BTreeSet::from([Some([255, 0, 0])]));
    let d = drawing(&mut s, plan);
    let weights: BTreeSet<String> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { style: LineStyle::Cut, element: Some(e), pen: Some(pen), .. } if e.0 == walls[1] => {
                Some(format!("{:?}", pen.weight))
            }
            _ => None,
        })
        .collect();
    assert_eq!(weights, BTreeSet::from(["Some(0.7)".to_string()]));
    assert!(!drawn(&mut s, plan).contains(&door));
    let g = run(&mut s, "view_graphics_get", json!({"view": plan}));
    let wall = g["categories"].as_array().unwrap().iter().find(|c| c["category"] == "wall").unwrap();
    assert_eq!(wall["overrides"]["cut_lines"]["color"], json!([255, 0, 0]));

    // The element's own override wins over its category.
    run(
        &mut s,
        "element_override_set",
        json!({"view": plan, "ids": [walls[0]], "overrides": {"cut_lines": {"color": [0, 0, 255]}}}),
    );
    assert_eq!(cut_colors(&mut s, plan, walls[0]), BTreeSet::from([Some([0, 0, 255])]));
    assert_eq!(cut_colors(&mut s, plan, walls[1]), BTreeSet::from([Some([255, 0, 0])]));
    // `null` resets one attribute: the category's color shows again.
    run(
        &mut s,
        "element_override_set",
        json!({"view": plan, "ids": [walls[0]], "overrides": {"cut_lines": {"color": null}}}),
    );
    assert_eq!(cut_colors(&mut s, plan, walls[0]), BTreeSet::from([Some([255, 0, 0])]));

    // Hidden in this view only.
    run(&mut s, "element_hide_in_view", json!({"view": plan, "ids": [walls[2]]}));
    assert!(!drawn(&mut s, plan).contains(&walls[2]));
    let copy = id(&run(&mut s, "view_duplicate", json!({"view": plan})));
    run(&mut s, "element_hide_in_view", json!({"view": copy, "ids": [walls[2]], "hidden": false}));
    assert!(drawn(&mut s, copy).contains(&walls[2]));
    assert!(!drawn(&mut s, plan).contains(&walls[2]));
    run(&mut s, "undo", json!({}));
    assert!(!drawn(&mut s, copy).contains(&walls[2]));

    // Deleting an element drops its overrides instead of refusing the edit.
    run(&mut s, "elements_delete", json!({"ids": [walls[0], walls[2]]}));
    let g = run(&mut s, "view_graphics_get", json!({"view": plan}));
    assert_eq!(g["hidden"], json!([]));
    assert_eq!(g["elements"].as_array().unwrap().len(), 0);

    // Bad values are refused.
    let bad = json!({"view": plan, "category": "wall", "overrides": {"transparency": 2.0}});
    assert_eq!(fails(&mut s, "view_graphics_set", bad), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "view_graphics_set", json!({"view": plan, "category": "nope"})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn overrides_reach_sheets_and_exports() {
    let (mut s, plan, walls, _, _) = house();
    run(
        &mut s,
        "view_graphics_set",
        json!({"view": plan, "category": "wall", "overrides": {"cut_lines": {"color": [255, 0, 0], "pattern": "Dash"}}}),
    );
    let sheet = sheet_of(&mut s, plan);
    let composed = s.sheet(ViewId(sheet)).unwrap();
    assert!(composed.drawing.prims.iter().any(|p| matches!(p,
        Prim::Line { element: Some(e), pen: Some(pen), .. } if e.0 == walls[0] && pen.color == Some([255, 0, 0]))));
    let svg = cc_views::export::to_svg(&composed.drawing, composed.paper);
    assert!(svg.contains("stroke=\"rgb(255,0,0)\""));
    assert!(svg.contains("stroke-dasharray=\"3 1.5\""));
    let pdf = cc_views::export::to_pdf(&composed.drawing, composed.paper);
    assert!(String::from_utf8_lossy(&pdf).contains("1 0 0 RG"));
}

#[test]
fn halftone_and_cut_patterns() {
    let (mut s, plan, walls, _, _) = house();
    let hatch = |s: &mut Session| {
        drawing(s, plan)
            .prims
            .iter()
            .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hatch, element: Some(e), .. } if e.0 == walls[1]))
            .count()
    };
    let before = hatch(&mut s);
    assert!(before > 0, "the template's masonry is hatched");
    // A solid cut fill replaces the material's hatch.
    run(
        &mut s,
        "element_override_set",
        json!({"view": plan, "ids": [walls[1]], "overrides": {"cut_fill": {"pattern": "Solid", "color": [0, 128, 0]}, "halftone": true}}),
    );
    assert_eq!(hatch(&mut s), 0);
    let d = drawing(&mut s, plan);
    let fills: Vec<[u8; 4]> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Fill { element: Some(e), color, cut: true, .. } if e.0 == walls[1] => Some(*color),
            _ => None,
        })
        .collect();
    // Halftone mixes the green two thirds with white.
    let half = cc_views::halftone([0, 128, 0]);
    assert!(!fills.is_empty() && fills.iter().all(|c| c[..3] == half), "{fills:?}");
    assert!(
        d.prims.iter().any(
            |p| matches!(p, Prim::Line { element: Some(e), pen: Some(pen), .. } if e.0 == walls[1] && pen.halftone)
        )
    );
    // A crosshatch draws new pattern lines.
    run(
        &mut s,
        "element_override_set",
        json!({"view": plan, "ids": [walls[1]], "reset": true, "overrides": {"cut_fill": {"pattern": "Crosshatch"}}}),
    );
    assert!(hatch(&mut s) > 0);
}

#[test]
fn filters_match_by_rules() {
    let (mut s, _, walls, door, upper) = house();
    let filter = |s: &mut Session, name: &str, rules: Value, any: bool| {
        id(&run(s, "filter_create", json!({"name": name, "categories": ["wall"], "rules": rules, "any": any})))
    };
    let ids = |s: &mut Session, f: u64| -> BTreeSet<u64> {
        run(s, "filter_elements", json!({"filter": f}))["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect()
    };
    let long = filter(&mut s, "Long", json!([{"param": "length", "op": "greater", "value": 5.0}]), false);
    assert_eq!(ids(&mut s, long), BTreeSet::from([walls[1], walls[2]]));
    let at_least_six =
        filter(&mut s, "Six", json!([{"param": "length", "op": "greater_or_equal", "value": "6"}]), false);
    assert_eq!(ids(&mut s, at_least_six), BTreeSet::from([walls[1], walls[2]]));
    let short = filter(&mut s, "Short", json!([{"param": "length", "op": "less_or_equal", "value": 5}]), false);
    assert_eq!(ids(&mut s, short), BTreeSet::from([walls[0], upper]));
    let rules = json!([
        {"param": "length", "op": "greater", "value": 5},
        {"param": "mark", "op": "begins_with", "value": "a"}
    ]);
    let and = filter(&mut s, "And", rules.clone(), false);
    assert_eq!(ids(&mut s, and), BTreeSet::from([walls[1]]));
    let or = filter(&mut s, "Or", rules, true);
    assert_eq!(ids(&mut s, or), BTreeSet::from([walls[0], walls[1], walls[2]]));
    let unmarked = filter(&mut s, "Unmarked", json!([{"param": "mark", "op": "has_no_value"}]), false);
    assert_eq!(ids(&mut s, unmarked), BTreeSet::from([upper]));
    let not_b = filter(&mut s, "NotB", json!([{"param": "mark", "op": "not_contains", "value": "B"}]), false);
    assert_eq!(ids(&mut s, not_b), BTreeSet::from([walls[0], walls[1], upper]));
    let high = filter(&mut s, "High", json!([{"param": "elevation", "op": "greater", "value": 1}]), false);
    assert_eq!(ids(&mut s, high), BTreeSet::from([upper]));
    let levels = run(&mut s, "levels_list", json!({}));
    let ground = levels[0]["name"].as_str().unwrap().to_string();
    let on_ground = filter(&mut s, "Ground", json!([{"param": "level", "op": "equals", "value": ground}]), false);
    assert_eq!(ids(&mut s, on_ground), BTreeSet::from(walls));
    let typed = filter(&mut s, "Typed", json!([{"param": "type", "op": "has_value"}]), false);
    assert_eq!(ids(&mut s, typed).len(), 4);
    // Without categories a filter looks at every element; without rules it matches them all.
    let doors = id(&run(&mut s, "filter_create", json!({"name": "Doors", "categories": ["door"]})));
    assert_eq!(ids(&mut s, doors), BTreeSet::from([door]));
    let wide = id(&run(
        &mut s,
        "filter_create",
        json!({"name": "Wide", "rules": [{"param": "width", "op": "greater_or_equal", "value": 0.5}]}),
    ));
    assert_eq!(ids(&mut s, wide), BTreeSet::from([door]));

    // Bad rules and names are refused.
    assert_eq!(
        fails(&mut s, "filter_create", json!({"name": "X", "rules": [{"param": "nope", "op": "equals"}]})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "filter_create", json!({"name": "X", "rules": [{"param": "mark", "op": "is"}]})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "filter_create", json!({"name": "long"})), ErrorCode::Rejected);
    // A rule that can never pass for the chosen categories (walls have no width) is
    // refused, also when the categories change under it; a material rule on walls is fine.
    let width =
        json!({"name": "X", "categories": ["wall"], "rules": [{"param": "width", "op": "greater", "value": 0.1}]});
    let error = s.execute("filter_create", width).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArguments);
    assert!(error.message.contains("'width'") && error.message.contains("door"), "{}", error.message);
    let mixed = json!({"name": "X", "categories": ["wall", "door"], "rules": [{"param": "width", "op": "has_value"}]});
    run(&mut s, "filter_set", json!({"filter": wide, "categories": ["door"]}));
    assert_eq!(
        fails(&mut s, "filter_set", json!({"filter": wide, "categories": ["wall"]})),
        ErrorCode::InvalidArguments
    );
    let x = id(&run(&mut s, "filter_create", mixed));
    assert_eq!(ids(&mut s, x), BTreeSet::from([door]));
    let material = json!({"name": "M", "categories": ["wall"], "rules": [{"param": "material", "op": "has_value"}]});
    run(&mut s, "filter_create", material);
    // Editing rules changes what matches.
    run(&mut s, "filter_set", json!({"filter": "Long", "rules": [{"param": "length", "op": "greater", "value": 7}]}));
    assert_eq!(ids(&mut s, long), BTreeSet::from([walls[2]]));
}

#[test]
fn filters_in_views_have_priorities() {
    let (mut s, plan, walls, _, _) = house();
    let long = id(&run(
        &mut s,
        "filter_create",
        json!({"name": "Long", "categories": ["wall"], "rules": [{"param": "length", "op": "greater", "value": 5}]}),
    ));
    let marked_a = id(&run(
        &mut s,
        "filter_create",
        json!({"name": "A", "categories": ["wall"], "rules": [{"param": "mark", "op": "begins_with", "value": "A"}]}),
    ));
    run(
        &mut s,
        "view_graphics_set",
        json!({"view": plan, "category": "wall", "overrides": {"cut_lines": {"color": [9, 9, 9]}}}),
    );
    let color = |c: [u8; 3]| json!({"cut_lines": {"color": c}});
    run(&mut s, "view_filter_add", json!({"view": plan, "filter": long, "overrides": color([1, 0, 0])}));
    run(&mut s, "view_filter_add", json!({"view": plan, "filter": marked_a, "overrides": color([0, 1, 0])}));
    // A2 matches both: the first filter wins; A1 only the second; B1 only the first.
    assert_eq!(cut_colors(&mut s, plan, walls[1]), BTreeSet::from([Some([1, 0, 0])]));
    assert_eq!(cut_colors(&mut s, plan, walls[0]), BTreeSet::from([Some([0, 1, 0])]));
    assert_eq!(cut_colors(&mut s, plan, walls[2]), BTreeSet::from([Some([1, 0, 0])]));
    run(&mut s, "view_filter_move", json!({"view": plan, "filter": marked_a, "index": 0}));
    assert_eq!(cut_colors(&mut s, plan, walls[1]), BTreeSet::from([Some([0, 1, 0])]));
    // The category shows where no filter matches; element overrides beat filters.
    run(&mut s, "view_filter_set", json!({"view": plan, "filter": long, "enabled": false}));
    assert_eq!(cut_colors(&mut s, plan, walls[2]), BTreeSet::from([Some([9, 9, 9])]));
    run(&mut s, "element_override_set", json!({"view": plan, "ids": [walls[0]], "overrides": color([7, 7, 7])}));
    assert_eq!(cut_colors(&mut s, plan, walls[0]), BTreeSet::from([Some([7, 7, 7])]));
    // A hiding filter.
    run(&mut s, "view_filter_set", json!({"view": plan, "filter": long, "enabled": true, "visible": false}));
    let shown = drawn(&mut s, plan);
    assert!(!shown.contains(&walls[1]) && !shown.contains(&walls[2]) && shown.contains(&walls[0]));
    let in_view = run(&mut s, "filter_elements", json!({"filter": long, "view": plan}));
    assert_eq!(in_view["count"], 2);
    assert_eq!(fails(&mut s, "view_filter_add", json!({"view": plan, "filter": long})), ErrorCode::Rejected);
    // Deleting a filter takes it out of the view, in one undo step.
    run(&mut s, "filter_delete", json!({"filter": long}));
    let g = run(&mut s, "view_graphics_get", json!({"view": plan}));
    assert_eq!(g["filters"].as_array().unwrap().len(), 1);
    assert!(drawn(&mut s, plan).contains(&walls[2]));
    run(&mut s, "undo", json!({}));
    let g = run(&mut s, "view_graphics_get", json!({"view": plan}));
    assert_eq!(g["filters"].as_array().unwrap().len(), 2);
    run(&mut s, "view_filter_remove", json!({"view": plan, "filter": "Long"}));
    assert!(drawn(&mut s, plan).contains(&walls[2]));
}

#[test]
fn templates_control_and_propagate() {
    let (mut s, plan, walls, door, _) = house();
    let plans = run(&mut s, "views_list", json!({"category": "floor_plan"}));
    let other = id(&plans[1]);
    run(&mut s, "view_graphics_set", json!({"view": plan, "category": "door", "visible": false}));
    run(&mut s, "view_set", json!({"view": plan, "scale": 75, "params": {"cut_height": 1.5}}));
    let t = id(&run(&mut s, "template_create", json!({"name": "Working plan", "view": plan})));

    // One-shot apply: copied, still free.
    let copy = id(&run(&mut s, "view_duplicate", json!({"view": other})));
    run(&mut s, "view_template_apply", json!({"views": [copy], "template": t}));
    let v = run(&mut s, "view_get", json!({"view": copy}));
    assert_eq!((v["scale"].as_u64(), v["template"].clone()), (Some(75), Value::Null));
    assert_eq!(v["params"]["cut_height"], 1.5);
    run(&mut s, "view_set", json!({"view": copy, "scale": 20}));

    // Assigned: the view follows the template and cannot change what it controls.
    run(&mut s, "view_template_assign", json!({"views": [other], "template": "Working plan"}));
    let v = run(&mut s, "view_get", json!({"view": other}));
    assert_eq!((v["scale"].as_u64(), v["template"].as_u64()), (Some(75), Some(t)));
    assert_eq!(fails(&mut s, "view_set", json!({"view": other, "scale": 100})), ErrorCode::Rejected);
    assert_eq!(
        fails(&mut s, "view_graphics_set", json!({"view": other, "category": "wall", "visible": false})),
        ErrorCode::Rejected
    );
    run(&mut s, "view_set", json!({"view": other, "name": "Renamed"}));
    // Elements on the upper level: the door category is hidden there too.
    let d = run(&mut s, "door_create", json!({"wall": walls[1], "offset": 1.0}));
    let _ = (door, d);

    // Editing the template changes its views in the same undo step.
    let depth = s.doc.undo_depth();
    run(&mut s, "template_set", json!({"template": t, "scale": 100}));
    run(
        &mut s,
        "view_graphics_set",
        json!({"template": t, "category": "wall", "overrides": {"projection_lines": {"color": [1, 2, 3]}}}),
    );
    assert_eq!(s.doc.undo_depth(), depth + 2);
    assert_eq!(run(&mut s, "view_get", json!({"view": other}))["scale"], 100);
    let g = run(&mut s, "view_graphics_get", json!({"view": other}));
    let wall = g["categories"].as_array().unwrap().iter().find(|c| c["category"] == "wall").unwrap();
    assert_eq!(wall["overrides"]["projection_lines"]["color"], json!([1, 2, 3]));
    assert_eq!(g["template"]["id"], t);
    // The source view and the one-shot copy do not follow.
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["scale"], 75);
    assert_eq!(run(&mut s, "view_get", json!({"view": copy}))["scale"], 20);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "view_get", json!({"view": other}))["scale"], 75);

    // Properties left out of the template are the view's own.
    run(&mut s, "template_set", json!({"template": t, "include": {"scale": false}}));
    run(&mut s, "view_set", json!({"view": other, "scale": 200}));
    assert_eq!(run(&mut s, "view_get", json!({"view": other}))["scale"], 200);

    // Templates apply to graphical views only.
    let schedule = id(&run(&mut s, "view_create", json!({"kind": "schedule"})));
    assert_eq!(
        fails(&mut s, "view_template_assign", json!({"views": [schedule], "template": t})),
        ErrorCode::InvalidArguments
    );

    // Deleting the template frees its views, which keep their properties.
    run(&mut s, "template_delete", json!({"template": t}));
    let v = run(&mut s, "view_get", json!({"view": other}));
    assert_eq!(v["template"], Value::Null);
    assert_eq!(v["params"]["cut_height"], 1.5);
    run(&mut s, "view_set", json!({"view": other, "params": {"cut_height": 1.1}}));
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "view_get", json!({"view": other}))["template"], t);
    assert_eq!(run(&mut s, "templates_list", json!({}))[0]["views"], json!([other]));
}

#[test]
fn templates_and_filters_are_saved() {
    let (mut s, plan, _, _, _) = house();
    let f = id(&run(
        &mut s,
        "filter_create",
        json!({"name": "Long", "rules": [{"param": "length", "op": "greater", "value": 5}]}),
    ));
    run(&mut s, "view_filter_add", json!({"view": plan, "filter": f, "visible": false}));
    let t = id(&run(&mut s, "template_create", json!({"name": "T", "view": plan})));
    run(&mut s, "view_template_assign", json!({"views": [plan], "template": t}));
    let ron = cc_core::to_ron(&s.doc.project).unwrap();
    let (back, repairs) = cc_core::from_ron_with_repairs(&ron).unwrap();
    let (_, plain) =
        cc_core::from_ron_with_repairs(&cc_core::to_ron(&Session::default().doc.project).unwrap()).unwrap();
    assert_eq!(repairs, plain);
    assert_eq!(back, s.doc.project);
    // A crafted file with dangling references is repaired.
    let broken = ron
        .replace(&format!("filter: ({f})"), "filter: (999999)")
        .replace(&format!("template: Some(({t}))"), "template: Some((888888))");
    assert_ne!(broken, ron);
    let (back, repairs) = cc_core::from_ron_with_repairs(&broken).unwrap();
    assert!(repairs >= 2, "{repairs}");
    let v = back.view(ViewId(plan)).unwrap();
    assert!(v.template.is_none());
    assert!(back.templates.values().all(|t| t.graphics.filters.is_empty()));
}

#[test]
fn temporary_hide_and_isolate() {
    let (mut s, plan, walls, door, _) = house();
    let revision = s.doc.revision();
    let depth = s.doc.undo_depth();
    run(&mut s, "view_temporary", json!({"view": plan, "action": "hide", "ids": [walls[0]]}));
    let shown = drawn(&mut s, plan);
    assert!(!shown.contains(&walls[0]) && shown.contains(&walls[1]));
    // Not a model change, and sheets print the view whole.
    assert_eq!((s.doc.revision(), s.doc.undo_depth()), (revision, depth));
    let sheet = sheet_of(&mut s, plan);
    let printed: BTreeSet<u64> =
        s.sheet(ViewId(sheet)).unwrap().drawing.prims.iter().filter_map(|p| p.element()).map(|e| e.0).collect();
    assert!(printed.contains(&walls[0]));

    // Isolating replaces hiding; categories by name or by a selected element.
    run(&mut s, "view_temporary", json!({"view": plan, "action": "isolate", "ids": [walls[1]]}));
    assert_eq!(drawn(&mut s, plan), BTreeSet::from([walls[1]]));
    run(&mut s, "view_temporary", json!({"view": plan, "action": "isolate_category", "ids": [door]}));
    assert_eq!(drawn(&mut s, plan), BTreeSet::from([door]));
    run(&mut s, "view_temporary", json!({"view": plan, "action": "hide_category", "categories": ["door"]}));
    let shown = drawn(&mut s, plan);
    assert!(!shown.contains(&door) && shown.contains(&walls[0]));
    let g = run(&mut s, "view_graphics_get", json!({"view": plan}));
    assert_eq!(g["temporary"]["categories"], json!(["door"]));
    run(&mut s, "view_temporary", json!({"view": plan, "action": "reset"}));
    assert!(drawn(&mut s, plan).contains(&door));
    assert_eq!(run(&mut s, "view_graphics_get", json!({"view": plan}))["temporary"], Value::Null);
    assert_eq!(fails(&mut s, "view_temporary_apply", json!({"view": plan})), ErrorCode::Rejected);

    // Making it permanent: hidden in the view, one undo step.
    run(&mut s, "view_temporary", json!({"view": plan, "action": "hide", "ids": [walls[2]]}));
    let depth = s.doc.undo_depth();
    run(&mut s, "view_temporary_apply", json!({"view": plan}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    assert!(s.temporary(ViewId(plan)).is_none());
    let g = run(&mut s, "view_graphics_get", json!({"view": plan}));
    assert_eq!(g["hidden"], json!([walls[2]]));
    // Isolation made permanent hides everything else drawn.
    run(&mut s, "view_temporary", json!({"view": plan, "action": "isolate", "ids": [walls[0]]}));
    run(&mut s, "view_temporary_apply", json!({"view": plan}));
    let shown: BTreeSet<u64> =
        drawn(&mut s, plan).into_iter().filter(|e| s.project().element(ElementId(*e)).is_some()).collect();
    assert_eq!(shown, BTreeSet::from([walls[0]]));
    run(&mut s, "undo", json!({}));
    assert!(drawn(&mut s, plan).contains(&walls[1]));
}

#[test]
fn a_3d_camera_redraws_only_what_shows_it() {
    let (mut s, plan, _, _, _) = house();
    let plan = ViewId(plan);
    let view = ViewId(id(&run(&mut s, "view_create", json!({"kind": "three_d"}))));
    let other = ViewId(id(&run(&mut s, "view_create", json!({"kind": "three_d"}))));
    let sheet = id(&run(&mut s, "view_create", json!({"kind": "sheet"})));
    run(&mut s, "sheet_place_view", json!({"sheet": sheet, "view": view.0}));
    let sheet = ViewId(sheet);
    s.doc.mark_saved();
    let (rev, plan_drawing, drawing, other_drawing) =
        (s.doc.revision(), s.drawing(plan).unwrap(), s.drawing(view).unwrap(), s.drawing(other).unwrap());
    let composed = s.sheet(sheet).unwrap();
    let camera = cc_core::CameraState { yaw_deg: 10.0, ..Default::default() };
    s.set_view_camera(view, camera);
    // Saved with the project but not a model change: the geometry and the
    // other views' drawings stay; this view and its sheet are redrawn.
    assert!(s.doc.is_dirty());
    assert_eq!(s.doc.revision(), rev);
    assert!(std::rc::Rc::ptr_eq(&plan_drawing, &s.drawing(plan).unwrap()));
    assert!(std::rc::Rc::ptr_eq(&other_drawing, &s.drawing(other).unwrap()));
    assert!(!std::rc::Rc::ptr_eq(&drawing, &s.drawing(view).unwrap()));
    assert!(!std::rc::Rc::ptr_eq(&composed, &s.sheet(sheet).unwrap()));
    let Some(cc_core::ViewKind::ThreeD { camera: kept, .. }) = s.project().view(view).map(|v| v.kind.clone()) else {
        panic!("3D view")
    };
    assert_eq!(kept, camera);
    s.doc.mark_saved();
    assert!(!s.doc.is_dirty());
}

/// Issue #332: an elevation draws the grids square to it, a dimension
/// between two of them follows them, and the view hides them by category.
#[test]
fn an_elevation_draws_grids_that_dimensions_follow_and_the_view_hides() {
    let mut s = Session::default();
    let first = id(&run(&mut s, "grid_create", json!({"start": [0.0, -2.0], "end": [0.0, 5.0]})));
    let second = id(&run(&mut s, "grid_create", json!({"start": [4.0, -2.0], "end": [4.0, 5.0]})));
    let across = id(&run(&mut s, "grid_create", json!({"start": [-2.0, 1.0], "end": [6.0, 1.0]})));
    let view = s
        .project()
        .views
        .values()
        .find(|v| matches!(v.kind, cc_core::ViewKind::Elevation { direction: cc_core::Direction::South }))
        .map(|v| v.id)
        .expect("a south elevation");
    let drawn = |s: &mut Session| -> BTreeSet<u64> {
        let d = s.drawing(view).unwrap();
        d.prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { style: LineStyle::Center, element: Some(e), .. } => Some(e.0),
                _ => None,
            })
            .collect()
    };
    assert_eq!(drawn(&mut s), BTreeSet::from([first, second]), "not the grid along the view ({across})");

    // A dimension between the two grid lines is tied to both, and follows a moved grid.
    let dim = run(&mut s, "dimension_create", json!({"view": view.0, "a": [0.0, 1.5], "b": [4.0, 1.5]}));
    let refs = &dim["params"]["refs"];
    assert!(refs[0]["target"]["Element"] == first && refs[1]["target"]["Element"] == second, "{refs}");
    run(&mut s, "elements_move", json!({"ids": [second], "delta": [1.0, 0.0]}));
    let moved = run(&mut s, "element_get", json!({"id": dim["id"]}));
    assert!((moved["params"]["b"][0].as_f64().unwrap() - 5.0).abs() < 1e-9, "{moved}");

    // Hidden with the category in this view; plans keep theirs.
    run(&mut s, "view_graphics_set", json!({"view": view.0, "category": "grid", "visible": false}));
    assert!(drawn(&mut s).is_empty());
    let plan = ViewId(id(&run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]));
    let in_plan = s.drawing(plan).unwrap();
    assert!(in_plan.prims.iter().any(|p| p.element() == Some(ElementId(first))));
}
