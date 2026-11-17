//! Reinforcing bars as other tools write them (IFC2X3, no type objects,
//! swept disk bodies) become native shaped bars of the reinforcement
//! category, one type per diameter and grade (#263). Objects of other
//! classes with the same geometry come in as components: identical ones
//! share one component type, whether their geometry is written in a local
//! placement or in world coordinates, and ones of different shapes do not.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::Component;
use cc_core::properties::{Atom, IfcValue, PropertySet, PropertyValue};
use cc_core::{Category, Element, ElementKind, Project, ShapedRebar, TypeData};
use cc_ifc::{ExportOptions, export, import};

/// Bars A and B: the same L shape written in world coordinates (their
/// placement is the storey's) at two places; C: another shape with A's
/// bounds size; F and G: U shapes opening up and down with equal bounds in
/// their own local placements, the origin on the bar; H: G's shape in another placement,
/// turned in plan.
const BARS: &str = "ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC2X3'));
ENDSEC;
DATA;
#1=IFCPROJECT('0YvctVUKr0kugbFTf53O9L',$,'Tank',$,$,$,$,(#2),#3);
#2=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#12,$);
#3=IFCUNITASSIGNMENT((#4));
#4=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);
#10=IFCBUILDINGSTOREY('1XvctVUKr0kugbFTf53O9L',$,'Level 1',$,$,#11,$,$,.ELEMENT.,0.);
#11=IFCLOCALPLACEMENT($,#12);
#12=IFCAXIS2PLACEMENT3D(#13,$,$);
#13=IFCCARTESIANPOINT((0.,0.,0.));
#14=IFCDIRECTION((0.,0.,1.));
#15=IFCDIRECTION((0.,1.,0.));
#20=IFCRELCONTAINEDINSPATIALSTRUCTURE('3XvctVUKr0kugbFTf53O9L',$,$,$,(#100,#110,#120,#130,#140,#150),#10);
#100=IFCREINFORCINGBAR('2AvctVUKr0kugbFTf53O9L',$,'Bar 12',$,$,#11,#101,'A','CA-50',12.,113.1,1306.,.MAIN.,.TEXTURED.);
#101=IFCPRODUCTDEFINITIONSHAPE($,$,(#102));
#102=IFCSHAPEREPRESENTATION(#2,'Body','AdvancedSweptSolid',(#103));
#103=IFCSWEPTDISKSOLID(#104,6.,$,$,$);
#104=IFCPOLYLINE((#105,#106,#107));
#105=IFCCARTESIANPOINT((5000.,2000.,0.));
#106=IFCCARTESIANPOINT((6000.,2000.,0.));
#107=IFCCARTESIANPOINT((6000.,2300.,0.));
#110=IFCREINFORCINGBAR('2BvctVUKr0kugbFTf53O9L',$,'Bar 12',$,$,#11,#111,'B','CA-50',12.,113.1,1306.,.MAIN.,.TEXTURED.);
#111=IFCPRODUCTDEFINITIONSHAPE($,$,(#112));
#112=IFCSHAPEREPRESENTATION(#2,'Body','AdvancedSweptSolid',(#113));
#113=IFCSWEPTDISKSOLID(#114,6.,$,$,$);
#114=IFCPOLYLINE((#115,#116,#117));
#115=IFCCARTESIANPOINT((8000.,2000.,0.));
#116=IFCCARTESIANPOINT((9000.,2000.,0.));
#117=IFCCARTESIANPOINT((9000.,2300.,0.));
#120=IFCREINFORCINGBAR('2CvctVUKr0kugbFTf53O9L',$,'Bar 12',$,$,#11,#121,'C','CA-50',12.,113.1,1306.,.MAIN.,.TEXTURED.);
#121=IFCPRODUCTDEFINITIONSHAPE($,$,(#122));
#122=IFCSHAPEREPRESENTATION(#2,'Body','AdvancedSweptSolid',(#123));
#123=IFCSWEPTDISKSOLID(#124,6.,$,$,$);
#124=IFCPOLYLINE((#125,#126,#127));
#125=IFCCARTESIANPOINT((5000.,4300.,0.));
#126=IFCCARTESIANPOINT((5000.,4000.,0.));
#127=IFCCARTESIANPOINT((6000.,4000.,0.));
#130=IFCREINFORCINGBAR('2FvctVUKr0kugbFTf53O9L',$,'Bar 10',$,$,#131,#133,'F','CA-50',10.,78.5,1300.,.MAIN.,.TEXTURED.);
#131=IFCLOCALPLACEMENT(#11,#132);
#132=IFCAXIS2PLACEMENT3D(#139,$,$);
#139=IFCCARTESIANPOINT((0.,10000.,0.));
#133=IFCPRODUCTDEFINITIONSHAPE($,$,(#134));
#134=IFCSHAPEREPRESENTATION(#2,'Body','AdvancedSweptSolid',(#135));
#135=IFCSWEPTDISKSOLID(#136,5.,$,$,$);
#136=IFCPOLYLINE((#137,#13,#138,#148));
#137=IFCCARTESIANPOINT((0.,300.,0.));
#138=IFCCARTESIANPOINT((1000.,0.,0.));
#140=IFCREINFORCINGBAR('2GvctVUKr0kugbFTf53O9L',$,'Bar 10',$,$,#141,#143,'G','CA-50',10.,78.5,1300.,.MAIN.,.TEXTURED.);
#141=IFCLOCALPLACEMENT(#11,#142);
#142=IFCAXIS2PLACEMENT3D(#149,$,$);
#149=IFCCARTESIANPOINT((3000.,10000.,0.));
#143=IFCPRODUCTDEFINITIONSHAPE($,$,(#144));
#144=IFCSHAPEREPRESENTATION(#2,'Body','AdvancedSweptSolid',(#145));
#145=IFCSWEPTDISKSOLID(#146,5.,$,$,$);
#146=IFCPOLYLINE((#160,#161,#162,#163));
#148=IFCCARTESIANPOINT((1000.,300.,0.));
#160=IFCCARTESIANPOINT((0.,-5.,0.));
#161=IFCCARTESIANPOINT((0.,295.,0.));
#162=IFCCARTESIANPOINT((1000.,295.,0.));
#163=IFCCARTESIANPOINT((1000.,-5.,0.));
#150=IFCREINFORCINGBAR('2HvctVUKr0kugbFTf53O9L',$,'Bar 10',$,$,#151,#143,'H','CA-50',10.,78.5,1300.,.MAIN.,.TEXTURED.);
#151=IFCLOCALPLACEMENT(#11,#152);
#152=IFCAXIS2PLACEMENT3D(#159,#14,#15);
#159=IFCCARTESIANPOINT((7000.,10000.,500.));
#200=IFCPROPERTYSET('4AvctVUKr0kugbFTf53O9L',$,'Custom',$,(#201));
#201=IFCPROPERTYSINGLEVALUE('Partition',$,IFCLABEL('P1'),$);
#202=IFCRELDEFINESBYPROPERTIES('4BvctVUKr0kugbFTf53O9L',$,$,$,(#100),#200);
#210=IFCPROPERTYSET('4CvctVUKr0kugbFTf53O9L',$,'Custom',$,(#211));
#211=IFCPROPERTYSINGLEVALUE('Partition',$,IFCLABEL('P2'),$);
#212=IFCRELDEFINESBYPROPERTIES('4DvctVUKr0kugbFTf53O9L',$,$,$,(#110),#210);
ENDSEC;
END-ISO-10303-21;
";

