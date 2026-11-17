//! Lua's pattern matching (`string.find`, `match`, `gmatch` and `gsub`)
//! with every step counted against the run's budget.
//!
//! Lua's own matcher runs inside one VM instruction, where the instruction
//! hook cannot stop it, and backtracking patterns take exponential time
//! (`("a"):rep(40):find(("a*"):rep(20) .. "b")`). These functions follow
//! the reference implementation's matcher step for step (same results,
//! same errors), but charge their steps to the sandbox's [`Meter`], which
//! stops the run when its instructions or its time are spent.

use crate::sandbox::Meter;
use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use std::cell::Cell;

/// Deepest recursion of the matcher (`MAXCCALLS` in Lua).
const MAX_DEPTH: usize = 200;
/// Most captures of a pattern (`LUA_MAXCAPTURES`).
const MAX_CAPTURES: usize = 32;
const ESC: u8 = b'%';
/// Characters that make a pattern more than plain text.
const SPECIALS: &[u8] = b"^$*+?.([%-";
/// Steps counted before they are charged to the meter.
const CHUNK: u64 = 4096;

type R<T> = mlua::Result<T>;

fn error(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Debug, Clone, Copy)]
enum CapLen {
    Unfinished,
    Position,
    Len(usize),
}

/// A capture's value: text, or a position (`()`).
enum Cap<'a> {
    Text(&'a [u8]),
    Position(usize),
}

/// The matcher over one subject and pattern.
struct State<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    depth: usize,
    level: usize,
    capture: [(usize, CapLen); MAX_CAPTURES],
    meter: &'a Meter,
    steps: u64,
}

fn match_class(c: u8, class: u8) -> bool {
    let res = match class.to_ascii_lowercase() {
        b'a' => c.is_ascii_alphabetic(),
        b'c' => c.is_ascii_control(),
        b'd' => c.is_ascii_digit(),
        b'g' => c.is_ascii_graphic(),
        b'l' => c.is_ascii_lowercase(),
        b'p' => c.is_ascii_punctuation(),
        b's' => matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c),
        b'u' => c.is_ascii_uppercase(),
        b'w' => c.is_ascii_alphanumeric(),
        b'x' => c.is_ascii_hexdigit(),
        _ => return class == c,
    };
    if class.is_ascii_uppercase() { !res } else { res }
}

impl<'a> State<'a> {
    fn new(src: &'a [u8], pat: &'a [u8], meter: &'a Meter) -> Self {
        Self { src, pat, depth: MAX_DEPTH, level: 0, capture: [(0, CapLen::Unfinished); MAX_CAPTURES], meter, steps: 0 }
    }

    fn reset(&mut self) {
        self.level = 0;
        self.depth = MAX_DEPTH;
    }

    /// Count a step, charging the meter every [`CHUNK`].
    fn step(&mut self, n: u64) -> R<()> {
        self.steps = self.steps.saturating_add(n);
        if self.steps >= CHUNK { self.flush() } else { Ok(()) }
    }

    /// Charge the steps not charged yet.
    fn flush(&mut self) -> R<()> {
        let n = std::mem::take(&mut self.steps);
        self.meter.charge(n)
    }

    /// The pattern's byte at `i` (0 past its end, as C's terminating zero).
    fn p(&self, i: usize) -> u8 {
        self.pat.get(i).copied().unwrap_or(0)
    }

    /// The subject's byte at `i` (0 past its end).
    fn s(&self, i: usize) -> u8 {
        self.src.get(i).copied().unwrap_or(0)
    }

    fn class_end(&self, p: usize) -> R<usize> {
        let mut p = p;
        let c = self.p(p);
        p += 1;
        match c {
            ESC => {
                if p >= self.pat.len() {
                    return Err(error("malformed pattern (ends with '%')"));
                }
                Ok(p + 1)
            }
            b'[' => {
                if self.p(p) == b'^' {
                    p += 1;
                }
                loop {
                    if p >= self.pat.len() {
                        return Err(error("malformed pattern (missing ']')"));
                    }
                    let c = self.p(p);
                    p += 1;
                    if c == ESC && p < self.pat.len() {
                        p += 1;
                    }
                    if self.p(p) == b']' {
                        break;
                    }
                }
                Ok(p + 1)
            }
            _ => Ok(p),
        }
    }

