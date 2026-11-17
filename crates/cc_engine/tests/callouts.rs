//! Callouts and detail views (issue #152): a rectangle in a parent view
//! makes a detail view of that region, cropped to it and live with the
//! model; the parent draws the callout's mark; the crop and the mark are
//! one rectangle; deleting the detail view removes the mark.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ViewId, ViewKind};
use cc_engine::Session;
use cc_views::Prim;
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> ViewId {
    ViewId(v["id"].as_u64().unwrap())
}

fn first_plan(s: &mut Session) -> ViewId {
    let plans = run(s, "views_list", json!({"category": "floor_plan"}));
    ViewId(plans[0]["id"].as_u64().unwrap())
}

/// The extents of the lines of elements in a view's drawing.
fn element_extents(s: &mut Session, view: ViewId) -> Option<(DVec2, DVec2)> {
    let d = s.drawing(view).unwrap();
    let mut out: Option<(DVec2, DVec2)> = None;
    for p in d.own_prims() {
        if let Prim::Line { a, b, element: Some(_), .. } = p {
            let (lo, hi) = out.unwrap_or((*a, *a));
            out = Some((lo.min(a.min(*b)), hi.max(a.max(*b))));
        }
    }
    out
}

#[test]
fn a_callout_makes_a_cropped_detail_view_of_the_plan() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    run(&mut s, "view_set", json!({"view": plan.0, "scale": 50}));
    run(&mut s, "wall_create", json!({"start": [0.0, 0.0], "end": [10.0, 0.0]}));
    run(&mut s, "wall_create", json!({"start": [10.0, 0.0], "end": [10.0, 8.0]}));

    let out = run(&mut s, "callout_create", json!({"parent": plan.0, "rect": [[11.0, 2.0], [8.0, -1.0]]}));
    let detail = id(&out);
    assert_eq!(out["category"], "floor_plan", "the parent's kind");
    assert_eq!(out["scale"], 10, "five times larger than 1:50");
    assert_eq!(out["crop"], json!([[8.0, -1.0], [11.0, 2.0]]));
    assert_eq!(out["callout"]["parent"], plan.0);
    assert_eq!(out["callout"]["rect"], out["crop"]);
    assert!(out["name"].as_str().unwrap().ends_with('1'), "{}", out["name"]);

    // The detail shows only the corner of the walls, inside the rectangle.
    let (lo, hi) = element_extents(&mut s, detail).expect("the walls' corner");
    assert!(lo.x >= 8.0 - 1e-9 && lo.y >= -1.0 - 1e-9 && hi.x <= 11.0 + 1e-9 && hi.y <= 2.0 + 1e-9, "{lo} {hi}");
    let (_, whole) = element_extents(&mut s, plan).unwrap();
    assert!(whole.y > 7.0, "the parent is not cropped");

    // The parent draws the mark: rectangle and tag (the detail's number over a dash).
    let d = s.drawing(plan).unwrap();
    let mark = *d.callouts.iter().find(|c| c.view == detail).expect("the callout's mark");
    assert_eq!(mark.rect.min, DVec2::new(8.0, -1.0));
    assert!((mark.radius - 0.3).abs() < 1e-9, "a 12 mm tag at 1:50: {}", mark.radius);
    assert!(mark.hits(DVec2::new(9.5, 2.0), 0.05), "on a side");
    assert!(!mark.hits(DVec2::new(9.5, 0.5), 0.05), "inside is not the mark");
    assert!(d.callout_at(mark.tag, 0.0).is_some());
    // Callouts are not drawn as section or elevation marks.
    assert!(d.sections.is_empty());

    // Placed on a sheet, the tag reads its number there and the sheet's number.
    let sheet = id(&run(&mut s, "view_create", json!({"kind": "sheet"})));
    run(&mut s, "sheet_place_view", json!({"sheet": sheet.0, "view": detail.0}));
    let number = s.project().view(sheet).and_then(|v| match &v.kind {
        ViewKind::Sheet { number, .. } => Some(number.clone()),
        _ => None,
    });
    let d = s.drawing(plan).unwrap();
    let texts: Vec<&str> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&number.unwrap().as_str()), "{texts:?}");

    // The detail carries its own annotations, drawn whole even outside the crop.
    run(&mut s, "text_create", json!({"view": detail.0, "position": [20.0, 20.0], "text": "Note"}));
    let d = s.drawing(detail).unwrap();
    assert!(d.prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if text == "Note")));
}

