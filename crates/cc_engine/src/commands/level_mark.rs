//! Level marks: the elevation of a point written in a plan, an elevation or
//! a section (see `cc_core::level_mark`).

use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::level_mark::{LevelMark, MAX_LEVEL};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, MeasurementSystem, TypeData, ViewKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Place a level mark in a plan, an elevation or a section: the finished
/// level of a point above a shelf line and its raw (structural) level under
/// it, each after the type's datum (the value written for the project's
/// zero) is added. Plans draw a circle with two filled quarters on the
/// point and measure the top of the highest floor of the plan's level under
/// it (the level where there is none), with the top of that floor's
/// structure as the raw level; elevations and sections draw a triangle
/// pointing down to the point and measure its height (`position` y), with
/// the raw level of a floor whose top the point lies on, in sections.
/// `element_get` gives the levels and texts (`derived`); `element_set`
/// changes the mark's `params`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LevelMarkCreate {
    /// View id or name: a floor or ceiling plan, an elevation or a section.
    pub view: Ref,
    /// The point measured, in the view's drawing coordinates (in elevations
    /// and sections `y` is the elevation).
    pub position: Point,
    /// Level mark type id or name (default: the default level mark type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Write the values on the left of the symbol (default: on the right).
    #[serde(default)]
    pub flip: Option<bool>,
    /// Finished level to write instead of the measured one, meters (an
    /// elevation of the project, before the datum).
    #[serde(default)]
    pub finished: Option<f64>,
    /// Raw level to write instead of the measured one, meters.
    #[serde(default)]
    pub raw: Option<f64>,
    /// Write the raw level when there is one (default true).
    #[serde(default)]
    pub show_raw: Option<bool>,
}

fn level_value(v: Option<f64>, name: &str) -> Result<Option<f64>> {
    let Some(v) = v else { return Ok(None) };
    let v = finite(v, name)?;
    if v.abs() > MAX_LEVEL {
        return Err(EngineError::invalid(format!("{name} must be within ±{MAX_LEVEL} m")));
    }
    Ok(Some(v))
}

impl Command for LevelMarkCreate {
    const NAME: &'static str = "level_mark_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let view_id = refs::view(p, &self.view)?;
        let view = p.view(view_id).ok_or_else(|| EngineError::not_found("view"))?;
        if !matches!(
            view.kind,
            ViewKind::FloorPlan { .. }
                | ViewKind::CeilingPlan { .. }
                | ViewKind::Elevation { .. }
                | ViewKind::Section { .. }
        ) {
            return Err(EngineError::invalid(t!("msg-level-mark-view")));
        }
        let level = view
            .level()
            .or_else(|| p.levels.first().map(|l| l.id))
            .ok_or_else(|| EngineError::rejected("no levels"))?;
        // A project gets its level mark type with its first mark.
        let ty = match (&self.type_ref, p.default_type(Category::LevelMark)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::LevelMark, r.as_ref())?),
        };
        let mark = LevelMark {
            flip: self.flip.unwrap_or(false),
            finished: level_value(self.finished, "finished")?,
            raw: level_value(self.raw, "raw")?,
            show_raw: self.show_raw.unwrap_or(true),
            ..LevelMark::new(view_id, point(self.position)?)
        };
        let kind = ElementKind::LevelMark(Box::new(mark));
        let id = edit(s, t!("tx-create-level-mark"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    let system = e.project().measurement_system();
                    e.add_type(new_type(id, None, system))
                }
            };
            Ok(add_element(e, level, type_id, kind))
        })?;
        Ok(created(s, id))
    }
}

/// A level mark type in the interface language: text 2.5 mm (3/32" in
/// imperial projects), the project's zero written as `datum` (0 by default).
fn new_type(id: ElementTypeId, name: Option<String>, system: MeasurementSystem) -> ElementType {
    let mut data = cc_core::level_mark::default_type(&crate::i18n::tr);
    let default_name = match system {
        MeasurementSystem::Metric => t!("tpl-type-level-mark"),
        MeasurementSystem::Imperial => {
            if let TypeData::LevelMark { text_size, .. } = &mut data {
                *text_size = IMPERIAL_TEXT_SIZE;
            }
            t!("tpl-imp-type-level-mark")
        }
    };
    ElementType { id, name: name.unwrap_or(default_name), data, info: Default::default(), uid: Default::default() }
}

/// Text size of an imperial project's default level mark type: 3/32",
/// paper millimeters.
const IMPERIAL_TEXT_SIZE: f64 = 3.0 / 32.0 * 25.4;

/// Create a level mark type: the text size, the datum (the value written
/// for the project's zero, e.g. 100 when the ground floor is +100.00) and
/// the suffixes after the finished and raw levels. A project gets one with
/// its first level mark; `type_set` changes them.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LevelMarkTypeCreate {
    /// Type name (default: the language's).
    #[serde(default)]
    pub name: Option<String>,
    /// Text size, paper millimeters (default 2.5; 3/32", 2.38125, in imperial projects).
    #[serde(default)]
    pub text_size: Option<f64>,
    /// Value written for the project's zero, meters (default 0).
    #[serde(default)]
    pub datum: Option<f64>,
    /// Text after the finished level (default: the language's, e.g. "(N.A.)").
    #[serde(default)]
    pub finished_suffix: Option<String>,
    /// Text after the raw level (default: the language's, e.g. "(N.O.)").
    #[serde(default)]
    pub raw_suffix: Option<String>,
}

impl Command for LevelMarkTypeCreate {
    const NAME: &'static str = "level_mark_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let size = match self.text_size {
            Some(v) => {
                let v = finite(v, "text_size")?;
                if !(0.5..=50.0).contains(&v) {
                    return Err(EngineError::invalid("text_size must be from 0.5 to 50 mm"));
                }
                Some(v)
            }
            None => None,
        };
        let datum = level_value(self.datum, "datum")?;
        let id = edit(s, t!("tx-create-level-mark-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_type(id, self.name.clone(), e.project().measurement_system());
            if let TypeData::LevelMark { text_size, datum: d, finished_suffix, raw_suffix } = &mut ty.data {
                *text_size = size.unwrap_or(*text_size);
                *d = datum.unwrap_or(*d);
                if let Some(text) = &self.finished_suffix {
                    finished_suffix.clone_from(text);
                }
                if let Some(text) = &self.raw_suffix {
                    raw_suffix.clone_from(text);
                }
            }
            Ok(e.add_type(ty))
        })?;
        Ok(serde_json::json!({ "id": id.0 }))
    }
}
