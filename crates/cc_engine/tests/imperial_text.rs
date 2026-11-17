//! What an imperial project shows: after creating one of each thing the
//! commands make on demand, no type name, schedule cell or header, drawing
//! text or check message states a metric unit. The same scene in a metric
//! project still does (so the scan finds what it looks for), and a few of
//! its texts are pinned.

use cc_core::{Category, ElementTypeId, ScheduleDef, ScheduleField, ViewId, ViewKind};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

/// A length in feet (`f`) and inches (`i`), meters.
fn ft(f: f64, i: f64) -> f64 {
    f * cc_core::units::FOOT + i * cc_core::units::INCH
}

fn p(x: f64, y: f64) -> Value {
    json!([ft(x, 0.0), ft(y, 0.0)])
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([
        [ft(x0, 0.0), ft(y0, 0.0)],
        [ft(x1, 0.0), ft(y0, 0.0)],
        [ft(x1, 0.0), ft(y1, 0.0)],
        [ft(x0, 0.0), ft(y1, 0.0)]
    ])
}

/// A project of `template` with one of each thing reachable by commands,
/// and the outputs of the checks and lists that report on them.
fn scene(template: &str) -> (Session, Vec<(String, String)>) {
    let mut s = Session::default();
    run(&mut s, "project_new", json!({ "template": template }));
    let mut said: Vec<(String, String)> = Vec::new();
    let keep = |source: &str, v: &Value, said: &mut Vec<(String, String)>| {
        strings(v, &mut |text| said.push((source.to_string(), text.to_string())));
    };

    // The building: walls, a floor, a roof, a room, openings.
    let corners = [(0.0, 0.0), (30.0, 0.0), (30.0, 20.0), (0.0, 20.0)];
    let mut walls = Vec::new();
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        walls.push(id(&run(&mut s, "wall_create", json!({ "start": p(a.0, a.1), "end": p(b.0, b.1) }))));
    }
    run(&mut s, "floor_create", json!({ "from_walls": true }));
    let roof = id(&run(&mut s, "roof_create", json!({ "from_walls": true })));
    run(&mut s, "room_create", json!({ "point": p(15.0, 10.0) }));
    run(&mut s, "door_create", json!({ "wall": walls[0], "point": p(8.0, 0.0) }));
    run(&mut s, "window_create", json!({ "wall": walls[0], "point": p(20.0, 0.0) }));
    let door_type = id(&run(&mut s, "door_type_create", json!({})));
    let french = id(&run(&mut s, "door_type_create", json!({ "style": "french", "leaves": 2 })));
    run(&mut s, "door_create", json!({ "wall": walls[2], "point": p(10.0, 20.0), "type": door_type }));
    run(&mut s, "door_create", json!({ "wall": walls[2], "point": p(20.0, 20.0), "type": french }));
    let curtain = id(&run(&mut s, "curtain_type_create", json!({ "name": "Storefront" })));
    run(&mut s, "wall_create", json!({ "start": p(0.0, 30.0), "end": p(20.0, 30.0), "type": curtain }));

    // Structure.
    let column = id(&run(&mut s, "column_create", json!({ "position": p(10.0, 10.0) })));
    let beam = id(&run(&mut s, "beam_create", json!({ "start": p(0.0, 10.0), "end": p(10.0, 10.0), "level": 2 })));
    run(&mut s, "footing_create", json!({ "position": p(10.0, 10.0) }));
    run(&mut s, "rebar_create", json!({ "host": beam, "layout": "bottom" }));
    run(&mut s, "rebar_create", json!({ "host": column, "layout": "stirrups" }));
    run(&mut s, "grid_create", json!({ "start": p(-5.0, 0.0), "end": p(35.0, 0.0) }));

    // Circulation.
    let stair = id(&run(&mut s, "stair_create", json!({ "start": p(25.0, 3.0), "direction": 90 })));
    let ramp =
        id(&run(&mut s, "ramp_create", json!({ "start": p(40.0, 0.0), "direction": 90, "height": ft(2.0, 0.0) })));
    run(&mut s, "railing_create", json!({ "path": [p(0.0, 25.0), p(10.0, 25.0)] }));
    run(&mut s, "sweep_create", json!({ "path": [p(1.0, 1.0), p(29.0, 1.0)] }));
    run(&mut s, "shaft_create", json!({ "outline": rect(2.0, 15.0, 4.0, 17.0) }));
    keep("stair_check", &run(&mut s, "stair_check", json!({ "id": stair })), &mut said);
    // A steep stair and a steep ramp break their rules: the messages too.
    let steep = id(&run(&mut s, "stair_create", json!({ "start": p(60.0, 40.0), "risers": 8, "tread": ft(0.0, 8.0) })));
    keep("stair_check", &run(&mut s, "stair_check", json!({ "id": steep })), &mut said);
    let steep = id(&run(&mut s, "ramp_create", json!({ "start": p(70.0, 40.0), "slope": 20, "height": ft(4.0, 0.0) })));
    keep("ramp_check", &run(&mut s, "ramp_check", json!({ "id": steep })), &mut said);
    keep("ramp_check", &run(&mut s, "ramp_check", json!({ "id": ramp })), &mut said);
    keep("roof_check", &run(&mut s, "roof_check", json!({ "id": roof })), &mut said);

    // Massing and site.
    run(&mut s, "mass_create", json!({ "outline": rect(50.0, 0.0, 80.0, 30.0), "height": ft(25.0, 0.0) }));
    run(
        &mut s,
        "topography_create",
        json!({
            "boundary": rect(-50.0, -50.0, 150.0, 100.0),
            "points": [[0, 0, 0], [ft(100.0, 0.0), 0, ft(5.0, 0.0)], [0, ft(80.0, 0.0), ft(2.0, 0.0)], [ft(100.0, 0.0), ft(80.0, 0.0), ft(8.0, 0.0)]],
        }),
    );
    run(&mut s, "pad_create", json!({ "elevation": ft(3.0, 0.0), "outline": rect(90.0, 10.0, 120.0, 40.0) }));

    // Plumbing and electrical.
    let water = id(&run(&mut s, "system_create", json!({ "kind": "cold_water", "code": "CW" })));
    let pipe = id(&run(&mut s, "pipe_create", json!({ "start": p(2.0, 2.0), "end": p(12.0, 2.0), "system": water })));
    run(&mut s, "valve_place", json!({ "pipe": pipe }));
    let panel_type = id(&run(&mut s, "device_type_create", json!({ "device": "panel" })));
    run(&mut s, "component_place", json!({ "position": p(1.0, 5.0), "type": panel_type }));
    let circuit = id(&run(&mut s, "system_create", json!({ "kind": "power", "code": "1" })));
    let outlet_type = id(&run(&mut s, "device_type_create", json!({ "device": "outlet" })));
    run(&mut s, "component_place", json!({ "position": p(5.0, 1.0), "type": outlet_type, "system": circuit }));
    run(&mut s, "conduit_create", json!({ "start": p(1.0, 5.0), "end": p(5.0, 1.0), "circuits": [circuit] }));
    run(&mut s, "cable_tray_create", json!({ "start": p(2.0, 8.0), "end": p(20.0, 8.0) }));
    keep("circuit_loads", &run(&mut s, "circuit_loads", json!({})), &mut said);
    keep("mep_sizing", &run(&mut s, "mep_sizing", json!({})), &mut said);

    // Cabling and lightning protection.
    let rack = run(&mut s, "rack_place", json!({ "position": p(28.0, 18.0) }));
    keep("rack_place", &rack, &mut said);
    run(&mut s, "data_outlet_place", json!({ "position": p(15.0, 19.0) }));
    keep("cabling_list", &run(&mut s, "cabling_list", json!({})), &mut said);
    run(&mut s, "air_terminal_place", json!({ "position": p(15.0, 10.0) }));
    run(&mut s, "lps_conductor_create", json!({ "start": p(0.0, 0.0), "end_offset": ft(-1.0, 0.0), "role": "down" }));
    run(&mut s, "earth_electrode_place", json!({ "position": p(-2.0, -2.0) }));
    keep("lps_check", &run(&mut s, "lps_check", json!({})), &mut said);

    // Types made by their commands, named by their defaults.
    for (command, args) in [
        ("lps_conductor_type_create", json!({})),
        ("cable_tray_type_create", json!({})),
        ("ramp_type_create", json!({})),
        ("sweep_type_create", json!({})),
        ("topography_type_create", json!({})),
        ("mass_type_create", json!({})),
        ("level_mark_type_create", json!({})),
        (
            "profile_create",
            json!({ "material": "steel", "shape": "i", "params": { "d": ft(0.0, 8.0), "bf": ft(0.0, 8.0), "tw": ft(0.0, 0.25), "tf": ft(0.0, 0.375) } }),
        ),
    ] {
        keep(command, &run(&mut s, command, args), &mut said);
    }

    // Annotation in the first plan.
    let plan = first_plan(&s);
    run(&mut s, "level_mark_create", json!({ "view": plan.0, "position": p(15.0, 10.0) }));
    run(&mut s, "text_create", json!({ "view": plan.0, "position": p(5.0, 5.0), "text": "Note" }));
    run(&mut s, "dimension_create", json!({ "view": plan.0, "a": p(0.0, -3.0), "b": p(30.0, -3.0) }));
    (s, said)
}

