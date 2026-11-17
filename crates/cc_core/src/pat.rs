//! The plain-text `.pat` hatch pattern format many CAD programs read and
//! write, as publicly described:
//!
//! ```text
//! ;%UNITS=MM
//! *NAME, optional description
//! ;%TYPE=MODEL
//! angle, x-origin, y-origin, delta-x, delta-y [, dash1, dash2, ...]
//! ```
//!
//! A header line starting with `*` opens a pattern; each following line is
//! a family of parallel lines through the origin at the angle (degrees),
//! the next line shifted by delta-x along it and delta-y across it, with
//! optional dashes (positive drawn, negative gaps, zero dots). Text after
//! `;` is a comment; the comments `;%UNITS=MM|INCH` and
//! `;%TYPE=MODEL|DRAFTING` are read as markers (a type marker before the
//! first header sets the file's default, one inside a pattern that
//! pattern's kind). Parsing is bounded: file size, line length, patterns,
//! families and dashes.

use crate::FillPatternId;
use crate::fill_pattern::{FillPattern, FillPatternKind, MAX_DASHES, MAX_FAMILIES, MAX_NAME, PatternLine};

/// Largest file read, bytes.
pub const MAX_PAT_BYTES: usize = 8 << 20;
/// Most patterns read from one file.
pub const MAX_PAT_PATTERNS: usize = 5000;
/// Longest line read, characters.
pub const MAX_LINE_CHARS: usize = 4096;
/// Millimeters in an inch.
pub const MM_PER_INCH: f64 = 25.4;

/// The units a file's lengths are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatUnits {
    /// The file's `;%UNITS=` marker, else millimeters.
    #[default]
    Auto,
    Millimeters,
    Inches,
}

/// How a file is read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatOptions {
    pub units: PatUnits,
    /// Every pattern's kind, in place of the file's markers.
    pub kind: Option<FillPatternKind>,
    /// The scale given to every pattern.
    pub scale: f64,
}

impl Default for PatOptions {
    fn default() -> Self {
        Self { units: PatUnits::Auto, kind: None, scale: 1.0 }
    }
}

/// A problem with a line of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct PatError {
    /// Line number, from 1 (0: the whole file).
    pub line: usize,
    /// The pattern being read, if any (it is left out).
    pub pattern: Option<String>,
    pub message: String,
}

/// What a file holds: the patterns read (ids 0, millimeters) and the
/// problems found; a pattern with a problem is left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedPat {
    pub patterns: Vec<FillPattern>,
    pub errors: Vec<PatError>,
}

struct Open {
    pattern: FillPattern,
    failed: bool,
}

/// Read `.pat` text.
pub fn parse(text: &str, options: &PatOptions) -> ParsedPat {
    let mut out = ParsedPat::default();
    if text.len() > MAX_PAT_BYTES {
        out.errors.push(PatError {
            line: 0,
            pattern: None,
            message: format!("the file is larger than {} MB", MAX_PAT_BYTES >> 20),
        });
        return out;
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Units come from the first marker (anywhere), unless chosen.
    let file_units = text.lines().find_map(|l| marker(l, "UNITS")).map(|u| match u.as_str() {
        "INCH" | "INCHES" | "IN" => PatUnits::Inches,
        _ => PatUnits::Millimeters,
    });
    let factor = match (options.units, file_units) {
        (PatUnits::Inches, _) | (PatUnits::Auto, Some(PatUnits::Inches)) => MM_PER_INCH,
        _ => 1.0,
    };
    let scale = if options.scale.is_finite() && options.scale > 0.0 { options.scale } else { 1.0 };
    let mut default_kind = FillPatternKind::Drafting;
    let mut open: Option<Open> = None;
    let finish = |open: Option<Open>, out: &mut ParsedPat| {
        let Some(o) = open else { return };
        if o.failed {
            return;
        }
        if o.pattern.lines.is_empty() {
            out.errors.push(PatError {
                line: 0,
                pattern: Some(o.pattern.name.clone()),
                message: "the pattern has no line families".into(),
            });
            return;
        }
        out.patterns.push(o.pattern);
    };
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        if raw.chars().count() > MAX_LINE_CHARS {
            let name = open.as_ref().map(|o| o.pattern.name.clone());
            out.errors.push(PatError { line: line_no, pattern: name, message: "the line is too long".into() });
            if let Some(o) = &mut open {
                o.failed = true;
            }
            continue;
        }
        if let Some(kind) = marker(raw, "TYPE") {
            let kind = if kind == "MODEL" { FillPatternKind::Model } else { FillPatternKind::Drafting };
            match &mut open {
                Some(o) => o.pattern.kind = options.kind.unwrap_or(kind),
                None => default_kind = kind,
            }
            continue;
        }
        let data = raw.split(';').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        if let Some(header) = data.strip_prefix('*') {
            finish(open.take(), &mut out);
            if out.patterns.len() >= MAX_PAT_PATTERNS {
                out.errors.push(PatError {
                    line: line_no,
                    pattern: None,
                    message: format!("only the first {MAX_PAT_PATTERNS} patterns are read"),
                });
                return out;
            }
            let (name, description) = match header.split_once(',') {
                Some((n, d)) => (n.trim(), d.trim()),
                None => (header.trim(), ""),
            };
            let mut failed = false;
            if name.is_empty() || name.chars().count() > MAX_NAME {
                out.errors.push(PatError {
                    line: line_no,
                    pattern: None,
                    message: format!("a pattern needs a name of 1 to {MAX_NAME} characters"),
                });
                failed = true;
            }
            open = Some(Open {
                pattern: FillPattern {
                    id: FillPatternId(0),
                    name: name.to_string(),
                    description: description.chars().take(crate::fill_pattern::MAX_DESCRIPTION).collect(),
                    kind: options.kind.unwrap_or(default_kind),
                    scale: scale.clamp(crate::fill_pattern::MIN_SCALE, crate::fill_pattern::MAX_SCALE),
                    lines: Vec::new(),
                },
                failed,
            });
            continue;
        }
        let Some(o) = &mut open else {
            out.errors.push(PatError {
                line: line_no,
                pattern: None,
                message: "a line family before any pattern header".into(),
            });
            continue;
        };
        if o.failed {
            continue;
        }
        let fail = |o: &mut Open, out: &mut ParsedPat, message: String| {
            out.errors.push(PatError { line: line_no, pattern: Some(o.pattern.name.clone()), message });
            o.failed = true;
        };
        match family(data, factor) {
            Ok(_) if o.pattern.lines.len() >= MAX_FAMILIES => {
                fail(o, &mut out, format!("a pattern has at most {MAX_FAMILIES} line families"));
            }
            Ok(l) => match l.problem() {
                Some(p) => fail(o, &mut out, p),
                None => o.pattern.lines.push(l),
            },
            Err(e) => fail(o, &mut out, e),
        }
    }
    finish(open, &mut out);
    out
}