    /// Whether `c` is in the set `[...]` from `p` (its `[`) to `ec` (its `]`).
    fn bracket(&self, c: u8, p: usize, ec: usize) -> bool {
        let mut p = p;
        let mut sig = true;
        if self.p(p + 1) == b'^' {
            sig = false;
            p += 1;
        }
        loop {
            p += 1;
            if p >= ec {
                break;
            }
            let pc = self.p(p);
            if pc == ESC {
                p += 1;
                if match_class(c, self.p(p)) {
                    return sig;
                }
            } else if self.p(p + 1) == b'-' && p + 2 < ec {
                p += 2;
                if self.p(p - 2) <= c && c <= self.p(p) {
                    return sig;
                }
            } else if pc == c {
                return sig;
            }
        }
        !sig
    }

    fn single(&self, s: usize, p: usize, ep: usize) -> bool {
        let Some(&c) = self.src.get(s) else { return false };
        match self.p(p) {
            b'.' => true,
            ESC => match_class(c, self.p(p + 1)),
            b'[' => self.bracket(c, p, ep.saturating_sub(1)),
            pc => pc == c,
        }
    }

    fn balance(&mut self, s: usize, p: usize) -> R<Option<usize>> {
        if p + 1 >= self.pat.len() {
            return Err(error("malformed pattern (missing arguments to '%b')"));
        }
        if s >= self.src.len() || self.s(s) != self.p(p) {
            return Ok(None);
        }
        let (open, close) = (self.p(p), self.p(p + 1));
        let mut depth = 1usize;
        let mut s = s;
        loop {
            s += 1;
            if s >= self.src.len() {
                return Ok(None);
            }
            self.step(1)?;
            let c = self.s(s);
            if c == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(s + 1));
                }
            } else if c == open {
                depth += 1;
            }
        }
    }

    fn max_expand(&mut self, s: usize, p: usize, ep: usize) -> R<Option<usize>> {
        let mut i = 0usize;
        while self.single(s + i, p, ep) {
            self.step(1)?;
            i += 1;
        }
        loop {
            if let Some(res) = self.do_match(s + i, ep + 1)? {
                return Ok(Some(res));
            }
            if i == 0 {
                return Ok(None);
            }
            i -= 1;
        }
    }

    fn min_expand(&mut self, s: usize, p: usize, ep: usize) -> R<Option<usize>> {
        let mut s = s;
        loop {
            if let Some(res) = self.do_match(s, ep + 1)? {
                return Ok(Some(res));
            }
            if self.single(s, p, ep) {
                s += 1;
            } else {
                return Ok(None);
            }
        }
    }

    fn start_capture(&mut self, s: usize, p: usize, what: CapLen) -> R<Option<usize>> {
        let level = self.level;
        let Some(slot) = self.capture.get_mut(level) else { return Err(error("too many captures")) };
        *slot = (s, what);
        self.level = level + 1;
        let res = self.do_match(s, p)?;
        if res.is_none() {
            self.level -= 1;
        }
        Ok(res)
    }

    fn end_capture(&mut self, s: usize, p: usize) -> R<Option<usize>> {
        let l = (0..self.level)
            .rev()
            .find(|&l| matches!(self.capture[l].1, CapLen::Unfinished))
            .ok_or_else(|| error("invalid pattern capture"))?;
        self.capture[l].1 = CapLen::Len(s.saturating_sub(self.capture[l].0));
        let res = self.do_match(s, p)?;
        if res.is_none() {
            self.capture[l].1 = CapLen::Unfinished;
        }
        Ok(res)
    }

    fn match_capture(&mut self, s: usize, digit: u8) -> R<Option<usize>> {
        let l = usize::from(digit.wrapping_sub(b'1'));
        if digit < b'1' || l >= self.level || matches!(self.capture[l].1, CapLen::Unfinished) {
            return Err(error(format!("invalid capture index %{}", i32::from(digit) - i32::from(b'0'))));
        }
        let (init, len) = self.capture[l];
        let CapLen::Len(len) = len else { return Ok(None) };
        self.step(len as u64)?;
        match (self.src.get(init..init + len), self.src.get(s..s + len)) {
            (Some(a), Some(b)) if a == b => Ok(Some(s + len)),
            _ => Ok(None),
        }
    }

    fn do_match(&mut self, s: usize, p: usize) -> R<Option<usize>> {
        if self.depth == 0 {
            return Err(error("pattern too complex"));
        }
        self.depth -= 1;
        let res = self.match_here(s, p);
        self.depth += 1;
        res
    }

    fn match_here(&mut self, s: usize, p: usize) -> R<Option<usize>> {
        let (mut s, mut p) = (s, p);
        loop {
            self.step(1)?;
            if p >= self.pat.len() {
                return Ok(Some(s));
            }
            match self.p(p) {
                b'(' => {
                    return if self.p(p + 1) == b')' {
                        self.start_capture(s, p + 2, CapLen::Position)
                    } else {
                        self.start_capture(s, p + 1, CapLen::Unfinished)
                    };
                }
                b')' => return self.end_capture(s, p + 1),
                b'$' if p + 1 == self.pat.len() => return Ok((s == self.src.len()).then_some(s)),
                ESC if self.p(p + 1) == b'b' => match self.balance(s, p + 2)? {
                    Some(next) => {
                        s = next;
                        p += 4;
                    }
                    None => return Ok(None),
                },
                ESC if self.p(p + 1) == b'f' => {
                    p += 2;
                    if self.p(p) != b'[' {
                        return Err(error("missing '[' after '%f' in pattern"));
                    }
                    let ep = self.class_end(p)?;
                    let previous = if s == 0 { 0 } else { self.s(s - 1) };
                    let current = self.s(s);
                    if !self.bracket(previous, p, ep - 1) && self.bracket(current, p, ep - 1) {
                        p = ep;
                    } else {
                        return Ok(None);
                    }
                }
                ESC if self.p(p + 1).is_ascii_digit() => match self.match_capture(s, self.p(p + 1))? {
                    Some(next) => {
                        s = next;
                        p += 2;
                    }
                    None => return Ok(None),
                },
                _ => {
                    let ep = self.class_end(p)?;
                    let after = self.p(ep);
                    if !self.single(s, p, ep) {
                        if matches!(after, b'*' | b'?' | b'-') {
                            p = ep + 1;
                            continue;
                        }
                        return Ok(None);
                    }
                    match after {
                        b'?' => {
                            if let Some(res) = self.do_match(s + 1, ep + 1)? {
                                return Ok(Some(res));
                            }
                            p = ep + 1;
                        }
                        b'+' => return self.max_expand(s + 1, p, ep),
                        b'*' => return self.max_expand(s, p, ep),
                        b'-' => return self.min_expand(s, p, ep),
                        _ => {
                            s += 1;
                            p = ep;
                        }
                    }
                }
            }
        }
    }

    /// Capture `i`, or the whole match `s..e` when the pattern has none.
    fn get_capture(&self, i: usize, s: usize, e: usize) -> R<Cap<'a>> {
        if i >= self.level {
            if i != 0 {
                return Err(error(format!("invalid capture index %{}", i + 1)));
            }
            return Ok(Cap::Text(self.src.get(s..e).unwrap_or_default()));
        }
        let (init, len) = self.capture[i];
        match len {
            CapLen::Unfinished => Err(error("unfinished capture")),
            CapLen::Position => Ok(Cap::Position(init + 1)),
            CapLen::Len(n) => Ok(Cap::Text(self.src.get(init..init + n).unwrap_or_default())),
        }
    }

    fn capture_value(&self, lua: &Lua, i: usize, s: usize, e: usize) -> R<LuaValue> {
        Ok(match self.get_capture(i, s, e)? {
            Cap::Text(t) => LuaValue::String(lua.create_string(t)?),
            Cap::Position(n) => LuaValue::Integer(i64::try_from(n).unwrap_or(i64::MAX)),
        })
    }

    /// The captures (the whole match `s..e` when there are none and `s` is
    /// given).
    fn captures(&self, lua: &Lua, s: Option<usize>, e: usize) -> R<MultiValue> {
        let n = if self.level == 0 && s.is_some() { 1 } else { self.level };
        let mut out = MultiValue::new();
        for i in 0..n {
            out.push_back(self.capture_value(lua, i, s.unwrap_or(0), e)?);
        }
        Ok(out)
    }
}

