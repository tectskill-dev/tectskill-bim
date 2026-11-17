//! A Model Context Protocol server over standard input/output: JSON-RPC 2.0
//! messages, one per line. Every engine command is an MCP tool with the
//! same name, description and input schema; one session (one open project)
//! lives as long as the server.

use cc_engine::{CommandKind, Session};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// Protocol revisions this server speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "TectSkill BIM engine. One project is open per session: start with \
project_new, project_open or ifc_import; save with project_save. Lengths are meters, angles degrees, \
points [x, y]. Levels, types, views and materials accept an id or a name; elements take ids (see \
elements_list). element_get/type_get/view_get show the params that element_set/type_set/view_set change. \
Every edit is one undo step (undo, redo).";

/// Longest message line read, in bytes; a longer one is skipped and answered
/// with a parse error.
pub const MAX_LINE: usize = 64 * 1024 * 1024;

/// One line of input.
enum Line {
    Text(String),
    /// Longer than [`MAX_LINE`] (the rest of it was read and dropped).
    TooLong,
    /// Not UTF-8.
    NotUtf8,
}

/// Read the next line (without its end), keeping at most `max` bytes of it;
/// `None` at the end of the input.
fn read_line(input: &mut impl BufRead, max: usize) -> std::io::Result<Option<Line>> {
    let mut bytes = Vec::new();
    let mut too_long = false;
    let mut any = false;
    loop {
        let buf = input.fill_buf()?;
        if buf.is_empty() {
            break;
        }
        any = true;
        let (chunk, done) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (&buf[..i], Some(i + 1)),
            None => (buf, None),
        };
        if !too_long {
            if bytes.len() + chunk.len() > max {
                too_long = true;
                bytes = Vec::new();
            } else {
                bytes.extend_from_slice(chunk);
            }
        }
        let used = done.unwrap_or(buf.len());
        input.consume(used);
        if done.is_some() {
            break;
        }
    }
    if !any {
        return Ok(None);
    }
    if too_long {
        return Ok(Some(Line::TooLong));
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(Some(String::from_utf8(bytes).map_or(Line::NotUtf8, Line::Text)))
}

/// Serve until standard input closes.
pub fn serve(session: Session, input: impl BufRead, output: impl Write) -> std::io::Result<()> {
    serve_with(session, input, output, MAX_LINE)
}

fn serve_with(
    mut session: Session,
    mut input: impl BufRead,
    mut output: impl Write,
    max: usize,
) -> std::io::Result<()> {
    while let Some(line) = read_line(&mut input, max)? {
        let line = match line {
            Line::Text(line) => line,
            Line::TooLong => {
                writeln!(
                    output,
                    "{}",
                    error(Value::Null, -32700, &format!("parse error: a message over {max} bytes"))
                )?;
                output.flush()?;
                continue;
            }
            Line::NotUtf8 => {
                writeln!(output, "{}", error(Value::Null, -32700, "parse error: the message is not UTF-8"))?;
                output.flush()?;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let replies = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => batch.into_iter().filter_map(|m| handle(&mut session, m)).collect(),
            Ok(message) => handle(&mut session, message).into_iter().collect(),
            Err(e) => vec![error(Value::Null, -32700, &format!("parse error: {e}"))],
        };
        for reply in replies {
            writeln!(output, "{reply}")?;
        }
        output.flush()?;
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// The reply to one message (none for notifications).
pub fn handle(session: &mut Session, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(json!({}));
    // Notifications (no id) get no reply; responses to our requests do not occur.
    let id = id?;
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
            reply(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "constructcad", "title": "TectSkill BIM", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => reply(id, json!({})),
        "tools/list" => reply(id, json!({ "tools": tools(session) })),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "tools/call needs a name"));
            };
            if session.kind_of(name).is_none() {
                return Some(error(id, -32602, &format!("unknown tool: {name}")));
            }
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            reply(id, call(session, name, args))
        }
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

fn tools(session: &Session) -> Vec<Value> {
    session
        .commands()
        .into_iter()
        .map(|c| {
            // Commands that work with files reach beyond the open project.
            let files = c.kind == CommandKind::File || c.reads_files;
            json!({
                "name": c.name,
                "description": c.description,
                "inputSchema": c.input_schema,
                "annotations": {
                    "readOnlyHint": c.kind == CommandKind::Query && !files,
                    "destructiveHint": c.kind != CommandKind::Query && (c.name.contains("delete") || c.kind == CommandKind::File),
                    "idempotentHint": c.kind == CommandKind::Query && !files,
                    "openWorldHint": files,
                },
            })
        })
        .collect()
}

/// A tool result: the command's JSON as text and as structured content;
/// failures are tool errors the model can read and correct.
fn call(session: &mut Session, name: &str, args: Value) -> Value {
    let result = session.execute(name, args);
    // What plugins printed: standard error (standard output carries the protocol).
    for line in session.take_extension_messages() {
        eprintln!("{line}");
    }
    match result {
        Ok(out) => {
            let text = serde_json::to_string_pretty(&out).unwrap_or_default();
            // Structured content must be an object.
            let structured = if out.is_object() { out } else { json!({ "result": out }) };
            json!({ "content": [{ "type": "text", "text": text }], "structuredContent": structured, "isError": false })
        }
        Err(e) => {
            let text = serde_json::to_string_pretty(&json!({ "error": e })).unwrap_or_default();
            json!({ "content": [{ "type": "text", "text": text }], "isError": true })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_and_non_utf8_lines_get_parse_errors() {
        let mut input = b"\xff\xfe{}\n".to_vec();
        input.extend_from_slice(&[b'x'; 100]);
        input.push(b'\n');
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n");
        let mut out = Vec::new();
        serve_with(Session::default(), &input[..], &mut out, 50).unwrap();
        let replies: Vec<Value> =
            String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(replies.len(), 3, "{replies:?}");
        assert_eq!(replies[0]["error"]["code"], -32700);
        assert_eq!(replies[1]["error"]["code"], -32700);
        assert_eq!(replies[2]["id"], 1);
        assert!(replies[2]["result"].is_object());
    }
}
