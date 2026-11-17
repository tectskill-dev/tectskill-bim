//! Slab reinforcement (`slab_rebar_create`): meshes clipped to the true
//! boundary, ring and radial bars, trimming bars around openings, hooks and
//! lap splices, in floors and foundation slabs.

use cc_core::{ElementId, ElementKind, RebarLayout};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};
use std::f64::consts::{PI, TAU};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).expect_err("should fail").code
}

fn id(v: &Value) -> ElementId {
    ElementId(v["id"].as_u64().unwrap())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn created(v: &Value) -> Vec<ElementId> {
    v["created"].as_array().unwrap().iter().map(|x| ElementId(x.as_u64().unwrap())).collect()
}

fn layout(s: &Session, set: ElementId) -> RebarLayout {
    match &s.project().element(set).unwrap().kind {
        ElementKind::Rebar(r) => r.layout.clone(),
        _ => panic!("a rebar set"),
    }
}

/// A circular floor of radius 2 m around the origin, with a 60 x 60 cm manhole when asked.
fn disc(s: &mut Session, manhole: bool) -> ElementId {
    let mut curves = vec![json!({"kind": "circle", "center": [0, 0], "radius": 2.0})];
    if manhole {
        curves.push(json!({"kind": "rectangle", "corner": [-0.3, -0.3], "opposite": [0.3, 0.3]}));
    }
    id(&run(s, "floor_create", json!({"curves": curves})))
}

/// The sets of `ids` that are straight bars at `angle` in the top (or bottom) layer.
fn straight(s: &Session, ids: &[ElementId], top: bool, angle: f64) -> ElementId {
    *ids.iter()
        .find(|x| matches!(layout(s, **x), RebarLayout::SlabStraight { top: t, angle_deg, .. } if t == top && close(angle_deg, angle)))
        .expect("the set")
}

/// The issue's reference: every bar of a mesh in a circular slab ends on the
/// circle, the cover inside it, and has its own length.
#[test]
fn a_mesh_in_a_circular_slab_is_clipped_to_the_circle() {
    let mut s = Session::default();
    let slab = disc(&mut s, false);
    let depth = s.doc.undo_depth();
    let out =
        run(&mut s, "slab_rebar_create", json!({"slabs": [slab.0], "spacing": 0.15, "cover": 0.03, "max_length": 0}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let sets = created(&out);
    // Two directions in each of the two layers; no openings, no trimming bars.
    assert_eq!((sets.len(), out["slabs"].as_u64()), (4, Some(1)));
    let geom = s.geometry().clone();
    let (along, across) =
        (&geom.rebar[&straight(&s, &sets, false, 0.0)], &geom.rebar[&straight(&s, &sets, false, 90.0)]);
    // The bars' center lines stay the cover and half a bar inside the circle.
    let reach = 2.0 - 0.03 - along.diameter / 2.0;
    let mut total = 0.0;
    for bar in &along.bars {
        let y = bar.points[0].y;
        assert!(close(y / 0.15, (y / 0.15).round()), "on a line of the mesh: {y}");
        let half = (reach * reach - y * y).sqrt();
        assert!(close(bar.points[0].x, -half) && close(bar.points[1].x, half), "{y}");
        assert!(close(bar.length(), 2.0 * half));
        total += 2.0 * half;
    }
    // One bar on every line that crosses the circle: 13 either side of the one through the center.
    assert_eq!(along.count(), 27);
    assert!(close(along.total_length(), total));
    // The second direction lies a bar further in, and is the first turned a quarter.
    assert!(close(across.bars[0].points[0].z - along.bars[0].points[0].z, along.diameter));
    assert!(close(across.total_length(), total));
    // The top layer mirrors it under the top face.
    let top = &geom.rebar[&straight(&s, &sets, true, 0.0)];
    let top_across = &geom.rebar[&straight(&s, &sets, true, 90.0)];
    assert!(top.bars[0].points[0].z > across.bars[0].points[0].z);
    assert!(close(top.bars[0].points[0].z - top_across.bars[0].points[0].z, top.diameter));
    // The schedule's weight, and the bars by length.
    assert!(close(along.weight(), PI * along.diameter * along.diameter / 4.0 * total * 7850.0));
    let first = straight(&s, &sets, false, 0.0);
    let d = run(&mut s, "element_get", json!({"id": first.0}))["derived"].clone();
    let bars = d["bars"].as_array().unwrap();
    // The bar through the center alone, the others in pairs, longest first.
    assert_eq!(bars.len(), 14);
    assert!(close(bars[0]["length"].as_f64().unwrap(), (2.0 * reach * 1000.0).round() / 1000.0));
    assert_eq!((bars[0]["count"].as_u64(), bars[1]["count"].as_u64()), (Some(1), Some(2)));
    assert_eq!(bars.iter().map(|b| b["count"].as_u64().unwrap()).sum::<u64>(), 27);
    // Deleting the slab takes its reinforcement along.
    run(&mut s, "elements_delete", json!({"ids": [slab.0]}));
    assert!(sets.iter().all(|x| s.project().element(*x).is_none()));
}

#[test]
fn bars_are_cut_at_a_manhole_and_trimmed_around_it() {
    let mut s = Session::default();
    let slab = disc(&mut s, true);
    let sets = created(&run(&mut s, "slab_rebar_create", json!({"slabs": [slab.0], "spacing": 0.15, "cover": 0.03})));
    // A trimming set joins the two directions of each layer.
    assert_eq!(sets.len(), 6);
    let geom = s.geometry().clone();
    let along = &geom.rebar[&straight(&s, &sets, false, 0.0)];
    let clear = 0.03 + along.diameter / 2.0;
    // Lines through the opening carry two bars, each ending the cover clear of it.
    let through: Vec<_> = along.bars.iter().filter(|b| b.points[0].y.abs() < 0.3 - 1e-9).collect();
    assert_eq!(through.len(), 2 * 3, "the lines at -0.15, 0 and 0.15");
    for bar in &through {
        let (x0, x1) = (bar.points[0].x, bar.points[1].x);
        assert!(close(x1, -0.3 - clear) || close(x0, 0.3 + clear), "{x0} {x1}");
    }
    // The lines along the opening's edges pass within the cover of it: they are cut too.
    assert_eq!(along.count(), 27 + 5);
    // Trimming: two bars along each side reaching 40 diameters past the opening, and four diagonals.
    let trim = *sets.iter().find(|x| matches!(layout(&s, **x), RebarLayout::SlabTrim { top: false, .. })).unwrap();
    let t = &geom.rebar[&trim];
    assert_eq!(t.count(), 4 * 2 + 4);
    let anchorage = 40.0 * t.diameter;
    let sides: Vec<_> = t.bars.iter().filter(|b| close(b.length(), 0.6 + 2.0 * anchorage)).collect();
    assert_eq!(sides.len(), 8);
    assert!(t.bars.iter().filter(|b| close(b.length(), 2.0 * anchorage)).count() == 4, "diagonals");
    // The first bar of a side lies the cover clear of the opening, the second 5 cm further.
    let mut below: Vec<f64> = sides
        .iter()
        .filter(|b| close(b.points[0].y, b.points[1].y) && b.points[0].y < 0.0)
        .map(|b| b.points[0].y)
        .collect();
    below.sort_by(f64::total_cmp);
    let own = 0.03 + t.diameter / 2.0;
    assert!(close(below[1], -0.3 - own) && close(below[0], -0.3 - own - 0.05), "{below:?}");
    // No trimming when turned off, and its values may be given.
    let none = created(&run(&mut s, "slab_rebar_create", json!({"slabs": [slab.0], "layers": "top", "trim": false})));
    assert_eq!(none.len(), 2);
    let args = json!({"slabs": [slab.0], "layers": "top", "trim": {"count": 3, "anchorage": 0.3, "diagonals": false}});
    let own = created(&run(&mut s, "slab_rebar_create", args));
    assert_eq!(s.geometry().rebar[&own[2]].count(), 4 * 3);
    assert!(s.geometry().rebar[&own[2]].bars.iter().all(|b| close(b.length(), 1.2)));
}

#[test]
fn ring_and_radial_bars_follow_a_circular_slab() {
    let mut s = Session::default();
    let slab = disc(&mut s, false);
    let args = json!({
        "slabs": [slab.0], "layout": "radial", "layers": "bottom", "spacing": 0.2, "cover": 0.03,
        "radial_count": 12, "start": 0.3, "intermediate": 1.0, "max_length": 0,
    });
    let sets = created(&run(&mut s, "slab_rebar_create", args));
    assert_eq!(sets.len(), 2);
    let geom = s.geometry().clone();
    let (rings, radial) = (&geom.rebar[&sets[0]], &geom.rebar[&sets[1]]);
    // Rings every 20 cm that stay inside the cover: 0.2 to 1.8 m, each a closed circle of its own length.
    assert_eq!(rings.count(), 9);
    for (i, ring) in rings.bars.iter().enumerate() {
        let radius = 0.2 * (i + 1) as f64;
        assert!(ring.closed && ring.points.iter().all(|p| close(p.truncate().length(), radius)));
        assert!(close(ring.length(), TAU * radius));
    }
    // Twelve bars from 0.3 m to the cover inside the edge, and twelve shorter ones between them.
    let reach = 2.0 - 0.03 - radial.diameter / 2.0;
    assert_eq!(radial.count(), 24);
    assert_eq!(radial.bars.iter().filter(|b| close(b.length(), reach - 0.3)).count(), 12);
    assert_eq!(radial.bars.iter().filter(|b| close(b.length(), reach - 1.0)).count(), 12);
    let angle = |b: &cc_geom::rebar::Bar| b.points[1].y.atan2(b.points[1].x).rem_euclid(TAU);
    assert!(
        radial
            .bars
            .iter()
            .all(|b| close((angle(b) / (TAU / 24.0)).round() * (TAU / 24.0), angle(b)) || angle(b) > TAU - 1e-9)
    );
    // The radial bars lie inside the rings.
    assert!(close(radial.bars[0].points[0].z - rings.bars[0].points[0].z, rings.diameter));
    // Cut from stock, a ring is made of pieces that each lap the next around it.
    let spliced =
        json!({"slabs": [slab.0], "layout": "radial", "layers": "top", "spacing": 0.9, "max_length": 6.0, "lap": 0.5});
    let sets = created(&run(&mut s, "slab_rebar_create", spliced));
    let rings = s.geometry().rebar[&sets[0]].clone();
    // Radii 0.9 and 1.8 m: 5.65 m in two pieces (one, with its lap, would pass the stock) and
    // 11.31 m in three, each lapping the next.
    assert_eq!(rings.count(), 2 + 3);
    assert!(close(rings.bars[0].length(), TAU * 0.9 / 2.0 + 0.5) && !rings.bars[0].closed);
    assert!(close(rings.total_length(), TAU * 0.9 + 2.0 * 0.5 + TAU * 1.8 + 3.0 * 0.5));
    assert!(rings.bars.iter().all(|b| b.length() <= 6.0 + 1e-9));
    // Without a count, as many radial bars as keep them a spacing apart at the edge.
    let count = match layout(&s, sets[1]) {
        RebarLayout::SlabRadial { count, .. } => count,
        other => panic!("{other:?}"),
    };
    assert_eq!(count, (TAU * 2.0 / 0.9).ceil() as u32);
}

#[test]
fn rectangular_slabs_take_hooks_angles_and_splices() {
    let mut s = Session::default();
    let boundary = json!([[0, 0], [30, 0], [30, 4], [0, 4]]);
    let floor = id(&run(&mut s, "floor_create", json!({"boundary": boundary})));
    let raft = id(&run(&mut s, "foundation_slab_create", json!({"boundary": [[40, 0], [44, 0], [44, 4], [40, 4]]})));
    let args = json!({
        "slabs": [floor.0], "layers": "top", "spacing": 0.5, "cover": 0.05, "hook": 0.10, "max_length": 12.0, "lap": 0.6,
    });
    let sets = created(&run(&mut s, "slab_rebar_create", args));
    let geom = s.geometry().clone();
    let (along, across) = (&geom.rebar[&straight(&s, &sets, true, 0.0)], &geom.rebar[&straight(&s, &sets, true, 90.0)]);
    let inset = 0.05 + along.diameter / 2.0;
    // The short bars: straight across with a 10 cm leg down at each edge.
    for bar in &across.bars {
        assert_eq!(bar.points.len(), 4);
        assert!(close(bar.points[0].z, bar.points[1].z - 0.10) && close(bar.points[3].z, bar.points[2].z - 0.10));
        assert!(close(bar.length(), 4.0 - 2.0 * inset + 0.20));
    }
    // The long ones: 29.89 m in three pieces lapping 0.6 m, a leg only at the slab's edges,
    // the next line cut half a piece off.
    let run_length = 30.0 - 2.0 * inset;
    let line = |y: f64| along.bars.iter().filter(|b| close(b.points[1].y, y)).collect::<Vec<_>>();
    let mut ys: Vec<f64> = along.bars.iter().map(|b| b.points[1].y).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| close(*a, *b));
    // Whichever of two neighbouring lines has the fewer pieces.
    let (mut first, mut second) = (line(ys[0]), line(ys[1]));
    if first.len() > second.len() {
        std::mem::swap(&mut first, &mut second);
    }
    assert_eq!((first.len(), second.len()), (3, 4));
    assert!(close(first.iter().map(|b| b.length()).sum::<f64>(), run_length + 2.0 * 0.6 + 0.20));
    assert!(
        first.iter().filter(|b| b.points.len() == 3).count() == 2
            && first.iter().filter(|b| b.points.len() == 2).count() == 1
    );
    // A mesh at 45° in the raft: bars of many lengths, none outside it.
    let sets = created(&run(
        &mut s,
        "slab_rebar_create",
        json!({"slabs": [raft.0], "angle": 45, "spacing": 0.25, "cover": 0.05}),
    ));
    assert_eq!(sets.len(), 4);
    let set = straight(&s, &sets, false, 45.0);
    let diagonal = s.geometry().rebar[&set].clone();
    let d = run(&mut s, "element_get", json!({"id": set.0}))["derived"].clone();
    assert!(d["bars"].as_array().unwrap().len() > 5);
    for bar in &diagonal.bars {
        let dir = (bar.points[1] - bar.points[0]).truncate().normalize();
        assert!(close(dir.x, dir.y) && bar.points.iter().all(|p| p.x > 40.0 && p.x < 44.0 && p.y > 0.0 && p.y < 4.0));
    }
    // The longest runs corner to corner, the cover inside both edges.
    let longest = diagonal.bars.iter().map(|b| b.length()).fold(0.0, f64::max);
    let inset = 0.05 + diagonal.diameter / 2.0;
    assert!(close(longest, (4.0 - 2.0 * inset) * std::f64::consts::SQRT_2), "{longest}");
}

#[test]
fn only_slabs_are_reinforced_and_values_are_checked() {
    let mut s = Session::default();
    let slab = disc(&mut s, false);
    let wall = id(&run(&mut s, "wall_create", json!({"start": [10, 0], "end": [14, 0]})));
    let strip = id(&run(&mut s, "strip_footing_create", json!({"start": [20, 0], "end": [24, 0]})));
    for bad in [
        json!({"slabs": [wall.0]}),
        json!({"slabs": [strip.0]}),
        json!({"slabs": [slab.0], "layout": "spiral"}),
        json!({"slabs": [slab.0], "layers": "middle"}),
        json!({"slabs": [slab.0], "spacing": 0.001}),
        json!({"slabs": [slab.0], "hook": 500.0}),
        json!({"slabs": [slab.0], "layout": "radial", "radial_count": 0}),
        json!({"slabs": [slab.0], "trim": {"count": 50}}),
        json!({"slabs": [slab.0], "trim": {"sides": 2}}),
    ] {
        assert_eq!(fails(&mut s, "slab_rebar_create", bad.clone()), ErrorCode::InvalidArguments, "{bad}");
    }
    // The beam and column command does not take slabs, nor a slab the bars of a wall.
    assert_eq!(fails(&mut s, "rebar_create", json!({"host": slab.0, "layout": "top"})), ErrorCode::InvalidArguments);
    // Without a list: the structural floors and the foundation slabs, not every floor.
    let none = run(&mut s, "slab_rebar_create", json!({"layers": "bottom"}));
    assert_eq!((created(&none).len(), none["slabs"].as_u64()), (0, Some(0)));
    run(&mut s, "element_set", json!({"id": slab.0, "params": {"structural": true}}));
    let carrying = json!({"boundary": [[30, 0], [34, 0], [34, 4], [30, 4]], "structural": true});
    let second = id(&run(&mut s, "floor_create", carrying));
    let raft = id(&run(&mut s, "foundation_slab_create", json!({"boundary": [[40, 0], [44, 0], [44, 4], [40, 4]]})));
    let all = run(&mut s, "slab_rebar_create", json!({"layers": "bottom"}));
    assert_eq!((created(&all).len(), all["slabs"].as_u64()), (6, Some(3)));
    run(&mut s, "elements_delete", json!({"ids": [second.0, raft.0]}));
    // `element_set` changes a set and refuses values out of range.
    let set = created(&all)[0];
    let before = s.geometry().rebar[&set].count();
    let wider = json!({"layout": {"SlabStraight": {"top": false, "angle_deg": 0.0, "spacing": 0.30}}});
    run(&mut s, "element_set", json!({"id": set.0, "params": wider}));
    assert!(s.geometry().rebar[&set].count() < before);
    let bad = json!({"layout": {"SlabStraight": {"top": false, "angle_deg": 0.0, "spacing": 0.0}}});
    assert!(s.execute("element_set", json!({"id": set.0, "params": bad})).is_err());
    // A cover as deep as the slab leaves no bars rather than bars outside it.
    let deep = created(&run(&mut s, "slab_rebar_create", json!({"slabs": [slab.0], "layers": "top", "cover": 3.0})));
    assert_eq!(s.geometry().rebar[&deep[0]].count(), 0);
}
