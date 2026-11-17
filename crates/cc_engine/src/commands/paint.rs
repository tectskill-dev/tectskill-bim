//! Painted faces: a material on one surface of an element (see
//! `cc_core::paint` for how a paint finds its face after the model changes).

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::paint::{FacePaint, MAX_PAINTS};
use cc_core::{DVec3, ElementId};
use cc_geom::paint::{FaceRef, nearest_face, resolve, surface};
use cc_geom::{ModelGeometry, Solid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// How far from a face a point given to `paint` or `paint_remove` may be, meters.
pub const PICK_TOLERANCE: f64 = 0.05;

fn vector(v: [f64; 3], what: &str) -> Result<DVec3> {
    let v = DVec3::from_array(v);
    if v.is_finite() && v.abs().max_element() < 1e9 {
        Ok(v)
    } else {
        Err(EngineError::invalid(format!("{what} is not a finite point")))
    }
}

/// The face of element `id`'s body at `point` (turned like `normal`, when given).
fn face_at(geom: &ModelGeometry, id: ElementId, point: DVec3, normal: Option<DVec3>) -> Result<(FaceRef, DVec3)> {
    let solids: Vec<&Solid> = geom.body_of(id).collect();
    if solids.is_empty() {
        return Err(EngineError::invalid(t!("err-paint-no-faces")));
    }
    let normal = normal.map(|n| n.try_normalize().ok_or_else(|| EngineError::invalid("normal is zero"))).transpose()?;
    let (face, d) = nearest_face(&solids, point, normal)
        .filter(|(_, d)| *d <= PICK_TOLERANCE)
        .ok_or_else(|| EngineError::invalid(t!("err-paint-no-face")))?;
    let n = solids.get(face.0).and_then(|s| s.faces.get(face.1)).map_or(DVec3::Z, |f| f.normal);
    // The point moved onto the face's plane (when it lies over it).
    let on = point - n * (point - first_vertex(&solids, face).unwrap_or(point)).dot(n);
    let on = if on.distance(point) <= d + 1e-9 { on } else { point };
    Ok((face, on))
}

fn first_vertex(solids: &[&Solid], (si, fi): FaceRef) -> Option<DVec3> {
    solids.get(si)?.faces.get(fi)?.verts.first().copied()
}

fn area(solids: &[&Solid], faces: &[FaceRef]) -> f64 {
    faces.iter().filter_map(|(si, fi)| solids.get(*si)?.faces.get(*fi)).map(|f| f.area()).sum()
}

/// Paint a face of an element with a material: the face at `point` (and
/// those touching it in its plane, or the rest of a curved surface). It shows
/// the material's colors, surface pattern and texture over the type's
/// material; painting a painted surface again replaces its paint. The paint
/// follows the face when the element changes (the face turned like it that
/// passes nearest its point).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Paint {
    /// Element id.
    pub element: u64,
    /// A point on the face, `[x, y, z]` meters (within 5 cm of it).
    pub point: [f64; 3],
    /// The face's outward normal, to pick between faces meeting at the
    /// point (a 3D pick gives it); default: the nearest face.
    #[serde(default)]
    pub normal: Option<[f64; 3]>,
    /// Material name (or index in `materials_list`).
    pub material: Ref,
}

impl Command for Paint {
    const NAME: &'static str = "paint";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.element)?;
        let material = refs::material(s.project(), &self.material)?;
        let point = vector(self.point, "point")?;
        let normal = self.normal.map(|n| vector(n, "normal")).transpose()?;
        let (project, geom) = s.project_and_geometry();
        let (face, on) = face_at(geom, id, point, normal)?;
        let solids: Vec<&Solid> = geom.body_of(id).collect();
        let faces = surface(&solids, face);
        let n = solids.get(face.0).and_then(|s| s.faces.get(face.1)).map_or(DVec3::Z, |f| f.normal);
        let paint = FacePaint { point: on, normal: n, material: material.clone() };
        // Paints of the same surface give way to the new one.
        let mut paints = project.element(id).map(|e| e.paints.clone()).unwrap_or_default();
        paints.retain(|p| resolve(&solids, p).iter().all(|f| !faces.contains(f)));
        if paints.len() >= MAX_PAINTS {
            return Err(EngineError::rejected(format!("an element keeps at most {MAX_PAINTS} paints")));
        }
        paints.push(paint);
        let painted = area(&solids, &faces);
        let count = faces.len();
        edit(s, t!("tx-paint"), |e| {
            if let Some(el) = e.element_mut(id) {
                el.paints = paints;
            }
            Ok(())
        })?;
        Ok(json!({ "element": id.0, "material": material, "faces": count, "area": painted }))
    }
}

/// Take the paint off a face of an element (the one at `point`), or every
/// paint of the element when no point is given.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaintRemove {
    /// Element id.
    pub element: u64,
    /// A point on the painted face, `[x, y, z]` meters (default: every paint of the element).
    #[serde(default)]
    pub point: Option<[f64; 3]>,
    /// The face's outward normal (see `paint`).
    #[serde(default)]
    pub normal: Option<[f64; 3]>,
}

impl Command for PaintRemove {
    const NAME: &'static str = "paint_remove";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.element)?;
        let point = self.point.map(|p| vector(p, "point")).transpose()?;
        let normal = self.normal.map(|n| vector(n, "normal")).transpose()?;
        let (project, geom) = s.project_and_geometry();
        let mut paints = project.element(id).map(|e| e.paints.clone()).unwrap_or_default();
        let before = paints.len();
        match point {
            Some(point) => {
                let (face, _) = face_at(geom, id, point, normal)?;
                let solids: Vec<&Solid> = geom.body_of(id).collect();
                paints.retain(|p| !resolve(&solids, p).contains(&face));
            }
            None => paints.clear(),
        }
        let removed = before - paints.len();
        if removed == 0 {
            return Err(EngineError::invalid(t!("err-paint-none")));
        }
        edit(s, t!("tx-paint-remove"), |e| {
            if let Some(el) = e.element_mut(id) {
                el.paints = paints;
            }
            Ok(())
        })?;
        Ok(json!({ "element": id.0, "removed": removed }))
    }
}

/// The painted faces of an element (or of every element): each paint's
/// material, point and normal, and the faces and area it covers now (none
/// when no face is turned like it any more).
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaintsList {
    /// Element id (default: every element with paints).
    #[serde(default)]
    pub element: Option<u64>,
}

impl Command for PaintsList {
    const NAME: &'static str = "paints_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let only = self.element.map(|e| refs::element(s.project(), e)).transpose()?;
        let (project, geom) = s.project_and_geometry();
        let mut out = Vec::new();
        for e in project.elements.values().filter(|e| !e.paints.is_empty() && only.is_none_or(|o| o == e.id)) {
            let solids: Vec<&Solid> = geom.body_of(e.id).collect();
            for p in &e.paints {
                let faces = resolve(&solids, p);
                out.push(json!({
                    "element": e.id.0,
                    "material": p.material,
                    "point": p.point.to_array(),
                    "normal": p.normal.to_array(),
                    "faces": faces.len(),
                    "area": area(&solids, &faces),
                }));
            }
        }
        Ok(Value::Array(out))
    }
}
