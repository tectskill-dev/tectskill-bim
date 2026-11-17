//! Fill patterns (hatches): the built-in patterns and the project's own (see
//! [`cc_core::fill_pattern`]), which materials (cut and surface patterns),
//! filled region types and view overrides use. The project's patterns are
//! project settings: each command changes them in one undo step. They come
//! from `.pat` files (`fill_pattern_import`, see [`cc_core::pat`]) or are
//! drawn up line family by line family (`fill_pattern_create`).
//!
//! Where a command takes a pattern by code it accepts a built-in code
//! (`diagonal`, `brick`…) or a project pattern's name; in model values
//! (`material_set`, view overrides, `type_set`) a project pattern is written
//! `{"Custom": <id>}`, as `fill_patterns_list` shows in `value`.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::fill_pattern::{
    self, FillPattern, FillPatternKind, MAX_FILL_PATTERNS, PatternLine, PatternSlot, PatternUsage,
};
use cc_core::pat::{self, PatOptions, PatUnits};
use cc_core::{FillPatternId, Pattern, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

/// A built-in pattern's code: `none`, `solid`, `diagonal`, `diagonal_back`,
/// `crosshatch`, `horizontal`, `vertical`, `grid`, `brick`, `concrete`,
/// `sand`, `insulation` or `earth` (`custom` for a project pattern).
pub fn builtin_code(p: Pattern) -> String {
    if matches!(p, Pattern::Custom(_)) {
        return "custom".into();
    }
    let mut out = String::new();
    for (i, c) in format!("{p:?}").chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A pattern's code: a built-in code, or a project pattern's name.
pub fn pattern_code(p: &Project, pattern: Pattern) -> String {
    match pattern {
        Pattern::Custom(id) => p.fill_pattern(id).map_or_else(|| "none".into(), |f| f.name.clone()),
        b => builtin_code(b),
    }
}

/// A pattern by code: a built-in code or a project pattern's name.
pub fn pattern_arg(p: &Project, code: &str) -> Result<Pattern> {
    if let Some(b) = Pattern::ALL.into_iter().find(|b| builtin_code(*b).eq_ignore_ascii_case(code.trim())) {
        return Ok(b);
    }
    if let Some(f) = p.fill_pattern_named(code) {
        return Ok(Pattern::Custom(f.id));
    }
    let codes: Vec<String> = Pattern::ALL.iter().map(|b| builtin_code(*b)).collect();
    Err(EngineError::invalid(format!(
        "unknown fill pattern '{code}' (a project pattern's name, or {})",
        codes.join(", ")
    )))
}

fn kind_arg(code: &str) -> Result<FillPatternKind> {
    FillPatternKind::from_code(code)
        .ok_or_else(|| EngineError::invalid(format!("unknown pattern kind '{code}' (drafting or model)")))
}

/// A free name: not empty, not a built-in code, not another pattern's.
fn pattern_name(p: &Project, name: &str, current: Option<FillPatternId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > fill_pattern::MAX_NAME {
        return Err(EngineError::invalid(format!(
            "a fill pattern needs a name of 1 to {} characters",
            fill_pattern::MAX_NAME
        )));
    }
    if Pattern::ALL.iter().any(|b| builtin_code(*b).eq_ignore_ascii_case(name)) {
        return Err(EngineError::invalid(format!("'{name}' is the name of a built-in pattern")));
    }
    if p.fill_patterns.values().any(|f| Some(f.id) != current && f.name.trim().eq_ignore_ascii_case(name)) {
        return Err(EngineError::invalid(format!("a fill pattern named '{name}' already exists")));
    }
    Ok(name.to_string())
}

/// `base`, or `base 2`, `base 3`… when taken.
fn free_name(p: &Project, base: &str, also_taken: &[String]) -> String {
    let base: String = base.trim().chars().take(fill_pattern::MAX_NAME - 4).collect();
    let taken = |n: &str| {
        pattern_name(p, n, None).is_err() || also_taken.iter().any(|t| t.trim().eq_ignore_ascii_case(n.trim()))
    };
    let mut name = base.clone();
    let mut k = 2;
    while taken(&name) && k < 10_000 {
        name = format!("{base} {k}");
        k += 1;
    }
    name
}

/// One line family of a pattern, lengths in millimeters (on paper for a
/// drafting pattern, in the world for a model pattern).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LineArg {
    /// Direction of the lines, degrees counterclockwise from +x.
    pub angle: f64,
    /// A point the first line passes through, `[x, y]` mm (default [0, 0]).
    #[serde(default)]
    pub origin: Option<[f64; 2]>,
    /// From one line to the next, `[along, across]` mm: the shift of the
    /// dashes along the line and the spacing between lines.
    pub offset: [f64; 2],
    /// Dash lengths in turn, mm: positive drawn, negative a gap, 0 a dot
    /// (none: continuous lines).
    #[serde(default)]
    pub dashes: Option<Vec<f64>>,
}

impl LineArg {
    fn line(&self) -> PatternLine {
        PatternLine {
            angle: self.angle,
            origin: self.origin.unwrap_or_default(),
            offset: self.offset,
            dashes: self.dashes.clone().unwrap_or_default(),
        }
    }
}

fn line_json(l: &PatternLine) -> Value {
    json!({ "angle": l.angle, "origin": l.origin, "offset": l.offset, "dashes": l.dashes })
}

fn usage_json(p: &Project, u: &PatternUsage) -> Value {
    json!({
        "materials": u.materials,
        "types": u.types.iter().map(|t| json!({"id": t.0, "name": p.element_type(*t).map(|x| x.name.as_str())})).collect::<Vec<_>>(),
        "views": u.views.iter().map(|v| json!({"id": v.0, "name": p.view(*v).map(|x| x.name.as_str())})).collect::<Vec<_>>(),
        "templates": u.templates.iter().map(|t| t.0).collect::<Vec<_>>(),
        "phase_graphics": u.phase_graphics,
    })
}

fn pattern_json(p: &Project, f: &FillPattern) -> Value {
    json!({
        "id": f.id.0,
        "name": f.name,
        "description": f.description,
        "kind": f.kind.code(),
        "scale": f.scale,
        "lines": f.lines.iter().map(line_json).collect::<Vec<_>>(),
        "built_in": false,
        "used_by": p.pattern_usage(Pattern::Custom(f.id)).count(),
        "value": Pattern::Custom(f.id),
    })
}

fn pattern_out(s: &Session, id: FillPatternId) -> Result<Value> {
    let p = s.project();
    p.fill_pattern(id).map(|f| pattern_json(p, f)).ok_or_else(|| EngineError::not_found("fill pattern"))
}

/// The fill patterns materials, filled regions and view overrides can use:
/// the built-in ones (by code) and the project's (by name, with their kind,
/// scale and line families), each with how many materials, types, views,
/// templates and phase graphics use it, and its `value` in model values.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternsList {}

impl Command for FillPatternsList {
    const NAME: &'static str = "fill_patterns_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let built_in = Pattern::ALL.iter().map(|b| {
            json!({
                "name": builtin_code(*b),
                "built_in": true,
                "used_by": p.pattern_usage(*b).count(),
                "value": b,
            })
        });
        let own = p.fill_patterns.values().map(|f| pattern_json(p, f));
        Ok(Value::Array(built_in.chain(own).collect()))
    }
}

