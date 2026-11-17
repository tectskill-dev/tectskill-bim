//! Light steel and wood framing of walls (see `cc_core::wall_framing`):
//! the host element and the catalog types its members get by default.

use super::structure_material::{type_or_row, type_or_turned_row};
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::wall_framing::{self as wf, FramingRoleKind, FramingSystem, WallFraming};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, LevelId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The catalog rows a system's members get by default: (series, designation).
#[derive(Debug, Clone, Copy)]
pub struct FramingRows {
    /// A column type of this row.
    pub stud: (&'static str, &'static str),
    /// Beam types of these rows: the track turned 90 degrees to lie flat.
    pub track: (&'static str, &'static str),
    pub header: (&'static str, &'static str),
}

pub fn framing_rows(system: FramingSystem) -> FramingRows {
    match system {
        FramingSystem::LightSteel => FramingRows {
            stud: ("NBR6355-Ue", "Ue 90x40x12x0,95"),
            track: ("NBR6355-U", "U 92x38x0,95"),
            header: ("NBR6355-Ue", "Ue 140x40x12x0,95"),
        },
        FramingSystem::Wood => FramingRows {
            stud: ("Timber-Sawn", "Peça 6x8"),
            track: ("Timber-Sawn", "Peça 6x8"),
            header: ("Timber-Sawn", "Peça 6x16"),
        },
    }
}

/// The type a project gets with its first wall framing.
pub fn new_wall_framing_type(id: ElementTypeId) -> ElementType {
    wf::host_type(id, t!("tpl-type-wall-framing"))
}

/// Frame straight walls in light steel or wood, one framing per wall. Studs
/// stand every `spacing` from the wall start (plus one flush with each end)
/// between a bottom track or plate and one top track (two plates for wood).
/// Each door or window gets king and jack studs, a header, a sill (windows)
/// and cripples. Blocking at mid-height is optional. The studs are columns
/// and the rest are beams, all ordinary elements that the framing keeps in
/// step with its parameters, its wall and the wall's doors and windows
/// (`element_set` on the framing regenerates them, keeping their ids). A
/// member edited or deleted by hand is detached and left alone, and
/// deleting the wall deletes its framing and members. The framing and its
/// members follow the wall's phases, design option and level. Default types come
/// from the profile catalog, made when missing:
/// - light steel: `Ue 90x40x12x0,95` studs, `U 92x38x0,95` tracks and `Ue 140x40x12x0,95` headers;
/// - wood: `Peça 6x8` studs and plates, and `Peça 6x16` headers.
///
/// The tracks and plates lie flat: their default beam type has the profile
/// turned 90 degrees, so its depth runs across the wall and its width
/// (38 mm, 6 cm) is their height. A given type is used as it draws: the
/// height of its section is the height of the track.
///
/// Curved walls, walls already framed and out-of-range values are refused
/// before anything is made. One undo step; returns `framings`, each with its
/// `id`, `wall` and `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WallFramingCreate {
    /// Ids of the straight walls to frame (each gets its own framing).
    pub walls: Vec<u64>,
    /// `light_steel` (default) or `wood`.
    #[serde(default)]
    pub system: Option<String>,
    /// Stud spacing on center from the wall start, meters, 0.2 to 1.2 (default 0.4 for light steel, 0.6 for wood).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Column type of the studs, id or name (default: the system's catalog stud).
    #[serde(default)]
    pub stud_type: Option<Ref>,
    /// Beam type of the tracks or plates and of the blocking, id or name (default: the system's catalog track or plate).
    #[serde(default)]
    pub track_type: Option<Ref>,
    /// Beam type of the headers and sills, id or name (default: the system's catalog header).
    #[serde(default)]
    pub header_type: Option<Ref>,
    /// An extra top plate (wood only; default on for wood).
    #[serde(default)]
    pub double_top: Option<bool>,
    /// A row of blocking at mid-height between the studs (default off).
    #[serde(default)]
    pub blocking: Option<bool>,
}

