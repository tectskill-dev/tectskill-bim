//! The ConstructCraft engine: every user-facing operation as a named command
//! that runs on a [`Session`] without a GUI.
//!
//! A command is a struct of typed parameters (deserialized from JSON, with a
//! JSON Schema generated from the same type) and a `run` method. The
//! [`catalog`] lists them; [`Session::execute`] runs one by name. The
//! command line (`ccad`), the MCP server and, area by area, the desktop app
//! all go through this one catalog, so an operation behaves the same
//! wherever it is called from.
//!
//! Lengths are in meters, angles in degrees, points are `[x, y]` in plan.
//! Edits go through `Document::edit`, so each command is one undo step.
//! Levels, types, views and materials can be named by id or by name;
//! elements by id.

mod defaults;
pub mod dimensions;
mod error;
pub mod extension;
pub mod gltf;
pub mod graph;
pub mod i18n;
pub mod import_material;
pub mod obj;
pub mod osm;
pub mod reference;
mod refs;
mod registry;
pub mod script;
mod session;
pub mod skp;

pub mod commands;

/// The fields of an enum value's variant as `*_get` shows them in `params`.
pub fn params_of<T: serde::Serialize>(value: &T) -> serde_json::Value {
    commands::variant(serde_json::to_value(value).unwrap_or_default()).1
}

/// An element's `params` as `element_get` shows them (an imported drawing
/// without the drawing itself, which is not a parameter).
pub fn element_params(kind: &cc_core::ElementKind) -> serde_json::Value {
    commands::cad::params(kind)
}

/// The `params` patch a `*_set` command needs to turn the enum value `old`
/// (a view kind, type data) into `new`. Unlike `params_of(new)`, it also
/// clears the fields `new` leaves out of its JSON form (an option set back
/// to none, a list emptied, a flag turned off), which a merge patch would
/// otherwise keep.
pub fn params_patch<T>(old: &T, new: &T) -> serde_json::Value
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq,
{
    let check = |patch: &serde_json::Value| commands::patch_variant(old, patch).ok().map(|v| v == *new);
    commands::clearing_patch(&params_of(old), &params_of(new), &check)
}

/// The merge patch that turns a whole value `old` (a struct a command takes
/// as a patch, such as a template's `underlay`) into `new`, clearing the
/// fields `new` leaves out as [`params_patch`] does.
pub fn value_patch<T>(old: &T, new: &T) -> serde_json::Value
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq,
{
    let json = |v: &T| serde_json::to_value(v).unwrap_or_default();
    let check = |patch: &serde_json::Value| commands::patch_value(old, patch).ok().map(|v| v == *new);
    commands::clearing_patch(&json(old), &json(new), &check)
}

/// [`params_patch`] for an element's kind, as `element_set` applies it.
pub fn element_params_patch(old: &cc_core::ElementKind, new: &cc_core::ElementKind) -> serde_json::Value {
    let check = |patch: &serde_json::Value| commands::edit::patch_kind(old, patch).ok().map(|v| v == *new);
    commands::clearing_patch(&element_params(old), &element_params(new), &check)
}

pub use defaults::{
    conduit_offset, default_roof_base, default_top, default_wall_face, device_height, drain_slope, next_number,
    pipe_offset, tray_offset, walls_loop, walls_loop_at,
};
pub use error::{EngineError, ErrorCode, Result};
pub use extension::Extension;
pub use refs::{Ref, category_name};
pub use registry::{Command, CommandInfo, CommandKind, catalog, command_kind, describe};
pub use session::{Session, new_project, new_project_with};
