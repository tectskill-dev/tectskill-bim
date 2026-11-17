//! Callouts and detail views (see `cc_core::callout`).

use super::edit;
use super::views::view_out;
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{Callout, CropBox, MeasurementSystem, Project, View, ViewCategory, ViewId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Standard metric scales a detail view's default is picked from.
const METRIC_SCALES: [u32; 12] = [1, 2, 5, 10, 20, 25, 50, 75, 100, 200, 250, 500];

/// The default scale of a detail of a view at 1:`parent`: the largest
/// standard scale of the project's `system` at most a fifth of it (1:50
/// gives 1:10; `1/4" = 1'-0"` gives `1 1/2" = 1'-0"`).
pub fn detail_scale(system: MeasurementSystem, parent: u32) -> u32 {
    let target = (parent / 5).max(1);
    let scales: &[u32] = match system {
        MeasurementSystem::Metric => &METRIC_SCALES,
        MeasurementSystem::Imperial => cc_core::scale::choices(system),
    };
    scales.iter().copied().filter(|s| *s <= target).max().unwrap_or(1)
}

/// The rectangle with corners `a` and `b`, checked.
pub(crate) fn crop_box(corners: [Point; 2]) -> Result<CropBox> {
    let rect = CropBox::new(point(corners[0])?, point(corners[1])?);
    if !rect.is_valid() {
        return Err(EngineError::invalid(format!(
            "the rectangle must be at least {} m on each side and within ±{} m",
            cc_core::callout::MIN_CROP,
            cc_core::callout::MAX_CROP
        )));
    }
    Ok(rect)
}

/// A rectangle as commands show it: `[[min x, min y], [max x, max y]]`.
pub(crate) fn rect_json(r: CropBox) -> Value {
    json!([[r.min.x, r.min.y], [r.max.x, r.max.y]])
}

/// A view's crop and callout as `view_get` shows them.
pub(crate) fn view_callout_json(p: &Project, v: &View) -> (Value, Value) {
    let crop = v.crop.map(rect_json).unwrap_or(Value::Null);
    let callout = match (v.callout, v.callout_rect()) {
        (Some(c), Some(rect)) => json!({
            "parent": c.parent.0,
            "parent_name": p.view(c.parent).map(|x| x.name.as_str()),
            "rect": rect_json(rect),
            "drafting": v.is_drafting(),
        }),
        _ => Value::Null,
    };
    (crop, callout)
}

/// Draw a callout in a plan, ceiling plan, section or elevation: a
/// rectangle (two opposite corners in the view's drawing coordinates)
/// standing for a detail view of that region. By default a new detail
/// view is made: of the parent's kind (it stays live with the model,
/// following the parent's level, section line or direction), cropped to
/// the rectangle, at a larger scale, with its own detail lines, regions,
/// detail components, text and dimensions. With `drafting`, the callout
/// points to that existing drafting view instead (no crop). The parent
/// draws the rectangle with rounded corners and a tag with the detail
/// number and the sheet number once the view is placed on a sheet.
/// Moving the callout is changing the detail view's `crop` (`view_set`);
/// deleting the detail view removes it (`callout_remove` keeps the view).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CalloutCreate {
    /// The view the callout is drawn in (id or name): a floor or ceiling plan, a section or an elevation.
    pub parent: Ref,
    /// Two opposite corners of the rectangle, in the parent's drawing
    /// coordinates (meters; in sections and elevations `y` is the height).
    pub rect: [Point; 2],
    /// Scale of the detail view: the denominator, or text such as `"1:10"`
    /// or an imperial label like `"1 1/2\" = 1'-0\""` (default: a standard
    /// scale of the project's units about five times larger than the
    /// parent's, 1:10 under 1:50).
    #[serde(default)]
    pub scale: Option<refs::ScaleArg>,
    /// Name of the detail view (default "Detail N").
    #[serde(default)]
    pub name: Option<String>,
    /// An existing drafting view (id or name) the callout points to instead of a new detail view.
    #[serde(default)]
    pub drafting: Option<Ref>,
}

