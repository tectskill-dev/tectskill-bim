//! Schedule definitions (`schedule_get`, `schedule_set`): filters with
//! comparisons, sorting in levels with headings and footers, rows that stand
//! for several elements, column formats and the table's look on paper.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

/// Walls 2, 4 and 4 m long on the first level and 3 and 5 m on the second,
/// with a schedule of their mark, level, length and count.
fn walls(s: &mut Session) -> (u64, [String; 2]) {
    let levels = run(s, "levels_list", json!({}));
    let levels: Vec<(u64, String)> = levels
        .as_array()
        .unwrap()
        .iter()
        .take(2)
        .map(|l| (l["id"].as_u64().unwrap(), l["name"].as_str().unwrap().to_string()))
        .collect();
    assert_eq!(levels.len(), 2, "the template has two levels");
    for (k, (level, lengths)) in [(levels[0].0, vec![2.0, 4.0, 4.0]), (levels[1].0, vec![3.0, 5.0])].iter().enumerate()
    {
        for (i, length) in lengths.iter().enumerate() {
            let y = 10.0 * k as f64 + 2.0 * i as f64;
            run(s, "wall_create", json!({"start": [0, y], "end": [*length, y], "level": level}));
        }
    }
    let view = id(&run(s, "view_create", json!({"kind": "schedule", "category": "wall"})));
    run(s, "schedule_set", json!({"view": view, "fields": ["Mark", "Level", "Length", "Count"]}));
    (view, [levels[0].1.clone(), levels[1].1.clone()])
}

/// The rows of a schedule: `(first cell, whether heading, whether footer, cells)`.
fn rows(s: &mut Session, view: u64) -> Vec<(bool, bool, Vec<String>)> {
    let table = run(s, "schedule_table", json!({"view": view}));
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let cells = r["cells"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
            (r["heading"] == json!(true), r["footer"] == json!(true), cells)
        })
        .collect()
}

fn lengths(rows: &[(bool, bool, Vec<String>)]) -> Vec<String> {
    rows.iter().filter(|r| !r.0 && !r.1).map(|r| r.2[2].clone()).collect()
}

#[test]
fn filters_compare_numbers_and_texts() {
    let mut s = Session::default();
    let (view, levels) = walls(&mut s);
    let m = |v: f64| cc_views::format::meters(v);
    assert_eq!(rows(&mut s, view).len(), 5);
    // A number compares as a number, in meters.
    run(&mut s, "schedule_set", json!({"view": view, "filters": [{"field": "Length", "op": "greater", "value": "3"}]}));
    let mut got = lengths(&rows(&mut s, view));
    got.sort();
    assert_eq!(got, [m(4.0), m(4.0), m(5.0)]);
    // Two filters: both must pass. A text compares without case.
    let both = json!([
        {"field": "Length", "op": "greater_or_equal", "value": "3"},
        {"field": "Level", "op": "equals", "value": levels[1].to_uppercase()},
    ]);
    run(&mut s, "schedule_set", json!({"view": view, "filters": both}));
    let mut got = lengths(&rows(&mut s, view));
    got.sort();
    assert_eq!(got, [m(3.0), m(5.0)]);
    // A field that is not shown filters too, and no filters shows every row again.
    run(&mut s, "schedule_set", json!({"view": view, "fields": ["Mark", "Level", "Length"]}));
    let hidden = json!([{"field": "Height", "op": "has_value"}, {"field": "Comments", "op": "has_no_value"}]);
    run(&mut s, "schedule_set", json!({"view": view, "filters": hidden}));
    assert_eq!(rows(&mut s, view).len(), 5);
    run(&mut s, "schedule_set", json!({"view": view, "filters": []}));
    assert_eq!(run(&mut s, "schedule_get", json!({"view": view}))["filters"], json!([]));
    // The older single filter (a text the value contains) still filters, and reads as the first filter.
    run(&mut s, "view_set", json!({"view": view, "params": {"def": {"filter": ["Level", levels[0]]}}}));
    assert_eq!(rows(&mut s, view).len(), 3);
    let got = run(&mut s, "schedule_get", json!({"view": view}));
    assert_eq!(got["filters"][0]["op"], json!("contains"), "{got}");
    // Refusals: an unknown field or comparison, a field of another category, too many filters.
    let bad = json!({"view": view, "filters": [{"field": "Nope", "op": "equals", "value": "1"}]});
    assert_eq!(fails(&mut s, "schedule_set", bad), ErrorCode::InvalidArguments);
    let bad = json!({"view": view, "filters": [{"field": "Length", "op": "near", "value": "1"}]});
    assert_eq!(fails(&mut s, "schedule_set", bad), ErrorCode::InvalidArguments);
    let bad = json!({"view": view, "filters": [{"field": "Sill", "op": "has_value"}]});
    assert_eq!(fails(&mut s, "schedule_set", bad), ErrorCode::InvalidArguments);
    let many: Vec<Value> = (0..9).map(|_| json!({"field": "Mark", "op": "has_value"})).collect();
    assert_eq!(fails(&mut s, "schedule_set", json!({"view": view, "filters": many})), ErrorCode::InvalidArguments);
    let plan = s.project().views.values().find(|v| v.is_plan()).map(|v| v.id.0).unwrap();
    assert_eq!(fails(&mut s, "schedule_get", json!({"view": plan})), ErrorCode::InvalidArguments);
}

