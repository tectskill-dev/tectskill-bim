//! Annular sector and polygonal column sections, and column reinforcement
//! face by face: shared corner bars, ties that follow the outline,
//! cross-ties, lifts and sets given to every column of a type.

use cc_core::{ElementId, ElementKind};
use cc_engine::{ErrorCode, Session};
use glam::DVec2;
use serde_json::{Value, json};

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

/// A column type whose section is the piece of a ring between 2.0 and 2.2 m turning 30°.
fn sector_type(s: &mut Session) -> u64 {
    let params = json!({"r_in": 2.0, "r_out": 2.2, "sweep": 30.0});
    run(
        s,
        "profile_create",
        json!({"name": "Setor", "material": "concrete", "shape": "annular_sector", "params": params}),
    );
    run(s, "profile_type_create", json!({"profile": "Setor", "category": "column"}))["id"].as_u64().unwrap()
}

fn sector_column(s: &mut Session, at: [f64; 2]) -> ElementId {
    let ty = sector_type_once(s);
    id(&run(s, "column_create", json!({"position": at, "type": ty})))
}

fn sector_type_once(s: &mut Session) -> u64 {
    match s.project().types.values().find(|t| s.project().type_sector(&t.data).is_some()) {
        Some(t) => t.id.0,
        None => sector_type(s),
    }
}

/// The plan points of a set's bars at the base (longitudinal) or of its first loop.
fn plan(s: &mut Session, set: ElementId) -> Vec<DVec2> {
    s.geom().rebar[&set].bars.iter().map(|b| b.points[0].truncate()).collect()
}

#[test]
fn an_annular_sector_column_is_a_piece_of_a_ring() {
    let mut s = Session::default();
    let column = sector_column(&mut s, [0.0, 0.0]);
    let d = run(&mut s, "element_get", json!({"id": column.0}))["derived"].clone();
    // The outer arc, a radial side, the inner arc and the other radial side.
    let faces = d["faces"].as_array().unwrap();
    assert_eq!(faces.iter().map(|f| f["curved"].as_bool().unwrap()).collect::<Vec<_>>(), [true, false, true, false]);
    let half = 15f64.to_radians();
    assert!(close(faces[0]["length"].as_f64().unwrap(), 2.2 * 2.0 * half));
    assert!(close(faces[2]["length"].as_f64().unwrap(), 2.0 * 2.0 * half));
    assert!(close(faces[1]["length"].as_f64().unwrap(), 0.2) && close(faces[3]["length"].as_f64().unwrap(), 0.2));
    // The column's volume is the ring's, not that of the chords it is drawn with.
    let area = half * (2.2f64.powi(2) - 2.0f64.powi(2));
    assert!((d["section"]["area"].as_f64().unwrap() - area).abs() < 1e-9, "{d}");
    // The mass per meter is the exact area's (the template's concrete: 2400 kg/m³).
    assert!((d["section"]["linear_mass"].as_f64().unwrap() - area * 2400.0).abs() < 1e-6, "{d}");
    let geom = s.geom().clone();
    let solid = geom.solids.iter().find(|x| x.element == column).expect("a body");
    let (lo, hi) = solid.bounds().unwrap();
    assert!((geom.volume(column) - area * (hi.z - lo.z)).abs() < 1e-9, "{}", geom.volume(column));
    assert!(solid.volume() < geom.volume(column) && solid.faces.iter().any(|f| f.smooth));
    // Values that make no ring are refused.
    let bad = json!({"profile": "Setor", "params": {"r_in": 3.0}});
    assert_eq!(fails(&mut s, "profile_set", bad), ErrorCode::InvalidArguments);
}

