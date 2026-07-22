//! Local Model Context Protocol transport for Agent-Forge.
//!
//! This module deliberately keeps the Forge database and repository-local
//! operations in one process. The remote Kleos MCP remains the coordination
//! surface; this server owns code-work evidence and Fluency documents.

use crate::db::Database;
use crate::json_io::Output;
use crate::tools;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};

/// Largest accepted MCP message body, preventing unbounded input buffering.
const MAX_MCP_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// Wire framing selected from the first message in a stdio session.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Framing {
    /// One compact JSON object per line.
    Ndjson,
    /// LSP-style `Content-Length` headers followed by a JSON body.
    ContentLength,
}

/// Describe one MCP tool with its complete object schema.
fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required}
    })
}

/// Return the local workflow tools advertised to MCP clients.
pub fn tool_list() -> Vec<Value> {
    vec![
        tool(
            "spec_task",
            "Create a code-work specification before editing.",
            json!({
                "task_description":{"type":"string"}, "task_type":{"type":"string","enum":["feature","bugfix","refactor","enhancement","test","docs"]},
                "acceptance_criteria":{"type":"array","items":{"type":"string"},"minItems":2}, "interface_contract":{"type":"string"},
                "edge_cases":{"type":"array","items":{"type":"string"},"minItems":3}, "files_to_touch":{"type":"array","items":{"type":"string"}},
                "dependencies":{"type":"string"}
            }),
            &[
                "task_description",
                "task_type",
                "acceptance_criteria",
                "interface_contract",
                "edge_cases",
            ],
        ),
        tool(
            "consider_approaches",
            "Record and compare implementation approaches.",
            json!({
                "spec_id":{"type":"string"}, "problem":{"type":"string"}, "chosen_index":{"type":"integer","minimum":0},
                "approaches":{"type":"array","minItems":2,"items":{"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"},"pros":{"type":"array","items":{"type":"string"}},"cons":{"type":"array","items":{"type":"string"}},"score":{"type":"number"}},"required":["name","description"]}}
            }),
            &["problem", "approaches"],
        ),
        tool(
            "log_hypothesis",
            "Record a debugging hypothesis before a bug fix.",
            json!({"bug_description":{"type":"string"},"hypothesis":{"type":"string"},"confidence":{"type":"number","minimum":0,"maximum":1},"spec_id":{"type":"string"}}),
            &["bug_description", "hypothesis"],
        ),
        tool(
            "log_outcome",
            "Record whether a debugging hypothesis was correct.",
            json!({"hypothesis_id":{"type":"string"},"outcome":{"type":"string","enum":["correct","incorrect","partial"]},"notes":{"type":"string"}}),
            &["hypothesis_id", "outcome"],
        ),
        tool(
            "verify",
            "Run verification commands and link evidence to a spec.",
            json!({
                "command":{"type":"string"},"expected_exit_code":{"type":"integer"},"spec_id":{"type":"string"},"criteria_index":{"type":"integer"},"timeout_secs":{"type":"integer","minimum":1},
                "steps":{"type":"array","items":{"type":"object","properties":{"command":{"type":"string"},"expected_exit_code":{"type":"integer"},"label":{"type":"string"}},"required":["command"]}}
            }),
            &[],
        ),
        tool(
            "challenge_code",
            "Build an adversarial review prompt for a source file.",
            json!({"file_path":{"type":"string"},"focus_areas":{"type":"array","items":{"type":"string"}}}),
            &["file_path"],
        ),
        tool(
            "comment_check",
            "Check declaration comment coverage in a source file.",
            json!({"file_path":{"type":"string"}}),
            &["file_path"],
        ),
        tool(
            "checkpoint",
            "Snapshot git state and emit a Fluency slice when spec prose is supplied.",
            json!({
                "name":{"type":"string"},"description":{"type":"string"},"spec_id":{"type":"string"},"intent":{"type":"string"},
                "components":{"type":"array","items":{"type":"string"}},"conditions":{"type":"array","items":{"type":"string"}},"emit":{"type":"boolean"},"repo_root":{"type":"string"}
            }),
            &["name"],
        ),
        tool(
            "session_diff",
            "Summarize the current git diff before completion.",
            json!({"base":{"type":"string"}}),
            &[],
        ),
        tool(
            "think",
            "Build a structured reasoning prompt.",
            json!({"problem":{"type":"string"},"constraints":{"type":"array","items":{"type":"string"}},"context":{"type":"string"}}),
            &["problem"],
        ),
        tool(
            "declare_unknowns",
            "Partition blocking and non-blocking unknowns.",
            json!({"unknowns":{"type":"array","minItems":1,"items":{"type":"object","properties":{"description":{"type":"string"},"blocking":{"type":"boolean"},"resolution_hint":{"type":"string"}},"required":["description","blocking"]}}}),
            &["unknowns"],
        ),
        tool(
            "update_spec",
            "Transition a specification lifecycle state.",
            json!({"spec_id":{"type":"string"},"status":{"type":"string","enum":["active","completed","failed","blocked"]},"note":{"type":"string"}}),
            &["spec_id", "status"],
        ),
        tool(
            "list_specs",
            "List specifications in the local Forge database.",
            json!({"status":{"type":"string"},"limit":{"type":"integer","minimum":1}}),
            &[],
        ),
        tool(
            "get_spec",
            "Fetch a specification and all linked evidence.",
            json!({"spec_id":{"type":"string"}}),
            &["spec_id"],
        ),
        tool(
            "review",
            "Assemble and optionally write a Fluency review record.",
            json!({"spec_id":{"type":"string"},"repo_root":{"type":"string"},"write":{"type":"boolean"}}),
            &["spec_id"],
        ),
    ]
}

