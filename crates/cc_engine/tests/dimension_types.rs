//! Dimension type parameters: the marks, witness and dimension lines, pens
//! and text a dimension type draws its dimensions with, set through
//! `type_set` like any other type parameter.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::Session;
use cc_views::{LineStyle, Prim};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// The primitives drawn for element `id` in `view`.
fn prims_of(s: &mut Session, view: u64, id: u64) -> Vec<Prim> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims.iter().filter(|p| p.element().is_some_and(|e| e.0 == id)).cloned().collect()
}

#[test]
fn dimension_types_draw_with_their_parameters() {
    let mut s = Session::default();
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap();
    let dim = run(&mut s, "dimension_create", json!({"view": plan, "a": [0.0, 0.0], "b": [4.0, 0.0], "offset": 1.0}));
    let (id, ty) = (dim["id"].as_u64().unwrap(), dim["type"]["id"].clone());

    // The template's type draws heavy ticks and no fills.
    let before = prims_of(&mut s, plan, id);
    let heavy = |p: &[Prim]| p.iter().filter(|p| matches!(p, Prim::Line { style: LineStyle::Cut, .. })).count();
    assert_eq!(heavy(&before), 2);
    assert!(!before.iter().any(|p| matches!(p, Prim::Fill { .. })));
    let params = run(&mut s, "type_get", json!({"type": ty}))["params"].clone();
    assert_eq!(params["style"]["mark"], "Tick", "{params}");

    run(
        &mut s,
        "type_set",
        json!({"type": ty, "params": {"style": {
            "mark": "Arrow", "line_weight": 0.35, "color": [200, 0, 0],
            "prefix": "~", "suffix": " m", "opaque_text": true
        }}}),
    );
    let after = prims_of(&mut s, plan, id);
    assert_eq!(heavy(&after), 0);
    let fills: Vec<[u8; 4]> =
        after.iter().filter_map(|p| if let Prim::Fill { color, .. } = p { Some(*color) } else { None }).collect();
    assert_eq!(fills, vec![[200, 0, 0, 255], [200, 0, 0, 255], [255, 255, 255, 255]]);
    assert!(after.iter().all(|p| match p {
        Prim::Line { pen, .. } => pen.is_some_and(|p| p.weight == Some(0.35) && p.color == Some([200, 0, 0])),
        _ => true,
    }));
    let text = after.iter().find_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None });
    assert!(text.as_deref().is_some_and(|t| t.starts_with('~') && t.ends_with(" m")), "{text:?}");

    // Out-of-range values are refused, without an undo step.
    assert!(s.execute("type_set", json!({"type": ty, "params": {"style": {"mark_size": -1.0}}})).is_err());
    assert!(s.execute("type_set", json!({"type": ty, "params": {"style": {"line_weight": 99.0}}})).is_err());
    run(&mut s, "undo", json!({}));
    assert_eq!(heavy(&prims_of(&mut s, plan, id)), 2);
}
