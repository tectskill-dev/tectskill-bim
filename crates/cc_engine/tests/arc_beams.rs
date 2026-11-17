//! Arc beams (`beam_create` with `mid`, `bulge` or `radius`) and the
//! reinforcement that follows them: bars at their own radii, radial
//! stirrups, second layers, side bars and lap splices.

use cc_core::{ElementId, ElementKind, TypeData};
use cc_engine::{ErrorCode, Session};
use glam::DVec2;
use serde_json::{Value, json};
use std::f64::consts::FRAC_PI_2;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).expect_err("should fail").code
}

fn id(v: &Value) -> ElementId {
    ElementId(v["id"].as_u64().unwrap())
}

/// Width and height of the beam's type.
fn size(s: &Session, beam: ElementId) -> (f64, f64) {
    let e = s.project().element(beam).unwrap();
    match s.project().element_type(e.type_id).map(|t| &t.data) {
        Some(TypeData::Beam { width, height, .. }) => (*width, *height),
        _ => panic!("a beam type"),
    }
}

/// A quarter circle of radius 2.5 m around the origin, from +X to +Y.
fn quarter(s: &mut Session) -> ElementId {
    id(&run(s, "beam_create", json!({"start": [2.5, 0], "end": [0, 2.5], "radius": 2.5})))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn an_arc_beam_takes_a_mid_point_a_bulge_or_a_radius() {
    let mut s = Session::default();
    let by_radius = quarter(&mut s);
    let m = DVec2::from_angle(FRAC_PI_2 / 2.0) * 2.5;
    let by_mid = id(&run(&mut s, "beam_create", json!({"start": [2.5, 0], "end": [0, 2.5], "mid": [m.x, m.y]})));
    let bulge = (FRAC_PI_2 / 4.0).tan();
    let by_bulge = id(&run(&mut s, "beam_create", json!({"start": [2.5, 0], "end": [0, 2.5], "bulge": bulge})));
    for beam in [by_radius, by_mid, by_bulge] {
        let d = run(&mut s, "element_get", json!({"id": beam.0}))["derived"].clone();
        assert!(close(d["radius"].as_f64().unwrap(), 2.5), "{d}");
        assert!(close(d["sweep"].as_f64().unwrap(), 90.0), "{d}");
        assert!(close(d["true_length"].as_f64().unwrap(), 2.5 * FRAC_PI_2), "{d}");
        let c = d["center"].as_array().unwrap();
        assert!(c[0].as_f64().unwrap().abs() < 1e-9 && c[1].as_f64().unwrap().abs() < 1e-9);
    }
    // The longer arc, and a clockwise one.
    let major = id(&run(
        &mut s,
        "beam_create",
        json!({"start": [12.5, 0], "end": [10, 2.5], "radius": 2.5, "major_arc": true}),
    ));
    let d = run(&mut s, "element_get", json!({"id": major.0}))["derived"].clone();
    assert!(close(d["sweep"].as_f64().unwrap(), 270.0), "{d}");
    let cw = id(&run(&mut s, "beam_create", json!({"start": [20, 2.5], "end": [22.5, 0], "radius": -2.5})));
    let d = run(&mut s, "element_get", json!({"id": cw.0}))["derived"].clone();
    assert!(close(d["sweep"].as_f64().unwrap(), -90.0), "{d}");
}

#[test]
fn an_arc_beam_is_level_and_not_too_tight() {
    let mut s = Session::default();
    let args = json!({"start": [2.5, 0], "end": [0, 2.5], "radius": 2.5, "top_offset": 3.0, "end_offset": 3.5});
    assert_eq!(fails(&mut s, "beam_create", args), ErrorCode::InvalidArguments);
    let tight = json!({"start": [0.05, 0], "end": [0, 0.05], "radius": 0.05});
    assert_eq!(fails(&mut s, "beam_create", tight), ErrorCode::Rejected);
    assert_eq!(
        fails(&mut s, "beam_create", json!({"start": [2.5, 0], "end": [0, 2.5], "bulge": 50})),
        ErrorCode::InvalidArguments
    );
    // The same checks on `element_set`.
    let beam = quarter(&mut s);
    let sloped = json!({"id": beam.0, "params": {"end_offset": 4.0}});
    assert_eq!(fails(&mut s, "element_set", sloped), ErrorCode::InvalidArguments);
    let bent = json!({"id": beam.0, "params": {"bulge": 50.0}});
    assert_eq!(fails(&mut s, "element_set", bent), ErrorCode::InvalidArguments);
    // A straight beam becomes an arc, and an arc straight again.
    run(&mut s, "element_set", json!({"id": beam.0, "params": {"bulge": 1.0}}));
    let d = run(&mut s, "element_get", json!({"id": beam.0}))["derived"].clone();
    assert!(close(d["sweep"].as_f64().unwrap(), 180.0), "{d}");
    run(&mut s, "element_set", json!({"id": beam.0, "params": {"bulge": 0.0}}));
    let d = run(&mut s, "element_get", json!({"id": beam.0}))["derived"].clone();
    assert!(d.get("radius").is_none(), "{d}");
}

#[test]
fn a_ring_beam_frames_into_the_columns_at_its_ends() {
    let mut s = Session::default();
    let c0 = id(&run(&mut s, "column_create", json!({"position": [2.5, 0]})));
    let c1 = id(&run(&mut s, "column_create", json!({"position": [0, 2.5]})));
    let beam = quarter(&mut s);
    let free = id(&run(&mut s, "beam_create", json!({"start": [12.5, 0], "end": [10, 2.5], "radius": 2.5})));
    let (width, height) = size(&s, beam);
    let geom = s.geom().clone();
    let (g, f) = (&geom.beams[&beam], &geom.beams[&free]);
    assert_eq!((g.start, g.end), (cc_geom::BeamEnd::Column(c0), cc_geom::BeamEnd::Column(c1)));
    let arc = g.arc.expect("an arc body");
    assert!(close(arc.radius, 2.5) && close(g.length, 2.5 * FRAC_PI_2));
    // Cut back at both columns: shorter than the free beam, and symmetric.
    assert!(g.cut_length() < f.cut_length() - 0.1, "{} vs {}", g.cut_length(), f.cut_length());
    assert!(
        (g.start_ext.0 - (g.length - g.end_ext.0)).abs() < 1e-6 && g.start_ext.0 > g.start_ext.1,
        "{:?} {:?}",
        g.start_ext,
        g.end_ext
    );
    // Each side stops on the face of the column: the corner lies on its outline.
    let e = s.project().element(c0).unwrap().clone();
    let ElementKind::Column(c) = &e.kind else { panic!("a column") };
    let data = &s.project().element_type(e.type_id).unwrap().data;
    let outline = cc_geom::column_outline(s.project(), data, c.position, c.rotation_deg);
    let on_outline = |p: DVec2| {
        (0..outline.len())
            .map(|i| cc_geom::polygon::closest_on_segment(outline[i], outline[(i + 1) % outline.len()], p).0)
            .any(|q| q.distance(p) < 1e-6)
    };
    assert!(on_outline(g.world(g.start_ext.0, g.half)) && on_outline(g.world(g.start_ext.1, -g.half)));
    // The free beam's volume is the section turned through the quarter circle (Pappus).
    let exact = width * height * 2.5 * FRAC_PI_2;
    assert!((geom.volume(free) - exact).abs() < 1e-9, "{} vs {exact}", geom.volume(free));
    // Its solid meshes the arcs with chords, a little short of that.
    let solid = geom.solids.iter().find(|x| x.element == free).expect("a body");
    assert!(solid.volume() < exact && exact - solid.volume() < exact * 1e-3, "{}", solid.volume());
    assert!(solid.faces.iter().any(|x| x.smooth) && solid.faces.iter().any(|x| !x.smooth));
    // Plans draw the arcs, and the schedule's cut length runs along the axis.
    assert!(f.outline().len() > 8);
    let d = run(&mut s, "element_get", json!({"id": free.0}))["derived"].clone();
    assert!(close(d["cut_length"].as_f64().unwrap(), 2.5 * FRAC_PI_2), "{d}");
}

/// The issue's reference: for R = 2.5 m and a 90° arc, each bar's length is
/// the arc length at the bar's own radius.
#[test]
fn bars_along_an_arc_beam_measure_the_arc_at_their_radius() {
    let mut s = Session::default();
    let beam = quarter(&mut s);
    let cover = 0.03;
    let set = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "bottom", "count": 3, "cover": cover})));
    let top = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "top", "count": 2, "cover": cover})));
    let geom = s.geom().clone();
    // The angle the bars turn: the axis less the cover at both ends.
    let angle = (2.5 * FRAC_PI_2 - 2.0 * cover) / 2.5;
    let mut total = 0.0;
    for r in [&geom.rebar[&set], &geom.rebar[&top]] {
        for bar in &r.bars {
            // Every point of the bar lies on one circle around the beam's center.
            let radius = bar.points[0].truncate().length();
            assert!(bar.points.len() > 10 && bar.points.iter().all(|p| close(p.truncate().length(), radius)));
            assert!(close(bar.length(), angle * radius), "{} vs {}", bar.length(), angle * radius);
            total += angle * radius;
        }
        // The bars keep one height.
        assert!(r.bars.iter().all(|b| b.points.iter().all(|p| close(p.z, b.points[0].z))));
    }
    // Three bars across the bottom: on the axis and at the same distance either side.
    let radii: Vec<f64> = geom.rebar[&set].bars.iter().map(|b| b.points[0].truncate().length()).collect();
    assert_eq!(radii.len(), 3);
    assert!(close(radii[1], 2.5) && close(radii[0] + radii[2], 5.0) && (radii[0] - radii[2]).abs() > 0.05, "{radii:?}");
    // The schedule's length and weight are the hand calculation's.
    let lengths = geom.rebar[&set].total_length() + geom.rebar[&top].total_length();
    assert!(close(lengths, total));
    let d = geom.rebar[&set].diameter;
    let weight = std::f64::consts::PI * d * d / 4.0 * geom.rebar[&set].total_length() * 7850.0;
    assert!(close(geom.rebar[&set].weight(), weight));
}

