//! The sandboxed Lua state scripts and plugins run in.
//!
//! Only the `table`, `string`, `math`, `utf8` and `coroutine` libraries are
//! loaded, and the base library without the functions that reach files or
//! load code (`dofile`, `loadfile`, `load`, `require`) or the collector:
//! a script touches the project only through engine commands, and files only
//! through the commands that read and write them, when it may. Each run has
//! an instruction budget and a time limit (a hook checks them in every
//! coroutine) and the state a memory limit; `pcall`, `xpcall` and
//! `coroutine.resume` pass their errors on, so a script cannot catch them
//! and keep going. Library functions whose work grows with their strings
//! run inside one instruction, so they are charged to the budget by their
//! size, `string.rep` refuses results past [`MAX_STRING`], and the pattern
//! functions count their steps ([`crate::patterns`]).

use mlua::{Function, HookTriggers, Lua, LuaOptions, MultiValue, StdLib, Table, Value as LuaValue, VmState};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How much a script may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Lua instructions per run (a script, or one plugin command).
    pub instructions: u64,
    /// Memory of the Lua state, in bytes.
    pub memory: usize,
    /// Wall-clock time per run.
    pub time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self { instructions: 500_000_000, memory: 256 * 1024 * 1024, time: Duration::from_secs(60) }
    }
}

/// The message of the error raised when the instruction budget is spent.
pub const BUDGET_SPENT: &str = "the script ran too long (instruction limit reached)";
/// The message of the error raised when the time limit is reached.
pub const TIME_SPENT: &str = "the script ran too long (time limit reached)";

/// Largest string `string.rep` and `string.gsub` make, in bytes.
pub const MAX_STRING: usize = 16 * 1024 * 1024;

/// Lines of `print` output kept per run, and their total size.
const MAX_LINES: usize = 10_000;
const MAX_OUTPUT: usize = 1024 * 1024;

/// Instructions between two checks of the budget and the clock.
const STEP: u32 = 1_000;

/// Bytes of a library function's strings charged as one instruction.
const BYTES_PER_INSTRUCTION: u64 = 16;

/// Passes the budget's error on through the functions that catch errors.
const PRELUDE: &str = r#"
local spent = ...
local raw_error = error
local raw_pcall, raw_xpcall, raw_resume = pcall, xpcall, coroutine.resume
local function check(...)
  local message = spent()
  if message then raw_error(message, 0) end
  return ...
end
pcall = function(...) return check(raw_pcall(...)) end
xpcall = function(...) return check(raw_xpcall(...)) end
coroutine.resume = function(...) return check(raw_resume(...)) end
"#;

/// A run's budget: instructions spent against the limit, and the time.
#[derive(Debug, Clone)]
pub struct Meter {
    spent: Rc<Cell<u64>>,
    limit: u64,
    deadline: Rc<Cell<Instant>>,
    time: Duration,
    max_string: usize,
}

impl Meter {
    fn new(limits: Limits) -> Meter {
        let meter = Meter {
            spent: Rc::new(Cell::new(0)),
            limit: limits.instructions,
            deadline: Rc::new(Cell::new(Instant::now())),
            time: limits.time,
            max_string: MAX_STRING.min(limits.memory),
        };
        meter.reset();
        meter
    }

    /// A full budget, from now.
    fn reset(&self) {
        self.spent.set(0);
        let now = Instant::now();
        // A time too long to add is as good as a year.
        let far = now.checked_add(Duration::from_secs(86_400 * 365)).unwrap_or(now);
        self.deadline.set(now.checked_add(self.time).unwrap_or(far));
    }

    /// Why the run must stop, if it must.
    pub fn exceeded(&self) -> Option<&'static str> {
        if self.spent.get() > self.limit {
            Some(BUDGET_SPENT)
        } else if Instant::now() > self.deadline.get() {
            Some(TIME_SPENT)
        } else {
            None
        }
    }

    /// Count `n` instructions' worth of work; an error when the budget or
    /// the time is spent.
    pub fn charge(&self, n: u64) -> mlua::Result<()> {
        self.spent.set(self.spent.get().saturating_add(n));
        match self.exceeded() {
            Some(message) => Err(mlua::Error::runtime(message)),
            None => Ok(()),
        }
    }

    /// Largest string a library function may make.
    pub fn max_string(&self) -> usize {
        self.max_string
    }
}

/// A sandboxed Lua state with its budget and captured output.
pub struct Sandbox {
    pub lua: Lua,
    limits: Limits,
    meter: Meter,
    output: Rc<RefCell<Output>>,
}

