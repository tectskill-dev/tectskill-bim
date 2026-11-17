//! Ramps: creating them in any shape and checking them against the
//! accessibility rules of their type.

use super::create::{justification, top};
use super::{add_element, created, edit};
use crate::defaults::default_top;
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, ElementId, ElementKind, ElementType, ElementTypeId, Project, Ramp, RampLayout, RampShape, Turn, TypeData,
    WallTop,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Names of the ramp shapes in commands.
pub const SHAPES: [(&str, RampShape); 4] = [
    ("straight", RampShape::Straight),
    ("l_shaped", RampShape::LShaped),
    ("u_shaped", RampShape::UShaped),
    ("spiral", RampShape::Spiral),
];

const TURNS: [(&str, Turn); 2] = [("left", Turn::Left), ("right", Turn::Right)];

/// Create a ramp starting at `start` (the middle of its foot) and rising
/// along `direction` up to the level above (or `height`). Flights follow the
/// type's rules: at most its steepest slope and its highest rise per flight,
/// with landings between them; `shape` lays them out in line, L- or
/// U-shaped (switchback), or as a helix. A project without a ramp type gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RampCreate {
    /// Foot of the ramp (middle of its lower edge).
    pub start: Point,
    /// Direction of the first flight in plan, degrees counter-clockwise from +X.
    #[serde(default)]
    pub direction: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Height when there is no level above (default 3; 10'-0" in imperial projects).
    #[serde(default)]
    pub height: Option<f64>,
    /// Level the ramp rises to (default: the level above).
    #[serde(default)]
    pub top_level: Option<Ref>,
    /// Plan shape: `straight` (default), `l_shaped`, `u_shaped` or `spiral`.
    #[serde(default)]
    pub shape: Option<String>,
    /// Side the ramp turns to, going up: `left` (default) or `right`.
    #[serde(default)]
    pub turn: Option<String>,
    /// Slope of the flights, percent (default: the steepest the type allows).
    #[serde(default)]
    pub slope: Option<f64>,
    /// Landing length, meters (default: the type's shortest landing).
    #[serde(default)]
    pub landing: Option<f64>,
    /// Clear gap between the flights of a U-shaped ramp, meters (default 0.1; 4" in imperial projects).
    #[serde(default)]
    pub gap: Option<f64>,
    /// Radius of the inner edge of a spiral ramp, meters (default 1.5; 5'-0" in imperial projects).
    #[serde(default)]
    pub inner_radius: Option<f64>,
}

impl RampCreate {
    /// The plan layout these parameters ask for.
    pub fn layout(&self, defaults: &cc_core::Defaults) -> Result<RampLayout> {
        let (gap, _, inner_radius) = defaults.flights;
        let mut layout = RampLayout { gap, inner_radius, ..Default::default() };
        if let Some(shape) = justification(self.shape.as_deref(), &SHAPES, "shape")? {
            layout.shape = shape;
        }
        if let Some(turn) = justification(self.turn.as_deref(), &TURNS, "turn")? {
            layout.turn = turn;
        }
        if let Some(v) = self.slope {
            let v = positive(v, "slope")?;
            if v > 100.0 {
                return Err(EngineError::invalid("slope must be at most 100 %"));
            }
            layout.slope = v / 100.0;
        }
        if let Some(v) = self.landing {
            layout.landing = positive(v, "landing")?.min(100.0);
        }
        if let Some(v) = self.gap {
            layout.gap = finite(v, "gap")?.clamp(0.0, 100.0);
        }
        if let Some(v) = self.inner_radius {
            layout.inner_radius = finite(v, "inner_radius")?.clamp(0.0, 100.0);
        }
        Ok(layout)
    }
}

/// A ramp type for a project that has none: 1.20 m wide, a 15 cm slab of
/// the stairs' material, the default rules (4'-0", a 6" slab and the
/// customary rules in imperial projects; see [`cc_core::Defaults::ramp`]).
pub fn new_ramp_type(p: &Project, id: ElementTypeId) -> ElementType {
    let d = p.defaults();
    let (key, width, thickness) = d.ramp;
    let material = p
        .default_type(Category::Stair)
        .and_then(|t| p.element_type(t))
        .map(|t| t.main_material().to_string())
        .or_else(|| p.materials.first().map(|m| m.name.clone()))
        .unwrap_or_default();
    ElementType {
        id,
        name: crate::i18n::tr(key),
        data: TypeData::Ramp { width, thickness, material, rules: d.ramp_rules },
        info: Default::default(),
        uid: Default::default(),
    }
}