#[test]
fn stirrups_of_an_arc_beam_are_radial() {
    let mut s = Session::default();
    let beam = quarter(&mut s);
    let args = json!({"host": beam.0, "layout": "stirrups", "spacing": 0.15, "cover": 0.03});
    let set = id(&run(&mut s, "rebar_create", args));
    let geom = s.geom().clone();
    let r = &geom.rebar[&set];
    // Spaced along the axis: 3.867 m at 0.15.
    let run_length = 2.5 * FRAC_PI_2 - 0.06;
    assert_eq!(r.count(), (run_length / 0.15).floor() as usize + 1);
    let mut angles = Vec::new();
    for bar in &r.bars {
        assert!(bar.closed && bar.points.len() == 4);
        // In plan a stirrup lies on a line through the center.
        let (a, b) = (bar.points[0].truncate(), bar.points[1].truncate());
        assert!(a.perp_dot(b).abs() < 1e-9, "{a} {b}");
        angles.push(a.y.atan2(a.x));
    }
    for w in angles.windows(2) {
        assert!(close((w[1] - w[0]) * 2.5, 0.15), "{}", (w[1] - w[0]) * 2.5);
    }
}

#[test]
fn bars_that_do_not_fit_go_in_a_second_layer() {
    let mut s = Session::default();
    let beam = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [5, 0]})));
    let (width, _) = size(&s, beam);
    let few = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "bottom", "count": 2})));
    let many = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "top", "count": 40})));
    let geom = s.geom().clone();
    let heights = |set: ElementId| {
        let mut z: Vec<f64> = geom.rebar[&set].bars.iter().map(|b| b.points[0].z).collect();
        z.sort_by(f64::total_cmp);
        z.dedup_by(|a, b| close(*a, *b));
        z
    };
    assert_eq!(heights(few).len(), 1);
    let layers = heights(many);
    assert!(layers.len() > 1, "{layers:?}");
    assert_eq!(geom.rebar[&many].count(), 40);
    // Layers are a bar and the clear distance apart, and bars in a layer no closer.
    let d = geom.rebar[&many].diameter;
    let pitch = d + d.max(0.02);
    assert!(close(layers[1] - layers[0], pitch), "{layers:?}");
    let top = layers.last().copied().unwrap();
    let mut across: Vec<f64> =
        geom.rebar[&many].bars.iter().filter(|b| close(b.points[0].z, top)).map(|b| b.points[0].y).collect();
    across.sort_by(f64::total_cmp);
    assert!(across.len() >= 2 && across.len() < 40 && across.last().unwrap() - across[0] < width);
    assert!(across.windows(2).all(|w| w[1] - w[0] >= pitch - 1e-9), "{across:?}");
}