#[derive(Default)]
struct Output {
    lines: Vec<String>,
    bytes: usize,
    dropped: bool,
}

/// The text `print` shows for `v`, if it fits in `room` bytes (`None` when
/// it does not; a string larger than `room` is not copied out of Lua).
fn printed(v: &LuaValue, room: usize) -> Option<String> {
    match v {
        LuaValue::String(s) => {
            let bytes = s.as_bytes();
            (bytes.len() <= room).then(|| String::from_utf8_lossy(&bytes).into_owned())
        }
        // Other values print short, but `__tostring` can return anything.
        v => {
            let text = v.to_string().unwrap_or_else(|_| v.type_name().into());
            (text.len() <= room).then_some(text)
        }
    }
}

/// Bytes of the strings among `values`.
fn string_bytes(values: &MultiValue) -> u64 {
    values.iter().map(|v| if let LuaValue::String(s) = v { s.as_bytes().len() as u64 } else { 0 }).sum()
}

/// `f` charged to `meter` by the size of its arguments and results.
fn charged(lua: &Lua, f: Function, meter: &Meter) -> mlua::Result<Function> {
    let meter = meter.clone();
    lua.create_function(move |_, args: MultiValue| {
        meter.charge(string_bytes(&args) / BYTES_PER_INSTRUCTION)?;
        let out: MultiValue = f.call(args)?;
        meter.charge(string_bytes(&out) / BYTES_PER_INSTRUCTION)?;
        Ok(out)
    })
}

/// `string.rep`, refusing results larger than the meter allows.
fn rep(lua: &Lua, f: Function, meter: &Meter) -> mlua::Result<Function> {
    let meter = meter.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let len = |i: usize| -> mlua::Result<u64> {
            Ok(match args.get(i).cloned() {
                Some(v @ (LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_))) => {
                    lua.coerce_string(v)?.map_or(0, |s| s.as_bytes().len() as u64)
                }
                _ => 0,
            })
        };
        let count = match args.get(1).cloned() {
            Some(v) => lua.coerce_integer(v)?.unwrap_or(0),
            None => 0,
        };
        if count > 0 {
            let n = count.unsigned_abs();
            let size = len(0)?.saturating_mul(n).saturating_add(len(2)?.saturating_mul(n - 1));
            if size > meter.max_string() as u64 {
                return Err(mlua::Error::runtime(format!(
                    "string.rep: the result would be larger than {} MB",
                    meter.max_string() >> 20
                )));
            }
            meter.charge(size / BYTES_PER_INSTRUCTION)?;
        }
        f.call::<MultiValue>(args)
    })
}

/// Charge the library functions whose work grows with their strings.
fn meter_library(lua: &Lua, meter: &Meter) -> mlua::Result<()> {
    let globals = lua.globals();
    if let Ok(string) = globals.get::<Table>("string") {
        let original: Function = string.get("rep")?;
        string.raw_set("rep", rep(lua, original, meter)?)?;
        for name in ["upper", "lower", "reverse", "sub", "format", "byte", "char"] {
            if let Ok(original) = string.get::<Function>(name) {
                string.raw_set(name, charged(lua, original, meter)?)?;
            }
        }
        crate::patterns::install(lua, &string, meter)?;
    }
    if let Ok(table) = globals.get::<Table>("table")
        && let Ok(original) = table.get::<Function>("concat")
    {
        table.raw_set("concat", charged(lua, original, meter)?)?;
    }
    Ok(())
}