/// One project fill pattern: its definition and everything that uses it
/// (materials, types, views, templates, phase graphics).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternGet {
    /// Fill pattern id or name.
    pub pattern: Ref,
}

impl Command for FillPatternGet {
    const NAME: &'static str = "fill_pattern_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::fill_pattern(p, &self.pattern)?;
        let mut out = pattern_out(s, id)?;
        if let Some(o) = out.as_object_mut() {
            o.insert("used_in".into(), usage_json(p, &p.pattern_usage(Pattern::Custom(id))));
        }
        Ok(out)
    }
}

/// Create a project fill pattern from line families: a unique name (not a
/// built-in code), drafting (sized on paper, the default) or model (sized
/// in the world, scaling with the view), a scale and the families.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternCreate {
    /// Name (unique, not a built-in pattern's code).
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// `drafting` (default: millimeters on paper) or `model` (millimeters in the world).
    #[serde(default)]
    pub kind: Option<String>,
    /// Multiplies every length (0.01 to 100; default 1).
    #[serde(default)]
    pub scale: Option<f64>,
    /// The line families (1 to 128).
    pub lines: Vec<LineArg>,
}

impl Command for FillPatternCreate {
    const NAME: &'static str = "fill_pattern_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        if p.fill_patterns.len() >= MAX_FILL_PATTERNS {
            return Err(EngineError::rejected(format!("a project has at most {MAX_FILL_PATTERNS} fill patterns")));
        }
        let mut pattern = FillPattern {
            id: FillPatternId(0),
            name: pattern_name(p, &self.name, None)?,
            description: self.description.unwrap_or_default().trim().to_string(),
            kind: self.kind.as_deref().map(kind_arg).transpose()?.unwrap_or_default(),
            scale: self.scale.unwrap_or(1.0),
            lines: self.lines.iter().map(LineArg::line).collect(),
        };
        if let Some(problem) = pattern.problem() {
            return Err(EngineError::invalid(problem));
        }
        let id = edit(s, t!("tx-fill-pattern-create"), |e| {
            let id = FillPatternId(e.alloc_id());
            pattern.id = id;
            e.fill_patterns_mut().insert(id, pattern);
            Ok(id)
        })?;
        pattern_out(s, id)
    }
}

