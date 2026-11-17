//! Strip footings that follow arc walls (#22): created along the wall's body
//! centerline as true arcs, joined to straight strips at the crossings of
//! their faces, with exact volumes.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::ElementId;
use cc_engine::{ErrorCode, Session};
use glam::DVec2;
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_2, PI};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

fn created(v: &Value) -> Vec<u64> {
    v["created"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}

fn volume(s: &mut Session, id: u64) -> f64 {
    run(s, "element_get", json!({"id": id}))["derived"]["volume"].as_f64().unwrap()
}

/// The strip's shape as `element_get` shows it: start, end and bulge.
fn strip(s: &mut Session, id: u64) -> (DVec2, DVec2, f64) {
    let shape = run(s, "element_get", json!({"id": id}))["params"]["shape"]["Strip"].clone();
    let p = |v: &Value| DVec2::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
    (p(&shape["start"]), p(&shape["end"]), shape["bulge"].as_f64().unwrap())
}

/// The bulge of a quarter circle turning counter-clockwise.
fn quarter() -> f64 {
    (FRAC_PI_2 / 4.0).tan()
}

#[test]
fn arc_walls_get_arc_strips_joined_to_straight_ones_with_exact_volumes() {
    let mut s = Session::default();
    // A straight wall along +X ending where a quarter circle around the origin starts.
    let straight = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [4, 0]})));
    let arc = id(&run(&mut s, "wall_create", json!({"start": [4, 0], "end": [0, 4], "bulge": quarter()})));
    // A lone quarter circle around (20, 0), meeting nothing.
    let lone = id(&run(&mut s, "wall_create", json!({"start": [24, 0], "end": [20, 4], "bulge": quarter()})));
    let out = run(&mut s, "strip_footings_under_walls", json!({"walls": [straight, arc, lone]}));
    // Arc walls are no longer skipped.
    assert_eq!(out["skipped"], json!(0), "{out}");
    let strips = created(&out);
    assert_eq!(strips.len(), 3, "{out}");
    let [line_strip, arc_strip, lone_strip] = [strips[0], strips[1], strips[2]];

    // The arc strip follows the wall's body centerline: the same ends and bulge.
    let (start, end, bulge) = strip(&mut s, arc_strip);
    assert!(start.distance(DVec2::new(4.0, 0.0)) < 1e-9 && end.distance(DVec2::new(0.0, 4.0)) < 1e-9);
    assert!((bulge - quarter()).abs() < 1e-12);
    let geom = s.geometry();
    let g = &geom.foundations[&ElementId(arc_strip)];
    let a = g.arc.expect("an arc strip");
    assert!(a.arc.center.length() < 1e-9 && (a.arc.radius - 4.0).abs() < 1e-9, "{a:?}");
    // Points along its body centerline lie on the wall's arc.
    for i in 0..=8 {
        let u = a.arc.length() * i as f64 / 8.0;
        assert!((a.world(u, 0.0).length() - 4.0).abs() < 1e-9);
    }
    // The footprint runs along the inner and outer circles (0.20 m wide strip).
    for p in &g.footprint {
        let r = p.length();
        assert!((r - 3.9).abs() < 1e-9 || (r - 4.1).abs() < 1e-9, "{p:?} at {r}");
    }

    // They join at the corner: the straight strip's end corners are the arc's start corners.
    let inner = DVec2::new((3.9f64 * 3.9 - 0.01).sqrt(), 0.1);
    let outer = DVec2::new((4.1f64 * 4.1 - 0.01).sqrt(), -0.1);
    let line_fp = &geom.foundations[&ElementId(line_strip)].footprint;
    for corner in [inner, outer] {
        assert!(line_fp.iter().any(|p| p.distance(corner) < 1e-9), "{corner:?} not in {line_fp:?}");
        assert!(g.footprint.iter().any(|p| p.distance(corner) < 1e-9), "{corner:?} not on the arc strip");
    }

    // Volumes, 0.40 m deep. The lone strip is a quarter of an annulus.
    let depth = 0.4;
    let sector = FRAC_PI_2 / 2.0 * (4.1f64.powi(2) - 3.9f64.powi(2)) * depth;
    assert!((volume(&mut s, lone_strip) - sector).abs() < 1e-9, "{} vs {sector}", volume(&mut s, lone_strip));
    // The joined arc strip runs from the miter to its end: outer arc from the
    // outer corner, the end along the Y axis, inner arc back to the inner corner.
    let (ao, ai) = (outer.y.atan2(outer.x), inner.y.atan2(inner.x));
    let area = 0.5 * (4.1f64.powi(2) * (FRAC_PI_2 - ao) - 3.9f64.powi(2) * (FRAC_PI_2 - ai) + inner.perp_dot(outer));
    assert!(
        (volume(&mut s, arc_strip) - area * depth).abs() < 1e-9,
        "{} vs {}",
        volume(&mut s, arc_strip),
        area * depth
    );
    // The straight strip stops at the miter.
    let trapezoid = 0.2 * (inner.x + outer.x) / 2.0;
    assert!((volume(&mut s, line_strip) - trapezoid * depth).abs() < 1e-9);
    // Its length is along the arc, with its radius.
    let derived = run(&mut s, "element_get", json!({"id": lone_strip}))["derived"].clone();
    assert!((derived["length"].as_f64().unwrap() - 2.0 * PI).abs() < 1e-9, "{derived}");
    assert!((derived["radius"].as_f64().unwrap() - 4.0).abs() < 1e-9, "{derived}");

    // Once is enough: the walls already have their strips (an arc matches only an arc).
    let again = run(&mut s, "strip_footings_under_walls", json!({"walls": [straight, arc, lone]}));
    assert_eq!((created(&again).len(), again["skipped"].as_u64()), (0, Some(3)), "{again}");
    // A straight strip on the chord of an arc wall is not its strip.
    let other = id(&run(&mut s, "wall_create", json!({"start": [34, 0], "end": [30, 4], "bulge": quarter()})));
    run(&mut s, "strip_footing_create", json!({"start": [34, 0], "end": [30, 4]}));
    let out = run(&mut s, "strip_footings_under_walls", json!({"walls": [other]}));
    assert_eq!(created(&out).len(), 1, "{out}");
}