/// The issue's example: 7 bars on each curved face and 3 on each radial
/// face, with outline ties every 15 cm.
#[test]
fn bars_by_face_share_the_corner_bars() {
    let mut s = Session::default();
    let column = sector_column(&mut s, [0.0, 0.0]);
    let args = json!({"host": column.0, "layout": "perimeter", "faces": [7, 3, 7, 3], "cover": 0.03});
    let bars = id(&run(&mut s, "rebar_create", args));
    let ties = id(&run(
        &mut s,
        "rebar_create",
        json!({"host": column.0, "layout": "stirrups", "spacing": 0.15, "cover": 0.03}),
    ));
    let geom = s.geom().clone();
    let (set, loops) = (&geom.rebar[&bars], &geom.rebar[&ties]);
    assert_eq!(set.count(), 7 + 7 + 3 + 3 - 4);
    // Vertical, all as long as the column is high.
    let height = set.bars[0].length();
    assert!(height > 1.0 && set.bars.iter().all(|b| close(b.length(), height) && close(b.points[0].x, b.points[1].x)));
    // The ring's center, in the column's frame: below its point, on its Y axis.
    let ty = s.project().element(column).unwrap().type_id;
    let sector = s.project().type_sector(&s.project().element_type(ty).unwrap().data).unwrap();
    // On the curved faces the bars stand on two circles, at even angles.
    let inset = 0.03 + loops.diameter + set.diameter / 2.0;
    let radii: Vec<f64> = set.bars.iter().map(|b| b.points[0].truncate().distance(sector.center)).collect();
    let on = |r: f64| radii.iter().filter(|x| (**x - r).abs() < 1e-9).count();
    assert_eq!((on(2.2 - inset), on(2.0 + inset)), (7, 7), "{radii:?}");
    let outer: Vec<f64> = set
        .bars
        .iter()
        .map(|b| b.points[0].truncate() - sector.center)
        .filter(|p| (p.length() - (2.2 - inset)).abs() < 1e-9)
        .map(|p| p.x.atan2(p.y))
        .collect();
    let mut outer = outer;
    outer.sort_by(f64::total_cmp);
    let step = outer[1] - outer[0];
    assert!(outer.windows(2).all(|w| close(w[1] - w[0], step)), "{outer:?}");
    // The ties run around the outline: two arcs and two radial sides, true lengths.
    let tie = &loops.bars[0];
    assert!(tie.closed && tie.points.len() > 8);
    let c = 0.03 + loops.diameter / 2.0;
    let reach = |r: f64| 15f64.to_radians() - (c / r).asin();
    let (ro, ri) = (2.2 - c, 2.0 + c);
    let side = ((ro * reach(ro).cos() - ri * reach(ri).cos()).powi(2)
        + (ro * reach(ro).sin() - ri * reach(ri).sin()).powi(2))
    .sqrt();
    let exact = 2.0 * ro * reach(ro) + 2.0 * ri * reach(ri) + 2.0 * side;
    assert!((tie.length() - exact).abs() < 1e-9, "{} vs {exact}", tie.length());
    assert_eq!(loops.count(), ((height - 0.06) / 0.15).floor() as usize + 1);
    // The schedule's weight is the hand calculation's.
    let steel = |d: f64, length: f64| std::f64::consts::PI * d * d / 4.0 * length * 7850.0;
    assert!(close(set.weight(), steel(set.diameter, 16.0 * height)));
    assert!((loops.weight() - steel(loops.diameter, loops.count() as f64 * exact)).abs() < 1e-9);
    // By spacing instead of counts.
    let spaced = json!({"host": column.0, "layout": "perimeter", "face_spacing": 0.1, "cover": 0.03});
    let spaced = id(&run(&mut s, "rebar_create", spaced));
    assert!(s.geom().rebar[&spaced].count() > 16);
}

#[test]
fn bars_by_face_are_checked_against_the_section() {
    let mut s = Session::default();
    let column = sector_column(&mut s, [0.0, 0.0]);
    let beam = id(&run(&mut s, "beam_create", json!({"start": [10, 0], "end": [14, 0]})));
    let args = |host: ElementId, extra: Value| {
        let mut v = json!({"host": host.0, "layout": "perimeter"});
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };
    for bad in [
        args(column, json!({"faces": [7, 3, 7]})),
        args(column, json!({"faces": [7, 1, 7, 3]})),
        args(column, json!({"faces": [7, 3, 7, 500]})),
        args(column, json!({"face_spacing": 0.001})),
        args(beam, json!({"faces": [2, 2, 2, 2]})),
        json!({"host": column.0, "layout": "stirrups", "faces": [2, 2, 2, 2]}),
        json!({"host": beam.0, "layout": "ties"}),
    ] {
        assert_eq!(fails(&mut s, "rebar_create", bad.clone()), ErrorCode::InvalidArguments, "{bad}");
    }
    // A plain rectangular column still spreads a count around itself, and takes faces too.
    let plain = id(&run(&mut s, "column_create", json!({"position": [20, 0]})));
    let four = id(&run(&mut s, "rebar_create", json!({"host": plain.0, "layout": "perimeter"})));
    assert_eq!(s.geom().rebar[&four].count(), 4);
    let by_face = id(&run(&mut s, "rebar_create", args(plain, json!({"faces": [3, 2, 3, 2]}))));
    assert_eq!(s.geom().rebar[&by_face].count(), 6);
}