/// Change a project fill pattern: name, description, kind, scale or line
/// families. What is drawn with it follows.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternSet {
    /// Fill pattern id or name.
    pub pattern: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New description.
    #[serde(default)]
    pub description: Option<String>,
    /// `drafting` or `model`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Multiplies every length (0.01 to 100).
    #[serde(default)]
    pub scale: Option<f64>,
    /// New line families, in place of all of them.
    #[serde(default)]
    pub lines: Option<Vec<LineArg>>,
}

impl Command for FillPatternSet {
    const NAME: &'static str = "fill_pattern_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::fill_pattern(p, &self.pattern)?;
        let mut pattern = p.fill_pattern(id).cloned().ok_or_else(|| EngineError::not_found("fill pattern"))?;
        if let Some(n) = &self.name {
            pattern.name = pattern_name(p, n, Some(id))?;
        }
        if let Some(d) = &self.description {
            pattern.description = d.trim().to_string();
        }
        if let Some(k) = &self.kind {
            pattern.kind = kind_arg(k)?;
        }
        if let Some(scale) = self.scale {
            pattern.scale = scale;
        }
        if let Some(lines) = &self.lines {
            pattern.lines = lines.iter().map(LineArg::line).collect();
        }
        if let Some(problem) = pattern.problem() {
            return Err(EngineError::invalid(problem));
        }
        edit(s, t!("tx-fill-pattern-modify"), |e| {
            e.fill_patterns_mut().insert(id, pattern);
            Ok(())
        })?;
        pattern_out(s, id)
    }
}