fn first_plan(s: &Session) -> ViewId {
    s.project().views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { mep: None, .. })).expect("a plan").id
}

/// Every string inside a JSON value.
fn strings(v: &Value, out: &mut dyn FnMut(&str)) {
    match v {
        Value::String(s) => out(s),
        Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
        Value::Object(map) => map.values().for_each(|i| strings(i, out)),
        _ => {}
    }
}

/// Everything the project shows: type names, every schedule (all fields)
/// as CSV, the texts of every drawing and of the views made for the scan
/// (MEP plans, a section, an elevation, a legend, the panel diagram), and
/// `said` (check and list outputs).
fn shown(s: &mut Session, said: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out = said;
    let types: Vec<(ElementTypeId, String)> = s.project().types.values().map(|t| (t.id, t.name.clone())).collect();
    for (tid, name) in &types {
        out.push((format!("type {}", tid.0), name.clone()));
        let got = run(s, "type_get", json!({ "type": tid.0 }));
        strings(&got, &mut |text| out.push((format!("type_get {}", tid.0), text.to_string())));
    }
    for kind in
        ["plumbing_plan", "electrical_plan", "communications_plan", "lightning_plan", "structural_plan", "legend"]
    {
        run(s, "view_create", json!({ "kind": kind }));
    }
    run(s, "view_create", json!({ "kind": "section", "start": p(-5.0, 10.0), "end": p(35.0, 10.0) }));
    run(s, "view_create", json!({ "kind": "elevation", "direction": "south" }));
    run(s, "view_create", json!({ "kind": "panel_diagram" }));
    let views: Vec<(ViewId, ViewKind)> = s.project().views.values().map(|v| (v.id, v.kind.clone())).collect();
    for (view, kind) in views {
        match kind {
            ViewKind::Schedule { .. } | ViewKind::ThreeD { .. } => {}
            ViewKind::Sheet { .. } => {
                if let Some(sheet) = s.sheet(view) {
                    texts(&sheet.drawing, &format!("sheet {}", view.0), &mut out);
                }
            }
            _ => {
                if let Some(d) = s.drawing(view) {
                    texts(&d, &format!("view {} {kind:?}", view.0).chars().take(40).collect::<String>(), &mut out);
                }
            }
        }
    }
    let elements: Vec<u64> = s.project().elements.keys().map(|e| e.0).collect();
    for e in elements {
        let got = run(s, "element_get", json!({ "id": e }));
        strings(&got, &mut |text| out.push((format!("element_get {e}"), text.to_string())));
    }
    s.sync();
    let project = s.project().clone();
    let geometry = s.geometry().clone();
    for category in Category::SCHEDULABLE {
        let def = ScheduleDef { fields: ScheduleField::available(category), ..Default::default() };
        let table = cc_views::schedule(&project, &geometry, category, &def);
        let csv = cc_views::to_csv(&table, cc_engine::i18n::tr, |c| match c {
            cc_views::Cell::Text(t) => t.clone(),
            cc_views::Cell::Key(k) => cc_engine::i18n::tr(k),
        });
        for line in csv.lines() {
            out.push((format!("schedule {category:?}"), line.to_string()));
        }
    }
    out
}

