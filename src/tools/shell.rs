/// Shell execution tool — runs commands in a subprocess with timeout.

use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Maximum output we'll capture (64KB)
const MAX_OUTPUT_BYTES: usize = 65536;
/// Execution timeout
const EXEC_TIMEOUT_SECS: u64 = 30;

pub async fn execute(params: &Value, cwd: &Path) -> Result<String> {
    let command = params.get("command").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("shell: 'command' required"))?;

    let working_dir: PathBuf = params.get("working_dir")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .unwrap_or_else(|| cwd.to_path_buf());

    // Build command
    let (shell, shell_arg) = if cfg!(windows) {
        ("cmd.exe", "/C")
    } else {
        ("sh", "-c")
    };

    let child = Command::new(shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(&working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let output = timeout(
        Duration::from_secs(EXEC_TIMEOUT_SECS),
        child.wait_with_output(),
    ).await
    .map_err(|_| anyhow::anyhow!("Command timed out after {}s", EXEC_TIMEOUT_SECS))??;

    let stdout = strip_ansi(&truncate_bytes(&output.stdout, MAX_OUTPUT_BYTES));
    let stderr = strip_ansi(&truncate_bytes(&output.stderr, MAX_OUTPUT_BYTES / 4));
    let exit_code = output.status.code().unwrap_or(-1);

    let mut result = String::new();
    result.push_str(&format!("$ {}\n", command));
    if !stdout.is_empty() { result.push_str(&stdout); result.push('\n'); }
    if !stderr.is_empty() {
        result.push_str(&format!("stderr:\n{}\n", stderr));
    }
    result.push_str(&format!("exit code: {}", exit_code));

    if exit_code != 0 {
        anyhow::bail!("{}", result);
    }

    Ok(result)
}

fn truncate_bytes(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.len() > max {
        format!("{}… (truncated {} bytes)", &s[..max], s.len() - max)
    } else {
        s.into_owned()
    }
}

fn strip_ansi(s: &str) -> String {
    let stripped = strip_ansi_escapes::strip(s.as_bytes());
    String::from_utf8(stripped).unwrap_or_else(|_| s.to_string())
}
