//! Wall reinforcement (`wall_rebar_create`): horizontal and ring bars,
//! vertical bars with bent ends and additional zones, ties, openings and
//! lap splices, in straight and arc walls.

use cc_core::{ElementId, ElementKind, RebarLayout, WallSide};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};
use std::f64::consts::PI;

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

/// The sets of `ids` that are horizontal (or vertical) bars near `side`.
fn family(s: &Session, ids: &[ElementId], horizontal: bool, side: WallSide) -> ElementId {
    *ids.iter()
        .find(|id| match layout(s, **id) {
            RebarLayout::WallHorizontal { side: x, .. } => horizontal && x == side,
            RebarLayout::WallVertical { side: x, .. } => !horizontal && x == side,
            _ => false,
        })
        .expect("the set")
}

/// A straight wall 6 m long and 3 m high along +X.
fn straight(s: &mut Session) -> ElementId {
    id(&run(s, "wall_create", json!({"start": [0, 0], "end": [6, 0], "height": 3.0})))
}

/// Offsets of the faces of the wall's structure (exterior, interior).
fn faces(s: &mut Session, wall: ElementId) -> (f64, f64) {
    let g = &s.geometry().walls[&wall];
    g.core.unwrap_or((g.half, -g.half))
}

/// A cylinder of centerline radius 2.2 m and 6 m high: two half-circle walls.
fn cylinder(s: &mut Session) -> [ElementId; 2] {
    let a = id(&run(s, "wall_create", json!({"start": [2.2, 0], "end": [-2.2, 0], "bulge": 1.0, "height": 6.0})));
    let b = id(&run(s, "wall_create", json!({"start": [-2.2, 0], "end": [2.2, 0], "bulge": 1.0, "height": 6.0})));
    [a, b]
}

/// The issue's reference: one command gives a cylindrical wall its four bar
/// families, and each ring bar is as long as the arc at its own radius.
#[test]
fn a_cylindrical_wall_gets_its_four_bar_families() {
    let mut s = Session::default();
    let walls = cylinder(&mut s);
    let depth = s.doc.undo_depth();
    let args = json!({"walls": [walls[0].0, walls[1].0], "cover_exterior": 0.03, "cover_interior": 0.04});
    let out = run(&mut s, "wall_rebar_create", args);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let sets = created(&out);
    assert_eq!((sets.len(), out["walls"].as_u64()), (8, Some(2)));
    let (ext, int) = faces(&mut s, walls[0]);
    let geom = s.geometry().clone();
    let on_wall: Vec<ElementId> = sets.iter().copied().filter(|x| geom.rebar[x].host == Some(walls[0])).collect();
    assert_eq!(on_wall.len(), 4);
    let mut steel = 0.0;
    for (side, face, cover) in [(WallSide::Exterior, ext, 0.03), (WallSide::Interior, int, 0.04)] {
        let rings = &geom.rebar[&family(&s, &on_wall, true, side)];
        // The ring bars are the outer layer: half a bar inside the cover. The
        // exterior (left) face of a counter-clockwise arc is its inner one.
        let v = face - side.sign() * (cover + rings.diameter / 2.0);
        let radius = 2.2 - v;
        // Every 15 cm from the cover up to the cover under the top.
        assert_eq!(rings.count(), ((6.0 - 2.0 * cover) / 0.15 + 1e-9).floor() as usize + 1);
        for bar in &rings.bars {
            assert!(bar.points.iter().all(|p| close(p.truncate().length(), radius)), "{radius}");
            // Half a circle: the two walls' bars meet at the joints, with no cover between them.
            assert!(close(bar.length(), PI * radius), "{} vs {}", bar.length(), PI * radius);
            assert!(bar.points.iter().all(|p| close(p.z, bar.points[0].z)));
        }
        steel += rings.total_length();
        // The vertical bars stand inside the rings, every 15 cm along the centerline.
        let bars = &geom.rebar[&family(&s, &on_wall, false, side)];
        let inner = 2.2 - (face - side.sign() * (cover + rings.diameter + bars.diameter / 2.0));
        assert_eq!(bars.count(), ((PI * 2.2 - 2.0 * cover) / 0.15 + 1e-9).floor() as usize + 1);
        for bar in &bars.bars {
            assert!(bar.points.len() == 2 && close(bar.points[0].truncate().length(), inner));
            assert!(close(bar.length(), 6.0 - 2.0 * cover));
        }
        let angles: Vec<f64> = bars.bars.iter().map(|b| b.points[0].y.atan2(b.points[0].x)).collect();
        assert!(angles.windows(2).all(|w| close((w[1] - w[0]) * 2.2, 0.15)), "even angles");
        steel += bars.total_length();
    }
    // The schedule's weight is the hand calculation's.
    let d = geom.rebar[&on_wall[0]].diameter;
    let weight: f64 = on_wall.iter().map(|x| geom.rebar[x].weight()).sum();
    assert!((weight - PI * d * d / 4.0 * steel * 7850.0).abs() < 1e-6);
    // Deleting the wall takes its reinforcement along.
    run(&mut s, "elements_delete", json!({"ids": [walls[0].0]}));
    assert!(on_wall.iter().all(|x| s.project().element(*x).is_none()));
    assert!(sets.iter().filter(|x| s.project().element(**x).is_some()).count() == 4);
}