#[test]
fn side_bars_run_on_both_faces_by_count_or_spacing() {
    let mut s = Session::default();
    let beam = quarter(&mut s);
    let (_, height) = size(&s, beam);
    let column = id(&run(&mut s, "column_create", json!({"position": [20, 20]})));
    assert_eq!(
        fails(&mut s, "rebar_create", json!({"host": column.0, "layout": "sides"})),
        ErrorCode::InvalidArguments
    );
    let one = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "sides"})));
    let two = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "sides", "count": 2})));
    let spaced = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "sides", "spacing": 0.05})));
    let geom = s.geom().clone();
    assert_eq!((geom.rebar[&one].count(), geom.rebar[&two].count()), (2, 4));
    // One bar per face at mid height, the two faces at different radii.
    let bars = &geom.rebar[&one].bars;
    let (lo, hi) = geom.beams[&beam].z_range;
    assert!(bars.iter().all(|b| close(b.points[0].z, (lo + hi) * 0.5)));
    let radii: Vec<f64> = bars.iter().map(|b| b.points[0].truncate().length()).collect();
    assert!(close(radii[0] + radii[1], 5.0) && (radii[0] - radii[1]).abs() > 0.05, "{radii:?}");
    // Each follows the arc at its radius.
    assert!(bars.iter().zip(&radii).all(|(b, r)| close(b.length(), b.length() / r * r) && b.points.len() > 10));
    // By spacing: as many as keep them 5 cm apart over the height, on both faces.
    let n = geom.rebar[&spaced].count();
    assert!(n >= 4 && n % 2 == 0 && (n / 2) as f64 >= (height - 0.2) / 0.05 - 1.0, "{n}");
    // `element_set` changes the count.
    run(&mut s, "element_set", json!({"id": one.0, "params": {"layout": {"Sides": {"count": 3, "spacing": 0.0}}}}));
    assert_eq!(s.geom().rebar[&one].count(), 6);
}

