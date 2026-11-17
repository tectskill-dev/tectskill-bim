//! `constructcraft mcp` (or `--mcp`): MCP clients pointed at the app instead of `ccad` get the
//! MCP server of the `ccad` next to it, on the same standard input and output, instead of a
//! window that never answers (issue #265).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The `ccad` arguments for the app's arguments, when they ask for the MCP server.
pub fn mcp_args(args: &[String]) -> Option<Vec<String>> {
    let first = args.first()?;
    matches!(first.as_str(), "mcp" | "--mcp").then(|| {
        let mut out = vec!["mcp".to_string()];
        out.extend(args.iter().skip(1).cloned());
        out
    })
}

/// The `ccad` executable beside `exe`.
fn ccad_beside(exe: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "ccad.exe" } else { "ccad" };
    let path = exe.parent()?.join(name);
    path.is_file().then_some(path)
}

/// Runs `ccad mcp` with the app's standard streams and returns its exit code.
pub fn run(args: Vec<String>) -> i32 {
    let Some(ccad) = std::env::current_exe().ok().as_deref().and_then(ccad_beside) else {
        eprintln!(
            "ConstructCraft: the MCP server is the `ccad` command line, which was not found next to this \
             program. Point the MCP client at ccad (ccad.exe on Windows) with the argument `mcp`."
        );
        return 2;
    };
    let mut command = Command::new(&ccad);
    command.args(args);
    // ccad is a console program: started from the app (a windowed program on Windows), it
    // would open a console window of its own; the standard streams are inherited either way.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("ConstructCraft: could not start {}: {e}", ccad.display());
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn mcp_arguments_go_to_ccad() {
        assert_eq!(mcp_args(&strings(&["--mcp"])), Some(strings(&["mcp"])));
        assert_eq!(
            mcp_args(&strings(&["mcp", "-p", "house.ccad", "--lang", "en"])),
            Some(strings(&["mcp", "-p", "house.ccad", "--lang", "en"]))
        );
        assert_eq!(mcp_args(&strings(&["house.ccad"])), None);
        assert_eq!(mcp_args(&strings(&["house.ccad", "--mcp"])), None, "only as the first argument");
        assert_eq!(mcp_args(&[]), None);
    }
}