#[test]
fn cross_ties_link_the_bars_between_the_corners() {
    let mut s = Session::default();
    let column = sector_column(&mut s, [0.0, 0.0]);
    // Ties need the longitudinal bars they hook around.
    let alone = id(&run(&mut s, "rebar_create", json!({"host": column.0, "layout": "ties", "spacing": 0.3})));
    assert_eq!(s.geom().rebar[&alone].count(), 0);
    let bars = json!({"host": column.0, "layout": "perimeter", "faces": [7, 3, 7, 3], "cover": 0.03});
    let bars = id(&run(&mut s, "rebar_create", bars));
    let geom = s.geom().clone();
    let (set, ties) = (&geom.rebar[&bars], &geom.rebar[&alone]);
    let height = set.bars[0].length();
    let levels = ((height - 0.05) / 0.3).floor() as usize + 1;
    // Five radial ties between the curved faces and one between the radial faces (which just
    // clears the inner bars), at each level.
    assert_eq!(ties.count(), 6 * levels, "{}", ties.count());
    let main = plan(&mut s, bars);
    let z0 = ties.bars[0].points[0].z;
    let level: Vec<_> = ties.bars.iter().filter(|b| close(b.points[0].z, z0)).collect();
    assert_eq!(level.len(), 6);
    for tie in &level {
        // Both ends stand on bars of the set, and the tie is its span plus its two hooks.
        let (a, b) = (tie.points[0].truncate(), tie.points[1].truncate());
        assert!(main.iter().any(|p| p.distance(a) < 1e-9) && main.iter().any(|p| p.distance(b) < 0.03), "{a} {b}");
        let d = ties.diameter;
        let hooks = (0.75 * std::f64::consts::PI + std::f64::consts::PI) * 2.0 * d + 2.0 * (5.0 * d).max(0.05);
        assert!(close(tie.length(), a.distance(b) + hooks), "{}", tie.length());
    }
    // A rectangular column with eight bars: one tie each way.
    let plain = id(&run(&mut s, "column_create", json!({"position": [20, 0]})));
    run(&mut s, "rebar_create", json!({"host": plain.0, "layout": "perimeter", "count": 8}));
    let cross = id(&run(&mut s, "rebar_create", json!({"host": plain.0, "layout": "ties", "spacing": 0.3})));
    let r = s.geom().rebar[&cross].clone();
    let z0 = r.bars[0].points[0].z;
    assert_eq!(r.bars.iter().filter(|b| close(b.points[0].z, z0)).count(), 2);
}

#[test]
fn a_polygon_section_has_a_face_per_side() {
    let mut s = Session::default();
    // An L of 60 x 60 cm with legs 20 cm thick.
    let pts = [[0.0, 0.0], [0.6, 0.0], [0.6, 0.2], [0.2, 0.2], [0.2, 0.6], [0.0, 0.6]];
    let curves: Vec<Value> =
        (0..6).map(|i| json!({"kind": "line", "start": pts[i], "end": pts[(i + 1) % 6]})).collect();
    run(&mut s, "profile_create", json!({"name": "L 60", "material": "concrete", "curves": curves}));
    let ty =
        run(&mut s, "profile_type_create", json!({"profile": "L 60", "category": "column"}))["id"].as_u64().unwrap();
    let column = id(&run(&mut s, "column_create", json!({"position": [0, 0], "type": ty})));
    let d = run(&mut s, "element_get", json!({"id": column.0}))["derived"].clone();
    let faces = d["faces"].as_array().unwrap();
    assert_eq!(faces.len(), 6);
    assert!(faces.iter().all(|f| !f["curved"].as_bool().unwrap()));
    let lengths: f64 = faces.iter().map(|f| f["length"].as_f64().unwrap()).sum();
    assert!(close(lengths, 2.4));
    // Two bars on each face: one at each of the six corners.
    let bars = json!({"host": column.0, "layout": "perimeter", "faces": [2, 2, 2, 2, 2, 2], "cover": 0.03});
    let bars = id(&run(&mut s, "rebar_create", bars));
    assert_eq!(s.geom().rebar[&bars].count(), 6);
    // A count without faces goes to the corners first, then the longest faces.
    let spread = id(&run(&mut s, "rebar_create", json!({"host": column.0, "layout": "perimeter", "count": 8})));
    assert_eq!(s.geom().rebar[&spread].count(), 8);
    // The tie follows the L: six corners, each side its cover inside.
    let ties = id(&run(&mut s, "rebar_create", json!({"host": column.0, "layout": "stirrups", "cover": 0.03})));
    let r = s.geom().rebar[&ties].clone();
    assert_eq!(r.bars[0].points.len(), 6);
    let c = 0.03 + r.diameter / 2.0;
    assert!(close(r.bars[0].length(), 2.4 - 8.0 * c), "{}", r.bars[0].length());
}

