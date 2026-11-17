//! Haunched (inverted-T) strip footings, straight and arc, their
//! reinforcement (`strip_rebar_create`), and foundation slabs drawn with
//! curves.

use cc_core::{ElementId, ElementKind, RebarLayout};
use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_2, PI};

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

/// The name of the project's strip footing type.
fn strip_type(s: &Session) -> String {
    let strip = |t: &&cc_core::ElementType| {
        matches!(t.data, cc_core::TypeData::Foundation { kind: cc_core::FoundationKind::Strip, .. })
    };
    s.project().types.values().find(strip).map(|t| t.name.clone()).expect("a strip type")
}

/// Make the strip type an inverted T: a footing 1.6 m wide, 0.4 m thick at
/// the beam and 0.2 m at its edges, under a beam 0.4 m wide and 0.5 m high.
fn haunch(s: &mut Session) {
    let params = json!({
        "width": 1.6, "thickness": 0.4,
        "haunch": {"edge_thickness": 0.2, "beam_width": 0.4, "beam_height": 0.5},
    });
    let name = strip_type(s);
    run(s, "type_set", json!({"type": name, "params": params}));
}

/// The section's area: the two slopes of the footing and the beam through it.
const AREA: f64 = 1.2 * 0.3 + 0.4 * 0.9;

#[test]
fn a_haunched_strip_is_an_inverted_t_swept_along_it() {
    let mut s = Session::default();
    let straight = id(&run(&mut s, "strip_footing_create", json!({"start": [0, 0], "end": [10, 0]})));
    // A quarter ring of radius 3 m around the origin.
    let ring = id(&run(&mut s, "strip_footing_create", json!({"start": [3, 20], "end": [0, 23], "radius": 3.0})));
    let before = s.geometry().foundations[&straight].clone();
    assert!(before.body().is_none() && close(before.volume(), 10.0 * 0.2 * 0.4), "a plain strip of the template");
    haunch(&mut s);
    let geom = s.geometry().clone();
    let (a, b) = (&geom.foundations[&straight], &geom.foundations[&ring]);
    // The top stays where it was: the beam's top; the body is 0.9 m deep.
    assert!(close(a.z_range.1, before.z_range.1) && close(a.z_range.1 - a.z_range.0, 0.9));
    assert!(a.body().is_some() && close(a.volume(), AREA * 10.0), "{}", a.volume());
    // The arc strip: the symmetric section turned about the center (Pappus), exactly.
    assert!((b.volume() - AREA * 3.0 * FRAC_PI_2).abs() < 1e-9, "{}", b.volume());
    assert!(close(geom.volume(straight), AREA * 10.0));
    // The solids: the T, not a box; the arc's chords a little short of the exact volume.
    let solid = |id: ElementId| geom.solids.iter().find(|x| x.element == id).expect("a body");
    assert!((solid(straight).volume() - AREA * 10.0).abs() < 1e-9);
    let chorded = solid(ring).volume();
    assert!(chorded < b.volume() && b.volume() - chorded < b.volume() * 1e-3, "{chorded}");
    let (lo, hi) = solid(straight).bounds().unwrap();
    assert!(close(hi.z - lo.z, 0.9) && close(hi.y - lo.y, 1.6));
    // A haunch that makes no section is refused, and the type keeps its own.
    let name = strip_type(&s);
    for bad in [
        json!({"haunch": {"edge_thickness": 0.9, "beam_width": 0.4, "beam_height": 0.5}}),
        json!({"haunch": {"edge_thickness": 0.2, "beam_width": 2.0, "beam_height": 0.5}}),
        json!({"haunch": {"edge_thickness": 0.2, "beam_width": 0.4, "beam_height": -1.0}}),
    ] {
        assert!(s.execute("type_set", json!({"type": name, "params": bad})).is_err(), "{bad}");
    }
    // Without a beam: a footing with sloped sides and a flat middle.
    let flat = json!({"haunch": {"edge_thickness": 0.2, "beam_width": 0.4, "beam_height": 0.0}});
    run(&mut s, "type_set", json!({"type": name, "params": flat}));
    assert!(close(s.geometry().foundations[&straight].volume(), (1.2 * 0.3 + 0.4 * 0.4) * 10.0));
    // And plain again.
    run(&mut s, "type_set", json!({"type": name, "params": {"haunch": null}}));
    assert!(close(s.geometry().foundations[&straight].volume(), 10.0 * 1.6 * 0.4));
}