/// Copy a project fill pattern under a new name (default: the name with a
/// number).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternDuplicate {
    /// Fill pattern id or name.
    pub pattern: Ref,
    /// Name of the copy.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for FillPatternDuplicate {
    const NAME: &'static str = "fill_pattern_duplicate";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        if p.fill_patterns.len() >= MAX_FILL_PATTERNS {
            return Err(EngineError::rejected(format!("a project has at most {MAX_FILL_PATTERNS} fill patterns")));
        }
        let id = refs::fill_pattern(p, &self.pattern)?;
        let mut copy = p.fill_pattern(id).cloned().ok_or_else(|| EngineError::not_found("fill pattern"))?;
        copy.name = match &self.name {
            Some(n) => pattern_name(p, n, None)?,
            None => free_name(p, &copy.name, &[]),
        };
        let new = edit(s, t!("tx-fill-pattern-create"), |e| {
            let new = FillPatternId(e.alloc_id());
            copy.id = new;
            e.fill_patterns_mut().insert(new, copy);
            Ok(new)
        })?;
        pattern_out(s, new)
    }
}

/// Put `with` wherever the project uses `id`, through the edit (one undo step).
fn replace_uses(e: &mut cc_core::EditCtx, usage: &PatternUsage, id: FillPatternId, with: Pattern) {
    let old = Pattern::Custom(id);
    let mut swap = |mut slot: PatternSlot| {
        if slot.get() == Some(old) {
            slot.set(with);
        }
    };
    if !usage.materials.is_empty() {
        for m in e.materials_mut().iter_mut() {
            fill_pattern::visit_material(m, &mut swap);
        }
    }
    for t in &usage.types {
        if let Some(ty) = e.type_mut(*t) {
            fill_pattern::visit_type(&mut ty.data, &mut swap);
        }
    }
    for v in &usage.views {
        if let Some(view) = e.view_mut(*v) {
            fill_pattern::visit_view_graphics(&mut view.graphics, &mut swap);
        }
    }
    if !usage.templates.is_empty() {
        for t in e.templates_mut().values_mut() {
            fill_pattern::visit_view_graphics(&mut t.graphics, &mut swap);
        }
    }
    if usage.phase_graphics {
        fill_pattern::visit_phase_graphics(e.phase_graphics_mut(), &mut swap);
    }
}

/// Delete a project fill pattern. Refused while something uses it, unless
/// `replace_with` names the pattern (a built-in code or another project
/// pattern) that takes its place there, in the same undo step.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternDelete {
    /// Fill pattern id or name.
    pub pattern: Ref,
    /// The pattern that takes its place where it is used.
    #[serde(default)]
    pub replace_with: Option<String>,
}

impl Command for FillPatternDelete {
    const NAME: &'static str = "fill_pattern_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::fill_pattern(p, &self.pattern)?;
        let usage = p.pattern_usage(Pattern::Custom(id));
        let with = self.replace_with.as_deref().map(|c| pattern_arg(p, c)).transpose()?;
        if with == Some(Pattern::Custom(id)) {
            return Err(EngineError::invalid("a pattern cannot replace itself"));
        }
        if !usage.is_empty() && with.is_none() {
            return Err(EngineError::rejected(format!(
                "the pattern is used by {} materials, types or views; give replace_with",
                usage.count()
            )));
        }
        edit(s, t!("tx-fill-pattern-delete"), |e| {
            if let Some(with) = with {
                replace_uses(e, &usage, id, with);
            }
            e.fill_patterns_mut().remove(&id);
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0, "moved": usage.count() }))
    }
}

/// What to do with an imported pattern whose name the project already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnClash {
    /// Import it under the name with a number.
    #[default]
    Rename,
    /// Put its lines in place of the project pattern's (what uses it follows).
    Replace,
    /// Leave it out.
    Skip,
}

