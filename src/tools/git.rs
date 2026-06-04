/// Git tools — read-only and write operations with auth support.

use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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

// ── Write operations ──────────────────────────────────────────────────────────

pub fn add(params: &Value, cwd: &PathBuf) -> Result<String> {
    let path = params["path"].as_str().unwrap_or(".");
    let out = Command::new("git")
        .args(["add", path])
        .current_dir(cwd)
        .output()?;
    if out.status.success() {
        Ok(format!("Staged: {}", path))
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

pub fn commit(params: &Value, cwd: &PathBuf) -> Result<String> {
    let message = params["message"].as_str()
        .ok_or_else(|| anyhow::anyhow!("git_commit: 'message' required"))?;
    let out = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(cwd)
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

pub fn push(params: &Value, cwd: &PathBuf) -> Result<String> {
    let remote = params["remote"].as_str().unwrap_or("origin");
    let branch = params["branch"].as_str();

    let mut args = vec!["push", remote];
    let branch_owned: String;
    if let Some(b) = branch {
        branch_owned = b.to_string();
        args.push(&branch_owned);
    }

    let out = Command::new("git").args(&args).current_dir(cwd).output()?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stderr).trim().to_string(); // git push → stderr
        Ok(if s.is_empty() { "Push successful.".to_string() } else { s })
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if is_ssh_auth_error(&stderr) {
            anyhow::bail!(
                "Push failed — SSH key not accepted.\n\
                 Make sure your SSH key is added to GitHub (Settings → SSH and GPG keys).\n\
                 Run `ssh-keygen -t ed25519` to create one if you don't have one yet.\n\
                 Error: {}", stderr
            )
        }
        if is_https_auth_error(&stderr) {
            let host = get_remote_host(cwd);
            anyhow::bail!(
                "[AUTH_REQUIRED:{}] git push failed — authentication needed. \
                 A token setup dialog has been opened. Once you save your token, \
                 please retry.", host
            )
        }
        anyhow::bail!("{}", stderr)
    }
}

pub fn pull(params: &Value, cwd: &PathBuf) -> Result<String> {
    let remote = params["remote"].as_str().unwrap_or("origin");
    let branch = params["branch"].as_str();

    let mut args = vec!["pull", remote];
    let branch_owned: String;
    if let Some(b) = branch {
        branch_owned = b.to_string();
        args.push(&branch_owned);
    }

    let out = Command::new("git").args(&args).current_dir(cwd).output()?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if s.is_empty() { "Already up to date.".to_string() } else { s })
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if is_ssh_auth_error(&stderr) {
            anyhow::bail!(
                "Pull failed — SSH key not accepted.\n\
                 Make sure your SSH key is added to GitHub (Settings → SSH and GPG keys).\n\
                 Error: {}", stderr
            )
        }
        if is_https_auth_error(&stderr) {
            let host = get_remote_host(cwd);
            anyhow::bail!(
                "[AUTH_REQUIRED:{}] git pull failed — authentication needed. \
                 A token setup dialog has been opened. Once you save your token, \
                 please retry.", host
            )
        }
        anyhow::bail!("{}", stderr)
    }
}

pub fn branch_op(params: &Value, cwd: &PathBuf) -> Result<String> {
    let action = params["action"].as_str().unwrap_or("list");
    let name   = params["name"].as_str();

    match action {
        "list" => {
            let out = Command::new("git").args(["branch", "-a"]).current_dir(cwd).output()?;
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                Ok(if s.is_empty() { "No branches found.".to_string() } else { s })
            } else {
                anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
            }
        }
        "create" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("git_branch create: 'name' required"))?;
            let out = Command::new("git").args(["checkout", "-b", n]).current_dir(cwd).output()?;
            if out.status.success() {
                Ok(format!("Created and switched to branch '{}'", n))
            } else {
                anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
            }
        }
        "switch" | "checkout" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("git_branch switch: 'name' required"))?;
            let out = Command::new("git").args(["checkout", n]).current_dir(cwd).output()?;
            if out.status.success() {
                Ok(format!("Switched to branch '{}'", n))
            } else {
                anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
            }
        }
        "delete" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("git_branch delete: 'name' required"))?;
            let out = Command::new("git").args(["branch", "-d", n]).current_dir(cwd).output()?;
            if out.status.success() {
                Ok(format!("Deleted branch '{}'", n))
            } else {
                anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
            }
        }
        _ => anyhow::bail!("git_branch: unknown action '{}'. Use: list, create, switch, delete.", action),
    }
}

