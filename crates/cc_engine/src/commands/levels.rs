//! Levels: create (with their plans, also as a storey above or below
//! another), rename, change elevation or storey height, delete.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{EditCtx, ElementKind, Level, LevelId, Project, View, ViewId, ViewKind, WallTop};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The height of a storey when no neighbor gives one, meters.
const STOREY: f64 = 3.0;

fn level_out(s: &Session, id: LevelId) -> Value {
    let p = s.project();
    p.level(id)
        .map(|l| json!({"id": l.id.0, "name": l.name, "elevation": l.elevation, "height": height_above(p, id)}))
        .unwrap_or(json!({ "id": id.0 }))
}

/// The height from level `id` up to the next level, if there is one.
pub fn height_above(p: &Project, id: LevelId) -> Option<f64> {
    p.level_above(id).map(|above| above.elevation - p.level_elevation(id))
}

/// The height from the level below `id` up to it, if there is one.
pub fn height_below(p: &Project, id: LevelId) -> Option<f64> {
    let elevation = p.level_elevation(id);
    p.levels.iter().map(|l| l.elevation).filter(|e| *e < elevation - 1e-9).reduce(f64::max).map(|e| elevation - e)
}

/// The levels above (`up`) or below elevation `z`.
fn beyond(p: &Project, z: f64, up: bool) -> Vec<LevelId> {
    p.levels.iter().filter(|l| if up { l.elevation > z + 1e-9 } else { l.elevation < z - 1e-9 }).map(|l| l.id).collect()
}

/// Move `levels` by `dz` and put the levels back in elevation order.
fn shift(e: &mut EditCtx, levels: &[LevelId], dz: f64) {
    let all = e.levels_mut();
    for l in all.iter_mut().filter(|l| levels.contains(&l.id)) {
        l.elevation += dz;
    }
    all.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
}

/// Add a level (default: 3 m above the highest, named "Level n") with its
/// floor and ceiling plans. With `above` or `below` it is a new storey next
/// to that level: the levels beyond it move away by the new storey's height,
/// so the other storeys keep theirs and the order of the levels stays.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LevelCreate {
    /// Level name (default: "Level n").
    #[serde(default)]
    pub name: Option<String>,
    /// Elevation in meters (not with `above` or `below`).
    #[serde(default)]
    pub elevation: Option<f64>,
    /// Add the level above this one; the levels above it move up by `height`.
    #[serde(default)]
    pub above: Option<Ref>,
    /// Add the level below this one; the levels below it move down by `height`.
    #[serde(default)]
    pub below: Option<Ref>,
    /// With `above` or `below`: the distance to that level, meters (default:
    /// the height of the storey next to it on that side, else 3 m).
    #[serde(default)]
    pub height: Option<f64>,
    /// Also create its floor and ceiling plans (default true).
    #[serde(default)]
    pub plans: Option<bool>,
}

impl Command for LevelCreate {
    const NAME: &'static str = "level_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let top = p.levels.iter().map(|l| l.elevation).fold(None, |m: Option<f64>, e| Some(m.map_or(e, |m| m.max(e))));
        let height = self.height.map(|h| positive(h, "height")).transpose()?;
        // The elevation, and the levels that move away with how far.
        let (elevation, moved) = match (&self.above, &self.below) {
            (Some(_), Some(_)) => return Err(EngineError::invalid("give `above` or `below`, not both")),
            (Some(r), None) | (None, Some(r)) => {
                if self.elevation.is_some() {
                    return Err(EngineError::invalid("`elevation` cannot be given with `above` or `below`"));
                }
                let up = self.above.is_some();
                let beside = refs::level(p, r)?;
                let z = p.level_elevation(beside);
                let (over, under) = (height_above(p, beside), height_below(p, beside));
                let (near, far) = if up { (over, under) } else { (under, over) };
                let h = height.or(near).or(far).unwrap_or(STOREY);
                let dz = if up { h } else { -h };
                (finite(z + dz, "elevation")?, Some((beyond(p, z, up), dz)))
            }
            (None, None) => {
                if height.is_some() {
                    return Err(EngineError::invalid("`height` needs `above` or `below`"));
                }
                match self.elevation {
                    Some(e) => (finite(e, "elevation")?, None),
                    None => (top.map_or(0.0, |t| t + STOREY), None),
                }
            }
        };
        let name = self.name.unwrap_or_else(|| t!("default-level-name", "n" => p.levels.len()));
        let plans = self.plans.unwrap_or(true);
        let id = edit(s, t!("tx-add-level"), |e| {
            let id = LevelId(e.alloc_id());
            if let Some((levels, dz)) = &moved {
                shift(e, levels, *dz);
            }
            let levels = e.levels_mut();
            levels.push(Level { id, name: name.clone(), elevation });
            levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
            if plans {
                for kind in [ViewKind::floor_plan(id), ViewKind::ceiling_plan(id)] {
                    let vid = ViewId(e.alloc_id());
                    e.add_view(View::new(vid, name.clone(), 50, kind));
                }
            }
            Ok(id)
        })?;
        Ok(level_out(s, id))
    }
}

