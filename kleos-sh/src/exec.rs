use std::process::Stdio;
use tokio::process::Command;

pub struct ExecResult {
    pub exit_code: i32,
}

pub async fn run_command(command: &str) -> Result<ExecResult, String> {
    #[cfg(unix)]
    let (shell, flag): (&str, &str) = ("/bin/sh", "-c");
    #[cfg(not(unix))]
    let (shell, flag): (&str, &str) = ("cmd", "/C");

    let mut child = Command::new(shell)
        .arg(flag)
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn shell: {}", e))?;

    let status = child
        .wait()
        .await
        .map_err(|e| format!("failed to wait on child: {}", e))?;

    Ok(ExecResult {
        exit_code: status.code().unwrap_or(1),
    })
}