pub fn stash(params: &Value, cwd: &PathBuf) -> Result<String> {
    let action  = params["action"].as_str().unwrap_or("push");
    let message = params["message"].as_str();

    let out = match action {
        "push" => {
            if let Some(m) = message {
                Command::new("git").args(["stash", "push", "-m", m]).current_dir(cwd).output()?
            } else {
                Command::new("git").args(["stash", "push"]).current_dir(cwd).output()?
            }
        }
        "pop"  => Command::new("git").args(["stash", "pop"]).current_dir(cwd).output()?,
        "list" => Command::new("git").args(["stash", "list"]).current_dir(cwd).output()?,
        "drop" => Command::new("git").args(["stash", "drop"]).current_dir(cwd).output()?,
        _ => anyhow::bail!("git_stash: unknown action '{}'. Use: push, pop, list, drop.", action),
    };

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if s.is_empty() { format!("git stash {} complete.", action) } else { s })
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

fn is_https_auth_error(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    s.contains("authentication failed")
        || s.contains("invalid username")
        || s.contains("could not read username")
        || s.contains("could not read password")
        || s.contains("access denied")
        || (s.contains("the requested url returned error: 403"))
        || (s.contains("the requested url returned error: 401"))
}

fn is_ssh_auth_error(stderr: &str) -> bool {
    stderr.to_lowercase().contains("permission denied (publickey)")
}

fn get_remote_host(cwd: &PathBuf) -> String {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .output()
        .ok();
    if let Some(out) = out {
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(host) = parse_git_url_host(&url) {
            return host;
        }
    }
    "github.com".to_string()
}

fn parse_git_url_host(url: &str) -> Option<String> {
    if url.starts_with("https://") || url.starts_with("http://") {
        let without_scheme = url.trim_start_matches("https://").trim_start_matches("http://");
        let host = without_scheme.split('/').next()?;
        Some(host.to_string())
    } else if url.contains('@') && url.contains(':') {
        // git@github.com:user/repo.git
        let after_at = url.split('@').nth(1)?;
        Some(after_at.split(':').next()?.to_string())
    } else {
        None
    }
}

/// Store a personal access token via git's credential system.
/// Automatically configures `credential.helper=store` if no helper is set,
/// so it works on fresh Linux/WSL installs without any user setup.
pub fn store_git_token(host: &str, token: &str) -> anyhow::Result<()> {
    // Ensure a credential helper is configured
    let helper_out = Command::new("git")
        .args(["config", "--global", "credential.helper"])
        .output()?;
    if String::from_utf8_lossy(&helper_out.stdout).trim().is_empty() {
        Command::new("git")
            .args(["config", "--global", "credential.helper", "store"])
            .output()?;
    }

    // Feed the token to git credential approve via stdin
    use std::io::Write;
    let mut child = Command::new("git")
        .args(["credential", "approve"])
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        write!(stdin, "protocol=https\nhost={}\nusername=token\npassword={}\n\n", host, token)?;
    }
    child.wait()?;
    Ok(())
}

