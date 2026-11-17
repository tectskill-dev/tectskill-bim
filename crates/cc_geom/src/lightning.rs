//! Lightning protection in the model: where an air terminal stands on a
//! roof, and a simplified coverage check of the roofs by the air
//! termination (air terminals and roof conductors).
//!
//! The check samples the exposed upward faces of roofs (and of floors above
//! the lowest level that nothing covers) and asks, for each point, whether
//! one air termination point protects it by itself: the point lies inside
//! the protected volume of a single rod under the rolling sphere method,
//! over a horizontal reference plane at the lowest point of the element's
//! exposed faces (its eaves). On flat parts (slope up to 1:10) a point
//! within half the class's mesh width of a roof conductor is protected as
//! by the mesh method. Mutual protection between terminals (the sphere
//! sagging between two rods) and the shielding by the building are left
//! out, which errs on the safe side.

use crate::{ModelGeometry, Ray, polygon};
use cc_core::{Category, ConductorRole, ElementId, ElementKind, LevelId, LpsClass, Project};
use glam::{DVec2, DVec3};

/// Most roof points sampled by one check (the spacing grows to stay under it).
const MAX_SAMPLES: usize = 4000;
/// Most air termination points (conductors are sampled farther apart to stay under it).
const MAX_AIR_POINTS: usize = 4000;
/// A point this close to an air termination point is on it, meters.
const ON_TERMINATION: f64 = 0.1;
/// Faces whose normal is at least this vertical are flat for the mesh method (slope 1:10).
const FLAT: f64 = 0.995;

/// The categories whose upper faces lightning protection covers or stands on.
fn is_roof_like(category: Category) -> bool {
    matches!(category, Category::Roof | Category::Floor)
}

/// The world height of the highest roof or floor surface over plan point
/// `p` (`None` where there is none).
pub fn surface_top(geom: &ModelGeometry, p: DVec2) -> Option<f64> {
    let (_, hi) = geom.bounds()?;
    let origin = p.extend(hi.z + 1.0);
    let ray = Ray { origin, dir: DVec3::NEG_Z };
    geom.solids
        .iter()
        .filter(|s| is_roof_like(s.category) && !s.secondary)
        .filter_map(|s| s.raycast(ray))
        .min_by(f64::total_cmp)
        .map(|t| origin.z - t)
        .filter(|z| z.is_finite())
}

/// The points of the air termination: the tip of each air terminal (the
/// top of its solids) and points along each roof (air) conductor, at most
/// `step` meters apart.
pub fn air_points(project: &Project, geom: &ModelGeometry) -> Vec<(ElementId, DVec3)> {
    let mut out = Vec::new();
    for e in project.elements.values() {
        if project.mep_data(e).and_then(|m| m.device()) != Some(cc_core::DeviceKind::AirTerminal) {
            continue;
        }
        let Some(base) = project.component_point(e) else { continue };
        let top = geom.solids_of(e.id).filter_map(|s| s.bounds()).map(|b| b.1.z).fold(f64::NEG_INFINITY, f64::max);
        if top.is_finite() {
            out.push((e.id, base.truncate().extend(top)));
        }
    }
    let conductors: Vec<(ElementId, DVec3, DVec3)> = air_conductors(project, geom);
    let total: f64 = conductors.iter().map(|(_, a, b)| a.distance(*b)).sum();
    let step = (total / MAX_AIR_POINTS as f64).max(0.5);
    for (id, a, b) in conductors {
        let n = ((a.distance(b) / step).ceil() as usize).clamp(1, MAX_AIR_POINTS);
        out.extend((0..=n).map(|i| (id, a.lerp(b, i as f64 / n as f64))));
        if out.len() > 2 * MAX_AIR_POINTS {
            break;
        }
    }
    out
}

/// The air termination conductors as world segments.
fn air_conductors(project: &Project, geom: &ModelGeometry) -> Vec<(ElementId, DVec3, DVec3)> {
    project
        .elements
        .values()
        .filter(|e| matches!(&e.kind, ElementKind::LightningConductor(c) if c.role == ConductorRole::Air))
        .filter_map(|e| geom.segments.get(&e.id).map(|g| (e.id, g.a, g.b)))
        .collect()
}

/// The result of a coverage check.
#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    pub class: LpsClass,
    /// Rolling sphere radius and mesh width used, meters.
    pub radius: f64,
    pub mesh: f64,
    /// Distance between the sampled points, meters.
    pub spacing: f64,
    /// Air termination points (terminal tips and conductor samples).
    pub air_points: usize,
    /// Roof points checked and how many of them are protected.
    pub checked: usize,
    pub protected: usize,
    /// The roof elements checked.
    pub roofs: Vec<ElementId>,
    /// The points left unprotected, with their element.
    pub unprotected: Vec<(ElementId, DVec3)>,
}

impl Coverage {
    /// Share of the checked points that are protected (1 with nothing to check).
    pub fn ratio(&self) -> f64 {
        if self.checked == 0 { 1.0 } else { self.protected as f64 / self.checked as f64 }
    }
}

