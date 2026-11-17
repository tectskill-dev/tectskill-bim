//! Scripts (macros): a list of commands with their arguments, as a JSON
//! array of `{"command": ..., "args": {...}}` objects or one such object per
//! line (`//` lines are comments). `ccad script` runs them, the app records
//! and plays them as macros.
//!
//! Arguments may hold two kinds of placeholders, resolved just before their
//! step runs: `{"$step": n}` (or `{"$step": n, "path": "/created/0"}`) is a
//! value of the result of the earlier step `n` (counted from 1; `path` is a
//! JSON pointer, `/id` by default), so a step can use what an earlier one
//! created; `{"$level": "active"}` is the active level (see
//! [`Session::set_active_level`]; without one, the lowest level). Recording
//! writes both (see [`link`] and [`mark_level`]).

use crate::error::{EngineError, Result};
use crate::session::Session;
use cc_core::LevelId;
use serde_json::{Map, Value, json};

/// Key of a reference to an earlier step's result.
pub const STEP_KEY: &str = "$step";
/// Key of the active level placeholder (its value is `"active"`).
pub const LEVEL_KEY: &str = "$level";
/// Deepest nesting of arguments and results walked.
const MAX_DEPTH: usize = 64;
/// Most values of one result searched for a created id.
const MAX_SEARCH: usize = 10_000;
/// Keys of integer arguments that are counts, indices or scales, never ids.
const NOT_IDS: &[&str] = &[
    "index",
    "indices",
    "edge",
    "remove",
    "count",
    "ports",
    "scale",
    "leaves",
    "panels",
    "side",
    "sides",
    "risers",
    "split",
    "winders",
    "number_start",
    "major_every",
    "dropped",
];

/// One step of a script.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub command: String,
    pub args: Value,
}

impl Step {
    pub fn to_json(&self) -> Value {
        json!({ "command": self.command, "args": self.args })
    }
}

/// Read a script.
pub fn parse(text: &str) -> Result<Vec<Step>> {
    let items: Vec<Value> = if text.trim_start().starts_with('[') {
        serde_json::from_str(text).map_err(|e| EngineError::invalid(format!("invalid script: {e}")))?
    } else {
        text.lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
            .map(|(i, l)| {
                serde_json::from_str(l)
                    .map_err(|e| EngineError::invalid(format!("invalid script, line {}: {e}", i + 1)))
            })
            .collect::<Result<_>>()?
    };
    items
        .into_iter()
        .map(|item| {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| EngineError::invalid("each step needs a \"command\""))?;
            Ok(Step { command: command.to_string(), args: item.get("args").cloned().unwrap_or(json!({})) })
        })
        .collect()
}

/// A script with one step per line (the format macros are saved in).
pub fn to_text(steps: &[Step]) -> String {
    steps.iter().map(|s| format!("{}\n", s.to_json())).collect()
}

/// A failed step: its index (from 0) and the error.
#[derive(Debug, Clone, PartialEq)]
pub struct StepError {
    pub index: usize,
    pub command: String,
    pub error: EngineError,
}

/// Run the steps in order, stopping at the first error; returns each
/// completed step's result. Placeholders in the arguments are resolved first
/// (see the module documentation).
pub fn run(session: &mut Session, steps: &[Step]) -> std::result::Result<Vec<Value>, (Vec<Value>, StepError)> {
    let mut results = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let args = match active_level(session, &step.args).and_then(|l| resolve(&step.args, &results, l, 0)) {
            Ok(args) => args,
            Err(error) => return Err((results, StepError { index, command: step.command.clone(), error })),
        };
        match session.execute(&step.command, args) {
            Ok(out) => results.push(out),
            Err(error) => return Err((results, StepError { index, command: step.command.clone(), error })),
        }
    }
    Ok(results)
}

/// What a recorded step made: the ids allocated while it ran (ids are shared
/// by every kind of object, so none of them names an older object) and its
/// result.
#[derive(Debug, Clone)]
pub(crate) struct Made {
    pub ids: std::ops::Range<u64>,
    pub result: Value,
}

/// The level `{"$level": "active"}` stands for, when the arguments hold one:
/// the session's active level, else the lowest level.
fn active_level(session: &Session, args: &Value) -> Result<Option<LevelId>> {
    if !has_level_placeholder(args, 0) {
        return Ok(None);
    }
    match session.active_level() {
        Some(level) => Ok(Some(level)),
        None => crate::refs::level_or_first(session.project(), None).map(Some),
    }
}

fn has_level_placeholder(v: &Value, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match v {
        Value::Object(m) => m.contains_key(LEVEL_KEY) || m.values().any(|v| has_level_placeholder(v, depth + 1)),
        Value::Array(a) => a.iter().any(|v| has_level_placeholder(v, depth + 1)),
        _ => false,
    }
}