// ── Read operations (existing) ────────────────────────────────────────────────

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

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// All tests create throwaway repos under the OS temp directory.
// No test touches the project repo or ~/.gitconfig.

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialise any test that mutates process-wide env vars (GIT_CONFIG_GLOBAL).
    // Rust runs tests in parallel by default; without this lock two tests could
    // clobber each other's env.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ── Repo factory helpers ──────────────────────────────────────────────────

    /// Spin up an isolated git repo in the OS temp dir.
    /// Uses a per-repo gitconfig so identity is never written to ~/.gitconfig.
    fn make_repo() -> (TempDir, PathBuf) {
        let dir  = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Force branch name to "main" for consistency across git versions
        Command::new("git")
            .args(["-c", "init.defaultBranch=main", "init"])
            .current_dir(&path).output().unwrap();

        // Identity scoped to this repo only (-c flags, not --global)
        Command::new("git")
            .args(["-c", "user.email=test@hyperlite.local",
                   "-c", "user.name=HyperLite Test",
                   "commit", "--allow-empty", "-m", "repo-init"])
            .current_dir(&path).output().unwrap();

        (dir, path)
    }

    /// Spin up a bare repo to act as a local "remote".
    /// No network involved — push/pull go to a path under /tmp.
    fn make_bare_remote() -> TempDir {
        let dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["-c", "init.defaultBranch=main", "init", "--bare"])
            .current_dir(dir.path()).output().unwrap();
        dir
    }

    /// Write a file, stage it, and commit — used to seed repos quickly.
    fn seed(path: &PathBuf, filename: &str, content: &str, message: &str) {
        fs::write(path.join(filename), content).unwrap();
        Command::new("git")
            .args(["-c", "user.email=test@hyperlite.local",
                   "-c", "user.name=HyperLite Test",
                   "add", "."])
            .current_dir(path).output().unwrap();
        Command::new("git")
            .args(["-c", "user.email=test@hyperlite.local",
                   "-c", "user.name=HyperLite Test",
                   "commit", "-m", message])
            .current_dir(path).output().unwrap();
    }

    // ── Pure-function tests (no disk I/O) ─────────────────────────────────────

    #[test]
    fn parse_https_url_github() {
        assert_eq!(
            parse_git_url_host("https://github.com/user/repo.git"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn parse_https_url_gitlab() {
        assert_eq!(
            parse_git_url_host("https://gitlab.com/group/repo"),
            Some("gitlab.com".to_string())
        );
    }

    #[test]
    fn parse_ssh_url() {
        assert_eq!(
            parse_git_url_host("git@github.com:user/repo.git"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn parse_invalid_url_returns_none() {
        assert_eq!(parse_git_url_host("not-a-url"), None);
        assert_eq!(parse_git_url_host(""), None);
    }

    #[test]
    fn https_auth_error_recognized() {
        let cases = [
            "fatal: Authentication failed for 'https://github.com/user/repo.git'",
            "remote: Invalid username or password.",
            "fatal: could not read Username for 'https://github.com'",
            "fatal: could not read Password for 'https://github.com'",
            "The requested URL returned error: 403",
            "The requested URL returned error: 401",
            "remote: Access denied",
        ];
        for case in &cases {
            assert!(is_https_auth_error(case), "should be auth error: {}", case);
        }
    }

    #[test]
    fn https_auth_error_not_triggered_by_unrelated_errors() {
        let cases = [
            "fatal: not a git repository",
            "error: failed to push some refs to 'origin'",
            "hint: Updates were rejected because the remote contains work",
        ];
        for case in &cases {
            assert!(!is_https_auth_error(case), "should NOT be auth error: {}", case);
        }
    }

    #[test]
    fn ssh_auth_error_recognized() {
        assert!(is_ssh_auth_error(
            "git@github.com: Permission denied (publickey)."
        ));
    }

    #[test]
    fn ssh_auth_error_not_triggered_by_https_failures() {
        assert!(!is_ssh_auth_error("fatal: Authentication failed"));
        assert!(!is_ssh_auth_error(""));
    }

    // ── status ────────────────────────────────────────────────────────────────

    #[test]
    fn status_clean_repo_after_init_commit() {
        let (_dir, path) = make_repo();
        let result = status(&path).unwrap();
        // Branch header line is always present; nothing staged
        assert!(result.contains("##") || result.contains("nothing"));
    }

    #[test]
    fn status_shows_untracked_file() {
        let (_dir, path) = make_repo();
        fs::write(path.join("untracked.txt"), "hi").unwrap();
        let result = status(&path).unwrap();
        assert!(result.contains("untracked.txt"));
    }

    // ── add ───────────────────────────────────────────────────────────────────

    #[test]
    fn add_specific_file() {
        let (_dir, path) = make_repo();
        fs::write(path.join("foo.txt"), "content").unwrap();
        let r = add(&serde_json::json!({"path": "foo.txt"}), &path).unwrap();
        assert!(r.contains("foo.txt"));
    }

    #[test]
    fn add_dot_stages_all() {
        let (_dir, path) = make_repo();
        fs::write(path.join("a.txt"), "a").unwrap();
        fs::write(path.join("b.txt"), "b").unwrap();
        add(&serde_json::json!({"path": "."}), &path).unwrap();
        // Check staging area via git diff --cached
        let staged = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(&path).output().unwrap();
        let out = String::from_utf8_lossy(&staged.stdout);
        assert!(out.contains("a.txt") && out.contains("b.txt"));
    }

    // ── commit ────────────────────────────────────────────────────────────────

    #[test]
    fn commit_appears_in_log() {
        let (_dir, path) = make_repo();
        seed(&path, "hello.txt", "hello", "my special commit");
        let log_out = log(&serde_json::json!({"n": 5}), &path).unwrap();
        assert!(log_out.contains("my special commit"));
    }

    #[test]
    fn commit_without_message_returns_error() {
        let (_dir, path) = make_repo();
        fs::write(path.join("x.txt"), "x").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        assert!(commit(&serde_json::json!({}), &path).is_err());
    }

    // ── branch ────────────────────────────────────────────────────────────────

    #[test]
    fn branch_list_shows_main() {
        let (_dir, path) = make_repo();
        let r = branch_op(&serde_json::json!({"action": "list"}), &path).unwrap();
        assert!(r.contains("main"));
    }

    #[test]
    fn branch_create_and_switch() {
        let (_dir, path) = make_repo();
        let create = branch_op(
            &serde_json::json!({"action": "create", "name": "feature-abc"}),
            &path,
        ).unwrap();
        assert!(create.contains("feature-abc"));

        let switch = branch_op(
            &serde_json::json!({"action": "switch", "name": "main"}),
            &path,
        ).unwrap();
        assert!(switch.contains("main"));
    }

    #[test]
    fn branch_delete() {
        let (_dir, path) = make_repo();
        // Create a branch, then delete it
        branch_op(&serde_json::json!({"action": "create", "name": "temp-branch"}), &path).unwrap();
        branch_op(&serde_json::json!({"action": "switch", "name": "main"}), &path).unwrap();
        let del = branch_op(
            &serde_json::json!({"action": "delete", "name": "temp-branch"}),
            &path,
        ).unwrap();
        assert!(del.contains("temp-branch"));
    }

    #[test]
    fn branch_unknown_action_returns_error() {
        let (_dir, path) = make_repo();
        assert!(branch_op(&serde_json::json!({"action": "explode"}), &path).is_err());
    }

    // ── stash ─────────────────────────────────────────────────────────────────

    #[test]
    fn stash_push_and_pop_roundtrip() {
        let (_dir, path) = make_repo();
        seed(&path, "base.txt", "original", "base commit");

        // Modify the file without committing
        fs::write(path.join("base.txt"), "in-progress work").unwrap();

        stash(&serde_json::json!({"action": "push", "message": "wip"}), &path).unwrap();

        // File should now be back to original after stash push
        let content = fs::read_to_string(path.join("base.txt")).unwrap();
        assert_eq!(content.trim(), "original");

        stash(&serde_json::json!({"action": "pop"}), &path).unwrap();

        // File should be restored after stash pop
        let content = fs::read_to_string(path.join("base.txt")).unwrap();
        assert_eq!(content.trim(), "in-progress work");
    }

    #[test]
    fn stash_list_empty_returns_ok() {
        let (_dir, path) = make_repo();
        let r = stash(&serde_json::json!({"action": "list"}), &path).unwrap();
        // Empty stash list produces empty output or a "complete" placeholder
        assert!(r.is_empty() || r.contains("complete"));
    }

    // ── push / pull — LOCAL bare remote, zero network ─────────────────────────

    #[test]
    fn push_to_local_bare_remote() {
        // Both repos live in /tmp — completely isolated from the project repo
        let remote = make_bare_remote();
        let (work_dir, work_path) = make_repo();

        Command::new("git")
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .current_dir(&work_path).output().unwrap();

        seed(&work_path, "hello.txt", "hello world", "initial commit");

        let result = push(
            &serde_json::json!({"remote": "origin", "branch": "main"}),
            &work_path,
        );
        assert!(result.is_ok(), "push should succeed: {:?}", result.err());

        // Keep TempDirs alive until the assertion is done
        drop(work_dir);
        drop(remote);
    }

    #[test]
    fn pull_from_local_bare_remote() {
        let remote = make_bare_remote();

        // Repo 1: push a commit to the bare remote
        let (repo1_dir, repo1_path) = make_repo();
        Command::new("git")
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .current_dir(&repo1_path).output().unwrap();
        seed(&repo1_path, "shared.txt", "shared content", "shared commit");
        Command::new("git")
            .args(["push", "origin", "main"])
            .current_dir(&repo1_path).output().unwrap();

        // Repo 2: clone from the bare remote, then pull via our tool
        let repo2_dir = TempDir::new().unwrap();
        let repo2_path = repo2_dir.path().to_path_buf();
        Command::new("git")
            .args(["clone", remote.path().to_str().unwrap(), repo2_path.to_str().unwrap()])
            .output().unwrap();

        let result = pull(
            &serde_json::json!({"remote": "origin"}),
            &repo2_path,
        );
        assert!(result.is_ok(), "pull should succeed: {:?}", result.err());

        drop(repo1_dir);
        drop(repo2_dir);
        drop(remote);
    }

    #[test]
    fn push_to_nonexistent_remote_is_error_not_auth_error() {
        let (_dir, path) = make_repo();
        seed(&path, "f.txt", "x", "commit");
        // No remote added — push should fail with a plain error, not AUTH_REQUIRED
        let result = push(&serde_json::json!({"remote": "origin", "branch": "main"}), &path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(!msg.contains("[AUTH_REQUIRED:"), "should not trigger auth dialog for missing remote: {}", msg);
    }

    // ── store_git_token — isolated from ~/.gitconfig ──────────────────────────

    #[test]
    fn store_git_token_writes_to_isolated_config() {
        // Hold the env lock so no other test interferes with GIT_CONFIG_GLOBAL
        let _guard = ENV_LOCK.lock().unwrap();

        let config_dir = TempDir::new().unwrap();
        let creds_file  = config_dir.path().join("git-credentials");
        let config_file = config_dir.path().join("gitconfig");

        // Pre-configure a credential helper that writes to our temp credentials file
        // so git credential approve never touches ~/.git-credentials
        fs::write(
            &config_file,
            format!("[credential]\n\thelper = store --file {}\n", creds_file.display()),
        ).unwrap();

        // Redirect git's global config to our temp file for the duration of this test
        std::env::set_var("GIT_CONFIG_GLOBAL", config_file.to_str().unwrap());
        let result = store_git_token("github.com", "ghp_test_fake_token_abc123");
        std::env::remove_var("GIT_CONFIG_GLOBAL");

        assert!(result.is_ok(), "store_git_token failed: {:?}", result.err());

        // The token should now be in our temp credentials file, not ~/.git-credentials
        let creds = fs::read_to_string(&creds_file).unwrap_or_default();
        assert!(creds.contains("github.com"), "host should be in credentials file");
        assert!(creds.contains("ghp_test_fake_token_abc123"), "token should be stored");

        drop(config_dir);
    }
}
