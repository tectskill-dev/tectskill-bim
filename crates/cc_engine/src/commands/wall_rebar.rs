//! Wall reinforcement: horizontal (ring) and vertical bars near each face
//! of a wall, and ties between the faces.

use super::rebar::{bar_type, splice};
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    BarEnd, Category, ElementId, ElementKind, ElementTypeId, LevelId, Project, Rebar, RebarLayout, SpacingZone, Splice,
    WallSide, wall_bars,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The spacing of horizontal bars up to a height.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneArg {
    /// Height above the wall's base up to which the spacing applies, meters.
    pub up_to: f64,
    /// Distance between the bars that start below that height, meters.
    pub spacing: f64,
}

/// A wall's horizontal bars (ring bars in an arc wall).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct HorizontalBars {
    /// Bar type id or name (default: the type nearest 10 mm; 1/2" in imperial projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Distance between the bars up the wall, meters (default 0.15; 6" in imperial projects).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Instead of one `spacing`: spacings by height from the base, closest at the bottom, for
    /// example `[{"up_to": 2, "spacing": 0.10}, {"up_to": 4, "spacing": 0.12}, {"up_to": 6,
    /// "spacing": 0.15}]`; the last runs to the top of the wall.
    #[serde(default)]
    pub zones: Vec<ZoneArg>,
}

/// How an end of a vertical bar is bent.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct EndArg {
    /// Leg across the wall, meters: towards the outside of the bar's own face when positive
    /// (into the slab beyond it), towards the other face when negative.
    #[serde(default)]
    pub across: f64,
    /// A second leg after it, meters: back beside the bar when positive (up again at the
    /// bottom end), on past its end when negative. 0 for an L hook.
    #[serde(default)]
    pub along: f64,
}

/// A wall's vertical bars.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct VerticalBars {
    /// Bar type id or name (default: the type nearest 10 mm; 1/2" in imperial projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Distance between the bars along the wall, on its centerline, meters (default 0.15; 6"
    /// in imperial projects).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Distance of the first bar after the start of the wall, meters (default 0): half the
    /// spacing puts the bars of an additional set between those of the first.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Height above the base the bars start at, meters (default: the base).
    #[serde(default)]
    pub from: Option<f64>,
    /// Height above the base the bars end at, meters (default: the top): `1.5` makes the
    /// additional bars of the bottom 1.5 m.
    #[serde(default)]
    pub to: Option<f64>,
    /// The bottom end (default: straight).
    #[serde(default)]
    pub bottom: Option<EndArg>,
    /// The top end (default: straight).
    #[serde(default)]
    pub top: Option<EndArg>,
}

/// Ties between the two faces of a wall.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TieBars {
    /// Bar type id or name (default: the type nearest 5 mm; 3/8" in imperial projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// A tie at every this many horizontal bars (default 1: every one).
    #[serde(default)]
    pub every_horizontal: Option<u32>,
    /// A tie at every this many vertical bars (default 2: every other one).
    #[serde(default)]
    pub every_vertical: Option<u32>,
    /// The angles the tie's two ends are hooked by, degrees, 90 to 180: at the exterior
    /// face, then the interior (default `[135, 135]`; `[135, 90]` or `[135, 180]` are
    /// common). The hooks count in the tie's length.
    #[serde(default)]
    pub hooks: Option<[f64; 2]>,
}

