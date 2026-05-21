use crate::{handle_jsonrpc, App};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};

// SECURITY (SEC-C4): cap message size to prevent OOM via a malicious
// unbounded line.
const MAX_MCP_MSG_SIZE: usize = 10 * 1024 * 1024;

/// Reads one JSON-RPC message delimited by a single newline, per the MCP
/// stdio transport spec
/// (https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#stdio).
/// Returns `Ok(None)` on EOF. Blank lines are skipped defensively.
fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>, String> {
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        if line.len() > MAX_MCP_MSG_SIZE {
            return Err(format!(
                "message size {} exceeds max {}",
                line.len(),
                MAX_MCP_MSG_SIZE
            ));
        }
        let trimmed = line.trim_end_matches(|c| c == '\n' || c == '\r');
        if trimmed.is_empty() {
            continue;
        }
        let value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
        return Ok(Some(value));
    }
}

/// Writes one JSON-RPC message terminated by a single newline. Messages MUST
/// NOT contain embedded newlines (serde_json compact encoding guarantees this).
fn write_message<W: Write>(writer: &mut W, value: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    writer.write_all(&body).map_err(|e| e.to_string())?;
    writer.write_all(b"\n").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Runs the stdio JSON-RPC loop against the given `App` until EOF.
pub async fn serve(app: App) -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(message) = read_message(&mut reader)? {
        if let Some(response) = handle_jsonrpc(&app, message).await {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn read_message_parses_one_line() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n";
        let mut reader = Cursor::new(&input[..]);
        let v = read_message(&mut reader)
            .expect("read_message ok")
            .expect("not eof");
        assert_eq!(v["method"], "initialize");
        assert_eq!(v["id"], 1);
    }

    #[test]
    fn read_message_tolerates_crlf() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n";
        let mut reader = Cursor::new(&input[..]);
        let v = read_message(&mut reader)
            .expect("ok")
            .expect("not eof");
        assert_eq!(v["method"], "ping");
    }

    #[test]
    fn read_message_skips_blank_lines() {
        let input = b"\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
        let mut reader = Cursor::new(&input[..]);
        let v = read_message(&mut reader)
            .expect("ok")
            .expect("not eof");
        assert_eq!(v["method"], "tools/list");
    }

    #[test]
    fn read_message_returns_none_on_eof() {
        let input: &[u8] = b"";
        let mut reader = Cursor::new(input);
        let v = read_message(&mut reader).expect("ok");
        assert!(v.is_none());
    }

    #[test]
    fn read_message_returns_none_after_trailing_blank() {
        // A lone newline is a blank line: the loop skips it, then read_line
        // returns 0 (EOF), so we yield Ok(None).
        let input: &[u8] = b"\n";
        let mut reader = Cursor::new(input);
        let v = read_message(&mut reader).expect("ok");
        assert!(v.is_none());
    }

    #[test]
    fn read_message_rejects_oversized_line() {
        // Build a single line that exceeds the cap on raw byte length.
        // The line is a JSON string of 'a's wrapped in quotes; valid JSON
        // structure isn't relevant because the cap check fires first.
        let mut payload = Vec::with_capacity(MAX_MCP_MSG_SIZE + 16);
        payload.push(b'"');
        payload.resize(MAX_MCP_MSG_SIZE + 1, b'a');
        payload.extend_from_slice(b"\"\n");
        let mut reader = Cursor::new(payload);
        let err = read_message(&mut reader).expect_err("must reject");
        assert!(err.contains("exceeds max"), "got: {err}");
    }

    #[test]
    fn write_message_emits_compact_json_plus_newline() {
        let mut buf = Vec::new();
        let v = json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}});
        write_message(&mut buf, &v).unwrap();
        // Compact encoding => no embedded newlines in body. Trailing byte is \n.
        assert_eq!(*buf.last().unwrap(), b'\n');
        // Exactly one '\n' (the framing terminator).
        assert_eq!(buf.iter().filter(|&&b| b == b'\n').count(), 1);
        // No CR -- MCP spec mandates LF-only delimitation.
        assert!(!buf.contains(&b'\r'));
        // Body roundtrips as JSON.
        let body = &buf[..buf.len() - 1];
        let back: Value = serde_json::from_slice(body).unwrap();
        assert_eq!(back["id"], 1);
    }

    #[test]
    fn round_trip_write_then_read() {
        let mut buf = Vec::new();
        let v_out = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {"name": "memory.search"}
        });
        write_message(&mut buf, &v_out).unwrap();
        let mut reader = Cursor::new(buf);
        let v_in = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(v_in, v_out);
        // Subsequent read = EOF.
        assert!(read_message(&mut reader).unwrap().is_none());
    }
}
