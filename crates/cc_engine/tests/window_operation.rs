//! Windows whose panels open (#239): the template's casement types, their
//! swings in plans and the operation set through a component definition.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::Session;
use cc_views::{LineStyle, Prim};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn plan(s: &mut Session) -> u64 {
    run(s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap()
}

/// Leaves (projection lines) and thin or dashed lines (the two sash lines,
/// then the arcs) of element `id` in the plan.
fn symbol(s: &mut Session, view: u64, id: u64) -> (usize, usize) {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let mut leaves = 0;
    let mut thin = 0;
    for p in &d.prims {
        if let Prim::Line { style, element: Some(e), .. } = p
            && e.0 == id
        {
            match style {
                LineStyle::Projection => leaves += 1,
                LineStyle::Thin | LineStyle::Hidden => thin += 1,
                _ => {}
            }
        }
    }
    (leaves, thin)
}

fn window_in_wall(s: &mut Session, ty: &str) -> u64 {
    let wall = run(s, "wall_create", json!({"start": [0, 0], "end": [5, 0]}));
    run(s, "window_create", json!({"wall": wall["id"], "point": [2.5, 0.0], "type": ty}))["id"].as_u64().unwrap()
}

#[test]
fn casement_windows_draw_their_swings() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let fixed = window_in_wall(&mut s, "Janela 120x120");
    let single = window_in_wall(&mut s, "Janela de abrir 60x100");
    let double = window_in_wall(&mut s, "Janela de abrir 2 folhas 120x120");
    let (leaves, sash) = symbol(&mut s, view, fixed);
    assert_eq!(leaves, 0, "a fixed window has no leaf");
    let (one, with_arc) = symbol(&mut s, view, single);
    assert_eq!(one, 1, "one leaf");
    assert!(with_arc > sash, "and its arc");
    let (two, with_arcs) = symbol(&mut s, view, double);
    assert_eq!(two, 2, "two leaves");
    assert!(with_arcs > with_arc, "and their arcs");
}

#[test]
fn a_flipped_hand_moves_the_hinge() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let single = window_in_wall(&mut s, "Janela de abrir 60x100");
    let leaf = |s: &mut Session| {
        let d = s.drawing(cc_core::ViewId(view)).unwrap();
        d.prims
            .iter()
            .find_map(|p| match p {
                Prim::Line { a, b, style: LineStyle::Projection, element: Some(e), .. } if e.0 == single => {
                    Some((a.x + b.x) / 2.0)
                }
                _ => None,
            })
            .unwrap()
    };
    // Hinged on the left: at the start of the 60 cm opening around x = 2.5.
    let left = leaf(&mut s);
    assert!((left - 2.2).abs() < 1e-6, "{left}");
    run(&mut s, "element_set", json!({"id": single, "params": {"flip_hand": true}}));
    let right = leaf(&mut s);
    assert!((right - 2.8).abs() < 1e-6, "{right}");
}

#[test]
fn the_operation_is_part_of_the_definition() {
    let mut s = Session::default();
    let ty = run(&mut s, "type_get", json!({"type": "Janela de abrir 2 folhas 120x120"}));
    assert_eq!(ty["params"]["def"]["opening"]["operation"], "DoubleSideHung", "{ty}");
    // A fixed window does not write it.
    let fixed = run(&mut s, "type_get", json!({"type": "Janela 120x120"}));
    assert!(fixed["params"]["def"]["opening"].get("operation").is_none());
    // A new component may open one way or another.
    let def = json!({
        "kind": "Window",
        "params": [{"name": "L", "value": 0.8, "kind": "Length"}, {"name": "A", "value": 1.0, "kind": "Length"}],
        "items": [{"name": "Glass", "shape": {"Box": {"x": "-L / 2", "y": "-0.01", "z": "0", "width": "L", "depth": "0.02", "height": "A"}}}],
        "opening": {"width": "L", "height": "A", "operation": "SideHungRight"}
    });
    let created = run(&mut s, "component_create", json!({"name": "Right", "def": def}));
    let view = plan(&mut s);
    let wall = run(&mut s, "wall_create", json!({"start": [0, 3], "end": [5, 3]}));
    let w = run(&mut s, "window_create", json!({"wall": wall["id"], "point": [2.5, 3.0], "type": created["id"]}));
    assert_eq!(symbol(&mut s, view, w["id"].as_u64().unwrap()).0, 1);
}

fn view_named(s: &mut Session, category: &str, name: &str) -> u64 {
    let views = run(s, "views_list", json!({"category": category}));
    views.as_array().unwrap().iter().find(|v| v["name"] == name).unwrap()["id"].as_u64().unwrap()
}

