//! Niches (issue #304): recesses in one face of a wall, hosted like doors
//! and windows. A niche type is a component of `ComponentKind::Niche` whose
//! opening has a depth (`cc_core::openings::niche_def`); the wall's geometry
//! recesses its face there (`cc_geom::Recess`).

use super::create::{HostedArgs, hosted_at};
use super::{created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::openings::{self, OpeningNames};
use cc_core::units::INCH;
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, MeasurementSystem, Project, TypeData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Sizes a niche type may have, meters.
const SIZE: (f64, f64) = (0.05, 6.0);
const DEPTH: (f64, f64) = (0.01, 1.0);
const SILL: (f64, f64) = (0.0, 10.0);

/// Width, height, depth and sill of a niche type made with no sizes given.
fn default_sizes(p: &Project) -> (f64, f64, f64, f64) {
    if p.measurement_system() == MeasurementSystem::Imperial {
        (24.0 * INCH, 24.0 * INCH, 4.0 * INCH, 36.0 * INCH)
    } else {
        (0.6, 0.6, 0.1, 0.9)
    }
}

/// Create a niche type: a recess `width` wide, `height` tall and `depth`
/// deep in one face of a wall, its bottom `sill` above the wall's base by
/// default. In a wall thinner than the depth plus 1 cm the niche stops 1 cm
/// short of the other face (an opening through a wall is a door or a
/// window). Place niches with `niche_create`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct NicheTypeCreate {
    /// Type name, unique among niche types (default: the component's name and its width, height and depth,
    /// in centimeters; in feet and inches in imperial projects).
    #[serde(default)]
    pub name: Option<String>,
    /// Width along the wall, meters (default 0.6; 2'-0" in imperial projects).
    #[serde(default)]
    pub width: Option<f64>,
    /// Height, meters (default 0.6; 2'-0" in imperial projects).
    #[serde(default)]
    pub height: Option<f64>,
    /// How far it goes into the wall from its face, meters (default 0.1; 4" in imperial projects).
    #[serde(default)]
    pub depth: Option<f64>,
    /// Height of its bottom above the wall's base in new niches, meters (default 0.9; 3'-0" in imperial
    /// projects; 0 for a niche from the floor).
    #[serde(default)]
    pub sill: Option<f64>,
}

fn in_range(v: f64, (lo, hi): (f64, f64), what: &str) -> Result<f64> {
    let v = refs::finite(v, what)?;
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(EngineError::invalid(format!("{what} must be between {lo} and {hi} m")))
    }
}

/// A niche type of these sizes, named `name` or after its sizes (not yet in the project).
fn niche_type(p: &Project, name: Option<String>, sizes: (f64, f64, f64, f64)) -> Result<(String, TypeData)> {
    let (width, height, depth, sill) = sizes;
    let (width, height) = (in_range(width, SIZE, "width")?, in_range(height, SIZE, "height")?);
    let (depth, sill) = (in_range(depth, DEPTH, "depth")?, in_range(sill, SILL, "sill")?);
    let names = OpeningNames::new(&|key: &str| crate::i18n::tr(key));
    let component = names.niche.clone();
    let def = openings::niche_def(&names, (width, height, depth, sill));
    let taken = |name: &str| p.types_of(Category::Niche).any(|t| t.name.trim() == name);
    let name = match name.map(|n| n.trim().to_string()) {
        Some(name) if name.is_empty() || taken(&name) => {
            return Err(EngineError::invalid(format!("the name '{name}' is empty or already used")));
        }
        Some(name) => name,
        None => {
            let base = if p.measurement_system() == MeasurementSystem::Imperial {
                let l = cc_views::format::length;
                format!("{component} {} x {} x {}", l(width), l(height), l(depth))
            } else {
                format!("{component} {:.0}x{:.0}x{:.0}", width * 100.0, height * 100.0, depth * 100.0)
            };
            let mut name = base.clone();
            let mut i = 1;
            while taken(&name) && i < 10_000 {
                i += 1;
                name = format!("{base} ({i})");
            }
            name
        }
    };
    Ok((name, TypeData::Component { component, def }))
}