fn texts(d: &cc_views::Drawing2D, source: &str, out: &mut Vec<(String, String)>) {
    for prim in &d.prims {
        if let cc_views::Prim::Text { text, .. } = prim {
            out.push((source.to_string(), text.clone()));
        }
    }
}

/// Metric units in `text`: millimeters, centimeters, square and cubic
/// meters, kilograms, megapascals, kilometers, or meters after a number.
fn metric_units(text: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | '·' | '×'))
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        let after_number = i > 0 && words[i - 1].chars().last().is_some_and(|c| c.is_ascii_digit());
        let unit = |u: &str| {
            *w == u
                || (w.starts_with(|c: char| c.is_ascii_digit()) && w.ends_with(u) && {
                    let n = &w[..w.len() - u.len()];
                    n.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')
                })
        };
        for (u, name) in [
            ("mm", "mm"),
            ("cm", "cm"),
            ("m²", "m²"),
            ("m³", "m³"),
            ("kg", "kg"),
            ("kg/m", "kg/m"),
            ("kg/m²", "kg/m²"),
            ("kg/m³", "kg/m³"),
            ("MPa", "MPa"),
            ("km", "km"),
            ("cm²", "cm²"),
            ("cm⁴", "cm⁴"),
            ("cm³", "cm³"),
        ] {
            if unit(u) {
                found.push(name);
            }
        }
        if (*w == "m" && (after_number || text.contains("(m)"))) || unit("m") && w.len() > 1 {
            found.push("m");
        }
    }
    found
}