/// Import fill patterns from a `.pat` file (the plain-text hatch pattern
/// format CAD programs share: `*NAME, description` headers, then lines of
/// `angle, x-origin, y-origin, delta-x, delta-y [, dashes…]`). All the
/// file's patterns or those named; each pattern is drafting or model as its
/// `;%TYPE=MODEL` marker says unless `kind` is given, lengths in the
/// file's `;%UNITS=` (else millimeters) unless `units` is given. Patterns
/// with a problem are left out and listed in `errors` with their line.
/// With `preview`, nothing changes: the result lists what the file holds.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternImport {
    /// Path of the .pat file (or give `text`).
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// The file's text (in place of `path`).
    #[serde(default)]
    pub text: Option<String>,
    /// Names of the patterns to import (default: all).
    #[serde(default)]
    pub names: Option<Vec<String>>,
    /// A name the project already has: `rename` (default), `replace` or `skip`.
    #[serde(default)]
    pub on_clash: OnClash,
    /// `drafting` or `model` for every pattern, in place of the file's markers.
    #[serde(default)]
    pub kind: Option<String>,
    /// Units of the file's lengths: `auto` (default: its `;%UNITS=` marker,
    /// else millimeters), `mm` or `inch`.
    #[serde(default)]
    pub units: Option<String>,
    /// The scale given to every imported pattern (0.01 to 100; default 1).
    #[serde(default)]
    pub scale: Option<f64>,
    /// Only read the file and list its patterns.
    #[serde(default)]
    pub preview: bool,
}

impl Command for FillPatternImport {
    const NAME: &'static str = "fill_pattern_import";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        let text = match (&self.path, self.text) {
            (Some(path), None) => {
                s.require_files("path")?;
                let len = std::fs::metadata(path)?.len();
                if len > pat::MAX_PAT_BYTES as u64 {
                    return Err(EngineError::invalid(format!(
                        "the file is larger than {} MB",
                        pat::MAX_PAT_BYTES >> 20
                    )));
                }
                String::from_utf8_lossy(&std::fs::read(path)?).into_owned()
            }
            (None, Some(text)) => text,
            _ => return Err(EngineError::invalid("give either path or text")),
        };
        let units = match self.units.as_deref().map(|u| u.trim().to_ascii_lowercase()) {
            None => PatUnits::Auto,
            Some(u) if u == "auto" => PatUnits::Auto,
            Some(u) if u == "mm" || u == "millimeters" => PatUnits::Millimeters,
            Some(u) if u == "inch" || u == "in" || u == "inches" => PatUnits::Inches,
            Some(u) => return Err(EngineError::invalid(format!("unknown units '{u}' (auto, mm or inch)"))),
        };
        let scale = self.scale.unwrap_or(1.0);
        if !(scale.is_finite() && (fill_pattern::MIN_SCALE..=fill_pattern::MAX_SCALE).contains(&scale)) {
            return Err(EngineError::invalid(format!(
                "the scale must be between {} and {}",
                fill_pattern::MIN_SCALE,
                fill_pattern::MAX_SCALE
            )));
        }
        let kind = self.kind.as_deref().map(kind_arg).transpose()?;
        let parsed = pat::parse(&text, &PatOptions { units, kind, scale });
        let errors: Vec<Value> = parsed
            .errors
            .iter()
            .map(|e| json!({ "line": e.line, "pattern": e.pattern, "message": e.message }))
            .collect();
        let mut wanted = parsed.patterns;
        if let Some(names) = &self.names {
            let missing: Vec<&String> =
                names.iter().filter(|n| !wanted.iter().any(|p| p.name.trim().eq_ignore_ascii_case(n.trim()))).collect();
            if !missing.is_empty() {
                let missing: Vec<&str> = missing.iter().map(|n| n.as_str()).collect();
                return Err(EngineError::not_found(format!("the file has no pattern named {}", missing.join(", "))));
            }
            wanted.retain(|p| names.iter().any(|n| p.name.trim().eq_ignore_ascii_case(n.trim())));
        }
        if self.preview {
            let p = s.project();
            let patterns: Vec<Value> = wanted
                .iter()
                .map(|f| {
                    json!({
                        "name": f.name,
                        "description": f.description,
                        "kind": f.kind.code(),
                        "scale": f.scale,
                        "lines": f.lines.iter().map(line_json).collect::<Vec<_>>(),
                        "exists": p.fill_pattern_named(&f.name).is_some(),
                    })
                })
                .collect();
            return Ok(json!({ "patterns": patterns, "errors": errors }));
        }
        // Work out each pattern's fate before the edit.
        let p = s.project();
        let mut new: Vec<FillPattern> = Vec::new();
        let mut replaced: Vec<(FillPatternId, FillPattern)> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for mut f in wanted {
            let clash = p.fill_pattern_named(&f.name).map(|x| x.id);
            let builtin = Pattern::ALL.iter().any(|b| builtin_code(*b).eq_ignore_ascii_case(f.name.trim()));
            let in_file = new.iter().any(|n| n.name.trim().eq_ignore_ascii_case(f.name.trim()));
            match (clash, self.on_clash) {
                (Some(_), OnClash::Skip) => skipped.push(f.name),
                (Some(id), OnClash::Replace) if !in_file => {
                    f.id = id;
                    f.name = p.fill_pattern(id).map_or(f.name.clone(), |x| x.name.clone());
                    replaced.retain(|(r, _)| *r != id);
                    replaced.push((id, f));
                }
                _ if clash.is_some() || builtin || in_file => {
                    let taken: Vec<String> = new.iter().map(|n| n.name.clone()).collect();
                    f.name = free_name(p, &f.name, &taken);
                    new.push(f);
                }
                _ => new.push(f),
            }
        }
        if p.fill_patterns.len() + new.len() > MAX_FILL_PATTERNS {
            return Err(EngineError::rejected(format!("a project has at most {MAX_FILL_PATTERNS} fill patterns")));
        }
        if new.is_empty() && replaced.is_empty() {
            return Ok(json!({ "imported": [], "replaced": [], "skipped": skipped, "errors": errors }));
        }
        let (added, swapped) = edit(s, t!("tx-fill-pattern-import"), |e| {
            let mut added = Vec::new();
            for mut f in new {
                let id = FillPatternId(e.alloc_id());
                f.id = id;
                added.push(json!({ "id": id.0, "name": f.name }));
                e.fill_patterns_mut().insert(id, f);
            }
            let mut swapped = Vec::new();
            for (id, f) in replaced {
                swapped.push(json!({ "id": id.0, "name": f.name }));
                e.fill_patterns_mut().insert(id, f);
            }
            Ok((added, swapped))
        })?;
        Ok(json!({ "imported": added, "replaced": swapped, "skipped": skipped, "errors": errors }))
    }
}