impl Command for RampCreate {
    const NAME: &'static str = "ramp_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let top = top(p, self.top_level.as_ref(), None)?.unwrap_or_else(|| default_top(p, level));
        let height = match top {
            WallTop::Level { level: t, .. } => p.level_elevation(t) - p.level_elevation(level),
            WallTop::Unconnected => self.height.unwrap_or(p.defaults().story),
        };
        let ty = match (&self.type_ref, p.default_type(Category::Ramp)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::Ramp, r.as_ref())?),
        };
        let ramp = Ramp {
            start: point(self.start)?,
            direction_deg: finite(self.direction.unwrap_or(0.0), "direction")?,
            base_offset: 0.0,
            height: positive(height, "height")?,
            top,
            layout: self.layout(p.defaults())?,
        };
        let id = edit(s, t!("tx-create-ramp"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_ramp_type(e.project(), id))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::Ramp(ramp)))
        })?;
        Ok(created(s, id))
    }
}

/// Check a ramp: its slope, flights (count, rise and run of each) and the
/// rules of its type it breaks (too steep, a flight rising too high, a
/// short landing).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RampCheck {
    /// Ramp element id.
    pub id: u64,
}

impl Command for RampCheck {
    const NAME: &'static str = "ramp_check";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let Some(e) = p.element(ElementId(self.id)) else {
            return Err(EngineError::not_found(format!("no element {}", self.id)));
        };
        let ElementKind::Ramp(r) = &e.kind else {
            return Err(EngineError::invalid(format!("element {} is not a ramp", self.id)));
        };
        let g = cc_geom::ramp_geom(p, e, r).ok_or_else(|| EngineError::invalid("the ramp has no ramp type"))?;
        let c = &g.check;
        let issues: Vec<Value> = c.issues.iter().map(|i| json!({"message": issue_text(i), "detail": i})).collect();
        Ok(json!({
            "slope": c.slope,
            "flights": c.flights,
            "flight_rise": c.flight_rise,
            "flight_run": c.flight_run,
            "run": g.run(),
            "ok": c.ok(),
            "issues": issues,
        }))
    }
}

/// A broken ramp rule in words.
pub fn issue_text(issue: &cc_core::RampIssue) -> String {
    use cc_core::RampIssue as I;
    if cc_views::format::imperial() {
        let (l, slope) = (cc_views::format::meters, cc_views::format::slope);
        return match *issue {
            I::TooSteep { slope: v, max } => t!("ramp-imp-issue-steep", "slope" => slope(v), "limit" => slope(max)),
            I::FlightTooHigh { rise, max } => t!("ramp-imp-issue-flight-rise", "rise" => l(rise), "limit" => l(max)),
            I::LandingTooShort { landing, min } => {
                t!("ramp-imp-issue-landing-short", "landing" => l(landing), "limit" => l(min))
            }
        };
    }
    let pct = |v: f64| format!("{:.2}", v * 100.0);
    let m = |v: f64| format!("{v:.2}");
    match *issue {
        I::TooSteep { slope, max } => t!("ramp-issue-steep", "slope" => pct(slope), "limit" => pct(max)),
        I::FlightTooHigh { rise, max } => t!("ramp-issue-flight-rise", "rise" => m(rise), "limit" => m(max)),
        I::LandingTooShort { landing, min } => {
            t!("ramp-issue-landing-short", "landing" => m(landing), "limit" => m(min))
        }
    }
}

/// Create a ramp type: a slab `thickness` thick and `width` wide, within the
/// accessibility rules given (by default 8.33 % slope, 0.80 m rise per flight
/// and 1.20 m landings).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RampTypeCreate {
    /// Type name (default: the template's ramp name).
    #[serde(default)]
    pub name: Option<String>,
    /// Width, meters (default 1.2; 4'-0" in imperial projects).
    #[serde(default)]
    pub width: Option<f64>,
    /// Slab thickness, meters (default 0.15; 6" in imperial projects).
    #[serde(default)]
    pub thickness: Option<f64>,
    /// Steepest slope, percent (default 8.33).
    #[serde(default)]
    pub max_slope: Option<f64>,
    /// Highest rise of one flight, meters (default 0.80; 30" in imperial projects).
    #[serde(default)]
    pub max_flight_rise: Option<f64>,
    /// Shortest landing, meters (default 1.20; 60" in imperial projects).
    #[serde(default)]
    pub min_landing: Option<f64>,
}

impl Command for RampTypeCreate {
    const NAME: &'static str = "ramp_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let defaults = s.project().defaults();
        let mut rules = defaults.ramp_rules;
        if let Some(v) = self.max_slope {
            rules.max_slope = positive(v, "max_slope")?.min(100.0) / 100.0;
        }
        if let Some(v) = self.max_flight_rise {
            rules.max_flight_rise = positive(v, "max_flight_rise")?.min(100.0);
        }
        if let Some(v) = self.min_landing {
            rules.min_landing = finite(v, "min_landing")?.clamp(0.0, 100.0);
        }
        let width = positive(self.width.unwrap_or(defaults.ramp.1), "width")?.clamp(0.3, 50.0);
        let thickness = positive(self.thickness.unwrap_or(defaults.ramp.2), "thickness")?.clamp(0.01, 5.0);
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-ramp-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_ramp_type(e.project(), id);
            if let TypeData::Ramp { width: w, thickness: t, rules: r, .. } = &mut ty.data {
                (*w, *t, *r) = (width, thickness, rules);
            }
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}