/// Reinforce walls, straight or arc: `horizontal` bars up the wall (ring
/// bars in an arc wall, each as long as the arc at its own radius) and
/// `vertical` bars along it near each face, and `ties` between the faces.
/// With none of the three given, horizontal and vertical bars every 15 cm
/// on both faces. Bars lie `cover` inside the faces of the wall's structure
/// (its core layers), stop at its ends and around its doors and windows,
/// and those longer than `max_length` are lap spliced, neighbouring splices
/// staggered. Horizontal bars may close up towards the base (`zones`);
/// vertical bars may cover part of the height, start between those of
/// another set (`offset`) and have bent ends. Each call adds sets, so a
/// second call makes the additional bars of a zone. One set per family and
/// face, in one undo step; the bars follow the wall.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WallRebarCreate {
    /// Wall ids (default: the structural walls of `level`, or of the project).
    #[serde(default)]
    pub walls: Vec<u64>,
    /// Level id or name, when no walls are given.
    #[serde(default)]
    pub level: Option<Ref>,
    /// The faces reinforced: `both` (default), `exterior` or `interior`.
    #[serde(default)]
    pub faces: Option<String>,
    /// Concrete cover of both faces, meters (default 0.025; 1 1/2" in imperial projects).
    #[serde(default)]
    pub cover: Option<f64>,
    /// Cover of the exterior face, when it differs.
    #[serde(default)]
    pub cover_exterior: Option<f64>,
    /// Cover of the interior face, when it differs.
    #[serde(default)]
    pub cover_interior: Option<f64>,
    /// The horizontal bars.
    #[serde(default)]
    pub horizontal: Option<HorizontalBars>,
    /// The vertical bars.
    #[serde(default)]
    pub vertical: Option<VerticalBars>,
    /// The ties between the faces, at the horizontal and vertical bars of the exterior face.
    #[serde(default)]
    pub ties: Option<TieBars>,
    /// The bars nearest the faces: `horizontal` (default) or `vertical`; the other family lies
    /// inside them.
    #[serde(default)]
    pub outer: Option<String>,
    /// Longest bar, meters (default: the stock length, 12 m, or 40' in imperial projects):
    /// longer bars are lap spliced. 0 keeps whole bars.
    #[serde(default)]
    pub max_length: Option<f64>,
    /// Lap length of the splices, meters (default: 40 bar diameters).
    #[serde(default)]
    pub lap: Option<f64>,
    /// Run the horizontal bars half a lap past each end joined to another wall that carries
    /// on in the same line (the walls of a cylinder), so the bars of the two lap there
    /// (default false: they meet end to end at the joint). The lap is `lap`,
    /// or 40 bar diameters where bars are kept whole (`max_length` 0).
    #[serde(default)]
    pub lap_at_joins: Option<bool>,
    /// The part of the structure the sets belong to (e.g. "FOUNDATION", "WALL"), which
    /// schedules filter and group by (default: none).
    #[serde(default)]
    pub partition: Option<String>,
}

fn spacing(value: f64, what: &str) -> Result<f64> {
    let value = finite(value, what)?;
    if value < wall_bars::MIN_SPACING {
        return Err(EngineError::invalid(format!("{what} must be at least {}", wall_bars::MIN_SPACING)));
    }
    Ok(value)
}

fn end(arg: Option<EndArg>, what: &str) -> Result<BarEnd> {
    let Some(arg) = arg else { return Ok(BarEnd::default()) };
    let end = BarEnd { across: finite(arg.across, what)?, along: finite(arg.along, what)? };
    if !end.is_valid() {
        return Err(EngineError::invalid(format!(
            "{what}: legs are at most {} m, and `along` needs an `across` leg first",
            BarEnd::MAX_LEG
        )));
    }
    Ok(end)
}

/// The walls to reinforce: those given (each a wall with a body of its own),
/// or the structural walls of the level or project; and how many were left out.
fn hosts(p: &Project, ids: &[u64], level: Option<&Ref>) -> Result<(Vec<(ElementId, LevelId)>, usize)> {
    let layered = |e: &cc_core::Element| p.element_type(e.type_id).is_some_and(|t| !t.layers().is_empty());
    if !ids.is_empty() {
        let mut out = Vec::new();
        for id in ids {
            let id = refs::element(p, *id)?;
            match p.element(id) {
                Some(e) if e.as_wall().is_some() && layered(e) => out.push((id, e.level)),
                Some(e) if e.as_wall().is_some() => {
                    return Err(EngineError::invalid("a curtain wall takes no reinforcement"));
                }
                _ => return Err(EngineError::invalid(format!("element {} is not a wall", id.0))),
            }
        }
        return Ok((out, 0));
    }
    let level = level.map(|r| refs::level(p, r)).transpose()?;
    let walls = || p.elements.values().filter(|e| level.is_none_or(|l| e.level == l)).filter(|e| e.as_wall().is_some());
    let chosen: Vec<(ElementId, LevelId)> =
        walls().filter(|e| layered(e) && e.as_wall().is_some_and(|w| w.structural)).map(|e| (e.id, e.level)).collect();
    Ok((chosen.clone(), walls().count() - chosen.len()))
}