/// Decided exceptions: conductor cross-sections stay mm² (wire gauges are
/// out of scope), in the panel diagram and the circuit lists.
fn allowed(source: &str, text: &str) -> bool {
    text.contains("mm²") && !text.contains(" mm ") && (source.contains("Diagram") || source == "circuit_loads")
}

#[test]
fn an_imperial_project_states_no_metric_unit() {
    let (mut s, said) = scene("imperial");
    let shown = shown(&mut s, said);
    assert!(shown.len() > 300, "{}", shown.len());
    let bad: Vec<String> = shown
        .iter()
        .filter(|(source, text)| !metric_units(text).is_empty() && !allowed(source, text))
        .map(|(source, text)| format!("{source}: {text} {:?}", metric_units(text)))
        .collect();
    let mut unique = bad.clone();
    unique.dedup();
    assert!(unique.is_empty(), "{} metric texts:\n{}", unique.len(), unique.join("\n"));
}

/// What the imperial scene writes where it used to write metric sizes.
#[test]
fn the_imperial_scene_writes_inches() {
    let (mut s, said) = scene("imperial");
    let checks: Vec<String> =
        said.iter().filter(|(source, _)| source.ends_with("_check")).map(|(_, text)| text.clone()).collect();
    let shown = shown(&mut s, said);
    let has = |want: &str| shown.iter().any(|(_, text)| text == want);
    let within = |want: &str| shown.iter().any(|(_, text)| text.contains(want));
    for want in [
        "Rampa 4'-0\"",
        "Rodapé 4\"",
        "Condutor Ø3/8\"",
        "Eletrocalha 8\"×4\"",
        "Porta de giro 2'-8\" x 6'-8\"",
        "Porta-janela 2 folhas 5'-0\" x 6'-8\" (2)",
        "CW Ø1/2\"",
        "8\"×4\"",
        "8\"×8\"",
        "36\"×36\"",
    ] {
        assert!(has(want), "{want}");
    }
    for want in [
        "Comprimento,Altura,Espessura,Área,Volume",
        ",\"8\"\" × 8\"\"\",",
        " lb/ft,",
        " lb,",
        // A wall's thickness: 6", not 0'-6".
        ",\"6\"\"\",",
        ",1:12,",
    ] {
        assert!(within(want), "{want}");
    }
    // The broken rules in inches, and the ramp's slope per 12.
    assert!(checks.iter().any(|m| m.contains("7 3/4\"")), "{checks:?}");
    assert!(checks.iter().any(|m| m.contains(":12")), "{checks:?}");
    assert!(checks.iter().all(|m| !m.contains("cm") && !m.contains(" m,") && !m.contains('%')), "{checks:?}");
    // New types sized in round inches.
    let p = s.project();
    let ramp = p.types.values().find(|t| t.category() == Category::Ramp).expect("ramp type");
    assert!(matches!(ramp.data, cc_core::TypeData::Ramp { width, .. } if (width - ft(4.0, 0.0)).abs() < 1e-12));
    let tray = p.types.values().find(|t| t.category() == Category::CableTray).expect("tray type");
    assert_eq!(tray.tray_section().map(|s| (s.0, s.1)), Some((ft(0.0, 8.0), ft(0.0, 4.0))));
}