#[test]
fn column_bars_are_cut_in_lifts_and_given_to_a_type() {
    let mut s = Session::default();
    let a = sector_column(&mut s, [0.0, 0.0]);
    let b = sector_column(&mut s, [5.0, 0.0]);
    let other = id(&run(&mut s, "column_create", json!({"position": [20, 0]})));
    run(&mut s, "element_set", json!({"id": a.0, "params": {"top": "Unconnected", "height": 6.0}}));
    let depth = s.doc.undo_depth();
    let args = json!({
        "host": a.0, "layout": "perimeter", "faces": [3, 2, 3, 2], "same_type": true, "max_length": 3.5, "lap": 0.5,
    });
    let out = run(&mut s, "rebar_create", args);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let also = out["also"].as_array().unwrap();
    assert_eq!(also.len(), 1);
    let on_b = ElementId(also[0].as_u64().unwrap());
    let ElementKind::Rebar(r) = &s.project().element(on_b).unwrap().kind else { panic!("rebar") };
    assert_eq!(r.host, b);
    assert!(s.project().rebar_of(other).next().is_none());
    // 6 m in lifts of at most 3.5 m lapping 0.5 m: two pieces of 3.25 m per bar, all cut at one height.
    let geom = s.geom().clone();
    let set = &geom.rebar[&id(&out)];
    assert_eq!(set.count(), 6 * 2);
    assert!(set.bars.iter().all(|x| close(x.length(), 3.25)), "{:?}", set.bars[0]);
    assert!(close(set.total_length(), 6.0 * (6.0 + 0.5)));
    let mut cuts: Vec<f64> = set.bars.iter().map(|x| x.points[0].z).collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| close(*x, *y));
    assert_eq!(cuts.len(), 2);
    // Without `max_length` a column's bars are whole.
    let whole = id(&run(&mut s, "rebar_create", json!({"host": a.0, "layout": "perimeter", "faces": [2, 2, 2, 2]})));
    assert!(s.geom().rebar[&whole].bars.iter().all(|x| close(x.length(), 6.0)));
}

/// A column whose section is the closed outline `pts`.
fn outline_column(s: &mut Session, name: &str, pts: &[[f64; 2]], at: [f64; 2]) -> ElementId {
    let n = pts.len();
    let curves: Vec<Value> =
        (0..n).map(|i| json!({"kind": "line", "start": pts[i], "end": pts[(i + 1) % n]})).collect();
    run(s, "profile_create", json!({"name": name, "material": "concrete", "curves": curves}));
    let ty = run(s, "profile_type_create", json!({"profile": name, "category": "column"}))["id"].as_u64().unwrap();
    id(&run(s, "column_create", json!({"position": at, "type": ty})))
}