#[test]
fn horizontal_bars_close_up_by_zones_and_either_family_may_be_outer() {
    let mut s = Session::default();
    let wall = straight(&mut s);
    let zones =
        json!([{"up_to": 1.0, "spacing": 0.10}, {"up_to": 2.0, "spacing": 0.20}, {"up_to": 3.0, "spacing": 0.25}]);
    let args =
        json!({"walls": [wall.0], "horizontal": {"zones": zones}, "faces": "exterior", "cover": 0.05, "max_length": 0});
    let sets = created(&run(&mut s, "wall_rebar_create", args));
    assert_eq!(sets.len(), 1);
    let r = s.geometry().rebar[&sets[0]].clone();
    let base = s.geometry().walls[&wall].base_z;
    let heights: Vec<f64> = r.bars.iter().map(|b| b.points[0].z - base).collect();
    assert!(close(heights[0], 0.05));
    assert_eq!(heights.iter().filter(|h| **h < 1.0).count(), 10);
    let mid: Vec<f64> = heights.iter().copied().filter(|h| (1.0..2.0).contains(h)).collect();
    assert!(mid.len() == 5 && mid.windows(2).all(|w| close(w[1] - w[0], 0.20)), "{mid:?}");
    assert!(heights.iter().filter(|h| **h >= 2.0).count() == 4 && *heights.last().unwrap() <= 2.95 + 1e-9);
    // Straight bars from end to end, the cover inside each free end.
    assert!(r.bars.iter().all(|b| b.points.len() == 2 && close(b.length(), 6.0 - 0.10)));
    // A spacing and zones together, zones out of order, a bad face and a bad family are refused.
    for bad in [
        json!({"walls": [wall.0], "horizontal": {"spacing": 0.1, "zones": zones}}),
        json!({"walls": [wall.0], "horizontal": {"zones": [{"up_to": 2.0, "spacing": 0.1}, {"up_to": 1.0, "spacing": 0.1}]}}),
        json!({"walls": [wall.0], "horizontal": {"spacing": 0.001}}),
        json!({"walls": [wall.0], "faces": "top"}),
        json!({"walls": [wall.0], "outer": "diagonal"}),
        json!({"walls": [wall.0], "vertical": {"from": 2.0, "to": 1.0}}),
        json!({"walls": [wall.0], "vertical": {"bottom": {"along": 0.5}}}),
        json!({"walls": [wall.0], "ties": {"every_vertical": 0}}),
    ] {
        assert_eq!(fails(&mut s, "wall_rebar_create", bad.clone()), ErrorCode::InvalidArguments, "{bad}");
    }
    // With the vertical bars outside, the horizontal ones lie a bar further in.
    let other = id(&run(&mut s, "wall_create", json!({"start": [0, 10], "end": [6, 10], "height": 3.0})));
    let args = json!({"walls": [other.0], "faces": "exterior", "cover": 0.03, "outer": "vertical"});
    let sets = created(&run(&mut s, "wall_rebar_create", args));
    let (ext, _) = faces(&mut s, other);
    let geom = s.geometry().clone();
    let (h, v) = (
        &geom.rebar[&family(&s, &sets, true, WallSide::Exterior)],
        &geom.rebar[&family(&s, &sets, false, WallSide::Exterior)],
    );
    assert!(close(v.bars[0].points[0].y, 10.0 + ext - 0.03 - v.diameter / 2.0));
    assert!(close(h.bars[0].points[0].y, 10.0 + ext - 0.03 - v.diameter - h.diameter / 2.0));
}