fn bar<'a>(p: &'a Project, mark: &str) -> (&'a Element, &'a Component) {
    p.elements
        .values()
        .find_map(|e| match &e.kind {
            ElementKind::Component(c) if e.mark == mark => Some((e, c)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no bar {mark}"))
}

fn partition(sets: &[PropertySet]) -> Option<String> {
    sets.iter().filter(|s| s.name == "Custom").flat_map(|s| &s.properties).find_map(|p| match &p.value {
        PropertyValue::Single(Some(IfcValue { value: Atom::Text(t), .. })) => Some(t.clone()),
        _ => None,
    })
}

/// The bars as objects of a class with no native element (proxies).
fn proxies() -> String {
    BARS.replace("IFCREINFORCINGBAR(", "IFCBUILDINGELEMENTPROXY(")
}

#[test]
fn identical_objects_share_one_type() {
    let (p, report) = import(&proxies(), Project::from_template()).unwrap();
    assert_eq!(report.generic.values().sum::<usize>(), 6, "{report:?}");
    let (a, ca) = bar(&p, "A");
    let (b, cb) = bar(&p, "B");
    let (c, _) = bar(&p, "C");
    let (f, _) = bar(&p, "F");
    let (g, cg) = bar(&p, "G");
    let (h, ch) = bar(&p, "H");
    // The same shape in world coordinates at two places: one type.
    assert_eq!(a.type_id, b.type_id, "A and B");
    // Another shape (same name, same bounds size): its own type.
    assert_ne!(a.type_id, c.type_id, "A and C");
    // Equal bounds, different shapes: two types.
    assert_ne!(f.type_id, g.type_id, "F and G");
    // The same shape in two local placements: one type.
    assert_eq!(g.type_id, h.type_id, "G and H");
    // Each bar stays where it was: B is A moved 3 m along X.
    assert!(
        (cb.position - ca.position - glam::DVec2::new(3.0, 0.0)).length() < 1e-6,
        "{} {}",
        ca.position,
        cb.position
    );
    assert!((ca.base_offset - cb.base_offset).abs() < 1e-9);
    assert!((ch.position - glam::DVec2::new(7.0, 10.0)).length() < 1e-6, "{}", ch.position);
    assert!((ch.rotation_deg - cg.rotation_deg - 90.0).abs() < 1e-6);
    // Per-bar property sets stay on the bars.
    assert_eq!(partition(&ca.properties).as_deref(), Some("P1"));
    assert_eq!(partition(&cb.properties).as_deref(), Some("P2"));
    // The shared type keeps the class.
    let Some(TypeData::Component { def, .. }) = p.element_type(a.type_id).map(|t| &t.data) else { panic!() };
    assert_eq!(def.ifc.as_ref().map(|c| c.entity.as_str()), Some("IFCBUILDINGELEMENTPROXY"));
    let bar_types = p
        .types
        .values()
        .filter(|t| matches!(&t.data, TypeData::Component { def, .. } if def.ifc.as_ref().is_some_and(|c| c.entity == "IFCBUILDINGELEMENTPROXY")))
        .count();
    assert_eq!(bar_types, 4);
    // Geometry survives: each bar's volume is its swept disk's.
    let geom = cc_geom::ModelGeometry::build(&p);
    let (va, vb) = (geom.volume(a.id), geom.volume(b.id));
    let expected = std::f64::consts::PI * 0.006 * 0.006 * 1.3;
    assert!((va - expected).abs() < 0.05 * expected, "{va} vs {expected}");
    assert!((va - vb).abs() < 1e-9);
}

fn shaped<'a>(p: &'a Project, mark: &str) -> (&'a Element, &'a ShapedRebar) {
    p.elements
        .values()
        .find_map(|e| match &e.kind {
            ElementKind::ShapedRebar(r) if e.mark == mark => Some((e, r)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no bar {mark}"))
}

fn diameter(p: &Project, e: &Element) -> f64 {
    match p.element_type(e.type_id).map(|t| &t.data) {
        Some(TypeData::Rebar { diameter, .. }) => *diameter,
        other => panic!("{other:?}"),
    }
}

/// Reinforcing bars become native shaped bars: one type per diameter and
/// grade, their center lines where the file puts them, their lengths and
/// weights in the reinforcement figures, their property sets kept; export
/// writes them back as bars, which import again the same.
#[test]
fn bars_become_native_reinforcement() {
    let (p, report) = import(BARS, Project::from_template()).unwrap();
    assert!(report.generic.is_empty(), "{report:?}");
    assert_eq!(report.imported.get(&Category::Rebar), Some(&6), "{report:?}");
    let marks = ["A", "B", "C", "F", "G", "H"];
    let bars: Vec<(&Element, &ShapedRebar)> = marks.iter().map(|m| shaped(&p, m)).collect();
    // Ø12 CA-50 and Ø10 CA-50: two types, whatever the shapes.
    for (e, _) in &bars[..3] {
        assert_eq!(e.type_id, bars[0].0.type_id);
        assert!((diameter(&p, e) - 0.012).abs() < 1e-9);
    }
    for (e, _) in &bars[3..] {
        assert_eq!(e.type_id, bars[3].0.type_id);
        assert!((diameter(&p, e) - 0.010).abs() < 1e-9);
    }
    let ty = p.element_type(bars[0].0.type_id).unwrap();
    assert_eq!(ty.name, "Ø 12.0 mm CA-50");
    assert!(matches!(&ty.data, TypeData::Rebar { material, .. } if material == "CA-50"));
    // H: G's U turned a quarter in plan, 0.5 m up.
    let (_, h) = bars[5];
    let expected = [(7.005, 10.0), (6.705, 10.0), (6.705, 11.0), (7.005, 11.0)];
    for (q, (x, y)) in h.bars[0].iter().zip(expected) {
        assert!((q.x - x).abs() < 1e-9 && (q.y - y).abs() < 1e-9 && (q.z - 0.5).abs() < 1e-9, "{q}");
    }
    // Lengths and weights as the reinforcement schedules read them.
    let geom = cc_geom::ModelGeometry::build(&p);
    let a = &geom.rebar[&bars[0].0.id];
    assert!((a.total_length() - 1.3).abs() < 1e-9 && a.count() == 1 && a.host.is_none());
    let weight = std::f64::consts::PI * 0.006 * 0.006 * 1.3 * cc_geom::rebar::STEEL_DENSITY;
    assert!((a.weight() - weight).abs() < 1e-9, "{}", a.weight());
    assert!((geom.rebar[&bars[5].0.id].total_length() - 1.6).abs() < 1e-9);
    // Per-bar property sets stay on the bars.
    assert_eq!(partition(&bars[0].1.properties).as_deref(), Some("P1"));
    assert_eq!(partition(&bars[1].1.properties).as_deref(), Some("P2"));

    // Round trip.
    let text = export(&p, &ExportOptions::default());
    let (again, report) = import(&text, Project::from_template()).unwrap();
    assert!(report.generic.is_empty(), "{report:?}");
    let geom2 = cc_geom::ModelGeometry::build(&again);
    for (mark, (e, r)) in marks.iter().zip(&bars) {
        let (e2, r2) = shaped(&again, mark);
        assert_eq!(e2.uid, e.uid);
        assert!((diameter(&again, e2) - diameter(&p, e)).abs() < 1e-9);
        let (l, l2) = (geom.rebar[&e.id].total_length(), geom2.rebar[&e2.id].total_length());
        assert!((l - l2).abs() < 1e-6, "{mark}: {l} vs {l2}");
        assert_eq!(partition(&r2.properties), partition(&r.properties));
    }
    let types = again.types_of(Category::Rebar).filter(|t| t.name.starts_with("Ø")).count();
    assert_eq!(types, 2);
}

/// A bar with a hollow disk or of another shape stays a component; one
/// crafted with a huge or broken path cannot panic.
#[test]
fn other_bars_stay_components() {
    let hollow = BARS.replace("#103=IFCSWEPTDISKSOLID(#104,6.,$,$,$);", "#103=IFCSWEPTDISKSOLID(#104,6.,3.,$,$);");
    let (p, report) = import(&hollow, Project::from_template()).unwrap();
    assert_eq!(report.generic.values().sum::<usize>(), 1, "{report:?}");
    assert!(matches!(p.elements.values().find(|e| e.mark == "A").map(|e| &e.kind), Some(ElementKind::Component(_))));
    let broken = BARS.replace("#104=IFCPOLYLINE((#105,#106,#107));", "#104=IFCPOLYLINE((#105));");
    let (p, _) = import(&broken, Project::from_template()).unwrap();
    assert!(p.elements.values().filter(|e| matches!(e.kind, ElementKind::ShapedRebar(_))).count() >= 5);
}

/// A ring bar: half a circle of 2.166 m radius on its center line, a
/// composite curve of one trimmed circle, as exporters write ring bars.
const RING: &str = "ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC2X3'));
ENDSEC;
DATA;
#1=IFCPROJECT('0RvctVUKr0kugbFTf53O9L',$,'Ring',$,$,$,$,(#2),#3);
#2=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#12,$);
#3=IFCUNITASSIGNMENT((#4,#5));
#4=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);
#5=IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.);
#10=IFCBUILDINGSTOREY('1RvctVUKr0kugbFTf53O9L',$,'Level 1',$,$,#11,$,$,.ELEMENT.,0.);
#11=IFCLOCALPLACEMENT($,#12);
#12=IFCAXIS2PLACEMENT3D(#13,$,$);
#13=IFCCARTESIANPOINT((0.,0.,0.));
#14=IFCCARTESIANPOINT((0.,0.,350.));
#15=IFCAXIS2PLACEMENT3D(#14,$,$);
#20=IFCRELCONTAINEDINSPATIALSTRUCTURE('3RvctVUKr0kugbFTf53O9L',$,$,$,(#100),#10);
#100=IFCREINFORCINGBAR('2RvctVUKr0kugbFTf53O9L',$,'Ring',$,$,#11,#101,'N1','CA-50',8.,50.3,6805.,.MAIN.,.TEXTURED.);
#101=IFCPRODUCTDEFINITIONSHAPE($,$,(#102));
#102=IFCSHAPEREPRESENTATION(#2,'Body','AdvancedSweptSolid',(#103));
#103=IFCSWEPTDISKSOLID(#104,4.,$,$,$);
#104=IFCCOMPOSITECURVE((#105),.F.);
#105=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#106);
#106=IFCTRIMMEDCURVE(#107,(IFCPARAMETERVALUE(0.)),(IFCPARAMETERVALUE(3.14159265358979)),.T.,.PARAMETER.);
#107=IFCCIRCLE(#15,2166.);
ENDSEC;
END-ISO-10303-21;
";

/// A ring bar's length is its arc's, not that of the facets its mesh is
/// drawn with (24 a turn made a half ring 0.24% short in the schedules).
#[test]
fn ring_bars_measure_their_arc() {
    let (p, report) = import(RING, Project::from_template()).unwrap();
    assert_eq!(report.imported.get(&Category::Rebar), Some(&1), "{report:?}");
    let (e, _) = shaped(&p, "N1");
    let geom = cc_geom::ModelGeometry::build(&p);
    let length = geom.rebar[&e.id].total_length();
    let arc = std::f64::consts::PI * 2.166;
    assert!((length - arc).abs() < 1e-4 * arc, "{length} vs {arc}");
}