/// The value of a `;%KEY=VALUE` marker comment, upper case.
fn marker(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(';')?.trim_start().strip_prefix('%')?;
    let (k, v) = rest.split_once('=')?;
    k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_ascii_uppercase())
}

/// A line family from its comma-separated numbers, lengths times `factor`.
fn family(data: &str, factor: f64) -> Result<PatternLine, String> {
    let mut values = Vec::new();
    for (i, token) in data.split(',').enumerate() {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if values.len() >= 5 + MAX_DASHES {
            return Err(format!("a family has at most {MAX_DASHES} dashes"));
        }
        let v: f64 = token.parse().map_err(|_| format!("'{token}' (value {}) is not a number", i + 1))?;
        if !v.is_finite() {
            return Err(format!("'{token}' is not a finite number"));
        }
        values.push(v);
    }
    let [angle, x, y, dx, dy, ..] = values[..] else {
        return Err("a line family needs an angle, an origin and an offset (5 numbers)".into());
    };
    let dashes = values.get(5..).unwrap_or_default().iter().map(|d| d * factor).collect();
    Ok(PatternLine { angle, origin: [x * factor, y * factor], offset: [dx * factor, dy * factor], dashes })
}

/// A number with no more digits than it needs (up to 6 decimals).
fn num(v: f64) -> String {
    let s = format!("{:.6}", if v.abs() < 5e-7 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// `patterns` as `.pat` text in millimeters, their scales applied.
pub fn write(patterns: &[FillPattern]) -> String {
    let mut out = String::from(";%UNITS=MM\n");
    for p in patterns {
        let name: String = p.name.chars().map(|c| if matches!(c, ',' | ';' | '\n' | '\r') { ' ' } else { c }).collect();
        let description: String =
            p.description.chars().map(|c| if matches!(c, ';' | '\n' | '\r') { ' ' } else { c }).collect();
        out.push('*');
        out.push_str(name.trim());
        if !description.trim().is_empty() {
            out.push_str(", ");
            out.push_str(description.trim());
        }
        out.push('\n');
        out.push_str(match p.kind {
            FillPatternKind::Model => ";%TYPE=MODEL\n",
            FillPatternKind::Drafting => ";%TYPE=DRAFTING\n",
        });
        let s = p.scale;
        for l in &p.lines {
            let mut fields = vec![
                num(l.angle),
                num(l.origin[0] * s),
                num(l.origin[1] * s),
                num(l.offset[0] * s),
                num(l.offset[1] * s),
            ];
            fields.extend(l.dashes.iter().map(|d| num(d * s)));
            out.push_str(&fields.join(","));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; my own patterns\r\n\
        ;%UNITS=INCH\r\n\
        *STRIPES, Plain stripes\r\n\
        45, 0,0, 0,.125\r\n\
        \r\n\
        *DASHED,Dashes and dots ; inline comment\r\n\
        0, 0,0, .1,.25, .5,-.25,0,-.25\r\n\
        90, 0,0, 0,.5\r\n\
        *TILE\r\n\
        ;%TYPE=MODEL\r\n\
        0,0,0,0,12\r\n\
        90,0,0,0,12\r\n";

    #[test]
    fn reads_headers_families_units_and_markers() {
        let parsed = parse(SAMPLE, &PatOptions::default());
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let names: Vec<&str> = parsed.patterns.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["STRIPES", "DASHED", "TILE"]);
        let stripes = &parsed.patterns[0];
        assert_eq!(stripes.description, "Plain stripes");
        assert_eq!(stripes.kind, FillPatternKind::Drafting);
        assert!((stripes.lines[0].offset[1] - 3.175).abs() < 1e-9);
        let dashed = &parsed.patterns[1];
        assert_eq!(dashed.description, "Dashes and dots");
        assert_eq!(dashed.lines.len(), 2);
        assert_eq!(dashed.lines[0].dashes.len(), 4);
        assert!((dashed.lines[0].dashes[0] - 12.7).abs() < 1e-9 && dashed.lines[0].dashes[2] == 0.0);
        assert_eq!(parsed.patterns[2].kind, FillPatternKind::Model);
        // Overrides: units and kind chosen by the caller.
        let mm =
            parse(SAMPLE, &PatOptions { units: PatUnits::Millimeters, kind: Some(FillPatternKind::Model), scale: 2.0 });
        assert!((mm.patterns[0].lines[0].offset[1] - 0.125).abs() < 1e-12);
        assert!(mm.patterns.iter().all(|p| p.kind == FillPatternKind::Model && p.scale == 2.0));
    }

    #[test]
    fn malformed_patterns_are_left_out_with_their_line() {
        let text = "*GOOD\n0,0,0,0,1\n*BAD, letters\n0,0,0,zero,1\n*SHORT\n0,0,0\n*FLAT\n0,0,0,1,0\n\
                    *EMPTY\n*NAN\n0,0,0,0,NaN\n*,no name\n0,0,0,0,1\n*DOTS\n0,0,0,0,1,0,0\n";
        let parsed = parse(text, &PatOptions::default());
        assert_eq!(parsed.patterns.len(), 1);
        assert_eq!(parsed.patterns[0].name, "GOOD");
        let bad: Vec<(usize, Option<&str>)> = parsed.errors.iter().map(|e| (e.line, e.pattern.as_deref())).collect();
        assert!(bad.contains(&(4, Some("BAD"))), "{bad:?}");
        assert!(bad.contains(&(6, Some("SHORT"))));
        assert!(bad.contains(&(8, Some("FLAT"))));
        assert!(bad.contains(&(0, Some("EMPTY"))));
        assert!(bad.contains(&(11, Some("NAN"))));
        assert!(bad.contains(&(12, None)));
        assert!(bad.contains(&(15, Some("DOTS"))));
        // Data before any header.
        let orphan = parse("0,0,0,0,1\n", &PatOptions::default());
        assert!(orphan.patterns.is_empty() && orphan.errors.len() == 1);
    }

    #[test]
    fn hostile_sizes_are_bounded() {
        let huge = "x".repeat(MAX_PAT_BYTES + 1);
        let parsed = parse(&huge, &PatOptions::default());
        assert!(parsed.patterns.is_empty() && parsed.errors.len() == 1);
        let long_line = format!("*LONG\n0,0,0,0,1{}\n", ",1".repeat(MAX_LINE_CHARS));
        assert!(parse(&long_line, &PatOptions::default()).patterns.is_empty());
        let dashes = format!("*DASH\n0,0,0,0,1{}\n", ",1,-1".repeat(MAX_DASHES));
        assert!(parse(&dashes, &PatOptions::default()).patterns.is_empty());
        let families = format!("*MANY\n{}", "0,0,0,0,1\n".repeat(MAX_FAMILIES + 1));
        assert!(parse(&families, &PatOptions::default()).patterns.is_empty());
        let many = "*P\n0,0,0,0,1\n".repeat(MAX_PAT_PATTERNS + 3);
        let parsed = parse(&many, &PatOptions::default());
        assert_eq!(parsed.patterns.len(), MAX_PAT_PATTERNS);
        assert_eq!(parsed.errors.len(), 1);
    }

    #[test]
    fn written_text_reads_back() {
        let parsed = parse(SAMPLE, &PatOptions::default());
        let text = write(&parsed.patterns);
        let back = parse(&text, &PatOptions::default());
        assert!(back.errors.is_empty(), "{:?}", back.errors);
        assert_eq!(back.patterns.len(), 3);
        for (a, b) in parsed.patterns.iter().zip(&back.patterns) {
            assert_eq!((a.name.as_str(), a.kind), (b.name.as_str(), b.kind));
            for (la, lb) in a.lines.iter().zip(&b.lines) {
                assert!((la.offset[1] - lb.offset[1]).abs() < 1e-5);
                assert_eq!(la.dashes.len(), lb.dashes.len());
            }
        }
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(3.175), "3.175");
        assert_eq!(num(12.0), "12");
    }
}