#[test]
fn vertical_bars_bend_into_the_slab_and_take_additional_bars() {
    let mut s = Session::default();
    let wall = straight(&mut s);
    // Main bars every 20 cm, the bottom end 12 cm towards the outside of the face, then 60 cm up.
    let main = json!({"spacing": 0.20, "bottom": {"across": 0.12, "along": 0.60}, "top": {"across": -0.15}});
    let args = json!({"walls": [wall.0], "vertical": main, "faces": "exterior", "cover": 0.03, "max_length": 0});
    let first = created(&run(&mut s, "wall_rebar_create", args))[0];
    // Additional bars between them over the bottom 1.5 m.
    let extra = json!({"spacing": 0.20, "offset": 0.10, "to": 1.5});
    let args = json!({"walls": [wall.0], "vertical": extra, "faces": "exterior", "cover": 0.03});
    let second = created(&run(&mut s, "wall_rebar_create", args))[0];
    let (ext, _) = faces(&mut s, wall);
    let geom = s.geometry().clone();
    let base = geom.walls[&wall].base_z;
    let (a, b) = (&geom.rebar[&first], &geom.rebar[&second]);
    assert_eq!(a.count(), ((6.0f64 - 0.06) / 0.20 + 1e-9).floor() as usize + 1);
    let y = ext - 0.03 - a.diameter / 2.0;
    for bar in &a.bars {
        // Up the leg, across to the bar, up the wall, and across towards the other face at the top.
        assert_eq!(bar.points.len(), 5);
        let p = &bar.points;
        assert!(close(p[0].y, y + 0.12) && close(p[0].z, base + 0.03 + 0.60));
        assert!(close(p[1].y, y + 0.12) && close(p[1].z, base + 0.03));
        assert!(close(p[2].y, y) && close(p[2].z, base + 0.03) && close(p[3].z, base + 3.0 - 0.03));
        assert!(close(p[4].y, y - 0.15) && close(p[4].z, base + 3.0 - 0.03));
        assert!(close(bar.length(), 3.0 - 0.06 + 0.12 + 0.60 + 0.15));
    }
    // The additional bars: half a spacing on, from the cover to 1.5 m.
    assert!(b.count() >= a.count() - 1);
    assert!(close(b.bars[0].points[0].x - a.bars[0].points[2].x, 0.10));
    assert!(b.bars.iter().all(|bar| bar.points.len() == 2 && close(bar.length(), 1.5 - 0.03)));
    // `element_set` changes a set, and refuses values out of range.
    let params = json!({"layout": {"WallVertical": {"side": "Exterior", "spacing": 0.40}}});
    run(&mut s, "element_set", json!({"id": second.0, "params": params}));
    assert!(s.geometry().rebar[&second].count() < b.count());
    let bad = json!({"layout": {"WallVertical": {"side": "Exterior", "spacing": 0.0}}});
    assert!(s.execute("element_set", json!({"id": second.0, "params": bad})).is_err());
}