/// `args` with its placeholders replaced by the values they stand for.
fn resolve(args: &Value, results: &[Value], level: Option<LevelId>, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(EngineError::invalid("script arguments are nested too deeply"));
    }
    match args {
        Value::Object(m) if m.contains_key(STEP_KEY) => step_value(m, results),
        Value::Object(m) if m.contains_key(LEVEL_KEY) => match (m.get(LEVEL_KEY).and_then(Value::as_str), level) {
            (Some("active"), Some(level)) => Ok(json!(level.0)),
            _ => Err(EngineError::invalid(format!("\"{LEVEL_KEY}\" must be \"active\""))),
        },
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| Ok((k.clone(), resolve(v, results, level, depth + 1)?)))
            .collect::<Result<_>>()
            .map(Value::Object),
        Value::Array(a) => {
            a.iter().map(|v| resolve(v, results, level, depth + 1)).collect::<Result<_>>().map(Value::Array)
        }
        v => Ok(v.clone()),
    }
}

/// The value `{"$step": n, "path": …}` refers to.
fn step_value(m: &Map<String, Value>, results: &[Value]) -> Result<Value> {
    let n = m.get(STEP_KEY).and_then(Value::as_u64).unwrap_or(0);
    let step = usize::try_from(n)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| results.get(i))
        .ok_or_else(|| EngineError::invalid(format!("\"{STEP_KEY}\": {n} is not an earlier step")))?;
    let path = match m.get("path") {
        None => "/id",
        Some(Value::String(p)) => p.as_str(),
        Some(_) => return Err(EngineError::invalid("\"path\" must be a JSON pointer such as \"/id\"")),
    };
    step.pointer(path)
        .filter(|v| !v.is_null())
        .cloned()
        .ok_or_else(|| EngineError::invalid(format!("step {n} has no result at \"{path}\"")))
}

/// Replace the ids that earlier steps created by references to their results
/// (`made[i]` is what `steps[i]` made). Integers at keys of counts and
/// indices, and ids no earlier result holds, stay as they are.
pub(crate) fn link(steps: &mut [Step], made: &[Option<Made>]) {
    for (i, step) in steps.iter_mut().enumerate() {
        let earlier = made.get(..i).unwrap_or_default();
        link_value(&mut step.args, None, earlier, 0);
    }
}

fn link_value(v: &mut Value, key: Option<&str>, earlier: &[Option<Made>], depth: usize) {
    if depth > MAX_DEPTH || key.is_some_and(|k| NOT_IDS.contains(&k)) {
        return;
    }
    match v {
        Value::Object(m) if m.contains_key(STEP_KEY) || m.contains_key(LEVEL_KEY) => {}
        Value::Object(m) => {
            for (k, v) in m.iter_mut() {
                link_value(v, Some(k), earlier, depth + 1);
            }
        }
        Value::Array(a) => {
            for v in a {
                link_value(v, key, earlier, depth + 1);
            }
        }
        Value::Number(n) => {
            let Some(id) = n.as_u64() else { return };
            // The latest step that made the id and shows it in its result.
            let found = earlier.iter().enumerate().rev().find_map(|(j, made)| {
                let made = made.as_ref().filter(|m| m.ids.contains(&id))?;
                Some((j, find_id(&made.result, id)?))
            });
            if let Some((j, path)) = found {
                *v = if path == "/id" { json!({ STEP_KEY: j + 1 }) } else { json!({ STEP_KEY: j + 1, "path": path }) };
            }
        }
        _ => {}
    }
}

/// The JSON pointer of the first place `result` holds `id` (`/id` first).
fn find_id(result: &Value, id: u64) -> Option<String> {
    if result.get("id").and_then(Value::as_u64) == Some(id) {
        return Some("/id".into());
    }
    let mut budget = MAX_SEARCH;
    let mut path = String::new();
    find_in(result, id, &mut path, &mut budget, 0).then_some(path)
}

fn find_in(v: &Value, id: u64, path: &mut String, budget: &mut usize, depth: usize) -> bool {
    if depth > MAX_DEPTH || *budget == 0 {
        return false;
    }
    *budget -= 1;
    let len = path.len();
    let found = match v {
        Value::Number(n) => n.as_u64() == Some(id),
        Value::Object(m) => m.iter().any(|(k, v)| {
            path.truncate(len);
            path.push('/');
            path.push_str(&k.replace('~', "~0").replace('/', "~1"));
            find_in(v, id, path, budget, depth + 1)
        }),
        Value::Array(a) => a.iter().enumerate().any(|(i, v)| {
            path.truncate(len);
            path.push('/');
            path.push_str(&i.to_string());
            find_in(v, id, path, budget, depth + 1)
        }),
        _ => false,
    };
    if !found {
        path.truncate(len);
    }
    found
}

/// Replace the references to `level` (by id or name) at `level` keys by the
/// active level placeholder, so the step plays on the level active then.
pub(crate) fn mark_level(v: &mut Value, level: LevelId, name: &str, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match v {
        Value::Object(m) if m.contains_key(STEP_KEY) || m.contains_key(LEVEL_KEY) => {}
        Value::Object(m) => {
            for (k, v) in m.iter_mut() {
                let is_level = match &*v {
                    Value::Number(n) => n.as_u64() == Some(level.0),
                    Value::String(s) => s.trim().eq_ignore_ascii_case(name.trim()),
                    _ => false,
                };
                if k == "level" && is_level {
                    *v = json!({ LEVEL_KEY: "active" });
                } else {
                    mark_level(v, level, name, depth + 1);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                mark_level(v, level, name, depth + 1);
            }
        }
        _ => {}
    }
}