#[test]
fn moving_the_callout_is_the_crop_and_the_detail_follows_the_parent() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    let detail = id(&run(&mut s, "callout_create", json!({"parent": plan.0, "rect": [[0, 0], [2, 2]], "scale": 20})));
    assert_eq!(s.project().view(detail).unwrap().scale, 20);

    // Moving the crop moves the mark.
    run(&mut s, "view_set", json!({"view": detail.0, "crop": [[1, 1], [4, 3]]}));
    let d = s.drawing(plan).unwrap();
    assert_eq!(d.callouts[0].rect.max, DVec2::new(4.0, 3.0));
    // A callout needs its rectangle.
    assert!(s.execute("view_set", json!({"view": detail.0, "crop": null})).is_err());
    assert!(s.execute("view_set", json!({"view": detail.0, "crop": [[0, 0], [0, 5]]})).is_err());

    // A section's detail follows its section line.
    let section = id(&run(&mut s, "view_create", json!({"kind": "section", "start": [0.0, 0.0], "end": [10.0, 0.0]})));
    let sd = id(&run(&mut s, "callout_create", json!({"parent": section.0, "rect": [[1, 0], [3, 3]]})));
    assert_eq!(run(&mut s, "view_get", json!({"view": sd.0}))["category"], "section");
    run(&mut s, "view_set", json!({"view": section.0, "params": {"end": [10.0, 5.0]}}));
    let ViewKind::Section { end, .. } = s.project().view(sd).unwrap().kind else { panic!() };
    assert_eq!(end, DVec2::new(10.0, 5.0));
    // Its detail is not a second section mark in plans.
    let marks: Vec<ViewId> = s.drawing(plan).unwrap().sections.iter().map(|h| h.view).collect();
    assert_eq!(marks, vec![section]);
    let list = run(&mut s, "callouts_list", json!({"view": section.0}));
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Deleting the detail removes its mark; undo brings both back.
    run(&mut s, "view_delete", json!({"view": detail.0}));
    assert!(s.drawing(plan).unwrap().callouts.is_empty());
    run(&mut s, "undo", json!({}));
    assert_eq!(s.drawing(plan).unwrap().callouts.len(), 1);

    // Deleting the parent leaves the detail as an ordinary cropped view.
    run(&mut s, "view_delete", json!({"view": section.0}));
    let v = s.project().view(sd).unwrap();
    assert!(v.callout.is_none() && v.crop.is_some());

    // Callouts go in plans, ceiling plans, sections and elevations only.
    let three_d = id(&run(&mut s, "view_create", json!({"kind": "three_d"})));
    assert!(s.execute("callout_create", json!({"parent": three_d.0, "rect": [[0, 0], [1, 1]]})).is_err());
}

#[test]
fn a_callout_can_point_to_a_drafting_view() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    let drafting = id(&run(&mut s, "view_create", json!({"kind": "drafting", "name": "Eave detail"})));
    let out =
        run(&mut s, "callout_create", json!({"parent": plan.0, "rect": [[0, 0], [1, 1]], "drafting": "Eave detail"}));
    assert_eq!(id(&out), drafting);
    assert_eq!(out["crop"], Value::Null, "drafting views are not cropped");
    assert_eq!(out["callout"]["drafting"], true);
    assert_eq!(s.drawing(plan).unwrap().callouts[0].view, drafting);
    // One callout per drafting view.
    let again =
        s.execute("callout_create", json!({"parent": plan.0, "rect": [[2, 2], [3, 3]], "drafting": drafting.0}));
    assert!(again.is_err());
    // view_set's crop moves its callout.
    run(&mut s, "view_set", json!({"view": drafting.0, "crop": [[5, 5], [6, 7]]}));
    assert_eq!(s.drawing(plan).unwrap().callouts[0].rect.max, DVec2::new(6.0, 7.0));
    assert_eq!(s.project().view(drafting).unwrap().crop, None);
    // Removing the callout keeps the drafting view.
    run(&mut s, "callout_remove", json!({"view": drafting.0}));
    assert!(s.drawing(plan).unwrap().callouts.is_empty());
    assert!(s.project().view(drafting).is_some());
}

#[test]
fn files_with_dangling_callouts_are_repaired() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    let detail = id(&run(&mut s, "callout_create", json!({"parent": plan.0, "rect": [[0, 0], [2, 2]]})));
    let mut project = s.project().clone();
    project.views.remove(&plan);
    if let Some(v) = project.views.get_mut(&detail) {
        v.crop = Some(cc_core::CropBox { min: DVec2::splat(f64::NAN), max: DVec2::ONE });
    }
    let ron = cc_core::to_ron(&project).unwrap();
    let (project, repairs) = cc_core::from_ron_with_repairs(&ron).unwrap();
    assert!(repairs >= 2, "{repairs}");
    let v = project.views.get(&detail).unwrap();
    assert!(v.callout.is_none() && v.crop.is_none());
}