/// Write project fill patterns as `.pat` text (millimeters, each pattern's
/// scale applied, a `;%TYPE=` marker per pattern): to `path` when given;
/// the text is returned either way.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FillPatternExport {
    /// Fill pattern ids or names (default: all the project's).
    #[serde(default)]
    pub patterns: Option<Vec<Ref>>,
    /// Path of the .pat file to write.
    #[serde(default)]
    pub path: Option<PathBuf>,
}

impl Command for FillPatternExport {
    const NAME: &'static str = "fill_pattern_export";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let chosen: Vec<FillPattern> = match &self.patterns {
            Some(refs_) => refs_
                .iter()
                .map(|r| {
                    let id = refs::fill_pattern(p, r)?;
                    p.fill_pattern(id).cloned().ok_or_else(|| EngineError::not_found("fill pattern"))
                })
                .collect::<Result<_>>()?,
            None => p.fill_patterns.values().cloned().collect(),
        };
        let text = pat::write(&chosen);
        if let Some(path) = &self.path {
            std::fs::write(path, &text)?;
        }
        Ok(json!({ "count": chosen.len(), "text": text }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_codes_round_trip() {
        let p = Project::empty();
        for b in Pattern::ALL {
            assert_eq!(pattern_arg(&p, &builtin_code(b)).ok(), Some(b));
        }
        assert_eq!(builtin_code(Pattern::DiagonalBack), "diagonal_back");
        assert!(pattern_arg(&p, "zigzag").is_err());
    }
}
