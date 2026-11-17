//! Bar marks, bending schedules and the bar length check (#286): identical
//! bars share a mark across hosts, overrides win, frozen marks stay when the
//! model changes, renumbering restarts and undoes; the bending schedule's
//! totals add the bars exactly, and it goes on a sheet and to CSV; tags
//! write their set's bars; crafted settings are repaired.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{BendingOptions, ElementId, ElementKind, RebarLayout};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

fn ids(v: &Value) -> Vec<u64> {
    v["created"].as_array().unwrap_or_else(|| panic!("no created in {v}")).iter().map(|x| x.as_u64().unwrap()).collect()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn beam(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> u64 {
    id(&run(s, "beam_create", json!({"start": a, "end": b, "top_offset": 3.0})))
}

/// The sets of `host` by layout: bottom, top and stirrups of a beam.
fn beam_sets(s: &Session, host: u64) -> (u64, u64, u64) {
    let mut out = (0, 0, 0);
    for (e, r) in s.project().rebar_of(ElementId(host)) {
        match r.layout {
            RebarLayout::Longitudinal { face: cc_core::BarFace::Bottom, .. } => out.0 = e.id.0,
            RebarLayout::Longitudinal { .. } => out.1 = e.id.0,
            _ => out.2 = e.id.0,
        }
    }
    out
}

/// The marks of each set, as `element_get` gives them.
fn marks_of(s: &mut Session, sets: &[u64]) -> BTreeMap<u64, Value> {
    sets.iter().map(|id| (*id, run(s, "element_get", json!({"id": id}))["derived"]["marks"].clone())).collect()
}

fn list(s: &mut Session) -> Vec<Value> {
    run(s, "bar_marks_list", json!({}))["marks"].as_array().unwrap().clone()
}

/// Identical bars in different hosts share a mark, bars more than the
/// tolerance apart do not; an override wins; frozen marks stay when longer
/// bars come; renumbering restarts and clears the overrides; undo restores.
#[test]
fn marks_name_shapes_across_hosts_and_stay_frozen() {
    let mut s = Session::default();
    let a = beam(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let b = beam(&mut s, [0.0, 3.0], [5.0, 3.0]);
    run(&mut s, "rebar_typical", json!({}));
    let marks = list(&mut s);
    // 10 mm bottom bars, 8 mm top bars, 5 mm stirrups: diameters descending.
    assert_eq!(marks.len(), 3, "{marks:#?}");
    let names: Vec<&str> = marks.iter().map(|m| m["mark"].as_str().unwrap()).collect();
    assert_eq!(names, ["N1", "N2", "N3"]);
    assert!((num(&marks[0]["diameter"]) - 0.010).abs() < 1e-12);
    assert_eq!(marks[0]["count"], json!(4), "two bars in each beam");
    assert_eq!(marks[0]["code"], json!("straight 495"));
    assert_eq!(marks[0]["sets"].as_array().unwrap().len(), 2);
    assert_eq!(marks[0]["source"], json!("numbered"));
    assert!(marks[2]["code"].as_str().unwrap().starts_with("stirrup "), "{}", marks[2]["code"]);
    let (a_bottom, a_top, a_ties) = beam_sets(&s, a);
    let (b_bottom, ..) = beam_sets(&s, b);
    assert_eq!(run(&mut s, "element_get", json!({"id": a_bottom}))["derived"]["marks"], json!(["N1"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": b_bottom}))["derived"]["marks"], json!(["N1"]));

    // 3 mm longer: within the 5 mm tolerance, the same mark.
    let c = beam(&mut s, [0.0, 6.0], [5.003, 6.0]);
    run(&mut s, "rebar_typical", json!({"elements": [c]}));
    let marks = list(&mut s);
    assert_eq!(marks.len(), 3, "{marks:#?}");
    assert_eq!(marks[0]["count"], json!(6));

    // An override names the set's bars.
    run(&mut s, "element_set", json!({"id": a_bottom, "params": {"bar_mark": "X1"}}));
    assert_eq!(run(&mut s, "element_get", json!({"id": a_bottom}))["derived"]["marks"], json!(["X1"]));
    let x1 = list(&mut s).into_iter().find(|m| m["mark"] == "X1").unwrap();
    assert_eq!((x1["count"].clone(), x1["overridden"].clone()), (json!(2), json!(true)));

    // Frozen, the marks stay when longer bars (which would come first) are added.
    let frozen = run(&mut s, "bar_marks_update", json!({}));
    assert_eq!(frozen["frozen"], json!(["N1", "N2", "N3"]), "{frozen}");
    let sets: Vec<u64> =
        s.project().elements.values().filter(|e| e.rebar_partition().is_some()).map(|e| e.id.0).collect();
    let before = marks_of(&mut s, &sets);
    let d = beam(&mut s, [0.0, 9.0], [6.0, 9.0]);
    run(&mut s, "rebar_typical", json!({"elements": [d]}));
    assert_eq!(marks_of(&mut s, &sets), before);
    let (d_bottom, d_top, d_ties) = beam_sets(&s, d);
    assert_eq!(run(&mut s, "element_get", json!({"id": d_bottom}))["derived"]["marks"], json!(["N4"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": d_top}))["derived"]["marks"], json!(["N5"]));
    // A 2 cm longer stirrup? No: stirrups keep their section, so the same mark.
    assert_eq!(run(&mut s, "element_get", json!({"id": d_ties}))["derived"]["marks"], json!(["N3"]));
    // Freezing again adds only the new marks; nothing new makes no undo step.
    assert_eq!(run(&mut s, "bar_marks_update", json!({}))["frozen"], json!(["N4", "N5"]));
    let depth = s.doc.undo_depth();
    assert_eq!(run(&mut s, "bar_marks_update", json!({}))["frozen"], json!([]));
    assert_eq!(s.doc.undo_depth(), depth);

    // Renumbered: the longest 10 mm bars first, the override cleared.
    let renumbered = run(&mut s, "bar_marks_renumber", json!({}));
    assert_eq!(renumbered["settings"]["frozen"], json!(5), "{renumbered}");
    assert_eq!(run(&mut s, "element_get", json!({"id": d_bottom}))["derived"]["marks"], json!(["N1"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": a_bottom}))["derived"]["marks"], json!(["N2"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": a_bottom}))["params"]["bar_mark"], Value::Null);
    assert_eq!(run(&mut s, "element_get", json!({"id": a_top}))["derived"]["marks"], json!(["N4"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": a_ties}))["derived"]["marks"], json!(["N5"]));
    // Undone in one step: the override and the frozen marks are back.
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "element_get", json!({"id": a_bottom}))["derived"]["marks"], json!(["X1"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": d_bottom}))["derived"]["marks"], json!(["N4"]));
    assert_eq!(s.project().bar_marking.frozen.len(), 5);
    // Another prefix and start, keeping the override.
    run(&mut s, "bar_marks_renumber", json!({"prefix": "P", "start": 10, "keep_overrides": true}));
    assert_eq!(run(&mut s, "element_get", json!({"id": d_bottom}))["derived"]["marks"], json!(["P10"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": a_bottom}))["derived"]["marks"], json!(["X1"]));
    let _ = (a_top, b_bottom);
    // Bad settings are refused.
    assert_eq!(fails(&mut s, "bar_marks_renumber", json!({"start": 5_000_000})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "bar_marking_set", json!({"stock_length": 0.2})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "bar_marking_set", json!({"length_tolerance": 1.0})), ErrorCode::InvalidArguments);
    let set = run(&mut s, "bar_marking_set", json!({"length_tolerance": 0.001, "stock_length": 10}));
    assert_eq!(set["stock_length"], json!(10.0));
}

/// Bars longer than the stock length are found, set by set and mark by
/// mark; a set that splices its bars passes.
#[test]
fn long_bars_are_found_by_set_and_mark() {
    let mut s = Session::default();
    let long = beam(&mut s, [0.0, 0.0], [15.0, 0.0]);
    let short = beam(&mut s, [0.0, 3.0], [5.0, 3.0]);
    let whole = json!({"host": long, "layout": "bottom", "count": 2, "max_length": 0, "partition": " BEAMS "});
    let whole = id(&run(&mut s, "rebar_create", whole));
    run(&mut s, "rebar_create", json!({"host": long, "layout": "top", "count": 2}));
    run(&mut s, "rebar_create", json!({"host": short, "layout": "bottom", "count": 2}));
    let check = run(&mut s, "bar_length_check", json!({}));
    assert_eq!((check["max_length"].clone(), check["count"].clone()), (json!(12.0), json!(2)), "{check}");
    let bars = check["bars"].as_array().unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!((bars[0]["set"].clone(), bars[0]["host"].clone()), (json!(whole), json!(long)));
    // Sets without a partition are numbered first: the two pieces of the top bars and the short beam's.
    assert_eq!((bars[0]["partition"].clone(), bars[0]["mark"].clone()), (json!("BEAMS"), json!("N4")));
    assert!((num(&bars[0]["longest"]) - 14.95).abs() < 1e-9);
    // A shorter limit finds the spliced pieces (two of one top bar, three of the other) and the
    // short beam's bars too; a stock of 20 m none.
    assert_eq!(run(&mut s, "bar_length_check", json!({"max_length": 4.0}))["count"], json!(2 + 5 + 2));
    run(&mut s, "bar_marking_set", json!({"stock_length": 20}));
    assert_eq!(run(&mut s, "bar_length_check", json!({}))["count"], json!(0));
    assert_eq!(fails(&mut s, "bar_length_check", json!({"max_length": 0})), ErrorCode::InvalidArguments);
    // A partition too long to keep is refused.
    let bad = json!({"host": short, "layout": "top", "partition": "x".repeat(41)});
    assert_eq!(fails(&mut s, "rebar_create", bad), ErrorCode::InvalidArguments);
}

/// The steel grade and the mass per meter come from the bar type: bars of
/// another grade take another mark, and weights use the type's mass.
#[test]
fn grades_part_marks_and_the_type_gives_the_mass() {
    let mut s = Session::default();
    let a = beam(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let ten = run(&mut s, "types_list", json!({"category": "rebar"}));
    let ten = ten.as_array().unwrap().iter().find(|t| t["name"].as_str().unwrap().contains("10")).unwrap().clone();
    let set = id(&run(&mut s, "rebar_create", json!({"host": a, "layout": "bottom", "count": 2, "type": ten["id"]})));
    let d = run(&mut s, "element_get", json!({"id": set}))["derived"].clone();
    // A round 10 mm bar of steel: 0.6165 kg/m.
    assert!((num(&d["mass_per_meter"]) - std::f64::consts::PI * 0.01 * 0.01 / 4.0 * 7850.0).abs() < 1e-9);
    run(&mut s, "type_set", json!({"type": ten["id"], "params": {"grade": "CA-50", "mass_per_meter": 0.63}}));
    let d = run(&mut s, "element_get", json!({"id": set}))["derived"].clone();
    assert!((num(&d["weight"]) - num(&d["total_length"]) * 0.63).abs() < 1e-9, "{d}");
    let marks = list(&mut s);
    assert_eq!((marks[0]["grade"].clone(), marks.len()), (json!("CA-50"), 1));
    assert!((num(&marks[0]["weight"]) - num(&d["weight"])).abs() < 1e-9);
    // The same bars in a type of another grade are another mark.
    let other = run(&mut s, "type_duplicate", json!({"type": ten["id"], "name": "Ø 10 CA-60"}));
    run(&mut s, "type_set", json!({"type": other["id"], "params": {"grade": "CA-60"}}));
    let b = beam(&mut s, [0.0, 3.0], [5.0, 3.0]);
    run(&mut s, "rebar_create", json!({"host": b, "layout": "bottom", "count": 2, "type": other["id"]}));
    let marks = list(&mut s);
    assert_eq!(marks.len(), 2, "{marks:#?}");
    // A mass that is no mass is refused.
    let bad = json!({"type": ten["id"], "params": {"mass_per_meter": -1.0}});
    assert!(s.execute("type_set", bad).is_err());
}

/// The bars of a cylinder: ring bars are arcs with their radius, and the
/// two walls' rings of one face are one mark.
#[test]
fn ring_bars_are_marked_as_arcs() {
    let mut s = Session::default();
    let a = id(&run(&mut s, "wall_create", json!({"start": [2.2, 0], "end": [-2.2, 0], "bulge": 1.0, "height": 3.0})));
    let b = id(&run(&mut s, "wall_create", json!({"start": [-2.2, 0], "end": [2.2, 0], "bulge": 1.0, "height": 3.0})));
    let args = json!({"walls": [a, b], "horizontal": {}, "faces": "exterior", "partition": "WALL", "max_length": 0});
    let sets = ids(&run(&mut s, "wall_rebar_create", args));
    let marks = list(&mut s);
    assert_eq!(marks.len(), 1, "{marks:#?}");
    let m = &marks[0];
    assert_eq!(m["sets"].as_array().unwrap().len(), 2);
    let leg = &m["shape"]["legs"][0];
    let geom = s.geometry().clone();
    let bar = &geom.rebar[&ElementId(sets[0])].bars[0];
    let radius = bar.points[0].truncate().length();
    assert!((num(&leg["radius"]) - radius).abs() < 1e-9 && (num(&leg["sweep"]).abs() - 180.0).abs() < 1e-6, "{leg}");
    assert!((num(&m["unit_length"]) - std::f64::consts::PI * radius).abs() < 1e-9);
    assert!(m["code"].as_str().unwrap().starts_with("arc R="), "{}", m["code"]);
    assert_eq!(run(&mut s, "element_get", json!({"id": sets[0]}))["params"]["partition"], json!("WALL"));
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-bar-marks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A reinforced model: beams, a wall and a slab, in partitions.
fn reinforced(s: &mut Session) {
    beam(s, [0.0, 0.0], [5.0, 0.0]);
    beam(s, [0.0, 3.0], [5.0, 3.0]);
    run(s, "rebar_typical", json!({}));
    let wall = id(&run(s, "wall_create", json!({"start": [0, 6], "end": [4, 6], "height": 3.0})));
    // Vertical bars with a 30 cm leg turned across the wall at their foot.
    let args = json!({
        "walls": [wall],
        "partition": "WALLS",
        "horizontal": {},
        "vertical": {"bottom": {"across": 0.3}},
        "ties": {"every_horizontal": 2, "every_vertical": 2}
    });
    run(s, "wall_rebar_create", args);
    let slab = id(&run(s, "floor_create", json!({"boundary": [[0, 10], [4, 10], [4, 13], [0, 13]]})));
    run(s, "slab_rebar_create", json!({"slabs": [slab], "partition": "SLAB"}));
}

/// The issue's "done when": a bending schedule of a reinforced model goes on
/// a sheet and to CSV, and its totals per diameter equal the sums of its
/// rows and of the individual bars; the waste is added; the partition
/// filter and grouping work; the drawing has the sketches.
#[test]
fn bending_schedule_adds_up_goes_on_a_sheet_and_to_csv() {
    let mut s = Session::default();
    reinforced(&mut s);
    let view = id(&run(&mut s, "bending_schedule_create", json!({"name": "Bending", "waste_percent": 10})));

    // Exact sums: rows per diameter, the totals, and the bars one by one.
    let options = BendingOptions { waste_percent: Some(10.0), ..Default::default() };
    let (p, g) = s.project_and_geometry();
    let b = cc_views::bending::bending(p, g, &options, &mut |_| true);
    assert!(b.rows.len() >= 6, "{:?}", b.rows.len());
    let mut by_bars: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
    for set in g.rebar.values() {
        let e = by_bars.entry((set.diameter * 1e6).round() as i64).or_default();
        for bar in &set.bars {
            e.0 += bar.length();
            e.1 += bar.length() * set.mass_per_meter();
        }
    }
    assert_eq!(b.totals.len(), by_bars.len());
    for t in &b.totals {
        let rows: Vec<_> =
            b.rows.iter().filter(|r| (b.marks.marks[r.mark].diameter - t.diameter).abs() < 1e-12).collect();
        let (mut length, mut weight) = (0.0, 0.0);
        for r in &rows {
            (length, weight) = (length + r.length, weight + r.weight);
        }
        assert_eq!((length, weight), (t.length, t.weight), "the rows add up to the total");
        let (l, w) = by_bars[&((t.diameter * 1e6).round() as i64)];
        assert!((l - t.length).abs() < 1e-9 * l.max(1.0) && (w - t.weight).abs() < 1e-9 * w.max(1.0), "{t:?}");
    }
    let count: usize = b.rows.iter().map(|r| r.count).sum();
    assert_eq!(count, g.rebar.values().map(|set| set.bars.len()).sum::<usize>(), "every bar is in a row");

    // The table: rows per mark with shape codes, totals, the summary with waste.
    let table = run(&mut s, "schedule_table", json!({"view": view}));
    let rows = table["rows"].as_array().unwrap();
    assert_eq!(rows.len(), b.rows.len());
    let codes: Vec<&str> = rows.iter().map(|r| r["cells"][3].as_str().unwrap()).collect();
    assert!(codes.contains(&"straight 495"), "{codes:?}");
    assert!(codes.iter().any(|c| c.starts_with("stirrup ")), "{codes:?}");
    assert!(codes.iter().any(|c| c.starts_with("L 30-")), "the walls' bent vertical bars: {codes:?}");
    let summary = &table["summary"];
    assert_eq!(summary["caption"], json!(cc_engine::i18n::tr("bending-summary")));
    let srows = summary["rows"].as_array().unwrap();
    assert_eq!(srows.len(), b.totals.len());
    for (row, t) in srows.iter().zip(&b.totals) {
        assert_eq!(row["cells"][2], json!(cc_views::format::meters(t.length)));
        let weight = format!("{} kg", cc_views::format::number(t.weight, 2));
        assert_eq!(row["cells"][3], json!(weight));
        let with_waste = format!("{} kg", cc_views::format::number(t.weight * 1.1, 2));
        assert_eq!(row["cells"][5], json!(with_waste));
    }
    let all: f64 = b.totals.iter().map(|t| t.length).sum();
    assert_eq!(summary["totals"][2], json!(cc_views::format::meters(all)));

    // On a sheet, exported; and to CSV.
    let sheet = id(&run(&mut s, "view_create", json!({"kind": "sheet"})));
    run(&mut s, "sheet_place_view", json!({"sheet": sheet, "view": view}));
    let svg = scratch("bending.svg");
    run(&mut s, "sheet_export", json!({"sheet": sheet, "path": svg}));
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains("Bending") && text.contains("N1"), "the table on the sheet");
    let csv = scratch("bending.csv");
    let out = run(&mut s, "schedule_export_csv", json!({"view": view, "path": csv}));
    assert_eq!(out["rows"], json!(b.rows.len()));
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.contains("straight 495") && text.contains(&cc_engine::i18n::tr("bending-summary")), "{text}");
    assert!(text.contains(&cc_engine::i18n::tr("col-weight-with-waste")));

    // The drawing: sketches with their dimensions in the shape column.
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let texts: Vec<&str> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"495"), "the straight bars' length: {texts:?}");
    assert!(texts.contains(&"30"), "the bent legs: {texts:?}");
    assert!(!texts.iter().any(|t| t.starts_with("straight")), "sketches, not codes");
    let lines = d.prims.iter().filter(|p| matches!(p, cc_views::Prim::Line { .. })).count();
    assert!(lines > 4 * (rows.len() + 3), "{lines}");

    // Limited to a partition, and grouped.
    let walls = id(&run(&mut s, "bending_schedule_create", json!({"partitions": ["walls"]})));
    let (p, g) = s.project_and_geometry();
    let only = BendingOptions { partitions: vec!["walls".into()], ..Default::default() };
    let w = cc_views::bending::bending(p, g, &only, &mut |_| true);
    assert!(!w.rows.is_empty() && w.rows.len() < b.rows.len());
    assert!(w.rows.iter().all(|r| p.element(r.first).and_then(|e| e.rebar_partition()) == Some("WALLS")));
    let t = run(&mut s, "schedule_table", json!({"view": walls}));
    assert_eq!(t["rows"].as_array().unwrap().len(), w.rows.len());
    let grouped = id(&run(&mut s, "bending_schedule_create", json!({"group_by_partition": true})));
    let t = run(&mut s, "schedule_table", json!({"view": grouped}));
    let headings: Vec<&str> = t["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["heading"] == json!(true))
        .map(|r| r["cells"][0].as_str().unwrap())
        .collect();
    assert_eq!(headings, ["SLAB", "WALLS", cc_engine::i18n::tr("bending-no-partition").as_str()]);
    // The options are checked, and changed like the view's other parameters.
    assert_eq!(fails(&mut s, "bending_schedule_create", json!({"waste_percent": -1})), ErrorCode::InvalidArguments);
    run(&mut s, "view_set", json!({"view": walls, "params": {"def": {"bending": {"partitions": ["SLAB"]}}}}));
    let t = run(&mut s, "schedule_table", json!({"view": walls}));
    assert!(t["rows"].as_array().unwrap().iter().all(|r| r["cells"][3] != json!("straight 495")));
    // An ordinary reinforcement schedule lists the sets with their partition.
    let plain = id(&run(&mut s, "view_create", json!({"kind": "schedule", "category": "rebar"})));
    run(&mut s, "view_set", json!({"view": plain, "params": {"def": {"fields": ["Partition", "Weight"]}}}));
    let t = run(&mut s, "schedule_table", json!({"view": plain}));
    assert!(t["rows"].as_array().unwrap().iter().any(|r| r["cells"][0] == json!("WALLS")), "{t}");
    let _ = std::fs::remove_file(svg);
    let _ = std::fs::remove_file(csv);
}

/// Shape codes of a ring and an arc: the ring bars of a round slab and the
/// horizontal bars of an arc wall.
#[test]
fn rings_and_arcs_have_their_codes() {
    let mut s = Session::default();
    let slab =
        id(&run(&mut s, "floor_create", json!({"curves": [{"kind": "circle", "center": [0, 0], "radius": 2.0}]})));
    run(&mut s, "slab_rebar_create", json!({"slabs": [slab], "layout": "radial", "layers": "top", "max_length": 0}));
    let wall = id(&run(&mut s, "wall_create", json!({"start": [10, 0], "end": [14, 0], "bulge": 0.4, "height": 3})));
    run(&mut s, "wall_rebar_create", json!({"walls": [wall], "faces": "exterior"}));
    let codes: Vec<String> = list(&mut s).iter().map(|m| m["code"].as_str().unwrap().to_string()).collect();
    assert!(codes.iter().any(|c| c.starts_with("ring R=")), "{codes:?}");
    assert!(codes.iter().any(|c| c.starts_with("arc R=")), "{codes:?}");
}

/// A tag writes its set: count, mark, diameter, spacing and length; with a
/// multiplier; the view draws the set's bars; tags go with their set.
#[test]
fn bar_tags_write_their_set_and_go_with_it() {
    let mut s = Session::default();
    let wall = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [4, 0], "height": 3.0})));
    let created = ids(&run(&mut s, "wall_rebar_create", json!({"walls": [wall], "faces": "exterior"})));
    let vertical = *created
        .iter()
        .find(|c| matches!(&s.project().element(ElementId(**c)).unwrap().kind, ElementKind::Rebar(r) if matches!(r.layout, RebarLayout::WallVertical { .. })))
        .unwrap();
    let horizontal = *created.iter().find(|c| **c != vertical).unwrap();
    let view = id(&run(&mut s, "view_create", json!({"kind": "elevation", "direction": "south"})));
    let tag = run(&mut s, "bar_tag_create", json!({"view": view, "set": vertical, "position": [1.0, 1.5]}));
    let set = run(&mut s, "element_get", json!({"id": vertical}));
    let geom = s.geometry().clone();
    let bars = &geom.rebar[&ElementId(vertical)].bars;
    assert!(bars.windows(2).all(|w| (w[0].length() - w[1].length()).abs() < 1e-9));
    let expected = format!(
        "{} {} Ø10 c/15 C={}",
        set["derived"]["count"],
        set["derived"]["marks"][0].as_str().unwrap(),
        cc_views::format::number(bars[0].length() * 100.0, 0)
    );
    assert_eq!(tag["derived"]["text"], json!(expected), "{tag}");
    let twice = run(
        &mut s,
        "bar_tag_create",
        json!({"view": view, "set": vertical, "position": [1.0, 2.0], "multiplier": 2, "leader": [1.2, 1.0]}),
    );
    assert_eq!(twice["derived"]["text"], json!(format!("2x {expected}")));
    let short = run(
        &mut s,
        "bar_tag_create",
        json!({"view": view, "set": vertical, "position": [1.0, 2.5], "parts": {"count": false, "length": false}}),
    );
    assert!(short["derived"]["text"].as_str().unwrap().starts_with(set["derived"]["marks"][0].as_str().unwrap()));
    // The elevation draws the tagged set's bars (thin lines of the set) and the texts.
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let lines = d
        .prims
        .iter()
        .filter(|p| matches!(p, cc_views::Prim::Line { element: Some(e), .. } if e.0 == vertical))
        .count();
    assert_eq!(lines, bars.len(), "one line per straight bar");
    assert!(d.prims.iter().any(|p| matches!(p, cc_views::Prim::Text { text, .. } if *text == expected)));
    // In a plan the horizontal bars run along the wall, at their spacing.
    let plan = s.project().views.values().find(|v| v.is_plan()).map(|v| v.id.0).unwrap();
    let h = run(&mut s, "bar_tag_create", json!({"view": plan, "set": horizontal, "position": [0.5, 0.5]}));
    assert!(h["derived"]["text"].as_str().unwrap().contains(" c/15 C="), "{h}");
    // Only plans, elevations and sections, and only reinforcement.
    let schedule = id(&run(&mut s, "bending_schedule_create", json!({})));
    let bad = json!({"view": schedule, "set": vertical, "position": [0, 0]});
    assert_eq!(fails(&mut s, "bar_tag_create", bad), ErrorCode::InvalidArguments);
    let bad = json!({"view": view, "set": wall, "position": [0, 0]});
    assert_eq!(fails(&mut s, "bar_tag_create", bad), ErrorCode::InvalidArguments);
    let bad = json!({"view": view, "set": vertical, "position": [0, 0], "multiplier": 0});
    assert_eq!(fails(&mut s, "bar_tag_create", bad), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "element_set", json!({"id": id(&tag), "params": {"set": wall}})),
        ErrorCode::InvalidArguments
    );
    // Deleting the set deletes its tags; undo brings them back.
    run(&mut s, "elements_delete", json!({"ids": [vertical]}));
    assert!(s.project().element(ElementId(id(&tag))).is_none() && s.project().element(ElementId(id(&twice))).is_none());
    assert!(s.project().element(ElementId(id(&h))).is_some());
    run(&mut s, "undo", json!({}));
    assert!(s.project().element(ElementId(id(&tag))).is_some());
    // Deleting the wall takes its sets and their tags.
    run(&mut s, "elements_delete", json!({"ids": [wall]}));
    assert!(s.project().element(ElementId(id(&h))).is_none());
}

/// Crafted marking settings, frozen entries and tags are repaired on load.
#[test]
fn crafted_marking_and_tags_are_repaired() {
    let mut s = Session::default();
    let a = beam(&mut s, [0.0, 0.0], [5.0, 0.0]);
    run(&mut s, "rebar_typical", json!({}));
    run(&mut s, "bar_marks_update", json!({}));
    let (bottom, ..) = beam_sets(&s, a);
    let plan = s.project().views.values().find(|v| v.is_plan()).map(|v| v.id.0).unwrap();
    let tag = id(&run(&mut s, "bar_tag_create", json!({"view": plan, "set": bottom, "position": [1, 1]})));
    let mut p = s.project().clone();
    p.bar_marking.stock_length = -1.0;
    p.bar_marking.length_tolerance = 3.0;
    p.bar_marking.prefix = "x".repeat(100);
    p.bar_marking.frozen[0].shape.legs[0].length = -1.0;
    p.bar_marking.frozen[1].diameter = 0.0;
    let mut stray = p.element(ElementId(tag)).unwrap().clone();
    stray.id = ElementId(p.alloc_id());
    if let ElementKind::BarTag(t) = &mut stray.kind {
        t.set = ElementId(424_242);
    }
    p.elements.insert(stray.id, stray);
    if let Some(ElementKind::BarTag(t)) = p.elements.get_mut(&ElementId(tag)).map(|e| &mut e.kind) {
        t.multiplier = 0;
    }
    let text = cc_core::to_ron(&p).unwrap();
    let (q, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    // Stock length, tolerance, prefix, two frozen entries, the stray tag, the multiplier.
    assert_eq!(repairs, 7, "{:?}", q.bar_marking);
    assert_eq!((q.bar_marking.stock_length, q.bar_marking.length_tolerance), (12.0, 0.005));
    assert_eq!(q.bar_marking.frozen.len(), 1);
    assert_eq!(q.elements.values().filter(|e| matches!(e.kind, ElementKind::BarTag(_))).count(), 1);
    let ElementKind::BarTag(t) = &q.element(ElementId(tag)).unwrap().kind else { panic!() };
    assert_eq!(t.multiplier, 1);
    // A project without marking settings writes none.
    let plain = cc_core::to_ron(&cc_core::Project::from_template()).unwrap();
    assert!(!plain.contains("bar_marking"));
}

/// Bars longer than the stock length need splices: listed by set and mark
/// until the set cuts them; the bending schedule notes them.
#[test]
fn long_bars_are_flagged_until_spliced() {
    let mut s = Session::default();
    let b = beam(&mut s, [0.0, 0.0], [14.0, 0.0]);
    let set = id(&run(&mut s, "rebar_create", json!({"host": b, "layout": "bottom", "max_length": 0})));
    let check = run(&mut s, "bar_length_check", json!({}));
    assert_eq!(check["max_length"], json!(12.0));
    assert_eq!(check["count"], json!(2), "{check}");
    let bar = &check["bars"][0];
    assert_eq!((bar["set"].clone(), bar["host"].clone(), bar["mark"].clone()), (json!(set), json!(b), json!("N1")));
    assert!((num(&bar["longest"]) - 13.95).abs() < 1e-9);
    assert_eq!(run(&mut s, "bar_length_check", json!({"max_length": 14}))["count"], json!(0));
    let view = id(&run(&mut s, "bending_schedule_create", json!({})));
    let table = run(&mut s, "schedule_table", json!({"view": view}));
    assert_eq!(table["headers"].as_array().unwrap().last().unwrap(), &json!(cc_engine::i18n::tr("col-note")));
    assert_eq!(table["rows"][0]["cells"][8], json!(cc_engine::i18n::tr("bending-over-stock")));
    // Spliced at 12 m: nothing to flag, and no note.
    run(&mut s, "element_set", json!({"id": set, "params": {"splice": {"max_length": 12.0, "lap": 0.4}}}));
    assert_eq!(run(&mut s, "bar_length_check", json!({}))["count"], json!(0));
    let table = run(&mut s, "schedule_table", json!({"view": view}));
    assert_eq!(table["headers"].as_array().unwrap().len(), 8);
    // A shorter stock length flags the pieces again.
    run(&mut s, "bar_marking_set", json!({"stock_length": 6.0}));
    assert!(run(&mut s, "bar_length_check", json!({}))["count"].as_u64().unwrap() >= 2);
}

/// A set's partition, the mark given by hand and its type's steel grade go
/// through IFC: the file says them in `ConstructCAD_Rebar` and the import
/// reads them back, so the bars keep their mark and their part of the structure.
#[test]
fn partition_mark_and_grade_go_through_ifc() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [5, 0], "top_offset": 3.0})));
    let args = json!({"host": host, "layout": "bottom", "count": 2, "partition": "BEAMS"});
    let named = id(&run(&mut s, "rebar_create", args));
    let plain = id(&run(&mut s, "rebar_create", json!({"host": host, "layout": "top", "count": 2})));
    run(&mut s, "element_set", json!({"id": named, "params": {"bar_mark": "X1"}}));
    let ty = s.project().element(cc_core::ElementId(named)).unwrap().type_id.0;
    run(&mut s, "type_set", json!({"type": ty, "params": {"grade": "CA-50"}}));
    let path = scratch("marks.ifc");
    run(&mut s, "ifc_export", json!({"path": path}));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("ConstructCAD_Rebar"), "the set's own property set");
    for value in ["'Partition'", "'BEAMS'", "'BarMark'", "'X1'", "'Grade'", "'CA-50'"] {
        assert!(text.contains(value), "{value}");
    }
    // The set without a hand-given mark writes the mark its bars have, for other programs to read.
    assert!(text.contains("'Mark'") && text.contains("'N1'"), "the numbered mark");
    let _ = plain;

    let mut back = Session::default();
    run(&mut back, "ifc_import", json!({"path": path}));
    let bars: Vec<&cc_core::Element> =
        back.project().elements.values().filter(|e| e.rebar_partition().is_some()).collect();
    assert_eq!(bars.len(), 2);
    let kept = bars.iter().find(|e| e.rebar_partition() == Some("BEAMS")).expect("the partition");
    assert_eq!(kept.rebar_bar_mark(), Some("X1"));
    let other = bars.iter().find(|e| e.rebar_partition() == Some("")).expect("the set without one");
    assert_eq!(other.rebar_bar_mark(), None);
    let grades: Vec<String> = bars
        .iter()
        .filter_map(|e| match back.project().element_type(e.type_id).map(|t| &t.data) {
            Some(cc_core::TypeData::Rebar { grade, .. }) => Some(grade.clone()),
            _ => None,
        })
        .collect();
    assert!(grades.iter().all(|g| g == "CA-50"), "{grades:?}");
    // Written again, the file says each thing once: the set is not kept as an imported one too.
    let again = scratch("marks-again.ifc");
    run(&mut back, "ifc_export", json!({"path": again}));
    let text = std::fs::read_to_string(&again).unwrap();
    assert_eq!(text.matches("ConstructCAD_Rebar").count(), 2, "one set per reinforcement element");
    let marks = run(&mut back, "bar_marks_list", json!({}));
    let names: Vec<&str> = marks["marks"].as_array().unwrap().iter().map(|m| m["mark"].as_str().unwrap()).collect();
    assert!(names.contains(&"X1"), "{names:?}");
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(again);
}