impl Command for CalloutCreate {
    const NAME: &'static str = "callout_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let parent_id = refs::view(p, &self.parent)?;
        let parent = p.view(parent_id).ok_or_else(|| EngineError::not_found("view"))?.clone();
        if !parent.kind.croppable() {
            return Err(EngineError::invalid(t!("msg-callout-parent")));
        }
        let rect = crop_box(self.rect)?;
        let callout = Callout { parent: parent_id, rect: None };
        if let Some(r) = &self.drafting {
            let target = refs::view(p, r)?;
            let view = p.view(target).ok_or_else(|| EngineError::not_found("view"))?;
            if !view.is_drafting() {
                return Err(EngineError::invalid("not a drafting view"));
            }
            if view.callout.is_some() {
                return Err(EngineError::rejected(t!("msg-callout-taken")));
            }
            if self.scale.is_some() || self.name.is_some() {
                return Err(EngineError::invalid("scale and name belong to new detail views"));
            }
            edit(s, t!("tx-create-callout"), |e| {
                let v = e.view_mut(target).ok_or_else(|| EngineError::not_found("view"))?;
                v.callout = Some(Callout { rect: Some(rect), ..callout });
                Ok(())
            })?;
            return Ok(view_out(s, target));
        }
        if let Some(n) = &self.name
            && n.trim().is_empty()
        {
            return Err(EngineError::invalid("the name is empty"));
        }
        let n = p.views.values().filter(|v| v.is_detail()).count() + 1;
        let name = self.name.map(|n| n.trim().to_string()).unwrap_or_else(|| t!("default-callout-name", "n" => n));
        let scale = match &self.scale {
            Some(sc) => sc.ratio()?,
            None => detail_scale(p.measurement_system(), parent.scale),
        }
        .max(1);
        let id = edit(s, t!("tx-create-callout"), |e| {
            let id = ViewId(e.alloc_id());
            let mut view = View::new(id, name, scale, parent.kind.clone());
            // It shows the model as the parent does (its template aside).
            view.phase = parent.phase;
            view.phase_filter = parent.phase_filter;
            view.detail = parent.detail;
            view.visual_style = parent.visual_style;
            view.graphics = parent.graphics.clone();
            view.crop = Some(rect);
            view.callout = Some(callout);
            Ok(e.add_view(view))
        })?;
        Ok(view_out(s, id))
    }
}

/// Remove a view's callout from its parent; the view stays (a detail view
/// keeps its crop and becomes an ordinary view of its kind).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalloutRemove {
    /// The detail or drafting view the callout stands for (id or name).
    pub view: Ref,
}

impl Command for CalloutRemove {
    const NAME: &'static str = "callout_remove";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::view(s.project(), &self.view)?;
        edit(s, t!("tx-remove-callout"), |e| {
            let v = e.view_mut(id).ok_or_else(|| EngineError::not_found("view"))?;
            if v.callout.take().is_none() {
                return Err(EngineError::not_found("callout"));
            }
            Ok(())
        })?;
        Ok(view_out(s, id))
    }
}

/// The callouts drawn in a view: the detail or drafting view each stands
/// for and its rectangle.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalloutsList {
    /// The parent view (id or name).
    pub view: Ref,
}

impl Command for CalloutsList {
    const NAME: &'static str = "callouts_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::view(p, &self.view)?;
        Ok(p.callouts_in(id)
            .filter_map(|v| {
                let rect = v.callout_rect()?;
                Some(json!({
                    "view": v.id.0,
                    "name": v.name,
                    "category": super::query::snake(&format!("{:?}", v.category())),
                    "drafting": v.category() == ViewCategory::Drafting,
                    "rect": rect_json(rect),
                }))
            })
            .collect())
    }
}