/// A sampled roof point: its element, its position and whether its face is flat.
struct Sample {
    element: ElementId,
    point: DVec3,
    flat: bool,
}

/// Check how the air termination covers the roofs for `class`, sampling
/// the roofs about `spacing` meters apart (more when they are large).
pub fn check(project: &Project, geom: &ModelGeometry, class: LpsClass, spacing: f64) -> Coverage {
    let radius = class.sphere_radius();
    let mesh = class.mesh_width();
    let lowest: Option<LevelId> =
        project.levels.iter().min_by(|a, b| a.elevation.total_cmp(&b.elevation)).map(|l| l.id);
    let covers: Vec<&crate::Solid> = geom.solids.iter().filter(|s| is_roof_like(s.category) && !s.secondary).collect();
    let targets: Vec<&crate::Solid> = covers
        .iter()
        .copied()
        .filter(|s| match project.element(s.element) {
            Some(e) => s.category == Category::Roof || Some(e.level) != lowest,
            None => false,
        })
        .collect();
    // Upward faces and their area, to size the sampling grid.
    let faces: Vec<(ElementId, &crate::Face)> = targets
        .iter()
        .flat_map(|s| s.faces.iter().filter(|f| f.normal.z > 0.05 && f.verts.len() >= 3).map(move |f| (s.element, f)))
        .collect();
    let area: f64 = faces.iter().map(|(_, f)| f.area() * f.normal.z).sum();
    let spacing = if spacing.is_finite() && spacing > 0.05 { spacing } else { 1.0 };
    let spacing = spacing.max((area / MAX_SAMPLES as f64).sqrt());
    let mut samples: Vec<Sample> = Vec::new();
    for (element, f) in &faces {
        let flat = f.normal.z >= FLAT;
        let plan: Vec<DVec2> = f.verts.iter().map(|v| v.truncate()).collect();
        let v0 = f.verts[0];
        let z_at = |p: DVec2| v0.z - (f.normal.x * (p.x - v0.x) + f.normal.y * (p.y - v0.y)) / f.normal.z;
        samples.extend(f.verts.iter().map(|v| Sample { element: *element, point: *v, flat }));
        let (lo, hi) = plan.iter().fold((plan[0], plan[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let (nx, ny) = (((hi.x - lo.x) / spacing).floor() as usize, ((hi.y - lo.y) / spacing).floor() as usize);
        if nx.saturating_mul(ny) > MAX_SAMPLES {
            continue;
        }
        for i in 0..=nx {
            for j in 0..=ny {
                let p = lo + DVec2::new((i as f64 + 0.5) * spacing, (j as f64 + 0.5) * spacing);
                if p.x <= hi.x && p.y <= hi.y && polygon::contains(&plan, p) {
                    samples.push(Sample { element: *element, point: p.extend(z_at(p)), flat });
                }
            }
        }
        if samples.len() > 4 * MAX_SAMPLES {
            break;
        }
    }
    // Only exposed points: nothing of a roof or floor right above them.
    samples.retain(|s| {
        let ray = Ray { origin: s.point + DVec3::Z * 0.05, dir: DVec3::Z };
        s.point.is_finite() && !covers.iter().any(|c| c.raycast(ray).is_some())
    });
    // The reference plane of each element: its lowest exposed point.
    let mut base: std::collections::BTreeMap<ElementId, f64> = Default::default();
    for s in &samples {
        let z = base.entry(s.element).or_insert(s.point.z);
        *z = z.min(s.point.z);
    }
    let air = air_points(project, geom);
    let conductors = air_conductors(project, geom);
    let protected_by_mesh = |p: DVec3| {
        conductors.iter().any(|(_, a, b)| {
            let (a2, b2, p2) = (a.truncate(), b.truncate(), p.truncate());
            let d = b2 - a2;
            let t =
                if d.length_squared() < 1e-12 { 0.0 } else { ((p2 - a2).dot(d) / d.length_squared()).clamp(0.0, 1.0) };
            let q = a.lerp(*b, t);
            q.truncate().distance(p2) <= mesh * 0.5 && q.z >= p.z - 0.25
        })
    };
    let mut out = Coverage {
        class,
        radius,
        mesh,
        spacing,
        air_points: air.len(),
        checked: samples.len(),
        protected: 0,
        roofs: base.keys().copied().collect(),
        unprotected: Vec::new(),
    };
    for s in &samples {
        let reference = base.get(&s.element).copied().unwrap_or(s.point.z);
        let height = s.point.z - reference;
        let safe = air.iter().any(|(_, a)| {
            if a.distance(s.point) <= ON_TERMINATION {
                return true;
            }
            protected_reach(a.z - reference, height, radius)
                .is_some_and(|r| a.truncate().distance(s.point.truncate()) <= r)
        }) || (s.flat && protected_by_mesh(s.point));
        if safe {
            out.protected += 1;
        } else {
            out.unprotected.push((s.element, s.point));
        }
    }
    out
}

use cc_core::lightning::protected_reach;

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Document, Element, ElementTypeId, MepSegment};

    /// Names of the built-in parameters in these tests.
    fn name(key: &str) -> String {
        key.trim_start_matches("tpl-param-").replace('-', "_")
    }

    fn add(doc: &mut Document, type_id: ElementTypeId, kind: ElementKind) -> ElementId {
        doc.edit("t", |e| {
            let level = e.project().levels[0].id;
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind,
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: Vec::new(),
            });
            id
        })
    }

    fn add_type(doc: &mut Document, data: cc_core::TypeData) -> ElementTypeId {
        doc.edit("t", |e| {
            let id = ElementTypeId(e.alloc_id());
            e.add_type(cc_core::ElementType {
                id,
                name: "t".into(),
                data,
                info: Default::default(),
                uid: Default::default(),
            })
        })
    }

    fn flat_roof(doc: &mut Document, size: f64) {
        let roof_t = doc.project.default_type(Category::Roof).unwrap();
        let outline = vec![DVec2::ZERO, DVec2::new(size, 0.0), DVec2::new(size, size), DVec2::new(0.0, size)];
        let roof = cc_core::Roof::new(outline, 3.0, 0.0, cc_core::RoofShape::Flat);
        add(doc, roof_t, ElementKind::Roof(roof));
    }

    fn terminal(doc: &mut Document, at: DVec2, base: f64, height: f64) -> ElementId {
        let (component, _, def) = cc_core::component::device_library(cc_core::DeviceKind::AirTerminal, &name).unwrap();
        let ty = add_type(doc, cc_core::TypeData::Component { component, def });
        let mut c = cc_core::Component { position: at, base_offset: base, ..Default::default() };
        c.overrides.insert("height".into(), height);
        add(doc, ty, ElementKind::Component(c))
    }

    #[test]
    fn a_rod_covers_a_small_roof_and_less_with_a_smaller_sphere() {
        let mut doc = Document::new(Project::from_template());
        flat_roof(&mut doc, 6.0);
        let geom = ModelGeometry::build(&doc.project);
        let top = surface_top(&geom, DVec2::new(3.0, 3.0)).unwrap();
        assert!(top > 3.0 && top < 4.0, "{top}");
        assert!(surface_top(&geom, DVec2::new(30.0, 3.0)).is_none());
        // Nothing protects the bare roof.
        let bare = check(&doc.project, &geom, LpsClass::III, 1.0);
        assert!(bare.checked > 30 && bare.protected == 0, "{bare:?}");
        // A 2 m rod in the middle reaches sqrt(2 * 88) ≈ 13 m: the whole roof.
        let rod = terminal(&mut doc, DVec2::new(3.0, 3.0), top, 2.0);
        let geom = ModelGeometry::build(&doc.project);
        let one = check(&doc.project, &geom, LpsClass::III, 1.0);
        assert_eq!(one.air_points, 1);
        assert_eq!(one.protected, one.checked, "{:?}", one.unprotected);
        // A 0.2 m rod under a 20 m sphere reaches sqrt(0.2 * 39.8) ≈ 2.8 m, short of the corners.
        doc.edit("t", |e| {
            if let Some(ElementKind::Component(c)) = e.element_mut(rod).map(|el| &mut el.kind) {
                c.overrides.insert("height".into(), 0.2);
            }
        });
        let geom = ModelGeometry::build(&doc.project);
        let strict = check(&doc.project, &geom, LpsClass::I, 1.0);
        assert!(strict.protected > 0 && strict.protected < strict.checked, "{strict:?}");
    }

    #[test]
    fn roof_conductors_protect_a_flat_roof_as_a_mesh() {
        let mut doc = Document::new(Project::from_template());
        flat_roof(&mut doc, 10.0);
        let geom = ModelGeometry::build(&doc.project);
        let level_z = doc.project.level_elevation(doc.project.levels[0].id);
        let z = surface_top(&geom, DVec2::new(5.0, 5.0)).unwrap() - level_z;
        let ty = add_type(&mut doc, cc_core::TypeData::LightningConductor { diameter: 0.008, material: String::new() });
        // A conductor along the middle: every point is within 5 m (class III mesh 15 m).
        let run = MepSegment::new(DVec2::new(0.0, 5.0), DVec2::new(10.0, 5.0), z + 0.01);
        let conductor = cc_core::LightningConductor { run, role: ConductorRole::Air };
        add(&mut doc, ty, ElementKind::LightningConductor(conductor));
        let geom = ModelGeometry::build(&doc.project);
        let mesh = check(&doc.project, &geom, LpsClass::III, 1.0);
        assert_eq!(mesh.protected, mesh.checked, "{:?}", mesh.unprotected);
        // Class I asks for a 5 m mesh: the far edges are not covered by it.
        let strict = check(&doc.project, &geom, LpsClass::I, 1.0);
        assert!(strict.protected < strict.checked);
    }
}