impl Command for NicheTypeCreate {
    const NAME: &'static str = "niche_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let d = default_sizes(p);
        let sizes = (
            self.width.unwrap_or(d.0),
            self.height.unwrap_or(d.1),
            self.depth.unwrap_or(d.2),
            self.sill.unwrap_or(d.3),
        );
        let (name, data) = niche_type(p, self.name, sizes)?;
        let id = edit(s, t!("tx-create-niche-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            Ok(e.add_type(ElementType { id, name, data, info: Default::default(), uid: Default::default() }))
        })?;
        Ok(super::query::type_json(s, id).unwrap_or_else(|| json!({ "id": id.0 })))
    }
}

/// Place a niche in a wall: a recess in one of its faces, centered on the
/// wall unless `offset` or `point` is given. It is in the wall's left face
/// (seen from its start to its end), in the right one with `flip_facing`,
/// and with `point` in the face on the point's side. A project with no niche
/// type gets a default one (see `niche_type_create`) in the same undo step.
/// The wall keeps standing behind the niche: it is not split, and rooms,
/// joins and framing ignore it. A niche that would overlap a door, a window
/// or another niche of its wall cuts nothing; `element_get` then shows no
/// `volume`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct NicheCreate {
    /// Id of the host wall; omit it to use the wall nearest `point`.
    #[serde(default)]
    pub wall: Option<u64>,
    /// Distance along the wall from its start to the niche's center, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// A point near the wall: the niche is centered at its projection, in the face on its side.
    #[serde(default)]
    pub point: Option<crate::refs::Point>,
    /// Height of its bottom above the wall's base, meters (default: the type's).
    #[serde(default)]
    pub sill: Option<f64>,
    /// Niche type id or name (default: the first niche type; one is made when the project has none).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// In the wall's right face instead of its left one.
    #[serde(default)]
    pub flip_facing: Option<bool>,
}

impl Command for NicheCreate {
    const NAME: &'static str = "niche_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        // The type asked for, the first niche type, or a new default one.
        let existing = match &self.type_ref {
            Some(r) => Some(refs::type_of(p, Category::Niche, Some(r))?),
            None => p.types_of(Category::Niche).next().map(|t| t.id),
        };
        let new_type = match existing {
            Some(_) => None,
            None => Some(niche_type(p, None, default_sizes(p))?),
        };
        let data = match (&new_type, existing.and_then(|id| p.element_type(id))) {
            (Some((_, data)), _) => data.clone(),
            (None, Some(ty)) => ty.data.clone(),
            (None, None) => return Err(EngineError::not_found("type")),
        };
        let depth = match &data {
            TypeData::Component { def, .. } => def.opening_depth(),
            _ => None,
        };
        if depth.is_none() {
            return Err(EngineError::invalid("not a niche type: its opening has no depth"));
        }
        let probe = ElementType {
            id: ElementTypeId(0),
            name: String::new(),
            data,
            info: Default::default(),
            uid: Default::default(),
        };
        let args = HostedArgs {
            wall: self.wall,
            offset: self.offset,
            point: self.point,
            sill: self.sill,
            type_ref: None,
            flip_facing: self.flip_facing,
            flip_hand: None,
        };
        let (hosted, level) = hosted_at(p, &args, &probe)?;
        let id = edit(s, t!("tx-create-niche"), |e| {
            let type_id = match (existing, new_type) {
                (Some(id), _) => id,
                (None, Some((name, data))) => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(ElementType { id, name, data, info: Default::default(), uid: Default::default() })
                }
                (None, None) => return Err(EngineError::not_found("type")),
            };
            Ok(super::add_element(e, level, type_id, ElementKind::Niche(hosted)))
        })?;
        Ok(created(s, id))
    }
}
