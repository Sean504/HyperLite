/// Shell execution tool — runs commands in a subprocess with timeout.
/// In sandbox mode, wraps via bubblewrap (bwrap) for filesystem isolation.

use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const MAX_OUTPUT_BYTES: usize = 65536;
const EXEC_TIMEOUT_SECS: u64 = 30;

pub async fn execute(params: &Value, cwd: &Path) -> Result<String> {
    execute_with_sandbox(params, cwd, false).await
}

pub async fn execute_sandboxed(params: &Value, cwd: &Path) -> Result<String> {
    execute_with_sandbox(params, cwd, true).await
}

async fn execute_with_sandbox(params: &Value, cwd: &Path, sandbox: bool) -> Result<String> {
    let command = params.get("command").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("shell: 'command' required"))?;

    let working_dir: PathBuf = params.get("working_dir")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .unwrap_or_else(|| cwd.to_path_buf());

    let child = if sandbox && bwrap_available() {
        build_sandboxed_command(command, &working_dir)?
    } else {
        build_direct_command(command, &working_dir)
    };

    let output = timeout(
        Duration::from_secs(EXEC_TIMEOUT_SECS),
        child.wait_with_output(),
    ).await
    .map_err(|_| anyhow::anyhow!("Command timed out after {}s", EXEC_TIMEOUT_SECS))??;

    let stdout = strip_ansi(&truncate_bytes(&output.stdout, MAX_OUTPUT_BYTES));
    let stderr = strip_ansi(&truncate_bytes(&output.stderr, MAX_OUTPUT_BYTES / 4));
    let exit_code = output.status.code().unwrap_or(-1);

    let sandbox_tag = if sandbox && bwrap_available() { " [sandboxed]" } else { "" };
    let mut result = format!("$ {}{}\n", command, sandbox_tag);
    if !stdout.is_empty() { result.push_str(&stdout); result.push('\n'); }
    if !stderr.is_empty() { result.push_str(&format!("stderr:\n{}\n", stderr)); }
    result.push_str(&format!("exit code: {}", exit_code));

    if exit_code != 0 {
        anyhow::bail!("{}", result);
    }

    Ok(result)
}

fn build_direct_command(command: &str, working_dir: &Path) -> tokio::process::Child {
    let (shell, shell_arg) = if cfg!(windows) { ("cmd.exe", "/C") } else { ("sh", "-c") };
    Command::new(shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn shell")
}

fn build_sandboxed_command(command: &str, working_dir: &Path) -> Result<tokio::process::Child> {
    // bubblewrap sandbox:
    // - system dirs (usr, lib, bin) are read-only
    // - working directory is bind-mounted read-write so file tools still work
    // - /home, /root, /tmp get fresh tmpfs (changes don't escape)
    // - /proc and /dev are available for normal command operation
    let wd = working_dir.to_string_lossy();

    let mut cmd = Command::new("bwrap");
    cmd
        // Read-only system
        .args(["--ro-bind", "/usr", "/usr"])
        .args(["--ro-bind", "/lib", "/lib"])
        .args(["--ro-bind-try", "/lib64", "/lib64"])
        .args(["--ro-bind", "/bin", "/bin"])
        .args(["--ro-bind-try", "/sbin", "/sbin"])
        .args(["--ro-bind-try", "/etc/resolv.conf", "/etc/resolv.conf"])
        .args(["--ro-bind-try", "/etc/passwd", "/etc/passwd"])
        // Isolated home and tmp
        .args(["--tmpfs", "/tmp"])
        .args(["--tmpfs", "/home"])
        .args(["--tmpfs", "/root"])
        // Working dir writable
        .args(["--bind", &wd, &wd])
        // Process and device access
        .args(["--proc", "/proc"])
        .args(["--dev", "/dev"])
        // Run in working dir
        .args(["--chdir", &wd])
        // The command
        .args(["--", "sh", "-c", command])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    Ok(cmd.spawn()?)
}

/// Check if bwrap is available on this system.
pub fn bwrap_available() -> bool {
    which::which("bwrap").is_ok()
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