impl Sandbox {
    pub fn new(limits: Limits) -> mlua::Result<Sandbox> {
        let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
        let lua = Lua::new_with(libs, LuaOptions::new())?;
        lua.set_memory_limit(limits.memory)?;
        let globals = lua.globals();
        for name in ["dofile", "loadfile", "load", "require", "collectgarbage"] {
            globals.raw_set(name, LuaValue::Nil)?;
        }
        if let Ok(string) = globals.get::<Table>("string") {
            string.raw_set("dump", LuaValue::Nil)?;
        }

        let meter = Meter::new(limits);
        let hook = meter.clone();
        lua.set_global_hook(HookTriggers::new().every_nth_instruction(STEP), move |_, _| {
            hook.charge(u64::from(STEP))?;
            Ok(VmState::Continue)
        })?;
        let check = meter.clone();
        let check = lua.create_function(move |_, ()| Ok(check.exceeded()))?;
        lua.load(PRELUDE).set_name("=sandbox").call::<()>(check)?;
        meter_library(&lua, &meter)?;

        let output = Rc::new(RefCell::new(Output::default()));
        let sink = output.clone();
        let print = lua.create_function(move |_, args: MultiValue| {
            let mut out = sink.borrow_mut();
            if out.dropped || out.lines.len() >= MAX_LINES {
                out.dropped = true;
                return Ok(());
            }
            // Stop converting as soon as the output is full.
            let mut line = String::new();
            for (i, v) in args.iter().enumerate() {
                let room = MAX_OUTPUT.saturating_sub(out.bytes + line.len() + usize::from(i > 0));
                let Some(text) = printed(v, room) else {
                    out.dropped = true;
                    return Ok(());
                };
                if i > 0 {
                    line.push('\t');
                }
                line.push_str(&text);
            }
            out.bytes += line.len();
            out.lines.push(line);
            Ok(())
        })?;
        globals.raw_set("print", print)?;
        drop(globals);
        Ok(Sandbox { lua, limits, meter, output })
    }

    /// Start a new run: a full instruction budget and time.
    pub fn reset_budget(&self) {
        self.meter.reset();
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The lines printed since the last call (a last line notes any dropped).
    pub fn take_output(&self) -> Vec<String> {
        let mut out = self.output.borrow_mut();
        let mut lines = std::mem::take(&mut out.lines);
        if out.dropped {
            lines.push(format!("(output after {} lines or {} bytes left out)", MAX_LINES, MAX_OUTPUT));
        }
        *out = Output::default();
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sandbox: &Sandbox, src: &str) -> mlua::Result<LuaValue> {
        sandbox.reset_budget();
        sandbox.lua.load(src).eval()
    }

    #[test]
    fn print_stops_copying_when_the_output_is_full() {
        let sandbox = Sandbox::new(Limits::default()).unwrap();
        // 1,000 references to one 1 MB string: printed, a gigabyte.
        let src = "local s = string.rep('x', 1024 * 1024); local t = {}; for i = 1, 1000 do t[i] = s end; \
                   print(table.unpack(t)); print('after')";
        run(&sandbox, src).unwrap();
        let out = sandbox.take_output();
        assert_eq!(out.len(), 1, "{:?}", out.iter().map(String::len).collect::<Vec<_>>());
        assert!(out[0].contains("left out"));
        run(&sandbox, "print('a', 1, nil)").unwrap();
        assert_eq!(sandbox.take_output(), vec!["a\t1\tnil".to_string()]);
    }

    #[test]
    fn string_functions_count_against_the_budget() {
        let small = Limits { instructions: 5_000_000, ..Limits::default() };
        let sandbox = Sandbox::new(small).unwrap();
        // Each call is one instruction, but copies megabytes.
        let e = run(&sandbox, "local s = string.rep('x', 4 * 1024 * 1024); while true do local u = s:upper() end")
            .unwrap_err();
        assert!(e.to_string().contains("instruction limit"), "{e}");
        let e = run(&sandbox, "return string.rep('x', 1e9)").unwrap_err();
        assert!(e.to_string().contains("larger than"), "{e}");
        assert_eq!(run(&sandbox, "return ('ab'):rep(3, ',')").unwrap().to_string().unwrap(), "ab,ab,ab");
    }

    #[test]
    fn backtracking_patterns_stop() {
        let small = Limits { instructions: 5_000_000, ..Limits::default() };
        let sandbox = Sandbox::new(small).unwrap();
        let start = Instant::now();
        let e = run(&sandbox, "return ('a'):rep(40):find(('a*'):rep(20) .. 'b')").unwrap_err();
        assert!(e.to_string().contains("instruction limit"), "{e}");
        let e = run(&sandbox, "return ('a'):rep(1000000):find('a*b')").unwrap_err();
        assert!(e.to_string().contains("instruction limit"), "{e}");
        assert!(start.elapsed() < Duration::from_secs(20), "{:?}", start.elapsed());
    }

    #[test]
    fn the_clock_stops_a_run() {
        let short = Limits { time: Duration::from_millis(200), ..Limits::default() };
        let sandbox = Sandbox::new(short).unwrap();
        let start = Instant::now();
        let e = run(&sandbox, "while true do pcall(function() end) end").unwrap_err();
        assert!(e.to_string().contains("time limit"), "{e}");
        assert!(start.elapsed() < Duration::from_secs(20));
        // A new run gets its time again.
        assert!(run(&sandbox, "return 1").is_ok());
    }
}