fn arg(args: &MultiValue, i: usize) -> LuaValue {
    args.get(i).cloned().unwrap_or(LuaValue::Nil)
}

fn check_string(lua: &Lua, args: &MultiValue, i: usize, name: &str) -> R<mlua::String> {
    let v = arg(args, i);
    let type_name = v.type_name();
    match v {
        LuaValue::String(s) => Ok(s),
        v @ (LuaValue::Integer(_) | LuaValue::Number(_)) => lua
            .coerce_string(v)?
            .ok_or_else(|| error(format!("bad argument #{} to '{name}' (string expected, got {type_name})", i + 1))),
        _ => Err(error(format!("bad argument #{} to '{name}' (string expected, got {type_name})", i + 1))),
    }
}

fn opt_integer(lua: &Lua, args: &MultiValue, i: usize, name: &str) -> R<Option<i64>> {
    match arg(args, i) {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(n) => Ok(Some(n)),
        v => {
            let type_name = v.type_name();
            lua.coerce_integer(v)?.map(Some).ok_or_else(|| {
                error(format!(
                    "bad argument #{} to '{name}' (number has no integer representation, got {type_name})",
                    i + 1
                ))
            })
        }
    }
}

/// A 1-based position, negative from the end, as an offset (`posrelatI`).
fn start_at(pos: i64, len: usize) -> usize {
    let len_i = i64::try_from(len).unwrap_or(i64::MAX);
    let one_based = if pos > 0 {
        usize::try_from(pos).unwrap_or(usize::MAX)
    } else if pos == 0 || pos < -len_i {
        1
    } else {
        usize::try_from(len_i + pos + 1).unwrap_or(1)
    };
    one_based - 1
}