#[test]
fn bars_longer_than_the_stock_are_lap_spliced_and_staggered() {
    let mut s = Session::default();
    let beam = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [30, 0]})));
    let args = json!({"host": beam.0, "layout": "bottom", "count": 2, "cover": 0.025, "max_length": 12.0, "lap": 0.5});
    let set = id(&run(&mut s, "rebar_create", args));
    let whole = json!({"host": beam.0, "layout": "top", "count": 2, "cover": 0.025, "max_length": 0});
    let whole = id(&run(&mut s, "rebar_create", whole));
    let geom = s.geom().clone();
    let run_length = 30.0 - 0.05;
    assert_eq!(geom.rebar[&whole].count(), 2);
    assert!(close(geom.rebar[&whole].total_length(), 2.0 * run_length));
    // 29.95 m in pieces of at most 12 m lapping 0.5 m: three pieces, and
    // four in the neighbouring bar, cut half a piece off.
    let r = &geom.rebar[&set];
    let line = |y: f64| -> Vec<(f64, f64)> {
        let mut v: Vec<(f64, f64)> =
            r.bars.iter().filter(|b| close(b.points[0].y, y)).map(|b| (b.points[0].x, b.points[1].x)).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v
    };
    let mut ys: Vec<f64> = r.bars.iter().map(|b| b.points[0].y).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| close(*a, *b));
    assert_eq!(ys.len(), 2);
    let (even, odd) = (line(ys[0]), line(ys[1]));
    assert_eq!((even.len(), odd.len()), (3, 4), "{even:?} {odd:?}");
    for pieces in [&even, &odd] {
        assert!(close(pieces[0].0, 0.025) && close(pieces.last().unwrap().1, 30.0 - 0.025));
        assert!(pieces.iter().all(|(a, b)| b - a <= 12.0 + 1e-9 && b - a >= 0.5));
        assert!(pieces.windows(2).all(|w| close(w[0].1 - w[1].0, 0.5)), "each piece laps the next by 0.5 m");
    }
    assert!(close(r.total_length(), 2.0 * run_length + 2.0 * 0.5 + 3.0 * 0.5));
    // No splice of one bar lies within a lap of a splice of the other.
    let splices = |p: &[(f64, f64)]| p.windows(2).map(|w| (w[1].0 + w[0].1) * 0.5).collect::<Vec<f64>>();
    for a in splices(&even) {
        assert!(splices(&odd).iter().all(|b| (a - b).abs() > 1.0), "{even:?} {odd:?}");
    }
    // The default stock is 12 m with a lap of 40 diameters.
    let default = id(&run(&mut s, "rebar_create", json!({"host": beam.0, "layout": "sides"})));
    let ElementKind::Rebar(x) = &s.project().element(default).unwrap().kind else { panic!("rebar") };
    let splice = x.splice.expect("spliced by default");
    let d = s.geom().rebar[&default].diameter;
    assert!(close(splice.max_length, 12.0) && close(splice.lap, 40.0 * d), "{splice:?}");
    assert!(s.geom().rebar[&default].bars.iter().all(|b| b.length() <= 12.0 + 1e-9));
    // Out of range values are refused.
    let bad = json!({"host": beam.0, "layout": "bottom", "max_length": 0.5});
    assert_eq!(fails(&mut s, "rebar_create", bad), ErrorCode::InvalidArguments);
    let bad = json!({"host": beam.0, "layout": "bottom", "max_length": 6.0, "lap": 4.0});
    assert_eq!(fails(&mut s, "rebar_create", bad), ErrorCode::InvalidArguments);
}