#[test]
fn bars_stop_around_doors_and_windows() {
    let mut s = Session::default();
    let wall = straight(&mut s);
    let door = id(&run(&mut s, "door_create", json!({"wall": wall.0, "offset": 3.0})));
    let args = json!({"walls": [wall.0], "faces": "exterior", "cover": 0.03, "max_length": 0});
    let sets = created(&run(&mut s, "wall_rebar_create", args));
    let geom = s.geometry().clone();
    let g = &geom.walls[&wall];
    let opening = g.openings.iter().find(|o| o.element == door).expect("the door's opening");
    let (h, v) = (
        &geom.rebar[&family(&s, &sets, true, WallSide::Exterior)],
        &geom.rebar[&family(&s, &sets, false, WallSide::Exterior)],
    );
    // Below the door's head a level has two bars, one each side, the cover clear of the opening.
    let low: Vec<_> = h.bars.iter().filter(|b| b.points[0].z < opening.z1 - 0.01).collect();
    let high: Vec<_> = h.bars.iter().filter(|b| b.points[0].z > opening.z1 + 0.01).collect();
    assert!(!low.is_empty() && !high.is_empty() && low.len() % 2 == 0);
    for bar in &low {
        let (x0, x1) = (bar.points[0].x, bar.points[1].x);
        assert!(close(x1, opening.u0 - 0.03) || close(x0, opening.u1 + 0.03), "{x0} {x1}");
    }
    assert!(high.iter().all(|b| close(b.length(), 6.0 - 0.06)));
    // Over the door the vertical bars start at its head.
    let over: Vec<_> = v.bars.iter().filter(|b| b.points[0].x > opening.u0 && b.points[0].x < opening.u1).collect();
    assert!(!over.is_empty() && over.iter().all(|b| close(b.points[0].z, opening.z1) && b.length() < 1.5));
    let beside = v.bars.iter().filter(|b| b.points[0].x < opening.u0).count();
    assert!(beside > 5);
}