fn to_integer(n: usize) -> LuaValue {
    LuaValue::Integer(i64::try_from(n).unwrap_or(i64::MAX))
}

/// Plain search of `needle` in `hay`, charged to the meter.
fn plain_find(hay: &[u8], needle: &[u8], meter: &Meter) -> R<Option<usize>> {
    let Some(&first) = needle.first() else { return Ok(Some(0)) };
    if needle.len() > hay.len() {
        return Ok(None);
    }
    let mut steps = 0u64;
    for i in 0..=hay.len() - needle.len() {
        steps += 1;
        if hay.get(i) == Some(&first) {
            steps += needle.len() as u64;
            if hay.get(i..i + needle.len()) == Some(needle) {
                meter.charge(steps)?;
                return Ok(Some(i));
            }
        }
        if steps >= CHUNK {
            meter.charge(std::mem::take(&mut steps))?;
        }
    }
    meter.charge(steps)?;
    Ok(None)
}

/// `string.find` (`find`) and `string.match`.
fn find_aux(lua: &Lua, meter: &Meter, args: &MultiValue, find: bool) -> R<MultiValue> {
    let name = if find { "find" } else { "match" };
    let subject = check_string(lua, args, 0, name)?;
    let pattern = check_string(lua, args, 1, name)?;
    let (src, pat) = (subject.as_bytes(), pattern.as_bytes());
    let init = start_at(opt_integer(lua, args, 2, name)?.unwrap_or(1), src.len());
    let fail = || MultiValue::from_iter([LuaValue::Nil]);
    if init > src.len() {
        return Ok(fail());
    }
    let plain = find
        && (!matches!(arg(args, 3), LuaValue::Nil | LuaValue::Boolean(false))
            || !pat.iter().any(|c| SPECIALS.contains(c)));
    if plain {
        if let Some(at) = plain_find(src.get(init..).unwrap_or_default(), &pat, meter)? {
            return Ok(MultiValue::from_iter([to_integer(init + at + 1), to_integer(init + at + pat.len())]));
        }
        return Ok(fail());
    }
    let (anchor, pat) = match pat.split_first() {
        Some((b'^', rest)) => (true, rest),
        _ => (false, &pat[..]),
    };
    let mut st = State::new(&src, pat, meter);
    let mut s1 = init;
    loop {
        st.reset();
        if let Some(e) = st.do_match(s1, 0)? {
            st.flush()?;
            if find {
                let mut out = MultiValue::from_iter([to_integer(s1 + 1), to_integer(e)]);
                out.extend(st.captures(lua, None, 0)?);
                return Ok(out);
            }
            return st.captures(lua, Some(s1), e);
        }
        if anchor || s1 >= src.len() {
            break;
        }
        s1 += 1;
    }
    st.flush()?;
    Ok(fail())
}

