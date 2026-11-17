//! Piles and pile caps (#284): caps made with their piles (one, two, three,
//! four, a line, a circle, a ring), under columns turned as the columns or
//! radially, the piles following their caps and their types, tie beams
//! framed into the caps, free piles, the pile schedule, repairs of bad
//! files and the IFC round trip.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::pile::{Pile, PileCapRef};
use cc_core::{ElementId, ElementKind, TypeData};
use cc_engine::{ErrorCode, Session};
use glam::DVec2;
use serde_json::{Value, json};
use std::f64::consts::PI;

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
    v.as_array().unwrap_or_else(|| panic!("not a list: {v}")).iter().map(|x| x.as_u64().unwrap()).collect()
}

fn derived(s: &mut Session, id: u64) -> Value {
    run(s, "element_get", json!({"id": id}))["derived"].clone()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// The piles of cap `cap`, by their place in its layout.
fn piles(s: &Session, cap: u64) -> Vec<(u64, Pile)> {
    let mut out: Vec<(u64, Pile)> = s.project().piles_of(ElementId(cap)).map(|(e, p)| (e.id.0, p.clone())).collect();
    out.sort_by_key(|(_, p)| p.cap.map(|c| c.index));
    out
}

/// A cap's placement: position, rotation (degrees), top above its level.
fn placement(s: &Session, cap: u64) -> (DVec2, f64, f64) {
    cc_core::pile::cap_placement(s.project().element(ElementId(cap)).unwrap()).unwrap()
}

/// The pile type: round, 40 cm, bored.
fn pile_type(s: &mut Session) -> u64 {
    id(&run(s, "pile_type_create", json!({"diameter": 0.4, "kind": "bored"})))
}

/// The pile positions of a triangle of side `s` in its cap's frame.
fn triangle(s: f64) -> Vec<DVec2> {
    let r = s / 3f64.sqrt();
    (0..3).map(|i| DVec2::from_angle(f64::from(i) * 2.0 * PI / 3.0) * r).collect()
}

/// Area of the three-pile cap: the triangle `r + e` off its center, its
/// three corners cut `e` past the corner piles (each a small equilateral
/// triangle of height `e`).
fn triangle_cap_area(s: f64, e: f64) -> f64 {
    let r = s / (2.0 * 3f64.sqrt());
    3.0 * 3f64.sqrt() * (r + e).powi(2) - 3f64.sqrt() * e * e
}

/// Distance from `p` to the nearest edge of the closed polygon `poly`.
fn to_outline(poly: &[DVec2], p: DVec2) -> f64 {
    (0..poly.len())
        .map(|i| cc_geom::polygon::closest_on_segment(poly[i], poly[(i + 1) % poly.len()], p).0.distance(p))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn six_columns_on_a_circle_stand_on_three_pile_caps_tied_by_arc_beams() {
    let mut s = Session::default();
    // Six columns on a circle of 6 m.
    let radius = 6.0;
    let mut columns = Vec::new();
    for k in 0..6 {
        let p = DVec2::from_angle(f64::from(k) * PI / 3.0) * radius;
        columns.push(id(&run(&mut s, "column_create", json!({"position": [p.x, p.y]}))));
    }
    let piles_type = pile_type(&mut s);
    let (spacing, edge, thickness, length, embedment) = (1.2, 0.4, 0.8, 12.0, 0.1);
    let cap_type = run(
        &mut s,
        "pile_cap_type_create",
        json!({
            "layout": "triangle", "spacing": spacing, "edge_distance": edge, "thickness": thickness,
            "pile_type": piles_type, "pile_length": length, "embedment": embedment
        }),
    );
    assert_eq!(cap_type["pile_type"], json!(piles_type));
    let made = run(
        &mut s,
        "pile_caps_under_columns",
        json!({"columns": columns, "type": cap_type["id"], "rotation": "radial", "center": [0, 0]}),
    );
    let caps = ids(&made["created"]);
    assert_eq!(caps.len(), 6, "{made}");
    assert_eq!(ids(&made["piles"]).len(), 18, "{made}");
    // Each cap points away from the center, its piles where the layout puts them.
    for (k, cap) in caps.iter().enumerate() {
        let angle = f64::from(k as u32) * 60.0;
        let (position, rotation, top) = placement(&s, *cap);
        assert!(position.distance(DVec2::from_angle(angle.to_radians()) * radius) < 1e-9);
        assert!(((rotation - angle + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-9, "{rotation} vs {angle}");
        assert_eq!(top, 0.0);
        let piles = piles(&s, *cap);
        assert_eq!(piles.len(), 3);
        for ((_, p), local) in piles.iter().zip(triangle(spacing)) {
            let want = position + DVec2::from_angle(angle.to_radians()).rotate(local);
            assert!(p.position.distance(want) < 1e-9, "{} vs {want}", p.position);
            // Heads at the cap's bottom plus the embedment, toes the pile length down.
            assert!(close(p.cut_off, -thickness + embedment) && close(p.length(), length), "{p:?}");
        }
        // The first pile is outside, on the circle's radius.
        assert!(piles[0].1.position.length() > radius + 0.5);
    }
    // Volumes: a pile π d²/4 L, a cap its area times its thickness.
    let (pile, _) = piles(&s, caps[0])[0].clone();
    let d = derived(&mut s, pile);
    assert!(close(num(&d["volume"]), PI * 0.4 * 0.4 / 4.0 * length), "{d}");
    assert!(close(num(&d["length"]), length) && close(num(&d["toe_elevation"]), -thickness + embedment - length));
    assert_eq!(d["cap"], json!({"id": caps[0], "index": 0}));
    let d = derived(&mut s, caps[0]);
    let area = triangle_cap_area(spacing, edge);
    assert!(close(num(&d["area"]), area), "{d}");
    assert!(close(num(&d["volume"]), area * thickness), "{d}");
    assert_eq!(ids(&d["piles"]).len(), 3);

    // Arc tie beams close the ring and stop at the caps' outlines.
    let beams = run(&mut s, "tie_beams_create", json!({"caps": caps, "closed": true, "center": [0, 0]}));
    let beams = ids(&beams["created"]);
    assert_eq!(beams.len(), 6);
    let geom = s.geometry().clone();
    for (k, beam) in beams.iter().enumerate() {
        let (a, b) = (caps[k], caps[(k + 1) % 6]);
        let get = run(&mut s, "element_get", json!({"id": beam}));
        assert!(close(num(&get["params"]["bulge"]), (PI / 3.0 / 4.0).tan()), "{get}");
        assert_eq!(get["derived"]["start_join"], json!({"pile_cap": a}), "{get}");
        assert_eq!(get["derived"]["end_join"], json!({"pile_cap": b}), "{get}");
        assert_eq!(num(&get["params"]["top_offset"]), 0.0);
        let g = &geom.beams[&ElementId(*beam)];
        // Every corner of the end faces lies on its cap's outline.
        for (cap, at_end) in [(a, false), (b, true)] {
            let outline = &geom.foundations[&ElementId(cap)].footprint;
            let ext = if at_end { g.end_ext } else { g.start_ext };
            for (u, v) in [(ext.0, g.half), (ext.1, -g.half)] {
                let corner = g.world(u, v);
                assert!(to_outline(outline, corner) < 1e-6, "beam {beam} corner {corner} off cap {cap}");
            }
        }
        // Shorter than the arc between the caps' centers by the caps' reach.
        assert!(g.cut_length() < radius * PI / 3.0 - 1.0);
    }

    // The pile schedule lists mark, type, diameter, cut-off, toe, length, volume and cap.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "pile"}));
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]}));
    assert_eq!(table["headers"].as_array().unwrap().len(), 8, "{table}");
    let rows = table["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 18);
    for row in rows {
        let cells: Vec<&str> = row["cells"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
        assert!(!cells[0].is_empty(), "a mark: {row}");
        assert!(cells[3].contains("0,70") || cells[3].contains("0.70"), "cut-off: {row}");
        assert!(cells[4].contains("12,70") || cells[4].contains("12.70"), "toe: {row}");
    }
    // A foundation schedule shows the caps' pile counts.
    let p = s.project().clone();
    let cap = p.element(ElementId(caps[0])).unwrap();
    let field = cc_views::field_value(&p, &geom, cap, cc_core::ScheduleField::PileCount);
    assert_eq!(field.and_then(|(_, n)| n), Some(3.0));

    // IFC: 18 piles, 6 pile caps, and back to native caps with their piles.
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    assert_eq!(text.matches("=IFCPILE(").count(), 18);
    assert!(text.contains("=IFCPILETYPE("));
    let caps_written = text.lines().filter(|l| l.contains("=IFCFOOTING(") && l.contains(".PILE_CAP.")).count();
    assert_eq!(caps_written, 6);
    let (back, _) = cc_ifc::import(&text, cc_core::Project::from_template()).unwrap();
    let caps_back: Vec<_> = back.elements.values().filter(|e| back.pile_cap(e).is_some()).collect();
    assert_eq!(caps_back.len(), 6);
    for cap in &caps_back {
        let piles: Vec<_> = back.piles_of(cap.id).collect();
        assert_eq!(piles.len(), 3, "cap {}", cap.id);
        let c = back.pile_cap(cap).unwrap();
        for (_, p) in piles {
            let index = p.cap.unwrap().index as usize;
            assert!(p.position.distance(c.pile_positions()[index]) < 1e-6);
            assert!(close(p.length(), length));
        }
    }
    let free = back.elements.values().filter(|e| matches!(&e.kind, ElementKind::Pile(p) if p.cap.is_none())).count();
    assert_eq!(free, 0);
    assert_eq!(back.clone().repair(), 0);
}

#[test]
fn each_layout_places_its_piles_and_has_its_area() {
    let mut s = Session::default();
    let piles_type = pile_type(&mut s);
    let (sp, e) = (1.2, 0.4);
    let rect = |w: f64, h: f64| w * h;
    let cases: Vec<(Value, Vec<DVec2>, f64)> = vec![
        (json!({"layout": "single"}), vec![DVec2::ZERO], rect(2.0 * e, 2.0 * e)),
        (json!({"layout": "pair"}), vec![DVec2::new(-0.6, 0.0), DVec2::new(0.6, 0.0)], rect(sp + 2.0 * e, 2.0 * e)),
        (json!({"layout": "triangle"}), triangle(sp), triangle_cap_area(sp, e)),
        (
            json!({"layout": "square"}),
            vec![DVec2::new(-0.6, -0.6), DVec2::new(0.6, -0.6), DVec2::new(0.6, 0.6), DVec2::new(-0.6, 0.6)],
            rect(sp + 2.0 * e, sp + 2.0 * e),
        ),
        (
            json!({"layout": "line", "count": 4}),
            (0..4).map(|i| DVec2::new((f64::from(i) - 1.5) * sp, 0.0)).collect(),
            rect(3.0 * sp + 2.0 * e, 2.0 * e),
        ),
        (
            json!({"layout": "circle", "count": 8, "radius": 2.0}),
            (0..8).map(|i| DVec2::from_angle(f64::from(i) * PI / 4.0) * 2.0).collect(),
            PI * (2.0f64 + e).powi(2),
        ),
        (
            json!({"layout": "circle", "count": 8, "radius": 2.0, "ring_width": 0.8}),
            (0..8).map(|i| DVec2::from_angle(f64::from(i) * PI / 4.0) * 2.0).collect(),
            PI * (2.4f64.powi(2) - 1.6f64.powi(2)),
        ),
    ];
    let (center, rotation) = (DVec2::new(10.0, -4.0), 30.0f64);
    for (k, (mut args, local, area)) in cases.into_iter().enumerate() {
        args["spacing"] = json!(sp);
        args["edge_distance"] = json!(e);
        args["pile_type"] = json!(piles_type);
        args["thickness"] = json!(1.0);
        let ty = run(&mut s, "pile_cap_type_create", args.clone());
        let at = center + DVec2::new(20.0 * k as f64, 0.0);
        let cap = id(&run(
            &mut s,
            "pile_cap_create",
            json!({"position": [at.x, at.y], "rotation": rotation, "type": ty["id"]}),
        ));
        let piles = piles(&s, cap);
        assert_eq!(piles.len(), local.len(), "{args}");
        for ((_, p), q) in piles.iter().zip(&local) {
            let want = at + DVec2::from_angle(rotation.to_radians()).rotate(*q);
            assert!(p.position.distance(want) < 1e-9, "{args}: {} vs {want}", p.position);
        }
        let d = derived(&mut s, cap);
        assert!(close(num(&d["area"]), area), "{args}: {d}");
        assert!(close(num(&d["volume"]), area), "{args}: {d} (1 m thick)");
        let g = s.geometry().foundations[&ElementId(cap)].clone();
        assert!(close(g.area(), area));
        // A ring cap has its hole.
        assert_eq!(g.holes.len(), usize::from(args.get("ring_width").is_some()), "{args}");
    }
}

#[test]
fn piles_go_with_their_cap() {
    let mut s = Session::default();
    pile_type(&mut s);
    let ty = run(&mut s, "pile_cap_type_create", json!({"layout": "triangle"}));
    let cap = id(&run(&mut s, "pile_cap_create", json!({"position": [0, 0], "type": ty["id"]})));
    let before: Vec<(u64, Pile)> = piles(&s, cap);
    assert_eq!(before.len(), 3);
    // Moved.
    run(&mut s, "elements_move", json!({"ids": [cap], "delta": [3, 1]}));
    for ((a, p), (b, q)) in before.iter().zip(piles(&s, cap)) {
        assert_eq!(*a, b);
        assert!(q.position.distance(p.position + DVec2::new(3.0, 1.0)) < 1e-9);
    }
    // Turned about the cap's center.
    run(&mut s, "elements_rotate", json!({"ids": [cap], "angle": 90, "center": [3, 1]}));
    let (_, rotation, _) = placement(&s, cap);
    assert!((rotation - 90.0).abs() < 1e-9);
    for ((_, p), (_, q)) in before.iter().zip(piles(&s, cap)) {
        let want = DVec2::new(3.0, 1.0) + p.position.perp();
        assert!(q.position.distance(want) < 1e-9, "{} vs {want}", q.position);
    }
    // Copied: the copy has piles of its own.
    let copies = ids(&run(&mut s, "elements_copy", json!({"ids": [cap], "delta": [10, 0]}))["created"]);
    let copy = copies.iter().copied().find(|c| s.project().is_pile_cap(ElementId(*c))).unwrap();
    let copied = piles(&s, copy);
    assert_eq!(copied.len(), 3);
    assert!(copied.iter().all(|(id, _)| !before.iter().any(|(b, _)| b == id)));
    assert_eq!(piles(&s, cap).len(), 3);
    // Deleted with it, and back with undo.
    run(&mut s, "elements_delete", json!({"ids": [cap]}));
    assert!(before.iter().all(|(id, _)| s.project().element(ElementId(*id)).is_none()));
    run(&mut s, "undo", json!({}));
    assert_eq!(
        piles(&s, cap).iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        before.iter().map(|(id, _)| *id).collect::<Vec<_>>()
    );
}

#[test]
fn changing_the_layout_changes_the_piles_in_one_step() {
    let mut s = Session::default();
    pile_type(&mut s);
    let ty = id(&run(&mut s, "pile_cap_type_create", json!({"layout": "triangle", "spacing": 1.2})));
    let a = id(&run(&mut s, "pile_cap_create", json!({"position": [0, 0], "type": ty})));
    let b = id(&run(&mut s, "pile_cap_create", json!({"position": [10, 0], "type": ty, "rotation": 45})));
    let before = (piles(&s, a), piles(&s, b));
    let depth = s.doc.undo_depth();
    run(&mut s, "type_set", json!({"type": ty, "params": {"pile_cap": {"layout": "Square"}}}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    for cap in [a, b] {
        let now = piles(&s, cap);
        assert_eq!(now.len(), 4);
        let (position, rotation, _) = placement(&s, cap);
        for (k, (_, p)) in now.iter().enumerate() {
            let local = [(-0.6, -0.6), (0.6, -0.6), (0.6, 0.6), (-0.6, 0.6)][k];
            let want = position + DVec2::from_angle(rotation.to_radians()).rotate(DVec2::from(local));
            assert!(p.position.distance(want) < 1e-9);
        }
    }
    // The type's width and length follow the outline.
    let t = run(&mut s, "type_get", json!({"type": ty}));
    assert!(close(num(&t["params"]["width"]), 1.2 + 0.8) && close(num(&t["params"]["length"]), 1.2 + 0.8), "{t}");
    // Fewer piles: the extra ones go.
    run(&mut s, "type_set", json!({"type": ty, "params": {"pile_cap": {"layout": "Pair"}}}));
    assert_eq!(piles(&s, a).len(), 2);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!((piles(&s, a), piles(&s, b)), before);
    // A longer pile: every pile of every cap.
    run(&mut s, "type_set", json!({"type": ty, "params": {"pile_cap": {"pile_length": 20.0}}}));
    assert!(piles(&s, b).iter().all(|(_, p)| close(p.length(), 20.0)));
    // Raising the cap raises its piles.
    run(&mut s, "element_set", json!({"id": a, "params": {"top_offset": 1.0}}));
    assert!(piles(&s, a).iter().all(|(_, p)| close(p.cut_off, 1.0 - 0.8 + 0.1)));
}

#[test]
fn a_free_pile_by_its_toe_or_its_length() {
    let mut s = Session::default();
    // The first pile makes a pile type.
    let a = run(&mut s, "pile_create", json!({"position": [1, 2], "cut_off": -0.5, "toe": -10.5}));
    assert!(close(num(&a["derived"]["length"]), 10.0), "{a}");
    let ty = s.project().element(ElementId(id(&a))).unwrap().type_id;
    let Some(TypeData::Pile { section, .. }) = s.project().element_type(ty).map(|t| &t.data) else { panic!("type") };
    assert!(close(section.size(), 0.4));
    let b = run(&mut s, "pile_create", json!({"position": [4, 2], "length": 8.0}));
    assert!(close(num(&b["params"]["toe"]), -8.0), "{b}");
    assert!(close(num(&b["derived"]["volume"]), PI * 0.04 * 8.0), "{b}");
    assert_eq!(
        fails(&mut s, "pile_create", json!({"position": [0, 0], "toe": -3, "length": 3})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "pile_create", json!({"position": [0, 0], "toe": 1})), ErrorCode::InvalidArguments);
    // A square pile.
    let sq = run(&mut s, "pile_type_create", json!({"side": 0.3, "kind": "precast"}));
    let c = run(&mut s, "pile_create", json!({"position": [8, 2], "length": 6.0, "type": sq["id"]}));
    assert!(close(num(&c["derived"]["volume"]), 0.09 * 6.0), "{c}");
    // element_set takes a length too.
    let changed = run(&mut s, "element_set", json!({"id": id(&b), "params": {"length": 5.0}}));
    assert!(close(num(&changed["params"]["toe"]), -5.0), "{changed}");
    assert_eq!(
        fails(&mut s, "element_set", json!({"id": id(&b), "params": {"length": 0.0}})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn a_cap_pile_stays_in_its_cap() {
    let mut s = Session::default();
    let cap = id(&run(&mut s, "pile_cap_create", json!({"position": [0, 0]})));
    let piles_now = piles(&s, cap);
    assert_eq!(piles_now.len(), 2, "a default two-pile cap");
    let (pile, where_) = (piles_now[0].0, piles_now[0].1.position);
    // Dragged: it goes back (nothing changes).
    run(&mut s, "elements_move", json!({"ids": [pile], "delta": [2, 0]}));
    assert!(piles(&s, cap)[0].1.position.distance(where_) < 1e-9);
    // Its params follow the cap.
    assert_eq!(fails(&mut s, "element_set", json!({"id": pile, "params": {"position": [5, 5]}})), ErrorCode::Rejected);
    // Deleted alone: kept.
    let out = run(&mut s, "elements_delete", json!({"ids": [pile]}));
    assert_eq!(ids(&out["cap_piles_skipped"]), vec![pile]);
    assert!(s.project().element(ElementId(pile)).is_some());
    // Copied alone: a free pile.
    let copy = ids(&run(&mut s, "elements_copy", json!({"ids": [pile], "delta": [0, 5]}))["created"]);
    assert_eq!(copy.len(), 1);
    match &s.project().element(ElementId(copy[0])).unwrap().kind {
        ElementKind::Pile(p) => assert!(p.cap.is_none() && p.position.distance(where_ + DVec2::new(0.0, 5.0)) < 1e-9),
        other => panic!("{other:?}"),
    }
    assert_eq!(piles(&s, cap).len(), 2);
}

#[test]
fn plans_draw_caps_and_their_piles_dashed() {
    let mut s = Session::default();
    pile_type(&mut s);
    let ty =
        run(&mut s, "pile_cap_type_create", json!({"layout": "circle", "count": 6, "radius": 2.0, "ring_width": 0.8}));
    let cap = id(&run(&mut s, "pile_cap_create", json!({"position": [0, 0], "type": ty["id"]})));
    let level = s.project().element(ElementId(cap)).unwrap().level;
    let view = s
        .project()
        .views
        .values()
        .find(|v| v.level() == Some(level) && matches!(v.kind, cc_core::ViewKind::FloorPlan { .. }))
        .unwrap()
        .id;
    let d = s.drawing(view).unwrap();
    let lines = |element: u64, style: cc_views::LineStyle| {
        d.prims
            .iter()
            .filter(|p| matches!(p, cc_views::Prim::Line { element: Some(e), style: st, .. } if e.0 == element && *st == style))
            .count()
    };
    // The ring under the floor: both its circles dashed.
    let segments = cc_geom::pile::circle_segments();
    assert_eq!(lines(cap, cc_views::LineStyle::Hidden), 2 * segments);
    // Each pile dashed, with a center mark.
    for (pile, _) in piles(&s, cap) {
        assert_eq!(lines(pile, cc_views::LineStyle::Hidden), segments, "pile {pile}");
        assert_eq!(lines(pile, cc_views::LineStyle::Center), 2, "pile {pile}");
    }
    // With structure tags on, tagged with its size and its number of piles.
    s.doc.edit("tags", |e| {
        if let Some(cc_core::ViewKind::FloorPlan { structure_tags, .. }) = e.view_mut(view).map(|v| &mut v.kind) {
            *structure_tags = true;
        }
    });
    let d = s.drawing(view).unwrap();
    let texts: Vec<&str> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Text { text, element: Some(e), .. } if e.0 == cap => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["480×480 (6)"], "the ring 2 × 2.4 m across");
}

#[test]
fn bad_piles_and_caps_load_repaired() {
    let mut s = Session::default();
    pile_type(&mut s);
    let cap = id(&run(&mut s, "pile_cap_create", json!({"position": [0, 0]})));
    let free = id(&run(&mut s, "pile_create", json!({"position": [5, 0]})));
    let mut p = s.project().clone();
    let first = piles(&s, cap)[0].0;
    // A pile at a place the layout has not, one that is not a number, and a toe above its head.
    let mut extra = p.element(ElementId(first)).unwrap().clone();
    extra.id = ElementId(p.alloc_id());
    if let ElementKind::Pile(x) = &mut extra.kind {
        x.cap = Some(PileCapRef { cap: ElementId(cap), index: 99 });
    }
    p.elements.insert(extra.id, extra.clone());
    let mut twin = extra.clone();
    twin.id = ElementId(p.alloc_id());
    if let ElementKind::Pile(x) = &mut twin.kind {
        x.cap = Some(PileCapRef { cap: ElementId(cap), index: 0 });
    }
    p.elements.insert(twin.id, twin.clone());
    if let Some(ElementKind::Pile(x)) = p.elements.get_mut(&ElementId(free)).map(|e| &mut e.kind) {
        x.toe = x.cut_off + 3.0;
    }
    let cap_type = p.element(ElementId(cap)).unwrap().type_id;
    if let Some(TypeData::Foundation { pile_cap: Some(c), .. }) = p.types.get_mut(&cap_type).map(|t| &mut t.data) {
        c.spacing = f64::NAN;
        c.pile_length = -4.0;
        c.pile_type = Some(cap_type);
    }
    let text = cc_core::to_ron(&p).unwrap();
    let (back, repairs) = cc_core::from_ron_with_repairs(&text).unwrap();
    assert!(repairs >= 5, "{repairs}");
    assert!(back.element(extra.id).is_none() && back.element(twin.id).is_none());
    assert_eq!(back.piles_of(ElementId(cap)).count(), 2);
    let ElementKind::Pile(f) = &back.element(ElementId(free)).unwrap().kind else { panic!("pile") };
    assert!(close(f.length(), cc_core::pile::DEFAULT_PILE_LENGTH));
    let Some(TypeData::Foundation { pile_cap: Some(c), .. }) = back.element_type(cap_type).map(|t| &t.data) else {
        panic!("cap type")
    };
    assert!(c.spacing.is_finite() && close(c.pile_length, cc_core::pile::MIN_PILE_LENGTH) && c.pile_type.is_none());
    // A pile whose cap is gone goes, like a pad without its terrain.
    let mut p = back.clone();
    p.elements.remove(&ElementId(cap));
    assert!(p.repair() > 0);
    assert_eq!(p.elements.values().filter(|e| matches!(e.kind, ElementKind::Pile(_))).count(), 1);
    // Not a number: the pile goes.
    let mut p = back;
    if let Some(ElementKind::Pile(x)) = p.elements.get_mut(&ElementId(free)).map(|e| &mut e.kind) {
        x.cut_off = f64::NAN;
    }
    assert!(p.repair() > 0);
    assert!(p.element(ElementId(free)).is_none());
}