/// Deserialize tool arguments and invoke one typed Agent-Forge function.
fn call_typed<T, F>(db: &Database, arguments: Value, function: F) -> Result<Output, String>
where
    T: DeserializeOwned,
    F: FnOnce(&Database, T) -> tools::ToolResult,
{
    let input = serde_json::from_value(arguments).map_err(|error| error.to_string())?;
    function(db, input).map_err(|error| error.to_string())
}

/// Dispatch one advertised tool against the shared local database.
fn call_tool(db: &Database, name: &str, arguments: Value) -> Result<Output, String> {
    match name {
        "spec_task" => call_typed(db, arguments, tools::spec::spec_task),
        "consider_approaches" => call_typed(db, arguments, tools::approaches::consider_approaches),
        "log_hypothesis" => call_typed(db, arguments, tools::hypothesis::log_hypothesis),
        "log_outcome" => call_typed(db, arguments, tools::hypothesis::log_outcome),
        "verify" => call_typed(db, arguments, tools::verify::verify),
        "challenge_code" => call_typed(db, arguments, tools::verify::challenge_code),
        "comment_check" => call_typed(db, arguments, tools::comments::comment_check),
        "checkpoint" => call_typed(db, arguments, tools::session::checkpoint),
        "session_diff" => call_typed(db, arguments, tools::verify::session_diff),
        "think" => call_typed(db, arguments, tools::think::think),
        "declare_unknowns" => call_typed(db, arguments, tools::think::declare_unknowns),
        "update_spec" => call_typed(db, arguments, tools::spec::update_spec),
        "list_specs" => call_typed(db, arguments, tools::spec::list_specs),
        "get_spec" => call_typed(db, arguments, tools::spec::get_spec),
        "review" => call_typed(db, arguments, tools::emit::review),
        _ => Err(format!("unknown Agent-Forge tool: {name}")),
    }
}

/// Build a JSON-RPC error response.
fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

/// Convert an Agent-Forge output into the MCP content and structured-content contract.
fn tool_result(output: Output) -> Value {
    let structured = serde_json::to_value(&output).expect("Output serialization cannot fail");
    json!({
        "content": [{"type": "text", "text": structured.to_string()}],
        "structuredContent": structured,
        "isError": false
    })
}

/// Convert a tool failure into a normal MCP tool result marked as an error.
fn tool_failure(message: String) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