#[test]
fn a_ring_footing_is_reinforced_across_along_and_in_its_beam() {
    let mut s = Session::default();
    haunch(&mut s);
    let ring = id(&run(&mut s, "strip_footing_create", json!({"start": [3, 0], "end": [0, 3], "radius": 3.0})));
    let depth = s.doc.undo_depth();
    let args = json!({"strips": [ring.0], "cover": 0.05, "transverse_spacing": 0.2, "longitudinal_spacing": 0.2, "max_length": 0});
    let out = run(&mut s, "strip_rebar_create", args);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let sets = created(&out);
    // Across and along, bottom and top; and the beam's two top bars and stirrups.
    assert_eq!((sets.len(), out["strips"].as_u64()), (6, Some(1)));
    let find = |s: &Session, want: &dyn Fn(&RebarLayout) -> bool| {
        *sets.iter().find(|x| want(&layout(s, **x))).expect("the set")
    };
    let geom = s.geometry().clone();
    let base = geom.foundations[&ring].z_range.0;
    let run_length = 3.0 * FRAC_PI_2 - 0.10;

    // Across, bottom: straight radial bars, the cover inside each edge.
    let set = &geom.rebar[&find(&s, &|l| matches!(l, RebarLayout::StripTransverse { top: false, .. }))];
    assert_eq!(set.count(), (run_length / 0.2).floor() as usize + 1);
    let reach = 0.8 - 0.05 - set.diameter / 2.0;
    for bar in &set.bars {
        let (a, b) = (bar.points[0].truncate(), bar.points[1].truncate());
        assert!(bar.points.len() == 2 && a.perp_dot(b).abs() < 1e-9, "radial");
        assert!(close(bar.length(), 2.0 * reach) && close(bar.points[0].z, base + 0.05 + set.diameter / 2.0));
    }
    // Across, top: down at each edge, up the slopes, flat between the faces of the beam.
    let top = &geom.rebar[&find(&s, &|l| matches!(l, RebarLayout::StripTransverse { top: true, .. }))];
    let c = 0.05 + top.diameter / 2.0;
    let edge = 0.4 + (0.2 - 0.4) * (reach - 0.2) / 0.6 - c;
    for bar in &top.bars {
        assert_eq!(bar.points.len(), 6);
        let z: Vec<f64> = bar.points.iter().map(|p| p.z - base).collect();
        assert!(close(z[0], c) && close(z[5], c), "the ends reach the bottom layer");
        assert!(close(z[1], edge) && close(z[4], edge) && close(z[2], 0.4 - c) && close(z[3], 0.4 - c), "{z:?}");
        // The flat part is as wide as the beam; every point lies on one radial line.
        assert!(close(bar.points[2].distance(bar.points[3]), 0.4));
        assert!(bar.points.windows(2).all(|w| w[0].truncate().perp_dot(w[1].truncate()).abs() < 1e-9));
        let slope = (0.6f64.mul_add(0.0, reach - 0.2).powi(2) + (0.4 - c - edge).powi(2)).sqrt();
        assert!(close(bar.length(), 2.0 * (edge - c) + 2.0 * slope + 0.4));
    }

    // Along, bottom: ring bars each as long as the arc at its own radius, inside the bars across.
    let along = &geom.rebar[&find(&s, &|l| matches!(l, RebarLayout::StripLongitudinal { top: false, .. }))];
    assert_eq!(along.count(), 2 * (reach / 0.2).floor() as usize + 1);
    let angle = run_length / 3.0;
    for bar in &along.bars {
        let radius = bar.points[0].truncate().length();
        assert!(bar.points.iter().all(|p| close(p.truncate().length(), radius)));
        assert!(close(bar.length(), angle * radius));
        assert!(close(bar.points[0].z, base + 0.05 + along.diameter / 2.0 + set.diameter));
    }
    // Along, top: on the slopes only, lower towards the edges.
    let over = &geom.rebar[&find(&s, &|l| matches!(l, RebarLayout::StripLongitudinal { top: true, .. }))];
    assert_eq!(over.count(), along.count() - 1, "none under the beam");
    let height = |b: &cc_geom::rebar::Bar| b.points[0].z - base;
    let mut by_offset: Vec<(f64, f64)> =
        over.bars.iter().map(|b| ((b.points[0].truncate().length() - 3.0).abs(), height(b))).collect();
    by_offset.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(by_offset[0].0 >= 0.2 - 1e-9 && by_offset.windows(2).all(|w| w[1].1 <= w[0].1 + 1e-9), "{by_offset:?}");

    // The beam: two bars along its top, and stirrups over the full height in its width.
    let beam = &geom.rebar[&find(&s, &|l| matches!(l, RebarLayout::Longitudinal { .. }))];
    assert_eq!(beam.count(), 2);
    assert!(beam.bars.iter().all(|b| height(b) > 0.75 && (b.points[0].truncate().length() - 3.0).abs() < 0.2));
    let stirrups = &geom.rebar[&find(&s, &|l| matches!(l, RebarLayout::Stirrups { .. }))];
    let cs = 0.05 + stirrups.diameter / 2.0;
    for bar in &stirrups.bars {
        let z: Vec<f64> = bar.points.iter().map(|p| p.z - base).collect();
        assert!(bar.closed && close(z[0], cs) && close(z[2], 0.9 - cs), "{z:?}");
        assert!(close(bar.points[0].truncate().distance(bar.points[1].truncate()), 0.4 - 2.0 * cs));
    }
    // Deleting the strip takes its reinforcement along.
    run(&mut s, "elements_delete", json!({"ids": [ring.0]}));
    assert!(sets.iter().all(|x| s.project().element(*x).is_none()));
}