#[test]
fn sorting_runs_in_levels_with_headings_and_footers() {
    let mut s = Session::default();
    let (view, levels) = walls(&mut s);
    let m = |v: f64| cc_views::format::meters(v);
    // The first level first, whichever way their names sort.
    let desc = levels[0] > levels[1];
    // By level, then by length from the longest.
    let sorting = json!([{"field": "Level", "descending": desc}, {"field": "Length", "descending": true}]);
    run(&mut s, "schedule_set", json!({"view": view, "sorting": sorting}));
    let got = rows(&mut s, view);
    let by_level =
        |name: &str| -> Vec<String> { got.iter().filter(|r| r.2[1] == name).map(|r| r.2[2].clone()).collect() };
    assert_eq!(by_level(&levels[0]), [m(4.0), m(4.0), m(2.0)]);
    assert_eq!(by_level(&levels[1]), [m(5.0), m(3.0)]);
    // The rows of a level come together.
    let order: Vec<&str> = got.iter().map(|r| r.2[1].as_str()).collect();
    assert!(order.windows(2).filter(|w| w[0] != w[1]).count() == 1, "{order:?}");

    // A heading before each level, a footer with its count and total length after it, a blank line between.
    let level = json!({"field": "Level", "descending": desc, "header": true, "footer": true, "blank_line": true});
    let grouped = json!([level, {"field": "Length"}]);
    run(&mut s, "schedule_set", json!({"view": view, "sorting": grouped}));
    let got = rows(&mut s, view);
    let kinds: String = got
        .iter()
        .map(|r| {
            if r.0 {
                'h'
            } else if r.1 {
                'f'
            } else {
                '.'
            }
        })
        .collect();
    // Heading, three walls, footer, blank line, heading, two walls, footer.
    assert_eq!(kinds, "h...fhh..f", "{got:?}");
    assert_eq!(got[0].2[0], levels[0]);
    assert_eq!((got[4].2[0].as_str(), got[4].2[2].clone()), ("3", m(10.0)));
    assert!(got[5].2.iter().all(String::is_empty), "the blank line");
    assert_eq!(got[6].2[0], levels[1]);
    assert_eq!((got[9].2[0].as_str(), got[9].2[2].clone()), ("2", m(8.0)));
    // The totals still count every wall once.
    let table = run(&mut s, "schedule_table", json!({"view": view}));
    assert_eq!((table["totals"][0].clone(), table["totals"][2].clone()), (json!("5"), json!(m(18.0))));

    // The older single sort field still sorts, and reads as the first level.
    let old = json!({"def": {"sorting": [], "sort_by": "Length", "descending": true}});
    run(&mut s, "view_set", json!({"view": view, "params": old}));
    assert_eq!(lengths(&rows(&mut s, view)), [m(5.0), m(4.0), m(4.0), m(3.0), m(2.0)]);
    let got = run(&mut s, "schedule_get", json!({"view": view}));
    assert_eq!(
        got["sorting"],
        json!([{"field": "Length", "descending": true, "header": false, "footer": false, "blank_line": false}])
    );
    // At most four levels.
    let many: Vec<Value> = (0..5).map(|_| json!({"field": "Mark"})).collect();
    assert_eq!(fails(&mut s, "schedule_set", json!({"view": view, "sorting": many})), ErrorCode::InvalidArguments);
}