#[test]
fn ties_link_the_faces_and_long_bars_are_spliced() {
    let mut s = Session::default();
    let wall = straight(&mut s);
    let args = json!({
        "walls": [wall.0], "cover": 0.03, "max_length": 2.5, "lap": 0.5,
        "horizontal": {"spacing": 0.25}, "vertical": {"spacing": 0.25},
        "ties": {"every_horizontal": 2, "every_vertical": 3},
    });
    let sets = created(&run(&mut s, "wall_rebar_create", args));
    assert_eq!(sets.len(), 5);
    let ties = *sets.iter().find(|x| matches!(layout(&s, **x), RebarLayout::WallTies { .. })).unwrap();
    let (ext, int) = faces(&mut s, wall);
    let geom = s.geometry().clone();
    let (h, v) = (
        &geom.rebar[&family(&s, &sets, true, WallSide::Exterior)],
        &geom.rebar[&family(&s, &sets, false, WallSide::Exterior)],
    );
    // 3 m of vertical bar in pieces of at most 2.5 m lapping 0.5 m: two pieces, neighbours cut apart.
    let run_length = 3.0 - 0.06;
    let mut xs: Vec<f64> = v.bars.iter().map(|b| b.points[0].x).collect();
    xs.dedup_by(|a, b| close(*a, *b));
    assert_eq!(xs.len(), ((6.0f64 - 0.06) / 0.25 + 1e-9).floor() as usize + 1);
    let at = |x: f64| v.bars.iter().filter(|b| close(b.points[0].x, x)).map(|b| b.length()).collect::<Vec<f64>>();
    assert_eq!((at(xs[0]).len(), at(xs[1]).len()), (2, 3));
    assert!(
        close(at(xs[0]).iter().sum::<f64>(), run_length + 0.5)
            && close(at(xs[1]).iter().sum::<f64>(), run_length + 1.0)
    );
    assert!(v.bars.iter().chain(&h.bars).all(|b| b.length() <= 2.5 + 1e-9));
    // The horizontal bars too: 5.94 m in three pieces.
    let level = h.bars.iter().filter(|b| close(b.points[0].z, h.bars[0].points[0].z)).count();
    assert_eq!(level, 3);
    // A tie at every second level and every third vertical bar, from one face's bars to the other's.
    let levels = ((3.0f64 - 0.06) / 0.25 + 1e-9).floor() as usize + 1;
    let t = &geom.rebar[&ties];
    assert_eq!(t.count(), levels.div_ceil(2) * xs.len().div_ceil(3));
    let span = (ext - 0.03 - h.diameter / 2.0) - (int + 0.03 + h.diameter / 2.0);
    for bar in &t.bars {
        assert!(
            bar.points.len() == 2 && close(bar.points[0].x, bar.points[1].x) && close(bar.points[0].z, bar.points[1].z)
        );
        assert!(close((bar.points[0].y - bar.points[1].y).abs(), span));
        let hooks = 2.0 * (0.75 * PI * 2.0 * t.diameter + (5.0 * t.diameter).max(0.05));
        assert!(close(bar.length(), span + hooks));
    }
    // Other hooks: 135 degrees at the exterior face and 90 at the interior one are shorter.
    let args = json!({"walls": [wall.0], "ties": {"every_horizontal": 2, "every_vertical": 3, "hooks": [135, 90]}});
    let other = created(&run(&mut s, "wall_rebar_create", args));
    let o = s.geometry().rebar[&other[0]].clone();
    let straight_end = (5.0 * o.diameter).max(0.05);
    let hooks = (0.75 * PI + 0.5 * PI) * 2.0 * o.diameter + 2.0 * straight_end;
    assert!(o.count() == t.count() && o.bars.iter().all(|bar| close(bar.length(), span + hooks)));
    let kept = run(&mut s, "element_get", json!({"id": other[0].0}));
    assert_eq!(kept["params"]["layout"]["WallTies"]["hooks"], json!([135.0, 90.0]), "{kept}");
    // Hooks that are no hooks are refused, here and in an edit.
    let bad = json!({"walls": [wall.0], "ties": {"hooks": [135, 45]}});
    assert_eq!(fails(&mut s, "wall_rebar_create", bad), ErrorCode::InvalidArguments);
    let bad = json!({"id": other[0].0, "params": {"layout": {"WallTies": {"hooks": [200.0, 135.0]}}}});
    assert!(s.execute("element_set", bad).is_err());
    // A file written before hooks had angles reads as 135 degrees at both ends.
    let old: RebarLayout =
        serde_json::from_value(json!({"WallTies": {"every_horizontal": 1, "every_vertical": 2}})).unwrap();
    assert!(matches!(old, RebarLayout::WallTies { hooks, .. } if hooks == [135.0, 135.0]));
    // Ties alone, with no bars to hang them on, make none.
    let bare = id(&run(&mut s, "wall_create", json!({"start": [0, 10], "end": [6, 10], "height": 3.0})));
    let alone = created(&run(&mut s, "wall_rebar_create", json!({"walls": [bare.0], "ties": {}})));
    assert_eq!(s.geometry().rebar[&alone[0]].count(), 0);
}

#[test]
fn only_walls_with_a_body_are_reinforced() {
    let mut s = Session::default();
    let wall = straight(&mut s);
    let beam = id(&run(&mut s, "beam_create", json!({"start": [0, 5], "end": [4, 5]})));
    assert_eq!(fails(&mut s, "wall_rebar_create", json!({"walls": [beam.0]})), ErrorCode::InvalidArguments);
    // The beam and column command sends walls here.
    assert_eq!(
        fails(&mut s, "rebar_create", json!({"host": wall.0, "layout": "stirrups"})),
        ErrorCode::InvalidArguments
    );
    // Without a list: the structural walls only.
    let none = run(&mut s, "wall_rebar_create", json!({}));
    assert_eq!((created(&none).len(), none["skipped"].as_u64()), (0, Some(1)));
    run(&mut s, "element_set", json!({"id": wall.0, "params": {"structural": true}}));
    let all = run(&mut s, "wall_rebar_create", json!({}));
    assert_eq!((created(&all).len(), all["walls"].as_u64(), all["skipped"].as_u64()), (4, Some(1), Some(0)));
    // The sets are schedule rows with their length and weight.
    let rows = run(&mut s, "elements_list", json!({"category": "rebar"}));
    assert_eq!(rows.as_array().unwrap().len(), 4);
    let d = run(&mut s, "element_get", json!({"id": created(&all)[0].0}));
    assert!(d["params"]["layout"]["WallHorizontal"]["zones"].as_array().is_some(), "{d}");
}