/// `string.gmatch`: an iterator over the matches.
fn gmatch(lua: &Lua, meter: &Meter, args: &MultiValue) -> R<Function> {
    let subject = check_string(lua, args, 0, "gmatch")?;
    let pattern = check_string(lua, args, 1, "gmatch")?;
    let len = subject.as_bytes().len();
    let mut init = start_at(opt_integer(lua, args, 2, "gmatch")?.unwrap_or(1), len);
    if init > len {
        init = len + 1;
    }
    let meter = meter.clone();
    let next = Cell::new(init);
    let last: Cell<Option<usize>> = Cell::new(None);
    lua.create_function(move |lua, ()| {
        let (src, pat) = (subject.as_bytes(), pattern.as_bytes());
        let mut st = State::new(&src, &pat, &meter);
        let mut at = next.get();
        while at <= src.len() {
            st.reset();
            if let Some(e) = st.do_match(at, 0)?
                && Some(e) != last.get()
            {
                next.set(e);
                last.set(Some(e));
                st.flush()?;
                return st.captures(lua, Some(at), e);
            }
            at += 1;
        }
        st.flush()?;
        Ok(MultiValue::new())
    })
}

/// Append `bytes` to the result of `gsub`, at most `max` bytes in all.
fn append(out: &mut Vec<u8>, bytes: &[u8], max: usize) -> R<()> {
    if out.len().saturating_add(bytes.len()) > max {
        return Err(error(format!("gsub: the result would be larger than {} MB", max >> 20)));
    }
    out.extend_from_slice(bytes);
    Ok(())
}

/// The replacement string's text, with `%0`-`%9` and `%%`.
fn add_text(st: &State, out: &mut Vec<u8>, s: usize, e: usize, repl: &[u8], max: usize) -> R<()> {
    let mut i = 0;
    while let Some(off) = repl.get(i..).and_then(|r| r.iter().position(|&c| c == ESC)) {
        append(out, repl.get(i..i + off).unwrap_or_default(), max)?;
        let p = i + off + 1;
        let c = repl.get(p).copied().unwrap_or(0);
        if c == ESC {
            append(out, &[ESC], max)?;
        } else if c == b'0' {
            append(out, st.src.get(s..e).unwrap_or_default(), max)?;
        } else if c.is_ascii_digit() {
            match st.get_capture(usize::from(c - b'1'), s, e)? {
                Cap::Text(t) => append(out, t, max)?,
                Cap::Position(n) => append(out, n.to_string().as_bytes(), max)?,
            }
        } else {
            return Err(error("invalid use of '%' in replacement string"));
        }
        i = p + 1;
    }
    append(out, repl.get(i..).unwrap_or_default(), max)
}

/// Add the replacement of the match `s..e`; returns whether it changed the text.
fn add_value(lua: &Lua, st: &State, out: &mut Vec<u8>, s: usize, e: usize, repl: &LuaValue, max: usize) -> R<bool> {
    let value = match repl {
        LuaValue::Function(f) => f.call::<LuaValue>(st.captures(lua, Some(s), e)?)?,
        LuaValue::Table(t) => t.get::<LuaValue>(st.capture_value(lua, 0, s, e)?)?,
        other => {
            let text = lua.coerce_string(other.clone())?.ok_or_else(|| error("invalid replacement"))?;
            add_text(st, out, s, e, &text.as_bytes(), max)?;
            return Ok(true);
        }
    };
    match value {
        LuaValue::Nil | LuaValue::Boolean(false) => {
            append(out, st.src.get(s..e).unwrap_or_default(), max)?;
            Ok(false)
        }
        v @ (LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_)) => {
            let text = lua.coerce_string(v)?.ok_or_else(|| error("invalid replacement value"))?;
            append(out, &text.as_bytes(), max)?;
            Ok(true)
        }
        other => Err(error(format!("invalid replacement value (a {})", other.type_name()))),
    }
}