#[test]
fn strip_footings_are_drawn_as_arcs_and_edited() {
    let mut s = Session::default();
    // Through a point on the arc: a half circle of radius 2 around (2, 0).
    let half = id(&run(&mut s, "strip_footing_create", json!({"start": [0, 0], "end": [4, 0], "mid": [2, -2]})));
    let (_, _, bulge) = strip(&mut s, half);
    assert!((bulge - 1.0).abs() < 1e-9, "{bulge}");
    let ring = PI / 2.0 * (2.1f64.powi(2) - 1.9f64.powi(2)) * 0.4;
    assert!((volume(&mut s, half) - ring).abs() < 1e-9);
    // By radius (clockwise when negative) and by bulge.
    let by_radius = id(&run(&mut s, "strip_footing_create", json!({"start": [10, 0], "end": [14, 0], "radius": -2.0})));
    assert!((strip(&mut s, by_radius).2 + 1.0).abs() < 1e-9);
    let by_bulge =
        id(&run(&mut s, "strip_footing_create", json!({"start": [20, 0], "end": [24, 0], "bulge": quarter()})));
    assert!((strip(&mut s, by_bulge).2 - quarter()).abs() < 1e-12);
    // Bad arcs are refused: collinear points, a radius too short, a bulge out of range,
    // an arc too tight for the strip's width.
    for args in [
        json!({"start": [0, 9], "end": [4, 9], "mid": [2, 9]}),
        json!({"start": [0, 9], "end": [4, 9], "radius": 1.0}),
        json!({"start": [0, 9], "end": [4, 9], "bulge": 11.0}),
    ] {
        assert_eq!(fails(&mut s, "strip_footing_create", args), ErrorCode::InvalidArguments);
    }
    let tight = json!({"start": [0, 9], "end": [0.1, 9], "radius": 0.06});
    assert_eq!(fails(&mut s, "strip_footing_create", tight), ErrorCode::Rejected);

    // element_set patches the bulge, and refuses one out of range or too tight.
    run(&mut s, "element_set", json!({"id": by_bulge, "params": {"shape": {"Strip": {"bulge": 0.0}}}}));
    assert_eq!(strip(&mut s, by_bulge).2, 0.0);
    assert!((volume(&mut s, by_bulge) - 4.0 * 0.2 * 0.4).abs() < 1e-9);
    let bad = json!({"id": by_bulge, "params": {"shape": {"Strip": {"bulge": 20.0}}}});
    assert_eq!(fails(&mut s, "element_set", bad), ErrorCode::InvalidArguments);
    // A half circle 0.1 m across has a radius under the strip's half width.
    let tight_patch = json!({"id": by_bulge, "params": {"shape": {"Strip": {"end": [20.1, 0], "bulge": 1.0}}}});
    assert_eq!(fails(&mut s, "element_set", tight_patch), ErrorCode::Rejected);

    // A mirror turns the arc the other way; moving and rotating keep it.
    run(&mut s, "elements_move", json!({"ids": [half], "delta": [1, 1]}));
    assert!((strip(&mut s, half).2 - 1.0).abs() < 1e-9);
    run(&mut s, "elements_rotate", json!({"ids": [half], "center": [0, 0], "angle": 30}));
    assert!((strip(&mut s, half).2 - 1.0).abs() < 1e-9);
    let mirrored = run(&mut s, "elements_mirror", json!({"ids": [half], "axis": [[0, 0], [0, 1]]}));
    let copy = created(&mirrored)[0];
    let (a, b, bulge) = strip(&mut s, copy);
    assert!((bulge + 1.0).abs() < 1e-9);
    // The mirrored half circle bulges to the mirrored side.
    let (a0, b0, _) = strip(&mut s, half);
    let mid = |a: DVec2, b: DVec2, bulge: f64| {
        let arc = cc_core::arc::Arc::from_bulge(a, b, bulge).unwrap();
        arc.point(arc.length() / 2.0)
    };
    let m0 = mid(a0, b0, 1.0);
    assert!(mid(a, b, bulge).distance(DVec2::new(-m0.x, m0.y)) < 1e-9);
    assert!((volume(&mut s, copy) - ring).abs() < 1e-9);
}