#[test]
fn rows_may_stand_for_the_elements_alike() {
    let mut s = Session::default();
    let (view, levels) = walls(&mut s);
    let m = |v: f64| cc_views::format::meters(v);
    let depth = s.doc.undo_depth();
    // One row per level: lengths add up, the count says how many.
    run(&mut s, "schedule_set", json!({"view": view, "sorting": [{"field": "Level"}], "itemize": false}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let got = rows(&mut s, view);
    assert_eq!(got.len(), 2, "{got:?}");
    let varies = cc_engine::i18n::tr("schedule-varies");
    let row = got.iter().find(|r| r.2[1] == levels[0]).unwrap();
    assert_eq!(row.2[1..], [levels[0].clone(), m(10.0), "3".to_string()]);
    let row = got.iter().find(|r| r.2[1] == levels[1]).unwrap();
    assert_eq!(row.2[2..], [m(8.0), "2".to_string()]);
    // The totals count the walls, not the rows.
    let table = run(&mut s, "schedule_table", json!({"view": view}));
    assert_eq!((table["totals"][0].clone(), table["totals"][3].clone()), (json!("5"), json!("5")));
    // A length that is not totaled reads as varying where the row's walls differ in it.
    let columns = json!([{"field": "Length", "total": false}]);
    run(&mut s, "schedule_set", json!({"view": view, "columns": columns}));
    let got = rows(&mut s, view);
    assert!(got.iter().all(|r| r.2[2] == varies), "{got:?}");
    // By level and length: the two 4 m walls share a row, with the length they share.
    let sorting = json!([{"field": "Level"}, {"field": "Length"}]);
    run(&mut s, "schedule_set", json!({"view": view, "sorting": sorting}));
    let got = rows(&mut s, view);
    assert_eq!(got.len(), 4);
    let pair = got.iter().find(|r| r.2[3] == "2").expect("the row of two walls");
    assert_eq!(pair.2[2], m(4.0));
    assert_eq!(table_totals(&mut s, view)[2], "", "the length is not totaled");
    // With no sorting every element is alike: one row.
    run(&mut s, "schedule_set", json!({"view": view, "sorting": []}));
    assert_eq!(rows(&mut s, view).len(), 1);
    run(&mut s, "schedule_set", json!({"view": view, "itemize": true}));
    assert_eq!(rows(&mut s, view).len(), 5);
}

fn table_totals(s: &mut Session, view: u64) -> Vec<String> {
    run(s, "schedule_table", json!({"view": view}))["totals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn columns_take_headings_alignment_and_totals() {
    let mut s = Session::default();
    let (view, _) = walls(&mut s);
    let columns = json!([
        {"field": "Length", "heading": "  Run  ", "align": "Center"},
        {"field": "Level", "hidden": true},
        {"field": "Count", "total": false},
    ]);
    run(&mut s, "schedule_set", json!({"view": view, "columns": columns, "sorting": [{"field": "Level"}]}));
    let table = run(&mut s, "schedule_table", json!({"view": view}));
    // The level is hidden (and still sorts); the length has its own heading.
    let headers: Vec<&str> = table["headers"].as_array().unwrap().iter().map(|h| h.as_str().unwrap()).collect();
    assert_eq!(headers.len(), 3);
    assert_eq!(headers[1], "Run");
    assert_eq!(table["totals"][2], json!(""), "the count is not totaled");
    let got = run(&mut s, "schedule_get", json!({"view": view}));
    let length = got["columns"].as_array().unwrap().iter().find(|c| c["field"] == "Length").unwrap().clone();
    assert_eq!((length["heading"].clone(), length["align"].clone()), (json!("Run"), json!("Center")));
    // The CSV writes the heading; the drawing centers the column's texts.
    let csv = std::env::temp_dir().join(format!("ccad-schedule-def-{}.csv", std::process::id()));
    run(&mut s, "schedule_export_csv", json!({"view": view, "path": csv}));
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.lines().next().unwrap().contains("Run"), "{text}");
    let _ = std::fs::remove_file(csv);
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let centered = d
        .prims
        .iter()
        .filter(|p| matches!(p, cc_views::Prim::Text { anchor: cc_views::Anchor::CenterBottom, .. }))
        .count();
    // The title, and the column's heading, five lengths and total.
    assert_eq!(centered, 1 + 1 + 5 + 1);
    // A format that says nothing is dropped; another format leaves the others alone; `[]` clears them.
    run(&mut s, "schedule_set", json!({"view": view, "columns": [{"field": "Level", "hidden": false}]}));
    assert_eq!(run(&mut s, "schedule_table", json!({"view": view}))["headers"].as_array().unwrap().len(), 4);
    let ViewKindDef(def) = def_of(&s, view);
    assert_eq!(def.columns.len(), 2, "{:?}", def.columns);
    run(&mut s, "schedule_set", json!({"view": view, "columns": []}));
    let ViewKindDef(def) = def_of(&s, view);
    assert!(def.columns.is_empty());
    // Every field hidden shows them all rather than an empty table.
    let all: Vec<Value> =
        ["Mark", "Level", "Length", "Count"].iter().map(|f| json!({"field": f, "hidden": true})).collect();
    run(&mut s, "schedule_set", json!({"view": view, "columns": all}));
    assert_eq!(run(&mut s, "schedule_table", json!({"view": view}))["headers"].as_array().unwrap().len(), 4);
    // Refusals: a long heading, an unknown alignment, a field twice.
    let bad = json!({"view": view, "columns": [{"field": "Mark", "heading": "x".repeat(200)}]});
    assert_eq!(fails(&mut s, "schedule_set", bad), ErrorCode::InvalidArguments);
    let bad = json!({"view": view, "columns": [{"field": "Mark", "align": "Up"}]});
    assert_eq!(fails(&mut s, "schedule_set", bad), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "schedule_set", json!({"view": view, "fields": ["Mark", "Mark"]})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "schedule_set", json!({"view": view, "fields": []})), ErrorCode::InvalidArguments);
}

struct ViewKindDef(cc_core::ScheduleDef);

fn def_of(s: &Session, view: u64) -> ViewKindDef {
    match &s.project().view(cc_core::ViewId(view)).unwrap().kind {
        cc_core::ViewKind::Schedule { def, .. } => ViewKindDef(def.clone()),
        _ => panic!("a schedule"),
    }
}

#[test]
fn the_look_on_paper_is_the_schedules_own() {
    let mut s = Session::default();
    let (view, _) = walls(&mut s);
    let count = |s: &mut Session, what: fn(&cc_views::Prim) -> bool| {
        s.drawing(cc_core::ViewId(view)).unwrap().prims.iter().filter(|p| what(p)).count()
    };
    let is_line = |p: &cc_views::Prim| matches!(p, cc_views::Prim::Line { .. });
    let is_text = |p: &cc_views::Prim| matches!(p, cc_views::Prim::Text { .. });
    let (lines, texts) = (count(&mut s, is_line), count(&mut s, is_text));
    let height = |s: &mut Session| {
        let (lo, hi) = s.drawing(cc_core::ViewId(view)).unwrap().bounds().unwrap();
        hi.y - lo.y
    };
    let before = height(&mut s);
    // Without the title and the headers: two rows and their texts less.
    run(&mut s, "schedule_set", json!({"view": view, "appearance": {"show_title": false, "show_headers": false}}));
    assert_eq!(count(&mut s, is_text), texts - 1 - 4);
    assert!(height(&mut s) < before - 1e-9);
    // Without rules between rows and columns, and without the frame.
    run(
        &mut s,
        "schedule_set",
        json!({"view": view, "appearance": {"show_title": true, "show_headers": true, "grid_lines": false}}),
    );
    let plain = count(&mut s, is_line);
    assert!(plain < lines && plain > 0, "{plain} of {lines}");
    run(&mut s, "schedule_set", json!({"view": view, "appearance": {"outline": false}}));
    assert!(count(&mut s, is_line) < plain);
    // Larger text makes taller rows; the other values stay as they were.
    run(
        &mut s,
        "schedule_set",
        json!({"view": view, "appearance": {"grid_lines": true, "outline": true, "text_size": 5.0}}),
    );
    assert!(height(&mut s) > before + 1e-9);
    let got = run(&mut s, "schedule_get", json!({"view": view}))["appearance"].clone();
    assert_eq!(
        (got["text_size"].clone(), got["title_size"].clone(), got["grid_lines"].clone()),
        (json!(5.0), json!(3.0), json!(true))
    );
    // Sizes out of range are refused, here and through the view's parameters.
    let bad = json!({"view": view, "appearance": {"text_size": 0.1}});
    assert_eq!(fails(&mut s, "schedule_set", bad), ErrorCode::InvalidArguments);
    let bad = json!({"view": view, "params": {"def": {"appearance": {"title_size": 500.0}}}});
    assert_eq!(fails(&mut s, "view_set", bad), ErrorCode::InvalidArguments);
    // The definition is saved with the project, and a crafted one is repaired on load.
    let mut p = s.project().clone();
    if let Some(cc_core::ViewKind::Schedule { def, .. }) = p.views.get_mut(&cc_core::ViewId(view)).map(|v| &mut v.kind)
    {
        def.appearance.text_size = f64::NAN;
        def.sorting = vec![cc_core::SortLevel::new(cc_core::ScheduleField::Mark); 9];
    }
    let text = cc_core::to_ron(&p).unwrap();
    let (back, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert_eq!(repairs, 2);
    let cc_core::ViewKind::Schedule { def, .. } = &back.view(cc_core::ViewId(view)).unwrap().kind else { panic!() };
    assert_eq!((def.sorting.len(), def.appearance.text_size), (4, 2.2));
}
