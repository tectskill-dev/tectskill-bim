//! Slab reinforcement: a mesh, or ring and radial bars, in the top and
//! bottom layers of floors and foundation slabs, and trimming bars around
//! their openings.

use super::rebar::{bar_type, splice};
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, ElementId, ElementKind, ElementTypeId, FoundationShape, LevelId, Project, Rebar, RebarLayout, Splice,
    TypeData, slab_bars, wall_bars,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Trimming bars around the openings of a slab.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TrimBars {
    /// Bar type id or name (default: the type of the slab's bars).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Bars along each side of an opening (default 2).
    #[serde(default)]
    pub count: Option<u32>,
    /// Distance between them, meters (default 0.05; 2" in imperial projects).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// How far each bar reaches past the opening, meters (default: 40 bar diameters).
    #[serde(default)]
    pub anchorage: Option<f64>,
    /// A diagonal bar across each corner of the opening (default true).
    #[serde(default)]
    pub diagonals: Option<bool>,
}

/// Reinforce slabs: floors and foundation slabs of any boundary. `layout`
/// is `mesh` (default): straight bars in two directions, `angle` and square
/// to it, `spacing` (and `cross_spacing`) apart; or `radial`: ring bars
/// `spacing` apart around `center` and `radial_count` radial bars, with
/// shorter bars from `intermediate` between them. The bars go in the `top`
/// and `bottom` layers (`layers`), `cover` inside the faces of the slab's
/// structure, its edge and its openings: each bar is clipped to the true
/// boundary (arcs and circles as drawn), so every bar has its own length
/// (`element_get` lists a set's `derived.bars` by length). `hook` bends the
/// ends of mesh bars at the slab's edge. Bars longer than `max_length` are
/// lap spliced, neighbouring splices staggered. A slab with openings gets
/// trimming bars around them (`trim`; `false` for none). One set per layer
/// and direction, in one undo step; the bars follow the slab. Without
/// `slabs`, the structural floors (`floor_create` `structural`) and the
/// foundation slabs are reinforced.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SlabRebarCreate {
    /// Floor and foundation slab ids (default: the structural floors and the foundation slabs
    /// of `level`, or of the project).
    #[serde(default)]
    pub slabs: Vec<u64>,
    /// Level id or name, when no slabs are given.
    #[serde(default)]
    pub level: Option<Ref>,
    /// `mesh` (default) or `radial`.
    #[serde(default)]
    pub layout: Option<String>,
    /// The layers reinforced: `both` (default), `top` or `bottom`.
    #[serde(default)]
    pub layers: Option<String>,
    /// Bar type id or name (default: the type nearest 10 mm; 1/2" in imperial projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Bar type of the second direction of a mesh, or of the radial bars (default: `type`).
    #[serde(default)]
    pub cross_type: Option<Ref>,
    /// Distance between the bars of a mesh, or between ring bars, meters (default 0.15; 6" in
    /// imperial projects).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Distance between the bars of the second direction of a mesh (default: `spacing`).
    #[serde(default)]
    pub cross_spacing: Option<f64>,
    /// Direction of the first bars of a mesh, degrees from the plan's X axis (default 0); the
    /// second direction is square to it. The first direction is the outer layer.
    #[serde(default)]
    pub angle: Option<f64>,
    /// Bend at the ends of mesh bars on the slab's edge, meters: positive towards the middle of
    /// the slab (down for top bars, up for bottom ones), negative out of it (into the wall
    /// above or below). Default 0: straight ends.
    #[serde(default)]
    pub hook: Option<f64>,
    /// `radial`: the center of the rings and radial bars (default: the middle of the slab).
    #[serde(default)]
    pub center: Option<[f64; 2]>,
    /// `radial`: the number of radial bars (default: as many as keep them `spacing` apart at
    /// the edge).
    #[serde(default)]
    pub radial_count: Option<u32>,
    /// `radial`: the radius the radial bars start at, meters (default 0: the center).
    #[serde(default)]
    pub start: Option<f64>,
    /// `radial`: the radius from which a shorter bar runs between each two radial bars (default
    /// 0: none).
    #[serde(default)]
    pub intermediate: Option<f64>,
    /// Concrete cover, meters (default 0.025; 1 1/2" in imperial projects).
    #[serde(default)]
    pub cover: Option<f64>,
    /// Trimming bars around openings: their values, or `false` for none (default: two bars along
    /// each side reaching 40 diameters past the opening, and a diagonal across each corner).
    #[serde(default)]
    pub trim: Option<Value>,
    /// Longest bar, meters (default: the stock length, 12 m, or 40' in imperial projects):
    /// longer bars are lap spliced. 0 keeps whole bars.
    #[serde(default)]
    pub max_length: Option<f64>,
    /// Lap length of the splices, meters (default: 40 bar diameters).
    #[serde(default)]
    pub lap: Option<f64>,
    /// The part of the structure the sets belong to (e.g. "FOUNDATION", "BOTTOM SLAB"), which
    /// schedules filter and group by (default: none).
    #[serde(default)]
    pub partition: Option<String>,
}

