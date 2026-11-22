//! Endpoints called directly by Claude Code hooks of `type: "http"`.
//!
//! Claude Code POSTs the hook event JSON and reads the answer in its hook
//! output format. A non-2xx status is a non-blocking error on its side, so
//! this module never uses error statuses to carry a decision: a decision is
//! always a 200 with a `hookSpecificOutput` body, and "nothing to say" is an
//! empty 200.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use rusqlite::params;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/hooks/claude/pre-tool-use", post(pre_tool_use_handler))
}

/// One claimed supervisor injection: (severity, rule_id, message).
type Injection = (String, String, String);

/// PreToolUse: drain the supervisor injections of the session, with the same
/// claim as `GET /supervisor/pending?wait=0`, and turn them into a hook answer.
async fn pre_tool_use_handler(
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
    Json(event): Json<Value>,
) -> Result<Response, AppError> {
    let Some(session_id) = event
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return Ok(StatusCode::OK.into_response());
    };

    let user_id = auth.effective_user_id();
    let claimed: Vec<Injection> = db
        .transaction(move |tx| {
            let mut stmt = tx
                .prepare(
                    "UPDATE supervisor_injections
                     SET claimed_at = datetime('now')
                     WHERE user_id = ?1 AND session_id = ?2 AND claimed_at IS NULL
                     RETURNING severity, rule_id, message",
                )
                .map_err(|e| kleos_lib::EngError::DatabaseMessage(e.to_string()))?;
            let rows = stmt
                .query_map(params![user_id, session_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .map_err(|e| kleos_lib::EngError::DatabaseMessage(e.to_string()))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| kleos_lib::EngError::DatabaseMessage(e.to_string()))?);
            }
            Ok(out)
        })
        .await?;

    Ok(match render(&claimed) {
        Some(body) => Json(body).into_response(),
        None => StatusCode::OK.into_response(),
    })
}

/// Maps claimed injections to a PreToolUse answer, `None` when there is
/// nothing to report. Any `critical` severity (any case) denies the call and
/// carries every line; otherwise the lines are added as context.
fn render(injections: &[Injection]) -> Option<Value> {
    if injections.is_empty() {
        return None;
    }
    let critical = injections
        .iter()
        .any(|(severity, _, _)| severity.eq_ignore_ascii_case("critical"));
    let lines = injections
        .iter()
        .map(|(severity, rule_id, message)| format!("[supervisor:{severity}] {rule_id}: {message}"))
        .collect::<Vec<_>>()
        .join("\n");
    let output = if critical {
        json!({
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!("eidolon-supervisor blocked the tool call:\n{lines}"),
        })
    } else {
        json!({
            "hookEventName": "PreToolUse",
            "additionalContext": lines,
        })
    };
    Some(json!({ "hookSpecificOutput": output }))
}

#[cfg(test)]
mod tests {
    use super::render;

    fn inj(severity: &str, rule: &str, message: &str) -> (String, String, String) {
        (severity.to_string(), rule.to_string(), message.to_string())
    }

    #[test]
    fn nothing_pending_renders_no_body() {
        assert!(render(&[]).is_none());
    }

    #[test]
    fn warnings_become_additional_context() {
        let v = render(&[inj("warning", "r1", "a"), inj("info", "r2", "b \"q\"\nc")]).unwrap();
        let h = &v["hookSpecificOutput"];
        assert_eq!(h["hookEventName"], "PreToolUse");
        assert!(h.get("permissionDecision").is_none());
        assert_eq!(
            h["additionalContext"],
            "[supervisor:warning] r1: a\n[supervisor:info] r2: b \"q\"\nc"
        );
    }

    #[test]
    fn any_critical_denies_with_every_line() {
        let v = render(&[inj("Warning", "r1", "a"), inj("CRITICAL", "r2", "b")]).unwrap();
        let h = &v["hookSpecificOutput"];
        assert_eq!(h["permissionDecision"], "deny");
        assert_eq!(
            h["permissionDecisionReason"],
            "eidolon-supervisor blocked the tool call:\n[supervisor:Warning] r1: a\n[supervisor:CRITICAL] r2: b"
        );
        assert!(h.get("additionalContext").is_none());
    }
}