/// The ring bars of a cylinder made of two walls lap at the joints when
/// asked: each runs half a lap past the joint, along its own circle.
#[test]
fn ring_bars_lap_at_the_joints_of_a_cylinder() {
    let mut s = Session::default();
    let walls = cylinder(&mut s);
    let ids = [walls[0].0, walls[1].0];
    let butt = created(&run(&mut s, "wall_rebar_create", json!({"walls": ids, "horizontal": {}, "max_length": 0})));
    let args = json!({"walls": ids, "horizontal": {}, "lap": 0.6, "lap_at_joins": true});
    let lapped = created(&run(&mut s, "wall_rebar_create", args));
    let geom = s.geometry().clone();
    let on = |sets: &[ElementId], side| {
        let own: Vec<ElementId> = sets.iter().copied().filter(|x| geom.rebar[x].host == Some(walls[0])).collect();
        geom.rebar[&family(&s, &own, true, side)].clone()
    };
    for side in [WallSide::Exterior, WallSide::Interior] {
        let (plain, laps) = (on(&butt, side), on(&lapped, side));
        assert_eq!(plain.count(), laps.count());
        for (a, b) in plain.bars.iter().zip(&laps.bars) {
            // Half a circle, and 0.3 m past each joint on the same circle.
            let radius = a.points[0].truncate().length();
            assert!(close(a.length(), PI * radius) && close(b.length(), PI * radius + 0.6), "{}", b.length());
            assert!(b.points.iter().all(|p| close(p.truncate().length(), radius)));
        }
    }
    // Whole bars (no stock length) have no lap of their own: 40 diameters.
    let args = json!({"walls": [ids[0]], "horizontal": {}, "faces": "exterior", "max_length": 0, "lap_at_joins": true});
    let default = created(&run(&mut s, "wall_rebar_create", args));
    let set = &s.geometry().rebar[&default[0]];
    let radius = set.bars[0].points[0].truncate().length();
    assert!(close(set.bars[0].length(), PI * radius + 40.0 * set.diameter));

    // At a corner the other wall does not carry on: the bars stop at the joint, as without the option.
    let a = id(&run(&mut s, "wall_create", json!({"start": [10, 0], "end": [14, 0]})));
    run(&mut s, "wall_create", json!({"start": [14, 0], "end": [14, 3]}));
    let plain =
        created(&run(&mut s, "wall_rebar_create", json!({"walls": [a.0], "horizontal": {}, "faces": "exterior"})));
    let args = json!({"walls": [a.0], "horizontal": {}, "faces": "exterior", "lap_at_joins": true});
    let asked = created(&run(&mut s, "wall_rebar_create", args));
    let geom = s.geometry();
    assert!(close(geom.rebar[&plain[0]].total_length(), geom.rebar[&asked[0]].total_length()));
    // Vertical bars and ties are not bars along the wall: they do not keep the option.
    let d = run(&mut s, "element_get", json!({"id": asked[0].0}));
    assert_eq!(d["params"]["lap_at_joins"], json!(true), "{d}");
    let v = created(&run(&mut s, "wall_rebar_create", json!({"walls": [a.0], "vertical": {}, "lap_at_joins": true})));
    let d = run(&mut s, "element_get", json!({"id": v[0].0}));
    assert!(d["params"].get("lap_at_joins").is_none(), "{d}");
}
