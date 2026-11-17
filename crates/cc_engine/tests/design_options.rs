//! Design options through commands: sets and options, elements moved into
//! options, what views and schedules show, geometry and rooms per option,
//! hosted elements, the option being edited, deleting and accepting
//! options, templates, undo and repair.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementId, ViewId};
use cc_engine::{ErrorCode, Session};
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

fn drawn(s: &mut Session, view: u64) -> BTreeSet<u64> {
    s.drawing(ViewId(view)).unwrap().prims.iter().filter_map(|p| p.element()).map(|e| e.0).collect()
}

fn wall(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> u64 {
    id(&run(s, "wall_create", json!({"start": a, "end": b})))
}

/// An 8 x 4 box (main model) and a set "Layout" with options "Open" (no
/// partition, primary) and "Split" (a partition at x = 4); returns the
/// session, the plan, a copy showing "Split", the set, both options, the
/// box walls and the partition.
struct Fixture {
    s: Session,
    plan: u64,
    split_plan: u64,
    set: u64,
    open: u64,
    split: u64,
    walls: Vec<u64>,
    partition: u64,
}

fn fixture() -> Fixture {
    let mut s = Session::default();
    let plan = id(&run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]);
    let pts = [[0.0, 0.0], [8.0, 0.0], [8.0, 4.0], [0.0, 4.0]];
    let walls: Vec<u64> = (0..4).map(|i| wall(&mut s, pts[i], pts[(i + 1) % 4])).collect();
    let set = run(&mut s, "option_set_create", json!({"name": "Layout", "options": ["Open", "Split"]}));
    let (open, split) = (id(&set["options"][0]), id(&set["options"][1]));
    assert_eq!(set["primary"], json!(open));
    let set = id(&set);
    let partition = wall(&mut s, [4.0, 0.0], [4.0, 4.0]);
    run(&mut s, "elements_option_set", json!({"ids": [partition], "option": "Layout: Split"}));
    let split_plan = id(&run(&mut s, "view_duplicate", json!({"view": plan})));
    run(&mut s, "view_options_set", json!({"view": split_plan, "set": "Layout", "option": "Split"}));
    Fixture { s, plan, split_plan, set, open, split, walls, partition }
}

#[test]
fn views_show_the_main_model_and_their_option() {
    let Fixture { mut s, plan, split_plan, split, partition, walls, .. } = fixture();
    assert!(!drawn(&mut s, plan).contains(&partition));
    assert!(drawn(&mut s, split_plan).contains(&partition));
    assert!(drawn(&mut s, plan).contains(&walls[0]) && drawn(&mut s, split_plan).contains(&walls[0]));
    let v = run(&mut s, "view_get", json!({"view": split_plan}));
    assert_eq!(v["design_options"][0]["choice"]["id"], json!(split));
    assert_eq!(v["design_options"][0]["shown"]["name"], json!("Split"));
    let e = run(&mut s, "element_get", json!({"id": partition}));
    assert_eq!(e["design_option"]["name"], json!("Split"));
    assert_eq!(e["design_option"]["set"]["name"], json!("Layout"));
    // A non-primary option still has its geometry for queries.
    assert!(e["derived"]["volume"].as_f64().unwrap() > 0.1);
    // The partition joins the box only where it is shown.
    let tee = |g: &cc_geom::ModelGeometry| g.walls.contains_key(&ElementId(partition));
    assert!(tee(s.view_geometry(ViewId(split_plan))));
    assert!(!tee(s.view_geometry(ViewId(plan))));
    assert!(!tee(s.geometry()));
    // Schedules too.
    let sched = id(&run(&mut s, "view_create", json!({"kind": "schedule", "category": "wall"})));
    let rows = |s: &mut Session| s.schedule(ViewId(sched)).unwrap().rows.len();
    assert_eq!(rows(&mut s), 4);
    run(&mut s, "view_options_set", json!({"view": sched, "set": "Layout", "option": "Split"}));
    assert_eq!(rows(&mut s), 5);
    // Automatic again.
    run(&mut s, "view_options_set", json!({"view": sched, "set": "Layout", "option": null}));
    assert_eq!(rows(&mut s), 4);
    // Sheets and drafting views have no options.
    let sheet = id(&run(&mut s, "view_create", json!({"kind": "sheet"})));
    assert_eq!(
        fails(&mut s, "view_options_set", json!({"view": sheet, "set": "Layout", "option": "Split"})),
        ErrorCode::Rejected
    );
}

