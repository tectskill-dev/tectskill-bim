//! Strip footing reinforcement: bars across and along the footing slab,
//! and stirrups, side bars and longitudinal bars for the beam on top.

use super::rebar::{bar_type, splice};
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    BarFace, Category, ElementId, ElementKind, ElementTypeId, FoundationShape, LevelId, Project, Rebar, RebarLayout,
    Splice, wall_bars,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The bars of the beam on top of a strip footing (or of the whole section
/// of a strip without one).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StripBeamBars {
    /// Bar type id or name of the longitudinal and side bars (default: the type nearest 10 mm;
    /// 1/2" in imperial projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Longitudinal bars along the top of the beam (default 2; 0 for none).
    #[serde(default)]
    pub top: Option<u32>,
    /// Longitudinal bars along its bottom, in the footing (default 0: the footing's own bars).
    #[serde(default)]
    pub bottom: Option<u32>,
    /// Side bars on each side face (default 0).
    #[serde(default)]
    pub sides: Option<u32>,
    /// Stirrup type id or name (default: the type nearest 5 mm; 3/8" in imperial projects).
    #[serde(default)]
    pub stirrup_type: Option<Ref>,
    /// Stirrup spacing along the strip, meters (default 0.15; 6" in imperial projects; 0 for
    /// none). The stirrups run over the full height of the section, in the width of the beam.
    #[serde(default)]
    pub stirrups: Option<f64>,
}

/// Reinforce strip footings, straight or arc: bars across the footing
/// (`transverse_spacing` apart along it; radial in an arc strip) and bars
/// along it (`longitudinal_spacing` apart across it; ring bars in an arc
/// strip, each as long as the arc at its own radius), in the bottom and
/// top `layers`. A top bar across the footing follows its top, up the
/// slopes of a haunched section and flat between the faces of its beam, and
/// its ends turn down to the bottom layer, closing the edges. The bars
/// along a haunched footing follow the slopes, and leave the beam to its
/// own: `beam` gives it stirrups over the full height of the section, top,
/// bottom and side bars (a haunched strip gets stirrups and two top bars
/// by default; `false` for none). Bars along the strip longer than
/// `max_length` are lap spliced, neighbouring splices staggered. One set
/// per layer and direction, in one undo step; the bars follow the strip.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StripRebarCreate {
    /// Strip footing ids (default: those of `level`, or of the project).
    #[serde(default)]
    pub strips: Vec<u64>,
    /// Level id or name, when no strips are given.
    #[serde(default)]
    pub level: Option<Ref>,
    /// The layers of the footing reinforced: `both` (default), `top` or `bottom`.
    #[serde(default)]
    pub layers: Option<String>,
    /// Bar type id or name of the bars across the footing (default: the type nearest 10 mm; 1/2"
    /// in imperial projects).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Bar type of the bars along the footing (default: `type`).
    #[serde(default)]
    pub longitudinal_type: Option<Ref>,
    /// Distance between the bars across the footing, along it, meters (default 0.15; 6" in
    /// imperial projects; 0 for none).
    #[serde(default)]
    pub transverse_spacing: Option<f64>,
    /// Distance between the bars along the footing, across it, meters (default 0.15; 0 for
    /// none).
    #[serde(default)]
    pub longitudinal_spacing: Option<f64>,
    /// The bars of the beam part: their values, or `false` for none.
    #[serde(default)]
    pub beam: Option<Value>,
    /// Concrete cover, meters (default 0.025; 1 1/2" in imperial projects).
    #[serde(default)]
    pub cover: Option<f64>,
    /// Longest bar along the strip, meters (default: the stock length, 12 m, or 40' in imperial
    /// projects): longer bars are lap spliced. 0 keeps whole bars.
    #[serde(default)]
    pub max_length: Option<f64>,
    /// Lap length of the splices, meters (default: 40 bar diameters).
    #[serde(default)]
    pub lap: Option<f64>,
    /// Run the bars along the strip half a lap past each end joined to another strip that
    /// carries on in the same line (the strips of a ring footing), so the bars of the two
    /// lap there (default false: they stop the cover short of the end). The lap is `lap`,
    /// or 40 bar diameters where bars are kept whole (`max_length` 0).
    #[serde(default)]
    pub lap_at_joins: Option<bool>,
    /// The part of the structure the sets belong to (e.g. "FOUNDATION", "WALL"), which
    /// schedules filter and group by (default: none).
    #[serde(default)]
    pub partition: Option<String>,
}

/// The strips to reinforce, and whether each has a beam on top.
fn hosts(p: &Project, ids: &[u64], level: Option<&Ref>) -> Result<Vec<(ElementId, LevelId, bool)>> {
    let strip = |e: &cc_core::Element| match &e.kind {
        ElementKind::Foundation(f) if matches!(f.shape, FoundationShape::Strip { .. }) => {
            let haunched = p.element_type(e.type_id).is_some_and(|t| t.data.strip_haunch().is_some());
            Some((e.id, e.level, haunched))
        }
        _ => None,
    };
    if !ids.is_empty() {
        return ids
            .iter()
            .map(|id| {
                let id = refs::element(p, *id)?;
                p.element(id)
                    .and_then(strip)
                    .ok_or_else(|| EngineError::invalid(format!("element {} is not a strip footing", id.0)))
            })
            .collect();
    }
    let level = level.map(|r| refs::level(p, r)).transpose()?;
    Ok(p.elements.values().filter(|e| level.is_none_or(|l| e.level == l)).filter_map(strip).collect())
}

