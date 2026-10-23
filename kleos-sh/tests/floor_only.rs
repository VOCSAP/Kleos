use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn floor_only_emits_a_deny_envelope_for_a_denied_command() {
    let fixture_dir = std::env::temp_dir().join(format!("kleos-sh-floor-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("fixture directory");
    let policy_path = fixture_dir.join("floor.toml");
    fs::write(
        &policy_path,
        "schema = 1\n\
         [[deny]]\n\
         id = \"rm-recursive\"\n\
         verb = \"rm\"\n\
         flags_any = [\"recursive\"]\n\
         [[deny]]\n\
         id = \"git-rewrite\"\n\
         verb = \"git push\"\n\
         flags_any = [\"force\", \"delete\"]\n\
         [[deny]]\n\
         id = \"git-reset-hard\"\n\
         verb = \"git reset\"\n\
         flags_any = [\"hard\"]\n\
         [[deny]]\n\
         id = \"git-clean-force\"\n\
         verb = \"git clean\"\n\
         flags_any = [\"force\"]\n",
    )
    .expect("policy");

    let mut child = Command::new(env!("CARGO_BIN_EXE_kleos-sh"))
        .args([
            "--floor-only",
            "--floor-file",
            policy_path.to_str().expect("UTF-8 path"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("floor-only process");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(br#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/test"}}"#)
        .expect("payload");

    let output = child.wait_with_output().expect("floor-only output");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(output.status.success());
    let decision: serde_json::Value = serde_json::from_slice(&output.stdout).expect("deny JSON");
    assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        decision["hookSpecificOutput"]["permissionDecisionReason"],
        "floor: rm-recursive"
    );
}

#[test]
fn floor_only_rejects_excessive_raw_structure() {
    let fixture_dir = std::env::temp_dir().join(format!("kleos-sh-floor-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("fixture directory");
    let policy_path = fixture_dir.join("floor.toml");
    fs::write(
        &policy_path,
        "schema = 1\n[[deny]]\nid = \"rm-recursive\"\nverb = \"rm\"\nflags_any = [\"recursive\"]\n[[deny]]\nid = \"git-rewrite\"\nverb = \"git push\"\nflags_any = [\"force\", \"delete\"]\n[[deny]]\nid = \"git-reset-hard\"\nverb = \"git reset\"\nflags_any = [\"hard\"]\n[[deny]]\nid = \"git-clean-force\"\nverb = \"git clean\"\nflags_any = [\"force\"]\n",
    )
    .expect("policy");
    let commands = [
        "{".repeat(300),
        "if ".repeat(200),
        "$(".repeat(40_000),
        "(".repeat(20_000),
        "x".repeat(200 * 1024),
        format!("cat <<'EOF'\n{}\nEOF", "$(".repeat(22)),
    ];
    for command in commands {
        let payload = serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": command },
        });
        let mut child = Command::new(env!("CARGO_BIN_EXE_kleos-sh"))
            .args([
                "--floor-only",
                "--floor-file",
                policy_path.to_str().expect("UTF-8 path"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("floor-only process");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("payload");
        let output = child.wait_with_output().expect("floor-only output");
        assert!(output.status.success());
        let decision: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("deny JSON");
        assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
    }
    let _ = fs::remove_dir_all(&fixture_dir);
}