/// Along an arc the stock length is measured on each bar's own arc.
#[test]
fn splices_follow_the_arc() {
    let mut s = Session::default();
    // Three quarters of a circle of radius 5 m: 23.56 m of axis.
    let args = json!({"start": [5, 0], "end": [0, -5], "radius": 5.0, "major_arc": true});
    let beam = id(&run(&mut s, "beam_create", args));
    let args = json!({"host": beam.0, "layout": "bottom", "count": 2, "max_length": 12.0, "lap": 0.6});
    let set = id(&run(&mut s, "rebar_create", args));
    let geom = s.geom().clone();
    let r = &geom.rebar[&set];
    assert!(r.count() >= 5, "{}", r.count());
    for bar in &r.bars {
        let radius = bar.points[0].truncate().length();
        assert!(bar.points.iter().all(|p| close(p.truncate().length(), radius)));
        assert!(bar.length() <= 12.0 + 1e-9 && bar.length() > 0.6);
        // The length is the angle the piece turns times its radius.
        let (a, b) = (bar.points[0].truncate(), bar.points.last().unwrap().truncate());
        let turn = a.angle_to(b).rem_euclid(std::f64::consts::TAU);
        assert!((bar.length() - turn * radius).abs() < 1e-6, "{} vs {}", bar.length(), turn * radius);
    }
}

