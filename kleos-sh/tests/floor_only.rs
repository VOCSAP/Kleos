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