#[test]
fn a_plain_strip_gets_the_footing_bars_and_a_beam_only_when_asked() {
    let mut s = Session::default();
    let strip = id(&run(&mut s, "strip_footing_create", json!({"start": [0, 0], "end": [30, 0]})));
    let sets =
        created(&run(&mut s, "strip_rebar_create", json!({"strips": [strip.0], "max_length": 12.0, "lap": 0.5})));
    assert_eq!(sets.len(), 4);
    // The bars along it are spliced past the stock length.
    let along =
        *sets.iter().find(|x| matches!(layout(&s, **x), RebarLayout::StripLongitudinal { top: false, .. })).unwrap();
    let r = s.geometry().rebar[&along].clone();
    assert!(r.count() >= 3 && r.bars.iter().all(|b| b.length() <= 12.0 + 1e-9));
    // Its beam part is the whole section: stirrups around it when asked for.
    let args = json!({"strips": [strip.0], "transverse_spacing": 0, "longitudinal_spacing": 0, "beam": {"top": 0, "stirrups": 0.25, "sides": 1}});
    let beam = created(&run(&mut s, "strip_rebar_create", args));
    assert_eq!(beam.len(), 2);
    let stirrups = *beam.iter().find(|x| matches!(layout(&s, **x), RebarLayout::Stirrups { .. })).unwrap();
    let g = s.geometry().clone();
    let (w, h) = {
        let f = &g.foundations[&strip];
        (f.footprint.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max) * 2.0, f.z_range.1 - f.z_range.0)
    };
    let bar = &g.rebar[&stirrups].bars[0];
    let c = 0.025 + g.rebar[&stirrups].diameter / 2.0;
    assert!(close(bar.length(), 2.0 * (w - 2.0 * c) + 2.0 * (h - 2.0 * c)), "{}", bar.length());
    // Nothing to make, other elements and bad values.
    let none = json!({"strips": [strip.0], "transverse_spacing": 0, "longitudinal_spacing": 0, "beam": false});
    assert_eq!(created(&run(&mut s, "strip_rebar_create", none)).len(), 0);
    let pad = id(&run(&mut s, "footing_create", json!({"position": [50, 0]})));
    for bad in [
        json!({"strips": [pad.0]}),
        json!({"strips": [strip.0], "layers": "middle"}),
        json!({"strips": [strip.0], "transverse_spacing": 0.001}),
        json!({"strips": [strip.0], "beam": {"top": 500}}),
        json!({"strips": [strip.0], "beam": {"legs": 2}}),
    ] {
        assert_eq!(fails(&mut s, "strip_rebar_create", bad.clone()), ErrorCode::InvalidArguments, "{bad}");
    }
    // A slab's or a wall's bars do not go in a strip.
    let slab_bars = json!({"layout": {"SlabStraight": {"top": false, "angle_deg": 0.0, "spacing": 0.2}}});
    assert!(s.execute("element_set", json!({"id": sets[0].0, "params": slab_bars})).is_err());
}