/// Handle one MCP JSON-RPC request, returning no response for notifications.
pub fn handle_jsonrpc(db: &Database, request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => return id.map(|id| rpc_error(id, -32600, "missing JSON-RPC method")),
    };
    if id.is_none() {
        return None;
    }
    let id = id.expect("request ID checked above");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "agent-forge-mcp", "version": env!("CARGO_PKG_VERSION")}
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tool_list()}),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "tools/call requires params.name"));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(db, name, arguments) {
                Ok(output) => tool_result(output),
                Err(error) => tool_failure(error),
            }
        }
        _ => return Some(rpc_error(id, -32601, format!("method not found: {method}"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// Read a capped line so malformed peers cannot grow memory without bound.
fn read_line_capped<R: BufRead>(reader: &mut R) -> Result<Option<String>, String> {
    let mut output = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(|error| error.to_string())?;
        if available.is_empty() {
            return if output.is_empty() {
                Ok(None)
            } else {
                String::from_utf8(output)
                    .map(Some)
                    .map_err(|error| error.to_string())
            };
        }
        let (length, complete) = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| (index + 1, true))
            .unwrap_or((available.len(), false));
        if output.len() + length > MAX_MCP_MESSAGE_SIZE {
            return Err(format!("line exceeds max {MAX_MCP_MESSAGE_SIZE} bytes"));
        }
        output.extend_from_slice(&available[..length]);
        reader.consume(length);
        if complete {
            return String::from_utf8(output)
                .map(Some)
                .map_err(|error| error.to_string());
        }
    }
}

/// Parse a Content-Length header value and enforce the message-size cap.
fn parse_content_length(header: &str) -> Result<usize, String> {
    let length = header
        .strip_prefix("Content-Length:")
        .ok_or_else(|| "not a Content-Length header".to_string())?
        .trim()
        .parse::<usize>()
        .map_err(|error| error.to_string())?;
    if length > MAX_MCP_MESSAGE_SIZE {
        return Err(format!(
            "Content-Length {length} exceeds max {MAX_MCP_MESSAGE_SIZE}"
        ));
    }
    Ok(length)
}

/// Read and decode an exact Content-Length body.
fn read_body<R: Read>(reader: &mut R, length: usize) -> Result<Value, String> {
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

/// Read the first request and detect the peer's framing mode.
fn read_first<R: BufRead>(reader: &mut R) -> Result<Option<(Value, Framing)>, String> {
    loop {
        let Some(line) = read_line_capped(reader)? else {
            return Ok(None);
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') {
            return serde_json::from_str(trimmed)
                .map(|value| Some((value, Framing::Ndjson)))
                .map_err(|error| error.to_string());
        }
        if trimmed.starts_with("Content-Length:") {
            let length = parse_content_length(trimmed)?;
            while read_line_capped(reader)?.is_some_and(|header| !header.trim().is_empty()) {}
            return read_body(reader, length).map(|value| Some((value, Framing::ContentLength)));
        }
        return Err(format!("unexpected first line: {trimmed}"));
    }
}

/// Read the next request using the negotiated framing.
fn read_next<R: BufRead>(reader: &mut R, framing: Framing) -> Result<Option<Value>, String> {
    match framing {
        Framing::Ndjson => loop {
            let Some(line) = read_line_capped(reader)? else {
                return Ok(None);
            };
            if !line.trim().is_empty() {
                return serde_json::from_str(line.trim())
                    .map(Some)
                    .map_err(|error| error.to_string());
            }
        },
        Framing::ContentLength => {
            let mut length = None;
            loop {
                let Some(line) = read_line_capped(reader)? else {
                    return Ok(None);
                };
                if line.trim().is_empty() {
                    break;
                }
                if line.trim().starts_with("Content-Length:") {
                    length = Some(parse_content_length(line.trim())?);
                }
            }
            read_body(
                reader,
                length.ok_or_else(|| "missing Content-Length header".to_string())?,
            )
            .map(Some)
        }
    }
}

/// Write one response using the session's negotiated framing.
fn write_response<W: Write>(writer: &mut W, value: &Value, framing: Framing) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if framing == Framing::ContentLength {
        write!(writer, "Content-Length: {}\r\n\r\n", body.len())
            .map_err(|error| error.to_string())?;
    }
    writer.write_all(&body).map_err(|error| error.to_string())?;
    if framing == Framing::Ndjson {
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

/// Serve MCP requests over arbitrary buffered streams until input reaches EOF.
pub fn serve<R: BufRead, W: Write>(
    db: &Database,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    let Some((first, framing)) = read_first(reader)? else {
        return Ok(());
    };
    if let Some(response) = handle_jsonrpc(db, first) {
        write_response(writer, &response, framing)?;
    }
    loop {
        match read_next(reader, framing) {
            Ok(Some(request)) => {
                if let Some(response) = handle_jsonrpc(db, request) {
                    write_response(writer, &response, framing)?;
                }
            }
            Ok(None) => break,
            Err(error) if !error.contains("exceeds max") => {
                write_response(writer, &rpc_error(Value::Null, -32700, error), framing)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Serve the local MCP protocol over locked process stdin and stdout.
pub fn serve_stdio(db: &Database) -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(db, &mut BufReader::new(stdin.lock()), &mut stdout.lock())
}

#[cfg(test)]
/// Protocol and shared-database tests for the local MCP surface.
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Build a JSON-RPC tools/call request.
    fn request(id: i64, name: &str, arguments: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
    }

    /// Extract the structured result from a successful JSON-RPC response.
    fn structured(response: Value) -> Value {
        response["result"]["structuredContent"].clone()
    }

    /// Initialization and tool discovery advertise the Fluency operations.
    #[test]
    fn initializes_and_lists_fluency_tools() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        let response =
            handle_jsonrpc(&db, json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).unwrap();
        let names: Vec<&str> = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(names.contains(&"checkpoint"));
        assert!(names.contains(&"review"));
        assert!(names.contains(&"spec_task"));
    }

    /// Spec creation, checkpoint emission, and review share one local database.
    #[test]
    fn emits_and_reviews_from_one_database() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        let spec = structured(handle_jsonrpc(&db, request(1, "spec_task", json!({
            "task_description":"Teach a local MCP slice", "task_type":"feature",
            "acceptance_criteria":["MCP works", "Fluency emits"],
            "interface_contract":"Local stdio MCP", "edge_cases":["EOF", "bad JSON", "hollow prose"]
        }))).unwrap());
        let spec_id = spec["id"].as_str().unwrap();
        let checkpoint = structured(handle_jsonrpc(&db, request(2, "checkpoint", json!({
            "name":"local-mcp-test", "spec_id":spec_id, "intent":"Prove local state continuity",
            "components":["The local server retains one database across MCP calls."],
            "conditions":["Fluency must be compiled in."], "repo_root":dir.path()
        }))).unwrap());
        assert_eq!(checkpoint["success"], true);
        let slice_path = checkpoint["data"]["slice_path"].as_str().unwrap();
        let review = structured(
            handle_jsonrpc(
                &db,
                request(
                    3,
                    "review",
                    json!({
                        "spec_id":spec_id, "repo_root":dir.path()
                    }),
                ),
            )
            .unwrap(),
        );
        assert_eq!(review["success"], true);
        assert!(std::path::Path::new(slice_path).is_file());
        assert!(dir
            .path()
            .join("docs/agent-forge/work/teach-a-local-mcp-slice/record.md")
            .is_file());
    }

    /// Unknown methods and tools return errors without poisoning later calls.
    #[test]
    fn errors_do_not_terminate_dispatch() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        let method =
            handle_jsonrpc(&db, json!({"jsonrpc":"2.0","id":1,"method":"missing"})).unwrap();
        assert_eq!(method["error"]["code"], -32601);
        let tool = handle_jsonrpc(&db, request(2, "missing", json!({}))).unwrap();
        assert_eq!(tool["result"]["isError"], true);
        let ping = handle_jsonrpc(&db, json!({"jsonrpc":"2.0","id":3,"method":"ping"})).unwrap();
        assert!(ping.get("result").is_some());
    }

    /// NDJSON transport processes notifications and subsequent requests.
    #[test]
    fn serves_ndjson_until_eof() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
        let mut reader = BufReader::new(&input[..]);
        let mut output = Vec::new();
        serve(&db, &mut reader, &mut output).unwrap();
        let lines: Vec<&str> = std::str::from_utf8(&output).unwrap().lines().collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap()["id"], 1);
    }

    /// A malformed NDJSON request reports a parse error and the stream continues.
    #[test]
    fn malformed_ndjson_does_not_terminate_stream() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n{broken}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n";
        let mut reader = BufReader::new(&input[..]);
        let mut output = Vec::new();
        serve(&db, &mut reader, &mut output).unwrap();
        let responses: Vec<Value> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[1]["error"]["code"], -32700);
        assert_eq!(responses[2]["id"], 2);
    }

    /// Content-Length framing is detected and preserved in the response.
    #[test]
    fn serves_content_length_framing() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        let body = b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}";
        let input = format!(
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).unwrap()
        );
        let mut reader = BufReader::new(input.as_bytes());
        let mut output = Vec::new();
        serve(&db, &mut reader, &mut output).unwrap();
        let rendered = std::str::from_utf8(&output).unwrap();
        let (_, response_body) = rendered.split_once("\r\n\r\n").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(response_body).unwrap()["id"],
            7
        );
    }
}