impl Command for WallRebarCreate {
    const NAME: &'static str = "wall_rebar_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let [default_cover, default_spacing, main_bar, _, tie_bar] = p.defaults().rebar;
        let (walls, skipped) = hosts(p, &self.walls, self.level.as_ref())?;
        let sides: &[WallSide] = match self.faces.as_deref().unwrap_or("both") {
            "both" => &WallSide::ALL,
            "exterior" => &[WallSide::Exterior],
            "interior" => &[WallSide::Interior],
            other => {
                return Err(EngineError::invalid(format!("faces must be both, exterior or interior, not '{other}'")));
            }
        };
        let horizontal_outer = match self.outer.as_deref().unwrap_or("horizontal") {
            "horizontal" => true,
            "vertical" => false,
            other => return Err(EngineError::invalid(format!("outer must be horizontal or vertical, not '{other}'"))),
        };
        let cover = |own: Option<f64>, what: &str| -> Result<f64> {
            Ok(finite(own.or(self.cover).unwrap_or(default_cover), what)?.max(0.0))
        };
        let covers = [cover(self.cover_exterior, "cover_exterior")?, cover(self.cover_interior, "cover_interior")?];
        let cover_of = |side: WallSide| if side == WallSide::Exterior { covers[0] } else { covers[1] };
        // With nothing asked for: bars both ways.
        let typical = self.horizontal.is_none() && self.vertical.is_none() && self.ties.is_none();
        let horizontal = self.horizontal.clone().or(typical.then(HorizontalBars::default));
        let vertical = self.vertical.clone().or(typical.then(VerticalBars::default));
        let bar = |r: &Option<Ref>, diameter: f64| -> Result<ElementTypeId> {
            match r {
                Some(r) => refs::type_of(p, Category::Rebar, Some(r)),
                None => bar_type(p, diameter),
            }
        };
        // The sets every wall gets: type, layout, cover and splice.
        let mut sets: Vec<(ElementTypeId, RebarLayout, f64, Option<Splice>)> = Vec::new();
        if let Some(h) = &horizontal {
            let zones = match (h.spacing, h.zones.is_empty()) {
                (Some(_), false) => {
                    return Err(EngineError::invalid("give the horizontal bars a `spacing` or `zones`"));
                }
                (own, true) => {
                    vec![SpacingZone { up_to: 0.0, spacing: spacing(own.unwrap_or(default_spacing), "spacing")? }]
                }
                (None, false) => h
                    .zones
                    .iter()
                    .map(|z| {
                        Ok(SpacingZone { up_to: finite(z.up_to, "up_to")?, spacing: spacing(z.spacing, "spacing")? })
                    })
                    .collect::<Result<_>>()?,
            };
            let type_id = bar(&h.type_ref, main_bar)?;
            for &side in sides {
                let layout = RebarLayout::WallHorizontal { side, zones: zones.clone(), inner: !horizontal_outer };
                if !layout.is_valid() {
                    return Err(EngineError::invalid(format!(
                        "zones: at most {} of them, each `up_to` higher than the one before",
                        wall_bars::MAX_ZONES
                    )));
                }
                sets.push((type_id, layout, cover_of(side), splice(p, type_id, self.max_length, self.lap)?));
            }
        }
        if let Some(v) = &vertical {
            let type_id = bar(&v.type_ref, main_bar)?;
            let (from, to) = (finite(v.from.unwrap_or(0.0), "from")?, finite(v.to.unwrap_or(0.0), "to")?);
            let (bottom, top) = (end(v.bottom, "bottom")?, end(v.top, "top")?);
            for &side in sides {
                let layout = RebarLayout::WallVertical {
                    side,
                    spacing: spacing(v.spacing.unwrap_or(default_spacing), "spacing")?,
                    offset: finite(v.offset.unwrap_or(0.0), "offset")?,
                    from,
                    to,
                    bottom,
                    top,
                    inner: horizontal_outer,
                };
                if !layout.is_valid() {
                    return Err(EngineError::invalid("from must not be negative, and to (when given) above it"));
                }
                sets.push((type_id, layout, cover_of(side), splice(p, type_id, self.max_length, self.lap)?));
            }
        }
        if let Some(ties) = &self.ties {
            let layout = RebarLayout::WallTies {
                every_horizontal: ties.every_horizontal.unwrap_or(1),
                every_vertical: ties.every_vertical.unwrap_or(2),
                hooks: ties.hooks.unwrap_or_else(wall_bars::default_hooks),
            };
            if !layout.is_valid() {
                return Err(EngineError::invalid(format!(
                    "every_horizontal and every_vertical must be between 1 and {}, and hooks 90 to 180 degrees",
                    wall_bars::MAX_EVERY
                )));
            }
            sets.push((bar(&ties.type_ref, tie_bar)?, layout, covers[0].min(covers[1]), None));
        }
        if walls.is_empty() || sets.is_empty() {
            return Ok(json!({"created": [], "walls": 0, "skipped": skipped}));
        }
        let count = walls.len();
        let lap_at_joins = self.lap_at_joins.unwrap_or(false);
        let partition = super::rebar::partition_arg(self.partition.as_deref())?;
        let created = edit(s, t!("tx-create-rebar"), |e| {
            let mut ids = Vec::new();
            for (host, level) in &walls {
                for (type_id, layout, cover, splice) in &sets {
                    let rebar = Rebar {
                        splice: *splice,
                        lap_at_joins: lap_at_joins && matches!(layout, RebarLayout::WallHorizontal { .. }),
                        partition: partition.clone(),
                        ..Rebar::new(*host, layout.clone(), *cover)
                    };
                    ids.push(add_element(e, *level, *type_id, ElementKind::Rebar(rebar)));
                }
            }
            Ok(ids)
        })?;
        Ok(json!({"created": ids_json(&created), "walls": count, "skipped": skipped}))
    }
}