#[test]
fn the_metric_scene_still_shows_metric_units() {
    let (mut s, said) = scene("metric");
    let shown = shown(&mut s, said);
    let found: std::collections::BTreeSet<&str> = shown.iter().flat_map(|(_, t)| metric_units(t)).collect();
    for unit in ["mm", "cm", "m²", "m³", "kg", "m"] {
        assert!(found.contains(unit), "{unit} not found in {found:?}");
    }
}

#[test]
fn metric_units_are_recognized() {
    for (text, want) in [
        ("Ø 12,5 mm", vec!["mm"]),
        ("Baseboard 10 cm", vec!["cm"]),
        ("28,37 m²", vec!["m²"]),
        ("Length (m)", vec!["m"]),
        ("Ramp 1.20 m", vec!["m"]),
        ("12 kg", vec!["kg"]),
        ("7.85 kg/m", vec!["kg/m"]),
        ("200mm", vec!["mm"]),
    ] {
        assert_eq!(metric_units(text), want, "{text}");
    }
    for text in ["12'-6\"", "Room 101", "Ø 1/2\"", "28.37 ft²", "Mass", "3/32\"", "m", "Gm 3"] {
        assert!(metric_units(text).is_empty(), "{text}: {:?}", metric_units(text));
    }
}

/// Texts of the metric scene, pinned before the imperial work: metric
/// projects read as they always did.
#[test]
fn the_metric_scene_reads_as_before() {
    let (mut s, said) = scene("metric");
    let shown = shown(&mut s, said);
    let has = |want: &str| shown.iter().any(|(_, text)| text == want);
    let within = |want: &str| shown.iter().any(|(_, text)| text.contains(want));
    for want in [
        "Rampa 1,20 m",
        "Rodapé 10 cm",
        "Condutor Ø8 mm",
        "Eletrocalha 200×100",
        "Porta de giro 80x210",
        "Porta-janela 2 folhas 140x220 (2)",
        "Cota de nível 2,5 mm",
        "CW Ø20",
        "200×100",
        "Ø20 (1)",
        "20×20",
        "100×100",
        "8,3 %",
        "83,61 m²",
    ] {
        assert!(has(want), "{want}");
    }
    for want in [
        "Comprimento (m);Altura;Espessura (m);Área (m²);Volume (m³)",
        ";Ø 10,0 mm;2;3,00;6,00;3,70 kg;",
        ";0,20 m × 0,20 m;Rect 20x20;96,00 kg/m;3,00;0,120 m³;",
        ";1,00 m × 1,00 m;;1,00 m²;0,40;0,400 m³;",
        ";Ø 20 mm;3,05;CW;0,0 %;",
        ";Descida;Ø 8,0 mm;",
        ";1,20;8,3 %;",
        ";;22;;17,60;5,48 kg;",
    ] {
        assert!(within(want), "{want}");
    }
}
