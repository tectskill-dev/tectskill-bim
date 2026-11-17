//! Project templates: `project_new` starts a metric project by default or an
//! imperial one (feet and inches, round imperial sizes) with
//! `template: "imperial"`, and the imperial project works like the metric one.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{Category, LengthUnit, MeasurementSystem, SheetSize, TypeData, ViewId, ViewKind};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn imperial() -> Session {
    let mut s = Session::default();
    let out = run(&mut s, "project_new", json!({ "template": "imperial" }));
    assert_eq!(out["system"], json!("imperial"));
    s
}

#[test]
fn project_new_takes_a_template() {
    let mut s = imperial();
    assert_eq!(s.project().measurement_system(), MeasurementSystem::Imperial);
    assert_eq!(s.project().units.length.unit, LengthUnit::FeetInches);
    assert_eq!(run(&mut s, "project_info", json!({}))["system"], json!("imperial"));

    // No template is the metric one, as before.
    let out = run(&mut s, "project_new", json!({}));
    assert_eq!(out["system"], json!("metric"));
    assert_eq!(s.project().units, cc_core::Units::default());
    assert_eq!(s.project().levels[1].elevation, 3.0);
    let out = run(&mut s, "project_new", json!({ "template": "Metric" }));
    assert_eq!(out["system"], json!("metric"));

    // An unknown template is refused and the project is kept.
    let before = s.project().info.uid;
    let err = s.execute("project_new", json!({ "template": "us" })).expect_err("unknown template");
    assert!(err.message.contains("metric") && err.message.contains("imperial"), "{}", err.message);
    assert_eq!(s.project().info.uid, before);
}

#[test]
fn every_view_of_the_imperial_template_draws() {
    let mut s = imperial();
    s.sync();
    assert!(s.geometry().walls.is_empty());
    let views: Vec<_> = s.project().views.values().map(|v| (v.id, v.kind.clone())).collect();
    let mut drawn = 0;
    for (id, kind) in views {
        match kind {
            ViewKind::Schedule { .. } => assert!(s.schedule(id).is_some(), "schedule {id:?}"),
            ViewKind::Sheet { .. } => assert!(s.sheet(id).is_some(), "sheet {id:?}"),
            ViewKind::ThreeD { .. } => {}
            _ => {
                assert!(s.drawing(id).is_some(), "view {id:?}");
                drawn += 1;
            }
        }
    }
    assert!(drawn >= 9, "{drawn}");
    // Drawings follow the project's units.
    assert_eq!(cc_views::format::current().0.length.unit, LengthUnit::FeetInches);
}

#[test]
fn imperial_scales_read_the_customary_way() {
    let mut s = imperial();
    let plan = s.project().views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).expect("plan").id;
    let out = run(&mut s, "view_get", json!({ "view": plan.0 }));
    assert_eq!(out["scale"], json!(48));
    assert_eq!(out["scale_text"], json!("1/4\" = 1'-0\""));

    // A label, a ratio text or a number set the scale.
    let out = run(&mut s, "view_set", json!({ "view": plan.0, "scale": "1/8\" = 1'-0\"" }));
    assert_eq!(out["scale"], json!(96));
    assert_eq!(out["scale_text"], json!("1/8\" = 1'-0\""));
    let out = run(&mut s, "view_set", json!({ "view": plan.0, "scale": "1:50" }));
    assert_eq!((out["scale"].clone(), out["scale_text"].clone()), (json!(50), json!("1:50")));
    let out = run(&mut s, "view_set", json!({ "view": plan.0, "scale": 48 }));
    assert_eq!(out["scale"], json!(48));
    let err = s.execute("view_set", json!({ "view": plan.0, "scale": "1/5\" = 1'" })).expect_err("unreadable");
    assert_eq!(s.project().views[&plan].scale, 48, "{}", err.message);
    let out = run(&mut s, "view_create", json!({ "kind": "drafting", "scale": "3\" = 1'-0\"" }));
    assert_eq!(out["scale"], json!(4));

    // A callout's default detail scale is one of the imperial scales.
    let out = run(&mut s, "callout_create", json!({ "parent": plan.0, "rect": [[0, 0], [2, 2]] }));
    let scale = out["scale"].as_u64().expect("scale") as u32;
    assert!(cc_core::scale::choices(MeasurementSystem::Imperial).contains(&scale), "{scale}");
    assert_eq!(scale, 8, "1/4\" = 1'-0\" gives 1 1/2\" = 1'-0\"");

    // Metric projects are unchanged.
    run(&mut s, "project_new", json!({}));
    let plan = s.project().views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).expect("plan").id;
    let out = run(&mut s, "view_get", json!({ "view": plan.0 }));
    assert_eq!((out["scale"].clone(), out["scale_text"].clone()), (json!(50), json!("1:50")));
    let out = run(&mut s, "callout_create", json!({ "parent": plan.0, "rect": [[0, 0], [2, 2]] }));
    assert_eq!(out["scale"], json!(10));
}