/// `string.gsub`.
fn gsub(lua: &Lua, meter: &Meter, args: &MultiValue) -> R<MultiValue> {
    let subject = check_string(lua, args, 0, "gsub")?;
    let pattern = check_string(lua, args, 1, "gsub")?;
    let repl = arg(args, 2);
    if !matches!(
        repl,
        LuaValue::Integer(_) | LuaValue::Number(_) | LuaValue::String(_) | LuaValue::Function(_) | LuaValue::Table(_)
    ) {
        return Err(error(format!(
            "bad argument #3 to 'gsub' (string/function/table expected, got {})",
            repl.type_name()
        )));
    }
    let (src, pat) = (subject.as_bytes(), pattern.as_bytes());
    let max_n = opt_integer(lua, args, 3, "gsub")?.unwrap_or_else(|| i64::try_from(src.len()).unwrap_or(i64::MAX) + 1);
    let (anchor, pat) = match pat.split_first() {
        Some((b'^', rest)) => (true, rest),
        _ => (false, &pat[..]),
    };
    let max = meter.max_string();
    let mut st = State::new(&src, pat, meter);
    let mut out: Vec<u8> = Vec::new();
    let (mut at, mut last, mut n, mut changed) = (0usize, None, 0i64, false);
    while n < max_n {
        st.reset();
        match st.do_match(at, 0)? {
            Some(e) if Some(e) != last => {
                n += 1;
                changed |= add_value(lua, &st, &mut out, at, e, &repl, max)?;
                at = e;
                last = Some(e);
            }
            _ if at < src.len() => {
                append(&mut out, src.get(at..=at).unwrap_or_default(), max)?;
                at += 1;
            }
            _ => break,
        }
        if anchor {
            break;
        }
    }
    st.flush()?;
    let result = if changed {
        append(&mut out, src.get(at..).unwrap_or_default(), max)?;
        meter.charge(out.len() as u64 / 16)?;
        LuaValue::String(lua.create_string(&out)?)
    } else {
        // Unchanged: the subject as a string (Lua converts a number argument in place).
        LuaValue::String(lua.create_string(src)?)
    };
    Ok(MultiValue::from_iter([result, LuaValue::Integer(n)]))
}

