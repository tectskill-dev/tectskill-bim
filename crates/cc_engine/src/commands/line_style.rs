//! Line styles: the built-in styles and the project's own (see
//! [`cc_core::detail`]), used by detail lines and region boundaries. The
//! project's styles are project settings: `line_style_create`,
//! `line_style_set` and `line_style_delete` change them in one undo step
//! each; deleting a style moves what uses it to another style.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::detail::{DetailStyle, LineStyleDef, LineStyleRef, MAX_LINE_STYLES, MAX_WEIGHT_MM, MIN_WEIGHT_MM};
use cc_core::{ElementKind, LinePattern, LineStyleId, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Longest style name accepted.
const MAX_NAME: usize = 100;

/// A dash pattern's code: `solid`, `dash`, `short_dash`, `dot` or `dash_dot`.
pub fn pattern_code(p: LinePattern) -> &'static str {
    match p {
        LinePattern::Solid => "solid",
        LinePattern::Dash => "dash",
        LinePattern::ShortDash => "short_dash",
        LinePattern::Dot => "dot",
        LinePattern::DashDot => "dash_dot",
    }
}

fn pattern_arg(code: &str) -> Result<LinePattern> {
    LinePattern::ALL.into_iter().find(|p| pattern_code(*p).eq_ignore_ascii_case(code.trim())).ok_or_else(|| {
        let codes: Vec<&str> = LinePattern::ALL.iter().map(|p| pattern_code(*p)).collect();
        EngineError::invalid(format!("unknown line pattern '{code}' ({})", codes.join(", ")))
    })
}

fn weight_arg(w: f64) -> Result<f64> {
    if w.is_finite() && (MIN_WEIGHT_MM..=MAX_WEIGHT_MM).contains(&w) {
        Ok(w)
    } else {
        Err(EngineError::invalid(format!("the weight must be between {MIN_WEIGHT_MM} and {MAX_WEIGHT_MM} mm")))
    }
}

/// A free name for a style: not empty, not a built-in code, not another style's.
fn style_name(p: &Project, name: &str, current: Option<LineStyleId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(EngineError::invalid(format!("a line style needs a name of 1 to {MAX_NAME} characters")));
    }
    if DetailStyle::from_code(name).is_some() {
        return Err(EngineError::invalid(format!("'{name}' is the name of a built-in line style")));
    }
    if p.line_styles.values().any(|s| Some(s.id) != current && s.name.trim().eq_ignore_ascii_case(name)) {
        return Err(EngineError::invalid(format!("a line style named '{name}' already exists")));
    }
    Ok(name.to_string())
}

fn style_json(p: &Project, s: &LineStyleDef) -> Value {
    json!({
        "id": s.id.0,
        "name": s.name,
        "weight": s.weight_mm,
        "color": s.color,
        "pattern": pattern_code(s.pattern),
        "built_in": false,
        "used_by": p.line_style_users(s.id).len(),
    })
}

/// The line styles detail lines and region boundaries can use: the
/// built-in ones (by code) and the project's (by name), each with its
/// weight on paper (mm), color, dash pattern and how many elements use it.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LineStylesList {}

impl Command for LineStylesList {
    const NAME: &'static str = "line_styles_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let used = |r: LineStyleRef| {
            p.elements
                .values()
                .filter(|e| match &e.kind {
                    ElementKind::DetailLine(l) => l.style == r,
                    ElementKind::FilledRegion(g) | ElementKind::MaskingRegion(g) => g.boundary == Some(r),
                    _ => false,
                })
                .count()
        };
        let built_in = DetailStyle::ALL.iter().map(|b| {
            json!({
                "name": b.code(),
                "weight": b.weight_mm(),
                "color": [0, 0, 0],
                "pattern": pattern_code(b.pattern()),
                "built_in": true,
                "used_by": used(LineStyleRef::BuiltIn(*b)),
            })
        });
        let own = p.line_styles.values().map(|st| style_json(p, st));
        Ok(Value::Array(built_in.chain(own).collect()))
    }
}