/// A spacing, or `None` for 0 (no bars).
fn spacing(value: f64, what: &str) -> Result<Option<f64>> {
    let value = finite(value, what)?;
    if value == 0.0 {
        return Ok(None);
    }
    if value < wall_bars::MIN_SPACING {
        return Err(EngineError::invalid(format!(
            "{what} must be at least {} (or 0 for none)",
            wall_bars::MIN_SPACING
        )));
    }
    Ok(Some(value))
}

impl Command for StripRebarCreate {
    const NAME: &'static str = "strip_rebar_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let [default_cover, default_spacing, main_bar, _, stirrup_bar] = p.defaults().rebar;
        let strips = hosts(p, &self.strips, self.level.as_ref())?;
        let tops: &[bool] = match self.layers.as_deref().unwrap_or("both") {
            "both" => &[false, true],
            "top" => &[true],
            "bottom" => &[false],
            other => return Err(EngineError::invalid(format!("layers must be both, top or bottom, not '{other}'"))),
        };
        let bar = |r: &Option<Ref>, diameter: f64| -> Result<ElementTypeId> {
            match r {
                Some(r) => refs::type_of(p, Category::Rebar, Some(r)),
                None => bar_type(p, diameter),
            }
        };
        let type_id = bar(&self.type_ref, main_bar)?;
        let along_type =
            if self.longitudinal_type.is_some() { bar(&self.longitudinal_type, main_bar)? } else { type_id };
        let cover = finite(self.cover.unwrap_or(default_cover), "cover")?.max(0.0);
        let across = spacing(self.transverse_spacing.unwrap_or(default_spacing), "transverse_spacing")?;
        let along = spacing(self.longitudinal_spacing.unwrap_or(default_spacing), "longitudinal_spacing")?;
        // The beam's bars: asked for, or the default of a haunched strip.
        let (beam, asked): (Option<StripBeamBars>, bool) = match &self.beam {
            None | Some(Value::Bool(true)) => (Some(StripBeamBars::default()), false),
            Some(Value::Bool(false)) | Some(Value::Null) => (None, false),
            Some(v) => {
                let bars = serde_json::from_value(v.clone()).map_err(|e| EngineError::invalid(format!("beam: {e}")))?;
                (Some(bars), true)
            }
        };
        let mut beam_sets: Vec<(ElementTypeId, RebarLayout, Option<Splice>)> = Vec::new();
        if let Some(b) = &beam {
            let ty = bar(&b.type_ref, main_bar)?;
            let count = |n: u32, what: &str| match n {
                0..=200 => Ok(n),
                _ => Err(EngineError::invalid(format!("{what} must be at most 200"))),
            };
            for (face, n) in [
                (BarFace::Top, count(b.top.unwrap_or(2), "top")?),
                (BarFace::Bottom, count(b.bottom.unwrap_or(0), "bottom")?),
            ] {
                if n > 0 {
                    beam_sets.push((
                        ty,
                        RebarLayout::Longitudinal { face, count: n },
                        splice(p, ty, self.max_length, self.lap)?,
                    ));
                }
            }
            let sides = count(b.sides.unwrap_or(0), "sides")?;
            if sides > 0 {
                let layout = RebarLayout::Sides { count: sides, spacing: 0.0 };
                beam_sets.push((ty, layout, splice(p, ty, self.max_length, self.lap)?));
            }
            if let Some(step) = spacing(b.stirrups.unwrap_or(default_spacing), "stirrups")? {
                beam_sets.push((bar(&b.stirrup_type, stirrup_bar)?, RebarLayout::Stirrups { spacing: step }, None));
            }
        }
        let mut todo: Vec<(ElementId, LevelId, ElementTypeId, RebarLayout, Option<Splice>)> = Vec::new();
        for &(host, level, haunched) in &strips {
            for &top in tops {
                if let Some(step) = across {
                    todo.push((host, level, type_id, RebarLayout::StripTransverse { top, spacing: step }, None));
                }
                if let Some(step) = along {
                    let spliced = splice(p, along_type, self.max_length, self.lap)?;
                    todo.push((
                        host,
                        level,
                        along_type,
                        RebarLayout::StripLongitudinal { top, spacing: step },
                        spliced,
                    ));
                }
            }
            // A plain strip has no beam: it gets a beam's bars only when they are asked for.
            if haunched || asked {
                todo.extend(beam_sets.iter().map(|(ty, layout, splice)| (host, level, *ty, layout.clone(), *splice)));
            }
        }
        if todo.is_empty() {
            return Ok(json!({"created": [], "strips": 0}));
        }
        let lap_at_joins = self.lap_at_joins.unwrap_or(false);
        let partition = super::rebar::partition_arg(self.partition.as_deref())?;
        let created = edit(s, t!("tx-create-rebar"), |e| {
            Ok(todo
                .into_iter()
                .map(|(host, level, ty, layout, splice)| {
                    let lap_at_joins = lap_at_joins && layout.runs_along();
                    let rebar =
                        Rebar { splice, lap_at_joins, partition: partition.clone(), ..Rebar::new(host, layout, cover) };
                    add_element(e, level, ty, ElementKind::Rebar(rebar))
                })
                .collect::<Vec<_>>())
        })?;
        Ok(json!({"created": ids_json(&created), "strips": strips.len()}))
    }
}
