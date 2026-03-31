/// File tools: read, write, edit, list, grep, glob

use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use regex::Regex;

fn resolve(base: &Path, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { base.join(p) }
}

pub fn read_file(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("read_file: 'path' required"))?;

    let path = resolve(cwd, path_str);
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let content = std::fs::read_to_string(&path)?;

    // Optional line range
    let start = params.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let end   = params.get("end_line").and_then(|v| v.as_u64());

    if start > 1 || end.is_some() {
        let lines: Vec<&str> = content.lines().collect();
        let from = start.saturating_sub(1);
        let to   = end.map(|e| e as usize).unwrap_or(lines.len());
        let to   = to.min(lines.len());
        let selected: Vec<String> = lines[from..to]
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{:4} | {}", from + i + 1, l))
            .collect();
        return Ok(selected.join("\n"));
    }

    // Add line numbers for large files
    if content.lines().count() > 50 {
        let numbered: Vec<String> = content.lines()
            .enumerate()
            .map(|(i, l)| format!("{:4} | {}", i + 1, l))
            .collect();
        return Ok(numbered.join("\n"));
    }

    Ok(content)
}

pub fn write_file(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("write_file: 'path' required"))?;
    let content = params.get("content").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("write_file: 'content' required"))?;

    let path = resolve(cwd, path_str);

    // Create parent dirs if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, content)?;
    Ok(format!("Written {} bytes to {}", content.len(), path.display()))
}

pub fn edit_file(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("edit_file: 'path' required"))?;
    let old_text = params.get("old_text").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("edit_file: 'old_text' required"))?;
    let new_text = params.get("new_text").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("edit_file: 'new_text' required"))?;

    let path = resolve(cwd, path_str);
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let content = std::fs::read_to_string(&path)?;
    if !content.contains(old_text) {
        anyhow::bail!("old_text not found in {}. Cannot edit.", path.display());
    }

    // Only replace first occurrence (safer)
    let new_content = content.replacen(old_text, new_text, 1);
    std::fs::write(&path, &new_content)?;

    let additions = new_text.lines().count();
    let removals  = old_text.lines().count();
    Ok(format!("Edited {} (+{} -{} lines)", path.display(), additions, removals))
}

pub fn list_dir(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .unwrap_or(".");

    let path = resolve(cwd, path_str);
    if !path.exists() {
        anyhow::bail!("Directory not found: {}", path.display());
    }

    let mut entries: Vec<String> = std::fs::read_dir(&path)?
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = e.metadata().ok().map(|m| m.len()).unwrap_or(0);
            if is_dir {
                format!("  {}/", name)
            } else {
                format!("  {}  ({})", name, human_size(size))
            }
        })
        .collect();

    entries.sort();
    Ok(format!("{}:\n{}", path.display(), entries.join("\n")))
}

pub fn grep(params: &Value, cwd: &Path) -> Result<String> {
    let pattern = params.get("pattern").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("grep: 'pattern' required"))?;

    let search_path = params.get("path").and_then(|v| v.as_str())
        .unwrap_or(".");
    let file_glob = params.get("file_glob").and_then(|v| v.as_str())
        .unwrap_or("*");

    let re = Regex::new(pattern)
        .map_err(|e| anyhow::anyhow!("Invalid regex: {}", e))?;

    let base = resolve(cwd, search_path);
    let mut matches: Vec<String> = vec![];
    let mut files_searched = 0;

    let glob_re = glob_to_regex(file_glob);

    for entry in WalkDir::new(&base)
        .max_depth(6)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(name.as_ref(), "node_modules" | "target" | ".git" | "dist" | "__pycache__")
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let fname = entry.file_name().to_string_lossy().to_string();
        if !glob_re.is_match(&fname) { continue; }

        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        files_searched += 1;
        for (line_num, line) in content.lines().enumerate() {
            if re.is_match(line) {
                let rel = path.strip_prefix(&base)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string_lossy().to_string());
                matches.push(format!("{}:{}: {}", rel, line_num + 1, line.trim()));
            }
        }

        if matches.len() > 200 { break; }  // cap output
    }

    if matches.is_empty() {
        Ok(format!("No matches for '{}' in {} files", pattern, files_searched))
    } else {
        Ok(format!("{} matches in {} files:\n\n{}", matches.len(), files_searched, matches.join("\n")))
    }
}

pub fn glob_files(params: &Value, cwd: &Path) -> Result<String> {
    let pattern = params.get("pattern").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("glob: 'pattern' required"))?;

    let base_str = params.get("base_dir").and_then(|v| v.as_str())
        .unwrap_or(".");
    let base = resolve(cwd, base_str);

    let re = glob_to_regex(pattern);
    let mut found: Vec<String> = vec![];

    for entry in WalkDir::new(&base)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(name.as_ref(), "node_modules" | "target" | ".git" | "dist")
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let fname = entry.file_name().to_string_lossy().to_string();
        // Match on filename OR full relative path
        let rel = entry.path().strip_prefix(&base)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if re.is_match(&fname) || re.is_match(&rel) {
            found.push(rel);
        }
        if found.len() > 500 { break; }
    }

    found.sort();
    Ok(found.join("\n"))
}

pub fn create_dir(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("create_dir: 'path' required"))?;
    let path = resolve(cwd, path_str);
    std::fs::create_dir_all(&path)?;
    Ok(format!("Created directory: {}", path.display()))
}

pub fn delete_file(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("delete_file: 'path' required"))?;
    let path = resolve(cwd, path_str);
    if !path.exists() {
        anyhow::bail!("Path not found: {}", path.display());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(&path)?;
        Ok(format!("Deleted directory: {}", path.display()))
    } else {
        std::fs::remove_file(&path)?;
        Ok(format!("Deleted file: {}", path.display()))
    }
}

pub fn move_file(params: &Value, cwd: &Path) -> Result<String> {
    let from_str = params.get("from").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("move_file: 'from' required"))?;
    let to_str = params.get("to").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("move_file: 'to' required"))?;
    let from = resolve(cwd, from_str);
    let to   = resolve(cwd, to_str);
    if !from.exists() {
        anyhow::bail!("Source not found: {}", from.display());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&from, &to)?;
    Ok(format!("Moved: {} → {}", from.display(), to.display()))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn human_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 { format!("{:.1}G", bytes as f64 / 1_073_741_824.0) }
    else if bytes >= 1_048_576 { format!("{:.0}M", bytes as f64 / 1_048_576.0) }
    else if bytes >= 1024 { format!("{:.0}K", bytes as f64 / 1024.0) }
    else { format!("{}", bytes) }
}

/// Convert a simple glob pattern like "*.rs" or "src/**/*.ts" to a Regex.
fn glob_to_regex(glob: &str) -> Regex {
    let mut regex = String::from("(?i)^");
    for ch in glob.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '^' | '$' | '{' | '}' | '|' | '(' | ')' | '[' | ']' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            c  => regex.push(c),
        }
    }
    regex.push('$');
    Regex::new(&regex).unwrap_or_else(|_| Regex::new(".*").unwrap())
}