impl Command for WallFramingCreate {
    const NAME: &'static str = "wall_framing_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        if self.walls.is_empty() {
            return Err(EngineError::invalid("walls: give at least one wall id"));
        }
        let system = match &self.system {
            Some(c) => FramingSystem::from_code(c)
                .ok_or_else(|| EngineError::invalid(format!("unknown system '{c}' (light_steel, wood)")))?,
            None => FramingSystem::LightSteel,
        };
        let mut walls: Vec<(ElementId, LevelId)> = Vec::new();
        for &raw in &self.walls {
            let id = refs::element(p, raw)?;
            let Some((e, w)) = p.element(id).and_then(|e| e.as_wall().map(|w| (e, w))) else {
                return Err(EngineError::invalid(format!("element {raw} is not a wall")));
            };
            if w.arc().is_some() {
                return Err(EngineError::invalid(format!("wall {raw} is curved: only straight walls are framed")));
            }
            if p.wall_framing_of(id).is_some() {
                return Err(EngineError::invalid(format!("wall {raw} is framed already")));
            }
            if walls.iter().any(|(x, _)| *x == id) {
                return Err(EngineError::invalid(format!("wall {raw} is listed twice")));
            }
            walls.push((id, e.level));
        }
        let placeholder = ElementTypeId(0);
        let mut template = WallFraming::new(ElementId(0), system, placeholder, placeholder, placeholder);
        if let Some(v) = self.spacing {
            template.spacing = v;
        }
        if let Some(v) = self.double_top {
            template.double_top = v;
        }
        template.blocking = self.blocking.unwrap_or(false);
        template.check().map_err(EngineError::invalid)?;
        let given = |r: Option<&Ref>, c: Category| r.map(|r| refs::type_of(p, c, Some(r))).transpose();
        let stud_given = given(self.stud_type.as_ref(), Category::Column)?;
        let track_given = given(self.track_type.as_ref(), Category::Beam)?;
        let header_given = given(self.header_type.as_ref(), Category::Beam)?;
        let host_type = p.default_type(Category::WallFraming);
        let rows = framing_rows(system);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            template.stud_type = type_or_row(s, stud_given, Category::Column, rows.stud)?;
            // Tracks and plates lie flat: the section turned so its depth runs across the wall.
            template.track_type = type_or_turned_row(s, track_given, Category::Beam, rows.track, 90.0)?;
            template.header_type = type_or_row(s, header_given, Category::Beam, rows.header)?;
            template.check_types(s.project()).map_err(EngineError::invalid)?;
            for &(wall, _) in &walls {
                WallFraming { wall, ..template.clone() }.check_size(s.project()).map_err(EngineError::invalid)?;
            }
            let made = edit(s, t!("tx-wall-framing"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_wall_framing_type(id))
                    }
                };
                Ok(walls
                    .iter()
                    .map(|&(wall, level)| {
                        let kind = ElementKind::WallFraming(Box::new(WallFraming { wall, ..template.clone() }));
                        (add_element(e, level, type_id, kind), wall)
                    })
                    .collect::<Vec<_>>())
            })?;
            let p = s.project();
            let framings: Vec<Value> = made
                .iter()
                .map(|&(id, wall)| {
                    let members: Vec<u64> = p.host_members_of(id).iter().map(|m| m.0).collect();
                    json!({ "id": id.0, "wall": wall.0, "members": members })
                })
                .collect();
            Ok(json!({ "framings": framings }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-wall-framing"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a framing: its member ids by role
/// kind, how many roles are detached, its wall's length and whether the
/// layout would pass the member limit, so it has no members (`at_limit`).
pub(crate) fn derived(p: &cc_core::Project, f: &WallFraming, out: &mut serde_json::Map<String, Value>) {
    let ids =
        |k: FramingRoleKind| -> Vec<u64> { f.members.iter().filter(|m| m.role.kind == k).map(|m| m.id.0).collect() };
    out.insert(
        "members".into(),
        json!({
            "studs": ids(FramingRoleKind::Stud),
            "king_studs": ids(FramingRoleKind::KingStud),
            "jack_studs": ids(FramingRoleKind::JackStud),
            "cripples": ids(FramingRoleKind::Cripple),
            "bottom_tracks": ids(FramingRoleKind::BottomTrack),
            "top_tracks": ids(FramingRoleKind::TopTrack),
            "headers": ids(FramingRoleKind::Header),
            "sills": ids(FramingRoleKind::Sill),
            "blocking": ids(FramingRoleKind::Blocking),
        }),
    );
    out.insert("detached".into(), json!(f.detached.len()));
    out.insert("length".into(), json!(f.wall_length(p)));
    out.insert("at_limit".into(), json!(f.check_size(p).is_err()));
}