/// Put the metered `find`, `match`, `gmatch` and `gsub` in the `string`
/// table (which strings' methods come from).
pub fn install(lua: &Lua, string: &mlua::Table, meter: &Meter) -> R<()> {
    let m = meter.clone();
    string.raw_set("find", lua.create_function(move |lua, args: MultiValue| find_aux(lua, &m, &args, true))?)?;
    let m = meter.clone();
    string.raw_set("match", lua.create_function(move |lua, args: MultiValue| find_aux(lua, &m, &args, false))?)?;
    let m = meter.clone();
    string.raw_set("gmatch", lua.create_function(move |lua, args: MultiValue| gmatch(lua, &m, &args))?)?;
    let m = meter.clone();
    string.raw_set("gsub", lua.create_function(move |lua, args: MultiValue| gsub(lua, &m, &args))?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::sandbox::{Limits, Sandbox};
    use mlua::Lua;

    fn run(lua: &Lua, src: &str) -> String {
        let values: mlua::MultiValue = lua.load(src).eval().unwrap();
        let shown: Vec<String> =
            values.iter().map(|v| format!("{}:{}", v.type_name(), v.to_string().unwrap_or_default())).collect();
        shown.join("|")
    }

    /// The same results and errors as Lua's own functions.
    #[test]
    fn matches_like_lua() {
        let sandbox = Sandbox::new(Limits::default()).unwrap();
        let native = Lua::new();
        let cases = [
            r#"return ("hello world"):find("o w")"#,
            r#"return ("hello world"):find("o", 6)"#,
            r#"return ("hello world"):find("l+")"#,
            r#"return ("hello"):find("")"#,
            r#"return ("hello"):find("", 10)"#,
            r#"return ("hello"):find("", 6)"#,
            r#"return ("hello"):find("l", -2)"#,
            r#"return ("hello"):find("l", -20)"#,
            r#"return ("a.b"):find(".", 1, true)"#,
            r#"return ("a.b"):find("%.")"#,
            r#"return ("key = value"):match("^(%w+)%s*=%s*(%w+)$")"#,
            r#"return ("  trim me  "):match("^%s*(.-)%s*$")"#,
            r#"return ("THE (quick) fox"):find("%((%a+)%)")"#,
            r#"return ("f(a(b)c)d"):match("%b()")"#,
            r#"return ("THE (quick) fox"):match("%f[%a]%a+", 5)"#,
            r#"return ("hello"):match("()ll()")"#,
            r#"return ("abcabc"):match("(abc)%1")"#,
            r#"return ("aaa"):match("a-b")"#,
            r#"return ("aaab"):match("a-b")"#,
            r#"return ("ab"):match("a?b?c?")"#,
            r#"return ("2024-01-31"):match("(%d+)-(%d+)-(%d+)")"#,
            r#"return ("x = 1, y = 2"):gsub("(%w+) = (%w+)", "%2 = %1")"#,
            r#"return ("hello world"):gsub("o", {o = "0"})"#,
            r#"return ("hello world"):gsub("%w+", function(w) return w:upper() end)"#,
            r#"return ("hello world"):gsub("%w+", function(w) if w == "hello" then return nil end return 1.5 end)"#,
            r#"return ("hello world"):gsub("", "-")"#,
            r#"return ("abc"):gsub("%w", "%0%0", 2)"#,
            r#"return ("abc"):gsub("^a", "x")"#,
            r#"return ("hello"):gsub("l", "%%")"#,
            r#"return ("hello"):gsub("()l", "%1")"#,
            r#"return ("hello"):gsub("x", "y")"#,
            r#"local t = {} for k, v in ("a=1, b=2"):gmatch("(%w+)=(%w+)") do t[#t+1] = k .. v end return table.concat(t, ";")"#,
            r#"local t = {} for w in ("one two  three"):gmatch("%a*") do t[#t+1] = "[" .. w .. "]" end return table.concat(t)"#,
            r#"local t = {} for p in ("abc"):gmatch("()") do t[#t+1] = p end return table.concat(t, ",")"#,
            r#"local t = {} for w in ("abc abd"):gmatch("ab(.)", 3) do t[#t+1] = w end return table.concat(t, ",")"#,
            r#"return ("[a-z]"):find("[%[%]]")"#,
            r#"return ("x-y"):match("[a%-z]+")"#,
            r#"return ("]x"):match("[]x]+")"#,
            r#"return ("^x"):match("[x^]+")"#,
            r#"return ("A1b2"):gsub("[^%d]", "")"#,
            r#"return ("A1 b2\t"):gsub("[%u%s]", "_")"#,
            r#"return string.gsub(12345, "3", "x")"#,
            r#"return string.gsub(12345, "9", "x")"#,
            r#"return ("a,b,,c"):gsub(",", ";", 2)"#,
            r#"return string.find("abc", "b", 1.0)"#,
            r#"return string.match("abc", "^b")"#,
            r#"return string.find("a$b", "$b")"#,
            r#"return string.find("ab", "b$")"#,
        ];
        for case in cases {
            assert_eq!(run(&sandbox.lua, case), run(&native, case), "{case}");
        }
        for bad in [
            r#"return ("a"):find("%")"#,
            r#"return ("a"):find("[a")"#,
            r#"return ("a"):find("(a")"#,
            r#"return ("a"):match("a)")"#,
            r#"return ("a"):gsub("a", "%2")"#,
            r#"return ("a"):gsub("a", "%x")"#,
            r#"return ("a"):find("%1")"#,
            r#"return ("a"):match("%fa")"#,
            r#"return ("a"):match("%b")"#,
            r#"return ("a"):gsub("a", true)"#,
            r#"return ("a"):gsub("a", function() return {} end)"#,
        ] {
            assert!(sandbox.lua.load(bad).exec().is_err(), "{bad}");
            assert!(native.load(bad).exec().is_err(), "{bad}");
        }
    }
}