#[test]
fn walls_take_the_imperial_default_type() {
    let mut s = imperial();
    let foot = cc_core::units::FOOT;
    let out = run(&mut s, "wall_create", json!({ "start": [0, 0], "end": [20.0 * foot, 0] }));
    let id = out["id"].as_u64().expect("wall id");
    let wall = s.project().elements.values().find(|e| e.id.0 == id).expect("wall");
    let t = s.project().element_type(wall.type_id).expect("type");
    assert_eq!(t.category(), Category::Wall);
    assert_eq!(t.name, cc_core::template_default("tpl-imp-type-masonry-6"));
    assert!((t.thickness() - 6.0 * cc_core::units::INCH).abs() < 1e-12);
    s.sync();
    assert_eq!(s.geometry().walls.len(), 1);
    let plan = s.project().views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).expect("plan").id;
    let drawing = s.drawing(plan).expect("plan drawing");
    assert!(!drawing.prims.is_empty());
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-templates-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Sheets come in ANSI and architectural sizes, by name, and compose and
/// export at that size.
#[test]
fn imperial_sheets_take_us_sizes() {
    let mut s = imperial();
    // The template's sheet and a new one are ANSI B.
    let template_sheet = s.project().views.values().find_map(|v| match &v.kind {
        ViewKind::Sheet { size, .. } => Some(*size),
        _ => None,
    });
    assert_eq!(template_sheet, Some(SheetSize::AnsiB));
    let sheet = run(&mut s, "view_create", json!({ "kind": "sheet" }))["id"].as_u64().unwrap();
    let id = ViewId(sheet);
    assert!(matches!(s.project().views[&id].kind, ViewKind::Sheet { size: SheetSize::AnsiB, .. }));

    run(&mut s, "sheet_set", json!({ "sheet": sheet, "size": "ANSI D" }));
    assert!(matches!(s.project().views[&id].kind, ViewKind::Sheet { size: SheetSize::AnsiD, .. }));
    let paper = s.sheet(id).expect("composed").paper;
    assert!((paper.x - 34.0 * 25.4).abs() < 1e-9 && (paper.y - 22.0 * 25.4).abs() < 1e-9, "{paper}");
    let svg = scratch("ansi-d.svg");
    run(&mut s, "sheet_export", json!({ "sheet": sheet, "path": svg }));
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains(r#"width="863.6mm" height="558.8mm""#), "{}", &text[..300.min(text.len())]);
    let pdf = scratch("ansi-d.pdf");
    run(&mut s, "sheet_export", json!({ "sheet": sheet, "path": pdf }));
    let bytes = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    assert!(bytes.contains("/MediaBox [0 0 2448 1584]"), "34\" x 22\" in points");

    // Other spellings of the names; the A sizes too.
    for (name, size) in [
        ("ansi_b", SheetSize::AnsiB),
        ("ARCH D", SheetSize::ArchD),
        ("arch e1", SheetSize::ArchE1),
        ("a1", SheetSize::A1),
    ] {
        run(&mut s, "sheet_set", json!({ "sheet": sheet, "size": name }));
        assert!(matches!(&s.project().views[&id].kind, ViewKind::Sheet { size: got, .. } if *got == size), "{name}");
    }
    let err = s.execute("sheet_set", json!({ "sheet": sheet, "size": "Letter" })).expect_err("unknown paper");
    assert!(err.message.contains("ANSI"), "{}", err.message);

    // Metric projects still start their sheets on A3.
    run(&mut s, "project_new", json!({}));
    let sheet = run(&mut s, "view_create", json!({ "kind": "sheet" }))["id"].as_u64().unwrap();
    assert!(matches!(s.project().views[&ViewId(sheet)].kind, ViewKind::Sheet { size: SheetSize::A3, .. }));
}

/// A drawing in millimeters with one text, 250 mm tall.
fn text_dxf() -> String {
    let pairs = |groups: &[(i32, &str)]| -> String { groups.iter().map(|(c, v)| format!("{c:>3}\n{v}\n")).collect() };
    let mut s = pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]);
    s += &pairs(&[(0, "SECTION"), (2, "ENTITIES")]);
    s += &pairs(&[(0, "TEXT"), (8, "0"), (10, "100"), (20, "200"), (30, "0"), (40, "250"), (1, "Room")]);
    s += &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    s
}