/// A slab to reinforce: its element, level, whether it has openings, and
/// the farthest its boundary lies from `center` (or from its middle).
struct Host {
    id: ElementId,
    level: LevelId,
    openings: bool,
    outline: Vec<DVec2>,
}

fn slab(e: &cc_core::Element) -> Option<Host> {
    let (outline, openings) = match &e.kind {
        ElementKind::Floor(f) => (f.boundary.clone(), f.all_openings().next().is_some()),
        ElementKind::Foundation(f) => match &f.shape {
            FoundationShape::Slab { boundary } => (boundary.clone(), false),
            _ => return None,
        },
        _ => return None,
    };
    Some(Host { id: e.id, level: e.level, openings, outline })
}

fn hosts(p: &Project, ids: &[u64], level: Option<&Ref>) -> Result<Vec<Host>> {
    if !ids.is_empty() {
        return ids
            .iter()
            .map(|id| {
                let id = refs::element(p, *id)?;
                p.element(id).and_then(slab).ok_or_else(|| {
                    EngineError::invalid(format!("element {} is not a floor or a foundation slab", id.0))
                })
            })
            .collect();
    }
    // Without a list: the slabs that carry, not every floor finish.
    let level = level.map(|r| refs::level(p, r)).transpose()?;
    let carries = |e: &&cc_core::Element| !matches!(&e.kind, ElementKind::Floor(f) if !f.structural);
    Ok(p.elements.values().filter(|e| level.is_none_or(|l| e.level == l)).filter(carries).filter_map(slab).collect())
}

fn spacing(value: f64, what: &str) -> Result<f64> {
    let value = finite(value, what)?;
    if value < wall_bars::MIN_SPACING {
        return Err(EngineError::invalid(format!("{what} must be at least {}", wall_bars::MIN_SPACING)));
    }
    Ok(value)
}

fn diameter(p: &Project, type_id: ElementTypeId) -> f64 {
    match p.element_type(type_id).map(|t| &t.data) {
        Some(TypeData::Rebar { diameter, .. }) => *diameter,
        _ => 0.01,
    }
}

