/// Git tools — read-only, no permission required.

use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

pub fn status(cwd: &PathBuf) -> Result<String> {
    let out = Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(cwd)
        .output()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if s.is_empty() { "nothing to commit, working tree clean".to_string() } else { s })
    } else {
        let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
        anyhow::bail!("git status failed: {}", e)
    }
}

pub fn log(params: &Value, cwd: &PathBuf) -> Result<String> {
    let n    = params["n"].as_u64().unwrap_or(20).min(100);
    let path = params["path"].as_str();
    let n_flag = format!("-{}", n);

    let mut args = vec!["log", "--oneline", "--decorate", &n_flag];
    let path_str: String;
    if let Some(p) = path {
        args.push("--");
        path_str = p.to_string();
        args.push(&path_str);
    }

    let out = Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .output()?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if s.is_empty() { "No commits found.".to_string() } else { s })
    } else {
        let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
        anyhow::bail!("git log failed: {}", e)
    }
}

pub fn diff(params: &Value, cwd: &PathBuf) -> Result<String> {
    let staged = params["staged"].as_bool().unwrap_or(false);
    let path   = params["path"].as_str();

    let mut args: Vec<&str> = vec!["diff"];
    if staged { args.push("--staged"); }
    args.push("--stat");

    let path_str;
    if let Some(p) = path {
        args.push("--");
        path_str = p.to_string();
        args.push(&path_str);
    }

    let out = Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .output()?;

    if out.status.success() {
        // Also grab the actual diff (limited)
        let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();

        let mut full_args: Vec<&str> = vec!["diff"];
        if staged { full_args.push("--staged"); }
        if let Some(p) = path {
            full_args.push("--");
            full_args.push(p);
        }

        let full_out = Command::new("git")
            .args(&full_args)
            .current_dir(cwd)
            .output()?;

        let full = String::from_utf8_lossy(&full_out.stdout);
        // Cap at 200 lines
        let lines: Vec<&str> = full.lines().take(200).collect();
        let diff_text = lines.join("\n");
        let truncated = if full.lines().count() > 200 {
            format!("{}\n\n… (truncated, use read_file for full content)", diff_text)
        } else {
            diff_text
        };

        if stat.is_empty() && truncated.trim().is_empty() {
            Ok("No changes.".to_string())
        } else {
            Ok(format!("{}\n\n{}", stat, truncated).trim().to_string())
        }
    } else {
        let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
        anyhow::bail!("git diff failed: {}", e)
    }
}

pub fn blame(params: &Value, cwd: &PathBuf) -> Result<String> {
    let path = params["path"].as_str()
        .ok_or_else(|| anyhow::anyhow!("path is required"))?;

    let out = Command::new("git")
        .args(["blame", "--date=short", path])
        .current_dir(cwd)
        .output()?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout);
        // Cap at 100 lines
        let lines: Vec<&str> = s.lines().take(100).collect();
        Ok(lines.join("\n"))
    } else {
        let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
        anyhow::bail!("git blame failed: {}", e)
    }
}