#[test]
fn arc_beams_mirror_split_and_offset() {
    let mut s = Session::default();
    let beam = quarter(&mut s);
    let bulge = |s: &Session, id: ElementId| match &s.project().element(id).unwrap().kind {
        ElementKind::Beam(b) => b.bulge,
        _ => panic!("a beam"),
    };
    let before = bulge(&s, beam);
    assert!(before > 0.0);
    s.doc.edit("mirror", |e| {
        e.transform_elements(&[beam], &cc_core::PlanTransform::mirror_across(DVec2::ZERO, DVec2::X));
    });
    assert!(close(bulge(&s, beam), -before));
    // Splitting keeps both parts on the circle, each with its share of the sweep.
    let ElementKind::Beam(b) = s.project().element(beam).unwrap().kind.clone() else { panic!() };
    let arc = b.arc().unwrap();
    let at = arc.point(arc.length() / 3.0);
    let mut second = None;
    s.doc.edit("split", |e| second = e.split(beam, at).ok());
    let second = second.expect("split");
    let arcs: Vec<cc_core::arc::Arc> = [beam, second]
        .iter()
        .map(|id| match &s.project().element(*id).unwrap().kind {
            ElementKind::Beam(b) => b.arc().unwrap(),
            _ => panic!(),
        })
        .collect();
    assert!(arcs.iter().all(|a| close(a.radius, 2.5) && a.center.length() < 1e-9));
    assert!(
        close(arcs[0].length() * 2.0, arcs[1].length()) && close(arcs[0].length() + arcs[1].length(), arc.length())
    );
    // An offset copy is concentric.
    let mut copy = None;
    s.doc.edit("offset", |e| copy = e.offset_copy(beam, 0.5).ok());
    let ElementKind::Beam(c) = &s.project().element(copy.expect("copy")).unwrap().kind else { panic!() };
    let c = c.arc().unwrap();
    assert!(c.center.length() < 1e-9 && close((c.radius - 2.5).abs(), 0.5) && close(c.sweep, arcs[0].sweep));
}

/// Bars along beams that carry on from one another lap at the joint when asked.
#[test]
fn bars_lap_where_a_beam_carries_on_from_another() {
    let mut s = Session::default();
    let a = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [4, 0]})));
    let b = id(&run(&mut s, "beam_create", json!({"start": [4, 0], "end": [9, 0]})));
    let args = |host: ElementId, laps: bool| json!({"host": host.0, "layout": "bottom", "count": 2, "cover": 0.025, "max_length": 0, "lap": 0.4, "lap_at_joins": laps});
    let plain = id(&run(&mut s, "rebar_create", args(a, false)));
    let lapped = [id(&run(&mut s, "rebar_create", args(a, true))), id(&run(&mut s, "rebar_create", args(b, true)))];
    let geom = s.geom().clone();
    assert!(geom.rebar[&plain].bars.iter().all(|bar| close(bar.length(), 4.0 - 0.05)));
    // The free end keeps its cover; the joined one runs half a lap past the joint.
    for bar in &geom.rebar[&lapped[0]].bars {
        let (x0, x1) = (bar.points[0].x.min(bar.points[1].x), bar.points[0].x.max(bar.points[1].x));
        assert!(close(x0, 0.025) && close(x1, 4.2), "{x0} {x1}");
    }
    for bar in &geom.rebar[&lapped[1]].bars {
        let (x0, x1) = (bar.points[0].x.min(bar.points[1].x), bar.points[0].x.max(bar.points[1].x));
        assert!(close(x0, 3.8) && close(x1, 9.0 - 0.025), "{x0} {x1}");
    }
    // Stirrups are not bars along the beam.
    let st = id(&run(&mut s, "rebar_create", json!({"host": a.0, "layout": "stirrups", "lap_at_joins": true})));
    assert!(run(&mut s, "element_get", json!({"id": st.0}))["params"].get("lap_at_joins").is_none());
}