#[test]
fn rooms_are_bounded_by_the_walls_of_their_option() {
    let Fixture { mut s, plan, split_plan, split, .. } = fixture();
    let main_room = id(&run(&mut s, "room_create", json!({"point": [2.0, 2.0]})));
    run(&mut s, "option_active_set", json!({"option": split}));
    let split_room = id(&run(&mut s, "room_create", json!({"point": [2.0, 2.0]})));
    run(&mut s, "option_active_set", json!({"option": null}));
    let get = |s: &mut Session, r: u64| run(s, "element_get", json!({"id": r}));
    assert_eq!(get(&mut s, split_room)["design_option"]["id"], json!(split));
    assert_eq!(get(&mut s, main_room)["design_option"], Value::Null);
    let area = |s: &mut Session, r: u64| get(s, r)["derived"]["area"].as_f64().unwrap();
    let (whole, half) = (area(&mut s, main_room), area(&mut s, split_room));
    assert!(whole > 25.0 && half < whole * 0.6, "{whole} vs {half}");
    assert!(drawn(&mut s, split_plan).contains(&split_room) && !drawn(&mut s, plan).contains(&split_room));
}

#[test]
fn the_active_option_takes_new_elements_and_shows_in_automatic_views() {
    let Fixture { mut s, plan, split_plan, set, open, split, partition, .. } = fixture();
    let depth = s.doc.undo_depth();
    run(&mut s, "option_active_set", json!({"option": "Split"}));
    // Display state: no undo step.
    assert_eq!(s.doc.undo_depth(), depth);
    // Views without a choice show the option being edited.
    assert!(drawn(&mut s, plan).contains(&partition));
    let extra = wall(&mut s, [2.0, 0.0], [2.0, 4.0]);
    assert_eq!(run(&mut s, "element_get", json!({"id": extra}))["design_option"]["id"], json!(split));
    // A door follows its wall's option.
    let door = id(&run(&mut s, "door_create", json!({"wall": partition, "offset": 1.0})));
    assert_eq!(run(&mut s, "element_get", json!({"id": door}))["design_option"]["id"], json!(split));
    let list = run(&mut s, "options_list", json!({}));
    assert_eq!(list["active"]["id"], json!(split));
    assert_eq!(list["sets"][0]["options"][1]["elements"], json!(3));
    // Hosted elements cannot leave their host's option; annotations take none.
    assert_eq!(fails(&mut s, "elements_option_set", json!({"ids": [door], "option": null})), ErrorCode::Rejected);
    // Moving the wall moves its door.
    run(&mut s, "elements_option_set", json!({"ids": [partition], "option": open}));
    assert_eq!(run(&mut s, "element_get", json!({"id": door}))["design_option"]["id"], json!(open));
    run(&mut s, "option_active_set", json!({}));
    assert!(!drawn(&mut s, split_plan).contains(&partition));
    let _ = set;
}