/// Rename a level, change its elevation or the height up to the next level
/// (what is attached to a moved level moves along). A new `height` moves
/// every level above it by the difference, so the levels keep their order and
/// the other storeys their heights. Renaming it leaves its plans' names as
/// they are: a level and its views are named apart.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LevelSet {
    pub level: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New elevation, meters (not with `height`).
    #[serde(default)]
    pub elevation: Option<f64>,
    /// New height from this level up to the next one, meters; the levels
    /// above move along. Refused for the highest level.
    #[serde(default)]
    pub height: Option<f64>,
}

impl Command for LevelSet {
    const NAME: &'static str = "level_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::level(s.project(), &self.level)?;
        let elevation = self.elevation.map(|e| finite(e, "elevation")).transpose()?;
        if let Some(n) = &self.name
            && n.trim().is_empty()
        {
            return Err(EngineError::invalid("the name is empty"));
        }
        // The levels above and how far they move for the new height.
        let moved = match self.height.map(|h| positive(h, "height")).transpose()? {
            Some(_) if elevation.is_some() => {
                return Err(EngineError::invalid("give `elevation` or `height`, not both"));
            }
            Some(h) => {
                let p = s.project();
                let Some(old) = height_above(p, id) else {
                    return Err(EngineError::rejected("the highest level has no level above it"));
                };
                Some((beyond(p, p.level_elevation(id), true), finite(h - old, "height")?))
            }
            None => None,
        };
        edit(s, t!("tx-modify-level"), |e| {
            if let Some((levels, dz)) = &moved {
                shift(e, levels, *dz);
            }
            let levels = e.levels_mut();
            if let Some(l) = levels.iter_mut().find(|l| l.id == id) {
                if let Some(n) = &self.name {
                    l.name = n.trim().to_string();
                }
                if let Some(z) = elevation {
                    l.elevation = z;
                }
            }
            levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
            Ok(())
        })?;
        Ok(level_out(s, id))
    }
}

/// Delete a level no element is on, with its plan views; walls rising to it
/// keep their current height, unconnected. The last level is kept.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LevelDelete {
    pub level: Ref,
}

impl Command for LevelDelete {
    const NAME: &'static str = "level_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level(p, &self.level)?;
        if p.elements.values().any(|e| e.level == level) {
            return Err(EngineError::rejected(t!("msg-level-in-use")));
        }
        if p.levels.len() <= 1 {
            return Err(EngineError::rejected("the last level cannot be deleted"));
        }
        let views: Vec<_> = p.views.values().filter(|v| v.level() == Some(level)).map(|v| v.id).collect();
        let masses: Vec<_> = p
            .elements
            .values()
            .filter(|e| matches!(&e.kind, ElementKind::Mass(m) if m.floors.contains(&level)))
            .map(|e| e.id)
            .collect();
        let attached: Vec<_> = p
            .elements
            .values()
            .filter_map(|e| match &e.kind {
                ElementKind::Wall(w) if matches!(w.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.wall_height(e, w)))
                }
                ElementKind::Column(c) if matches!(c.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.column_span(e, c).1 - p.column_span(e, c).0))
                }
                ElementKind::Stair(st) if matches!(st.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.stair_span(e, st).1 - p.stair_span(e, st).0))
                }
                ElementKind::Ramp(r) if matches!(r.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.ramp_span(e, r).1 - p.ramp_span(e, r).0))
                }
                _ => None,
            })
            .collect();
        edit(s, t!("tx-delete-level"), |e| {
            for v in views {
                e.remove_view(v);
            }
            for (id, height) in attached {
                match e.element_mut(id).map(|x| &mut x.kind) {
                    Some(ElementKind::Wall(w)) => (w.top, w.height) = (WallTop::Unconnected, height),
                    Some(ElementKind::Column(c)) => (c.top, c.height) = (WallTop::Unconnected, height),
                    Some(ElementKind::Stair(st)) => (st.top, st.height) = (WallTop::Unconnected, height),
                    Some(ElementKind::Ramp(r)) => (r.top, r.height) = (WallTop::Unconnected, height),
                    _ => {}
                }
            }
            // Mass floors at the level go with it.
            for id in masses {
                if let Some(ElementKind::Mass(m)) = e.element_mut(id).map(|x| &mut x.kind) {
                    m.floors.retain(|l| *l != level);
                }
            }
            // Underlays showing the level stop showing it.
            let shown: Vec<_> = e
                .project()
                .views
                .values()
                .filter(|v| v.kind.underlay().is_some_and(|u| u.clone().repair(|l| l != level) > 0))
                .map(|v| v.id)
                .collect();
            for v in shown {
                if let Some(u) = e.view_mut(v).and_then(|v| v.kind.underlay_mut()) {
                    u.repair(|l| l != level);
                }
            }
            e.levels_mut().retain(|l| l.id != level);
            Ok(())
        })?;
        Ok(json!({ "deleted": level.0 }))
    }
}
