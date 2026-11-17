//! Door types: swing doors of one or two leaves and French doors (glazed,
//! from the floor), built from the definitions of `cc_core::openings`.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::openings::{self, OpeningNames};
use cc_core::{Category, DoorOperation, ElementType, ElementTypeId, Project, TypeData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Sizes a door type may have, meters.
const MIN_WIDTH: f64 = 0.4;
const MAX_WIDTH: f64 = 6.0;
const MIN_HEIGHT: f64 = 1.5;
const MAX_HEIGHT: f64 = 6.0;
/// Glass a French door's leaf keeps above its bottom rail, at least, meters.
const MIN_PANE: f64 = 0.2;

/// Create a door type. `style` is `swing` (a solid leaf, the default) or
/// `french` (a French door: glazed leaves of stiles and rails around a pane,
/// over a solid bottom rail `bottom_rail` tall, a parameter of the type);
/// `leaves` is 1 (hinged on the left; flip an element's hand for the right)
/// or 2 (a double door, leaves hinged on either jamb meeting in the middle,
/// drawn with two swings in plans and written to IFC as
/// `DOUBLE_DOOR_SINGLE_SWING`). Doors stand on the floor (sill 0). Place them
/// with `door_create`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DoorTypeCreate {
    /// Type name, unique among door types (default: the component's name and size, in centimeters; in feet
    /// and inches in imperial projects).
    #[serde(default)]
    pub name: Option<String>,
    /// `swing` (default) or `french`.
    #[serde(default)]
    pub style: Option<String>,
    /// Number of leaves: 1 (default) or 2.
    #[serde(default)]
    pub leaves: Option<u8>,
    /// Width of the opening, meters (default 0.8 for one leaf, 1.4 for two; 2'-8" and 5'-0" in imperial
    /// projects).
    #[serde(default)]
    pub width: Option<f64>,
    /// Height of the opening, meters (default 2.1, French doors 2.2; 6'-8" in imperial projects).
    #[serde(default)]
    pub height: Option<f64>,
    /// French doors: height of the solid bottom rail of each leaf, meters (default 0.4; 1'-4" in imperial
    /// projects).
    #[serde(default)]
    pub bottom_rail: Option<f64>,
    /// Material of the leaves (default: the template's wood, if the project has it).
    #[serde(default)]
    pub material: Option<String>,
    /// French doors: material of the panes (default: the template's glass, if the project has it).
    #[serde(default)]
    pub glass: Option<String>,
}

/// A material of the project by its template name, if the project has it.
fn template_material(p: &Project, key: &str) -> String {
    let name = crate::i18n::tr(key);
    p.material(&name).map_or(String::new(), |m| m.name.clone())
}

fn material_arg(p: &Project, name: Option<String>, key: &str) -> Result<String> {
    match name {
        Some(m) => refs::material(p, &Ref::Name(m)),
        None => Ok(template_material(p, key)),
    }
}

fn in_range(v: f64, (lo, hi): (f64, f64), what: &str) -> Result<f64> {
    let v = refs::finite(v, what)?;
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(EngineError::invalid(format!("{what} must be between {lo} and {hi} m")))
    }
}

impl Command for DoorTypeCreate {
    const NAME: &'static str = "door_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let glazed = match self.style.as_deref().map(str::trim) {
            None | Some("swing") => false,
            Some("french") => true,
            Some(other) => return Err(EngineError::invalid(format!("unknown style '{other}' (swing or french)"))),
        };
        let operation = match self.leaves.unwrap_or(1) {
            1 => DoorOperation::SingleSwingLeft,
            2 => DoorOperation::DoubleDoor,
            n => return Err(EngineError::invalid(format!("a door has 1 or 2 leaves, not {n}"))),
        };
        let double = operation.leaves() == 2;
        let [one, two, swing_height, french_height, rail] = p.defaults().door;
        let width = in_range(self.width.unwrap_or(if double { two } else { one }), (MIN_WIDTH, MAX_WIDTH), "width")?;
        let height = in_range(
            self.height.unwrap_or(if glazed { french_height } else { swing_height }),
            (MIN_HEIGHT, MAX_HEIGHT),
            "height",
        )?;
        let bottom_rail = self.bottom_rail.unwrap_or(rail);
        if glazed {
            // Under the head and the leaf's top rail, some glass is left.
            let max = height - openings::FRAME - openings::STILE - MIN_PANE;
            in_range(bottom_rail, (0.01, max), "bottom_rail")?;
            // Each leaf is wide enough for its two stiles and a pane.
            let leaf = (width - 2.0 * openings::FRAME) / operation.leaves() as f64;
            if leaf < 2.0 * openings::STILE + MIN_PANE {
                return Err(EngineError::invalid("the door is too narrow for glazed leaves"));
            }
        } else if self.bottom_rail.is_some() || self.glass.is_some() {
            return Err(EngineError::invalid("bottom_rail and glass are for French doors (style: french)"));
        }
        let material = material_arg(p, self.material, "tpl-mat-wood")?;
        let glass = if glazed { material_arg(p, self.glass, "tpl-mat-glass")? } else { String::new() };
        let names = OpeningNames::new(&|key: &str| crate::i18n::tr(key));
        let component = names.door_for(operation, glazed).clone();
        let def = if glazed {
            openings::french_door_def(&names, (width, height, bottom_rail), (&material, &glass), operation)
        } else {
            openings::operable_door_def(&names, (width, height), &material, operation)
        };
        let taken = |name: &str| p.types_of(Category::Door).any(|t| t.name.trim() == name);
        let name = match self.name.map(|n| n.trim().to_string()) {
            Some(name) if name.is_empty() || taken(&name) => {
                return Err(EngineError::invalid(format!("the name '{name}' is empty or already used")));
            }
            Some(name) => name,
            None => {
                let base = if p.measurement_system() == cc_core::MeasurementSystem::Imperial {
                    // As the template names them: `Door 2'-8" x 6'-8"`.
                    let l = cc_views::format::length;
                    format!("{component} {} x {}", l(width), l(height))
                } else {
                    format!("{component} {:.0}x{:.0}", width * 100.0, height * 100.0)
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
        let id = edit(s, t!("tx-create-door-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let data = TypeData::Component { component, def };
            Ok(e.add_type(ElementType { id, name, data, info: Default::default(), uid: Default::default() }))
        })?;
        Ok(super::query::type_json(s, id).unwrap_or_else(|| json!({ "id": id.0 })))
    }
}