#[test]
fn options_are_renamed_made_primary_deleted_and_accepted() {
    let Fixture { mut s, plan, split_plan, set, open, split, partition, walls, .. } = fixture();
    let depth = s.doc.undo_depth();
    assert_eq!(fails(&mut s, "option_set_create", json!({"name": "layout"})), ErrorCode::Rejected);
    assert_eq!(fails(&mut s, "option_create", json!({"set": set, "name": "split"})), ErrorCode::Rejected);
    let third = run(&mut s, "option_create", json!({"set": "Layout"}));
    // A default name in the session's language (which follows the system's).
    let name = third["option"]["name"].as_str().unwrap_or_default();
    assert!(!name.is_empty() && name != "Open" && name != "Split", "{name}");
    let third = id(&third["option"]);
    run(&mut s, "option_set", json!({"option": split, "name": "Two rooms", "primary": true}));
    assert!(drawn(&mut s, plan).contains(&partition));
    // The primary cannot be deleted; another can, with its elements.
    assert_eq!(fails(&mut s, "option_delete", json!({"option": split})), ErrorCode::Rejected);
    let w = wall(&mut s, [1.0, 1.0], [2.0, 1.0]);
    run(&mut s, "elements_option_set", json!({"ids": [w], "option": third}));
    let out = run(&mut s, "option_delete", json!({"option": third}));
    assert_eq!(out["elements_deleted"], json!(1));
    assert!(fails(&mut s, "element_get", json!({"id": w})) == ErrorCode::NotFound);
    // Accepting the primary keeps its elements in the main model and drops the rest.
    let porch = wall(&mut s, [0.0, -2.0], [8.0, -2.0]);
    run(&mut s, "elements_option_set", json!({"ids": [porch], "option": open}));
    run(&mut s, "option_set_rename", json!({"set": set, "name": "Plan"}));
    let out = run(&mut s, "option_accept_primary", json!({"set": "Plan"}));
    assert_eq!((out["elements_kept"].clone(), out["elements_deleted"].clone()), (json!(1), json!(1)));
    assert_eq!(run(&mut s, "element_get", json!({"id": partition}))["design_option"], Value::Null);
    assert_eq!(fails(&mut s, "element_get", json!({"id": porch})), ErrorCode::NotFound);
    assert!(run(&mut s, "options_list", json!({}))["sets"].as_array().unwrap().is_empty());
    // Views lose their choice of the deleted set.
    assert_eq!(run(&mut s, "view_get", json!({"view": split_plan}))["design_options"], json!([]));
    assert!(s.project().view(ViewId(split_plan)).unwrap().design_options.is_empty());
    // Everything undoes back to the fixture.
    while s.doc.undo_depth() > depth {
        s.doc.undo();
    }
    assert!(drawn(&mut s, split_plan).contains(&partition));
    assert!(!drawn(&mut s, plan).contains(&partition));
    assert!(drawn(&mut s, plan).contains(&walls[0]));
    // Deleting the whole set deletes its options' elements.
    let out = run(&mut s, "option_set_delete", json!({"set": set}));
    assert_eq!(out["elements_deleted"], json!(1));
    assert!(s.project().option_sets.is_empty());
}

#[test]
fn templates_can_control_the_options_shown() {
    let Fixture { mut s, plan, split, partition, .. } = fixture();
    let t = id(&run(&mut s, "template_create", json!({"name": "Split plans", "view": plan})));
    run(
        &mut s,
        "template_set",
        json!({"template": t, "include": {"design_options": true},
               "design_options": [{"set": "Layout", "option": "Split"}]}),
    );
    run(&mut s, "view_template_assign", json!({"views": [plan], "template": t}));
    assert!(drawn(&mut s, plan).contains(&partition));
    assert_eq!(
        fails(&mut s, "view_options_set", json!({"view": plan, "set": "Layout", "option": null})),
        ErrorCode::Rejected
    );
    let tj = run(&mut s, "templates_list", json!({}));
    let tj = tj.as_array().unwrap().iter().find(|x| id(x) == t).unwrap().clone();
    assert_eq!(tj["design_options"][0]["id"], json!(split));
}

#[test]
fn ifc_export_writes_the_primary_options_only() {
    let Fixture { mut s, partition, walls, .. } = fixture();
    let porch = wall(&mut s, [0.0, -2.0], [8.0, -2.0]);
    run(&mut s, "elements_option_set", json!({"ids": [porch], "option": "Open"}));
    // Whatever option is being edited.
    run(&mut s, "option_active_set", json!({"option": "Split"}));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let uid = |s: &Session, id: u64| cc_ifc::guid::encode(s.project().element(ElementId(id)).unwrap().uid.0);
    assert!(text.contains(&uid(&s, porch)) && text.contains(&uid(&s, walls[0])));
    assert!(!text.contains(&uid(&s, partition)));
    assert!(text.contains("ConstructCAD_DesignOption"));
    let (p, _) = cc_ifc::import(&text, cc_core::Project::from_template()).unwrap();
    assert_eq!(p.option_sets.len(), 1);
    assert_eq!(p.option_sets[0].name, "Layout");
    assert_eq!(p.option_sets[0].options.len(), 1);
    assert_eq!(p.option_sets[0].options[0].name, "Open");
    assert_eq!(p.elements.values().filter(|e| e.design_option.is_some()).count(), 1);
}