/// The issue's option: several closed stirrups that overlap, two in an L and three in a U.
#[test]
fn overlapping_stirrups_go_around_the_rectangles_of_the_section() {
    let mut s = Session::default();
    // An L of 60 x 60 cm with legs 20 cm thick.
    let l = [[0.0, 0.0], [0.6, 0.0], [0.6, 0.2], [0.2, 0.2], [0.2, 0.6], [0.0, 0.6]];
    let column = outline_column(&mut s, "L 60", &l, [0.0, 0.0]);
    let one = id(&run(&mut s, "rebar_create", json!({"host": column.0, "layout": "stirrups", "cover": 0.03})));
    let args = json!({"host": column.0, "layout": "stirrups", "cover": 0.03, "overlapping": true});
    let two = id(&run(&mut s, "rebar_create", args));
    let geom = s.geom().clone();
    let (one, two) = (&geom.rebar[&one], &geom.rebar[&two]);
    assert_eq!(two.count(), 2 * one.count());
    let c = 0.03 + two.diameter / 2.0;
    for bar in &two.bars {
        // Each leg of the L: 60 x 20 cm less the cover all around, closed, with four square corners.
        assert!(bar.closed && bar.points.len() == 4, "{bar:?}");
        assert!(close(bar.length(), 2.0 * (0.6 + 0.2) - 8.0 * c), "{}", bar.length());
    }
    // The two of a level cross: one lies along X, the other along Y.
    let wide = |b: &cc_geom::rebar::Bar| (b.points[1] - b.points[0]).length() > (b.points[2] - b.points[1]).length();
    let level: Vec<_> = two.bars.iter().filter(|b| close(b.points[0].z, two.bars[0].points[0].z)).collect();
    assert_eq!(level.len(), 2);
    assert!(wide(level[0]) != wide(level[1]));

    // A U of 80 x 60 cm with 20 cm legs and base: three.
    let u = [[0.0, 0.0], [0.8, 0.0], [0.8, 0.6], [0.6, 0.6], [0.6, 0.2], [0.2, 0.2], [0.2, 0.6], [0.0, 0.6]];
    let column = outline_column(&mut s, "U 80", &u, [5.0, 0.0]);
    let plain = id(&run(&mut s, "rebar_create", json!({"host": column.0, "layout": "stirrups"})));
    let three = id(&run(&mut s, "rebar_create", json!({"host": column.0, "layout": "stirrups", "overlapping": true})));
    assert_eq!(s.geom().rebar[&three].count(), 3 * s.geom().rebar[&plain].count());

    // Sections that are one rectangle, or not made of rectangles, and other layouts are refused.
    let plain = id(&run(&mut s, "column_create", json!({"position": [10, 0]})));
    let sector = sector_column(&mut s, [15.0, 0.0]);
    let beam = id(&run(&mut s, "beam_create", json!({"start": [0, 8], "end": [4, 8]})));
    for (host, layout) in [(plain, "stirrups"), (sector, "stirrups"), (beam, "stirrups"), (column, "ties")] {
        let bad = json!({"host": host.0, "layout": layout, "overlapping": true});
        assert_eq!(fails(&mut s, "rebar_create", bad), ErrorCode::InvalidArguments, "{layout}");
    }
}

/// Vertical bars cut at given heights (the construction joints), each lift lapping the next.
#[test]
fn column_bars_are_cut_at_given_heights() {
    let mut s = Session::default();
    let column = id(&run(&mut s, "column_create", json!({"position": [0, 0]})));
    run(&mut s, "element_set", json!({"id": column.0, "params": {"top": "Unconnected", "height": 6.0}}));
    // A height above the top is no cut.
    let args = json!({"host": column.0, "layout": "perimeter", "count": 4, "lifts": [2.0, 4.0, 9.0], "lap": 0.5});
    let set = id(&run(&mut s, "rebar_create", args));
    let geom = s.geom().clone();
    let r = &geom.rebar[&set];
    let base = r.bars.iter().map(|b| b.points[0].z).fold(f64::INFINITY, f64::min);
    assert_eq!(r.count(), 4 * 3);
    let mut pieces: Vec<(f64, f64)> = r.bars.iter().map(|b| (b.points[0].z - base, b.length())).collect();
    pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
    pieces.dedup_by(|a, b| close(a.0, b.0) && close(a.1, b.1));
    // From the base to a lap above 2 m, from 2 m to a lap above 4 m, and from 4 m to the top.
    assert_eq!(pieces.len(), 3, "{pieces:?}");
    for (got, want) in pieces.iter().zip([(0.0, 2.5), (2.0, 2.5), (4.0, 2.0)]) {
        assert!(close(got.0, want.0) && close(got.1, want.1), "{pieces:?}");
    }
    assert!(close(r.total_length(), 4.0 * (6.0 + 2.0 * 0.5)));
    // Without a lap of its own: 40 diameters.
    let args = json!({"host": column.0, "layout": "perimeter", "count": 4, "lifts": [3.0]});
    let set = id(&run(&mut s, "rebar_create", args));
    let r = &s.geom().rebar[&set];
    assert!(close(r.total_length(), 4.0 * (6.0 + 40.0 * r.diameter)));
    // Heights that do not rise, and lifts of other bars, are refused; so is an edit to bad ones.
    for bad in [json!([3.0, 2.0]), json!([0.0, 2.0]), json!([-1.0])] {
        let args = json!({"host": column.0, "layout": "perimeter", "lifts": bad});
        assert_eq!(fails(&mut s, "rebar_create", args), ErrorCode::InvalidArguments);
    }
    let args = json!({"host": column.0, "layout": "stirrups", "lifts": [2.0]});
    assert_eq!(fails(&mut s, "rebar_create", args), ErrorCode::InvalidArguments);
    assert!(s.execute("element_set", json!({"id": set.0, "params": {"lifts": [4.0, 1.0]}})).is_err());
}