/// Create a project line style for detail lines and region boundaries:
/// a unique name (not a built-in code), a weight on paper, a color and a
/// dash pattern. Commands then name it like a built-in style.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LineStyleCreate {
    /// Name (unique, not a built-in style's code).
    pub name: String,
    /// Width on paper, millimeters (0.05 to 5; default 0.25).
    #[serde(default)]
    pub weight: Option<f64>,
    /// Color `[r, g, b]`, 0 to 255 (default black).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Dash pattern: solid (default), dash, short_dash, dot or dash_dot.
    #[serde(default)]
    pub pattern: Option<String>,
}

impl Command for LineStyleCreate {
    const NAME: &'static str = "line_style_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        if p.line_styles.len() >= MAX_LINE_STYLES {
            return Err(EngineError::rejected(format!("a project has at most {MAX_LINE_STYLES} line styles")));
        }
        let name = style_name(p, &self.name, None)?;
        let weight = weight_arg(self.weight.unwrap_or(DetailStyle::Medium.weight_mm()))?;
        let pattern = self.pattern.as_deref().map(pattern_arg).transpose()?.unwrap_or_default();
        let color = self.color.unwrap_or([0, 0, 0]);
        let id = edit(s, t!("tx-line-style-create"), |e| {
            let id = LineStyleId(e.alloc_id());
            e.line_styles_mut().insert(id, LineStyleDef { id, name, weight_mm: weight, color, pattern });
            Ok(id)
        })?;
        let p = s.project();
        p.line_style(id).map(|st| style_json(p, st)).ok_or_else(|| EngineError::not_found("line style"))
    }
}

/// Change a project line style: name, weight, color or pattern. The lines
/// and boundaries drawn with it follow.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LineStyleSet {
    /// Line style id or name.
    pub style: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// Width on paper, millimeters (0.05 to 5).
    #[serde(default)]
    pub weight: Option<f64>,
    /// Color `[r, g, b]`.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Dash pattern: solid, dash, short_dash, dot or dash_dot.
    #[serde(default)]
    pub pattern: Option<String>,
}

impl Command for LineStyleSet {
    const NAME: &'static str = "line_style_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::line_style(p, &self.style)?;
        let mut style = p.line_style(id).cloned().ok_or_else(|| EngineError::not_found("line style"))?;
        if let Some(n) = &self.name {
            style.name = style_name(p, n, Some(id))?;
        }
        if let Some(w) = self.weight {
            style.weight_mm = weight_arg(w)?;
        }
        if let Some(c) = self.color {
            style.color = c;
        }
        if let Some(code) = &self.pattern {
            style.pattern = pattern_arg(code)?;
        }
        edit(s, t!("tx-line-style-modify"), |e| {
            e.line_styles_mut().insert(id, style);
            Ok(())
        })?;
        let p = s.project();
        p.line_style(id).map(|st| style_json(p, st)).ok_or_else(|| EngineError::not_found("line style"))
    }
}

/// Delete a project line style. The detail lines and region boundaries
/// drawn with it move to `replace_with` (a built-in code or another
/// project style's name; default `thin`) in the same undo step.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LineStyleDelete {
    /// Line style id or name.
    pub style: Ref,
    /// The style that takes its place where it is used (default `thin`).
    #[serde(default)]
    pub replace_with: Option<String>,
}

impl Command for LineStyleDelete {
    const NAME: &'static str = "line_style_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::line_style(p, &self.style)?;
        let with = match &self.replace_with {
            Some(code) => super::detail::style_arg(p, code)?,
            None => LineStyleRef::default(),
        };
        if with == LineStyleRef::Project(id) {
            return Err(EngineError::invalid("a style cannot replace itself"));
        }
        let users = p.line_style_users(id);
        let old = Some(LineStyleRef::Project(id));
        edit(s, t!("tx-line-style-delete"), |e| {
            for u in &users {
                match e.element_mut(*u).map(|x| &mut x.kind) {
                    Some(ElementKind::DetailLine(l)) => l.style = with,
                    Some(ElementKind::FilledRegion(r) | ElementKind::MaskingRegion(r)) if r.boundary == old => {
                        r.boundary = Some(with)
                    }
                    _ => {}
                }
            }
            e.line_styles_mut().remove(&id);
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0, "moved": users.len() }))
    }
}