/// Lines of element `id` in `view` in each style.
fn styles(s: &mut Session, view: u64, id: u64) -> Vec<LineStyle> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { style, element: Some(e), .. } if e.0 == id => Some(*style),
            _ => None,
        })
        .collect()
}

#[test]
fn window_swings_are_dashed_and_doors_are_not() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let single = window_in_wall(&mut s, "Janela de abrir 60x100");
    let wall = run(&mut s, "wall_create", json!({"start": [0, 4], "end": [5, 4]}));
    let door = run(&mut s, "door_create", json!({"wall": wall["id"], "point": [2.5, 4.0]}))["id"].as_u64().unwrap();
    assert!(styles(&mut s, view, single).contains(&LineStyle::Hidden), "the window's arc is dashed");
    assert!(!styles(&mut s, view, door).contains(&LineStyle::Hidden), "the door's is not");
}

#[test]
fn sliding_panels_are_offset_in_plan() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let sliding = window_in_wall(&mut s, "Janela de correr 150x120");
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let ys: Vec<f64> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { a, b, style: LineStyle::Thin, element: Some(e), .. }
                if e.0 == sliding && (a.y - b.y).abs() < 1e-9 =>
            {
                Some(a.y)
            }
            _ => None,
        })
        .collect();
    assert!(ys.iter().any(|y| (y + 0.02).abs() < 1e-6) && ys.iter().any(|y| (y - 0.02).abs() < 1e-6), "{ys:?}");
    assert!(!ys.iter().any(|y| (y.abs() - 0.01).abs() < 1e-6), "no fixed sash lines");
}

#[test]
fn windows_above_the_cut_are_dashed() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let high = window_in_wall(&mut s, "Janela 120x120");
    run(&mut s, "element_set", json!({"id": high, "params": {"sill_height": 1.8}}));
    // Its tag stays as it is.
    let lines: Vec<LineStyle> =
        styles(&mut s, view, high).into_iter().filter(|l| *l != LineStyle::Annotation).collect();
    assert!(!lines.is_empty() && lines.iter().all(|l| *l == LineStyle::Hidden), "{lines:?}");
}

#[test]
fn elevations_and_sections_show_the_opening_symbols() {
    let mut s = Session::default();
    let south = view_named(&mut s, "elevation", "Sul");
    let fixed = window_in_wall(&mut s, "Janela 120x120");
    let pivot = {
        let wall = run(&mut s, "wall_create", json!({"start": [6, 0], "end": [9, 0]}));
        let w = run(
            &mut s,
            "window_create",
            json!({"wall": wall["id"], "point": [7.5, 0.0], "type": "Janela maxim-ar 60x60"}),
        );
        w["id"].as_u64().unwrap()
    };
    let wall = run(&mut s, "wall_create", json!({"start": [10, 0], "end": [13, 0]}));
    let door = run(&mut s, "door_create", json!({"wall": wall["id"], "point": [11.5, 0.0]}))["id"].as_u64().unwrap();
    assert!(!styles(&mut s, south, fixed).contains(&LineStyle::Hidden), "a fixed window has no symbol");
    let top_hung = styles(&mut s, south, pivot).iter().filter(|l| **l == LineStyle::Hidden).count();
    assert_eq!(top_hung, 2, "the top-hung triangle");
    let swing = styles(&mut s, south, door).iter().filter(|l| **l == LineStyle::Hidden).count();
    assert_eq!(swing, 2, "the door's triangle");
    // A wall in front hides them: it is painted over them.
    let front =
        run(&mut s, "wall_create", json!({"start": [-1, -2], "end": [14, -2], "height": 3.0}))["id"].as_u64().unwrap();
    let d = s.drawing(cc_core::ViewId(south)).unwrap();
    let at = |test: &dyn Fn(&Prim) -> bool| d.prims.iter().position(test);
    let symbol =
        at(&|p| matches!(p, Prim::Line { style: LineStyle::Hidden, element: Some(e), .. } if e.0 == door)).unwrap();
    let cover = at(&|p| matches!(p, Prim::Fill { element: Some(e), .. } if e.0 == front)).unwrap();
    assert!(cover > symbol, "painted after the door's symbol");
    // A section through the openings shows them too, from the side it looks at.
    let section = run(&mut s, "view_create", json!({"kind": "section", "start": [14.0, -1.0], "end": [-1.0, -1.0]}));
    let id = section["id"].as_u64().unwrap();
    let both =
        styles(&mut s, id, door).contains(&LineStyle::Hidden) || styles(&mut s, id, pivot).contains(&LineStyle::Hidden);
    assert!(both, "seen in the section");
}