/// Annotation types made on demand are named in inches, and a level mark
/// type's default text is 3/32".
#[test]
fn imperial_on_demand_annotation_types_hold_no_millimeters() {
    let mut s = imperial();
    let plan = s.project().views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).expect("plan").id;

    // A level mark type with the first mark.
    run(&mut s, "level_mark_create", json!({ "view": plan.0, "position": [0, 0] }));
    let ty = s.project().types.values().find(|t| t.category() == Category::LevelMark).expect("level mark type");
    assert!(!ty.name.contains("mm"), "{}", ty.name);
    assert!(ty.name.contains("3/32\""), "{}", ty.name);
    let TypeData::LevelMark { text_size, .. } = ty.data else { panic!("a level mark type") };
    assert!((text_size - 3.0 / 32.0 * 25.4).abs() < 1e-9, "{text_size}");
    let made = run(&mut s, "level_mark_type_create", json!({}))["id"].as_u64().unwrap();
    let ty = &s.project().types[&cc_core::ElementTypeId(made)];
    assert!(!ty.name.contains("mm"), "{}", ty.name);

    // A text type per size when a drawing is exploded.
    let path = scratch("text.dxf");
    std::fs::write(&path, text_dxf()).unwrap();
    let id = run(&mut s, "cad_import", json!({ "view": plan.0, "path": path }))["id"].as_u64().unwrap();
    let texts_before = s.project().types.values().filter(|t| t.category() == Category::Text).count();
    let out = run(&mut s, "cad_explode", json!({ "id": id }));
    assert_eq!(out["texts"], json!(1), "{out}");
    let new: Vec<_> = s.project().types.values().filter(|t| t.category() == Category::Text).collect();
    assert_eq!(new.len(), texts_before + 1, "a new text type");
    let made = out["types"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>();
    let names: Vec<&str> = made
        .iter()
        .filter_map(|t| s.project().types.get(&cc_core::ElementTypeId(*t)))
        .filter(|t| t.category() == Category::Text)
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names.len(), 1, "{out}");
    assert!(!names[0].contains("mm") && names[0].ends_with('"'), "{}", names[0]);

    // Metric projects keep their names.
    run(&mut s, "project_new", json!({}));
    let plan = s.project().views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).expect("plan").id;
    run(&mut s, "level_mark_create", json!({ "view": plan.0, "position": [0, 0] }));
    let ty = s.project().types.values().find(|t| t.category() == Category::LevelMark).expect("level mark type");
    assert!(ty.name.ends_with("mm"), "{}", ty.name);
    let TypeData::LevelMark { text_size, .. } = ty.data else { panic!("a level mark type") };
    assert_eq!(text_size, 2.5);
}