impl Command for SlabRebarCreate {
    const NAME: &'static str = "slab_rebar_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let [default_cover, default_spacing, main_bar, ..] = p.defaults().rebar;
        let slabs = hosts(p, &self.slabs, self.level.as_ref())?;
        let radial = match self.layout.as_deref().unwrap_or("mesh") {
            "mesh" => false,
            "radial" => true,
            other => return Err(EngineError::invalid(format!("layout must be mesh or radial, not '{other}'"))),
        };
        let tops: &[bool] = match self.layers.as_deref().unwrap_or("both") {
            "both" => &[false, true],
            "top" => &[true],
            "bottom" => &[false],
            other => return Err(EngineError::invalid(format!("layers must be both, top or bottom, not '{other}'"))),
        };
        let bar = |r: &Option<Ref>| -> Result<ElementTypeId> {
            match r {
                Some(r) => refs::type_of(p, Category::Rebar, Some(r)),
                None => bar_type(p, main_bar),
            }
        };
        let type_id = bar(&self.type_ref)?;
        let cross_type = if self.cross_type.is_some() { bar(&self.cross_type)? } else { type_id };
        let cover = finite(self.cover.unwrap_or(default_cover), "cover")?.max(0.0);
        let step = spacing(self.spacing.unwrap_or(default_spacing), "spacing")?;
        let cross_step = spacing(self.cross_spacing.unwrap_or(step), "cross_spacing")?;
        let angle = finite(self.angle.unwrap_or(0.0), "angle")?;
        let hook = finite(self.hook.unwrap_or(0.0), "hook")?;
        let center = self
            .center
            .map(|c| Ok::<_, EngineError>(DVec2::new(finite(c[0], "center")?, finite(c[1], "center")?)))
            .transpose()?;
        let (start, intermediate) =
            (finite(self.start.unwrap_or(0.0), "start")?, finite(self.intermediate.unwrap_or(0.0), "intermediate")?);
        let spliced = |type_id| splice(p, type_id, self.max_length, self.lap);
        // The trimming bars, unless turned off.
        let trim: Option<TrimBars> = match &self.trim {
            None | Some(Value::Bool(true)) => Some(TrimBars::default()),
            Some(Value::Bool(false)) | Some(Value::Null) => None,
            Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| EngineError::invalid(format!("trim: {e}")))?),
        };
        let trim = trim
            .map(|t| -> Result<(ElementTypeId, u32, f64, f64, bool)> {
                let ty = if t.type_ref.is_some() { bar(&t.type_ref)? } else { type_id };
                let anchorage = finite(t.anchorage.unwrap_or(40.0 * diameter(p, ty)), "anchorage")?;
                let gap = spacing(t.spacing.unwrap_or(if default_spacing > 0.151 { 0.0508 } else { 0.05 }), "spacing")?;
                Ok((ty, t.count.unwrap_or(2), gap, anchorage, t.diagonals.unwrap_or(true)))
            })
            .transpose()?;
        // Its values are checked even when no slab has an opening.
        if let Some((_, count, gap, anchorage, diagonals)) = trim
            && !(RebarLayout::SlabTrim { top: false, count, spacing: gap, anchorage, diagonals }).is_valid()
        {
            return Err(EngineError::invalid(format!(
                "trim: 1 to {} bars a side, an anchorage of at most {} m",
                slab_bars::MAX_TRIM,
                slab_bars::MAX_LENGTH
            )));
        }
        // The sets of each slab: type, layout and splice.
        let mut todo: Vec<(ElementId, LevelId, ElementTypeId, RebarLayout, Option<Splice>)> = Vec::new();
        for host in &slabs {
            for &top in tops {
                let mut sets: Vec<(ElementTypeId, RebarLayout)> = Vec::new();
                if radial {
                    // As many radial bars as keep them a spacing apart at the edge.
                    let lo = host.outline.iter().fold(DVec2::INFINITY, |m, q| m.min(*q));
                    let hi = host.outline.iter().fold(DVec2::NEG_INFINITY, |m, q| m.max(*q));
                    let middle = center.unwrap_or((lo + hi) * 0.5);
                    let far = host.outline.iter().map(|q| q.distance(middle)).fold(0.0, f64::max);
                    let fit =
                        ((std::f64::consts::TAU * far / cross_step).ceil()).clamp(3.0, slab_bars::MAX_RADIAL as f64);
                    let count = self.radial_count.unwrap_or(fit as u32);
                    sets.push((type_id, RebarLayout::SlabRing { top, spacing: step, center, inner: false }));
                    sets.push((
                        cross_type,
                        RebarLayout::SlabRadial { top, count, center, start, intermediate, inner: true },
                    ));
                } else {
                    sets.push((
                        type_id,
                        RebarLayout::SlabStraight { top, angle_deg: angle, spacing: step, inner: false, hook },
                    ));
                    let cross = RebarLayout::SlabStraight {
                        top,
                        angle_deg: angle + 90.0,
                        spacing: cross_step,
                        inner: true,
                        hook,
                    };
                    sets.push((cross_type, cross));
                }
                if let Some((ty, count, gap, anchorage, diagonals)) = trim
                    && host.openings
                {
                    sets.push((ty, RebarLayout::SlabTrim { top, count, spacing: gap, anchorage, diagonals }));
                }
                for (ty, layout) in sets {
                    if !layout.is_valid() {
                        return Err(EngineError::invalid(format!(
                            "out of range: hooks, anchorages and radii are at most {} m, radial bars at most {}, trimming \
                             bars 1 to {} a side",
                            slab_bars::MAX_LENGTH,
                            slab_bars::MAX_RADIAL,
                            slab_bars::MAX_TRIM
                        )));
                    }
                    let splice = if layout.runs_along() { spliced(ty)? } else { None };
                    todo.push((host.id, host.level, ty, layout, splice));
                }
            }
        }
        if todo.is_empty() {
            return Ok(json!({"created": [], "slabs": 0}));
        }
        let partition = super::rebar::partition_arg(self.partition.as_deref())?;
        let created = edit(s, t!("tx-create-rebar"), |e| {
            Ok(todo
                .into_iter()
                .map(|(host, level, ty, layout, splice)| {
                    let rebar = Rebar { splice, partition: partition.clone(), ..Rebar::new(host, layout, cover) };
                    add_element(e, level, ty, ElementKind::Rebar(rebar))
                })
                .collect::<Vec<_>>())
        })?;
        Ok(json!({"created": ids_json(&created), "slabs": slabs.len()}))
    }
}