#[test]
fn a_circular_raft_keeps_its_true_curve() {
    let mut s = Session::default();
    let curves = json!([{"kind": "circle", "center": [0, 0], "radius": 2.5}]);
    let raft = id(&run(&mut s, "foundation_slab_create", json!({"curves": curves})));
    let geom = s.geometry().clone();
    let g = &geom.foundations[&raft];
    let thickness = g.z_range.1 - g.z_range.0;
    // The circle's area and volume, not those of the chords of its outline.
    assert!((g.area() - PI * 2.5 * 2.5).abs() < 1e-9, "{}", g.area());
    assert!((g.volume() - PI * 2.5 * 2.5 * thickness).abs() < 1e-9);
    assert!(cc_geom::polygon::area(&g.footprint) < g.area());
    // Queries, schedules and IFC quantities read the same values.
    assert!((geom.volume(raft) - PI * 2.5 * 2.5 * thickness).abs() < 1e-9, "{}", geom.volume(raft));
    let derived = run(&mut s, "element_get", json!({"id": raft.0}))["derived"].clone();
    assert!((derived["area"].as_f64().unwrap() - PI * 2.5 * 2.5).abs() < 1e-9, "{derived}");
    assert!((derived["volume"].as_f64().unwrap() - PI * 2.5 * 2.5 * thickness).abs() < 1e-9, "{derived}");
    // Its sketch can be read back and redrawn.
    let drawn = run(&mut s, "sketch_get", json!({"id": raft.0}));
    assert_eq!(drawn["curves"][0]["kind"], "circle", "{drawn}");
    // Reinforced as a slab: every bar of the mesh ends on the circle.
    let sets = created(&run(
        &mut s,
        "slab_rebar_create",
        json!({"slabs": [raft.0], "layers": "bottom", "spacing": 0.25, "cover": 0.05, "max_length": 0}),
    ));
    let r = s.geometry().rebar[&sets[0]].clone();
    let reach = 2.5 - 0.05 - r.diameter / 2.0;
    for bar in &r.bars {
        let y = bar.points[0].y;
        assert!(close(bar.length(), 2.0 * (reach * reach - y * y).sqrt()), "{y}");
    }
    // An outline with a hole in it is not a raft; moving the points by hand drops the curves.
    let holed = json!([{"kind": "circle", "center": [20, 0], "radius": 2.5}, {"kind": "circle", "center": [20, 0], "radius": 0.5}]);
    assert_eq!(fails(&mut s, "foundation_slab_create", json!({"curves": holed})), ErrorCode::InvalidArguments);
    let square = json!({"shape": {"Slab": {"boundary": [[0, 0], [4, 0], [4, 4], [0, 4]]}}});
    run(&mut s, "element_set", json!({"id": raft.0, "params": square}));
    assert!(close(s.geometry().foundations[&raft].area(), 16.0));
}

/// The ring bars of a footing made of two strips lap at the joints when asked.
#[test]
fn the_bars_along_a_ring_of_strips_lap_at_its_joints() {
    let mut s = Session::default();
    let a = id(&run(&mut s, "strip_footing_create", json!({"start": [3, 0], "end": [-3, 0], "bulge": 1.0})));
    let b = id(&run(&mut s, "strip_footing_create", json!({"start": [-3, 0], "end": [3, 0], "bulge": 1.0})));
    let base =
        json!({"strips": [a.0, b.0], "layers": "bottom", "transverse_spacing": 0, "cover": 0.05, "max_length": 0});
    let plain = created(&run(&mut s, "strip_rebar_create", base.clone()));
    let mut args = base;
    args["lap_at_joins"] = json!(true);
    args["lap"] = json!(0.5);
    args["max_length"] = json!(12.0);
    let lapped = created(&run(&mut s, "strip_rebar_create", args));
    let geom = s.geometry().clone();
    let (plain, lapped) = (&geom.rebar[&plain[0]], &geom.rebar[&lapped[0]]);
    assert!(plain.count() > 0 && plain.count() == lapped.count());
    for (p, l) in plain.bars.iter().zip(&lapped.bars) {
        let radius = p.points[0].truncate().length();
        // Half a ring less the cover at each end; lapped: half a ring and 0.25 m past each joint.
        assert!(close(p.length(), (PI * 3.0 - 0.10) * radius / 3.0), "{}", p.length());
        assert!(close(l.length(), PI * radius + 0.5), "{} at {radius}", l.length());
        assert!(l.points.iter().all(|q| close(q.truncate().length(), radius)));
    }
    // A strip that stands alone has no joint to lap at.
    let alone = id(&run(&mut s, "strip_footing_create", json!({"start": [20, 0], "end": [26, 0]})));
    let args =
        json!({"strips": [alone.0], "layers": "bottom", "transverse_spacing": 0, "cover": 0.05, "lap_at_joins": true});
    let sets = created(&run(&mut s, "strip_rebar_create", args));
    assert!(s.geometry().rebar[&sets[0]].bars.iter().all(|bar| close(bar.length(), 6.0 - 0.10)));
}
