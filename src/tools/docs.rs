/// Document tools — PDF reading, CSV analysis, structured web scraping, notes.

use anyhow::Result;
use serde_json::Value;
use std::path::Path;

// ── PDF Reader ────────────────────────────────────────────────────────────────

pub fn read_pdf(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("read_pdf: 'path' required"))?;

    let path = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        cwd.join(path_str)
    };

    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let bytes = std::fs::read(&path)?;
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| anyhow::anyhow!("PDF extraction failed: {}", e))?;

    if text.trim().is_empty() {
        anyhow::bail!("No extractable text found — PDF may be scanned/image-only.");
    }

    let max_chars = params.get("max_chars")
        .and_then(|v| v.as_u64())
        .unwrap_or(8000) as usize;

    let trimmed = if text.len() > max_chars {
        format!("{}\n\n… (truncated — {} chars total, showing first {})",
            &text[..max_chars], text.len(), max_chars)
    } else {
        text.clone()
    };

    let page_hint = text.matches('\x0C').count(); // form feeds = page breaks
    Ok(format!("PDF: {} (~{} pages)\n\n{}", path.file_name().unwrap_or_default().to_string_lossy(), page_hint + 1, trimmed))
}

// ── CSV Analysis ──────────────────────────────────────────────────────────────

pub fn analyze_csv(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("analyze_csv: 'path' required"))?;

    let path = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        cwd.join(path_str)
    };

    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let mut rdr = csv::Reader::from_path(&path)?;
    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();

    let mut rows: Vec<Vec<String>> = vec![];
    for result in rdr.records() {
        let record = result?;
        rows.push(record.iter().map(|s| s.to_string()).collect());
    }

    let row_count = rows.len();
    let col_count = headers.len();

    // Detect numeric columns and compute basic stats
    let mut col_stats: Vec<String> = vec![];
    for (col_i, header) in headers.iter().enumerate() {
        let values: Vec<f64> = rows.iter()
            .filter_map(|row| row.get(col_i)?.parse::<f64>().ok())
            .collect();

        if values.len() > row_count / 3 {
            let sum: f64 = values.iter().sum();
            let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mean = sum / values.len() as f64;
            col_stats.push(format!(
                "  {}: sum={:.2}, mean={:.2}, min={:.2}, max={:.2} ({} numeric values)",
                header, sum, mean, min, max, values.len()
            ));
        }
    }

    // Show first 5 rows as preview
    let preview_rows = rows.iter().take(5).map(|row| {
        headers.iter().zip(row.iter())
            .map(|(h, v)| format!("{}: {}", h, v))
            .collect::<Vec<_>>()
            .join(", ")
    }).collect::<Vec<_>>().join("\n");

    let mut out = format!(
        "CSV: {}\nRows: {}  Columns: {}\nHeaders: {}\n",
        path.file_name().unwrap_or_default().to_string_lossy(),
        row_count, col_count,
        headers.join(", ")
    );

    if !col_stats.is_empty() {
        out.push_str("\nNumeric column stats:\n");
        out.push_str(&col_stats.join("\n"));
    }

    out.push_str("\nFirst 5 rows:\n");
    out.push_str(&preview_rows);

    Ok(out)
}

// ── Web Scrape + Summarize ────────────────────────────────────────────────────

pub async fn scrape_page(params: &Value, client: &reqwest::Client) -> Result<String> {
    let url = params.get("url").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("scrape_page: 'url' required"))?;

    let focus = params.get("focus").and_then(|v| v.as_str()).unwrap_or("main content");

    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; HyperLite/1.0)")
        .send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {} from {}", resp.status(), url);
    }

    let html = resp.text().await?;
    let doc = scraper::Html::parse_document(&html);

    // Extract title
    let title = scraper::Selector::parse("title").ok()
        .and_then(|sel| doc.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    // Remove script, style, nav, footer, header noise
    let noise_tags = ["script", "style", "nav", "footer", "header", "aside", "noscript"];
    let body_sel = scraper::Selector::parse("body").unwrap();

    // Extract clean text from meaningful tags
    let content_sel = scraper::Selector::parse(
        "p, h1, h2, h3, h4, li, td, th, blockquote, pre, code, article, main, section"
    ).unwrap();

    let mut text_parts: Vec<String> = vec![];
    let mut seen = std::collections::HashSet::new();

    for el in doc.select(&content_sel) {
        // Skip if inside a noise tag
        let in_noise = el.ancestors().any(|a| {
            a.value().as_element()
                .map(|e| noise_tags.contains(&e.name()))
                .unwrap_or(false)
        });
        if in_noise { continue; }

        let text = el.text().collect::<String>();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.len() > 20 && !seen.contains(&text) {
            seen.insert(text.clone());
            text_parts.push(text);
        }
    }

    let content = text_parts.join("\n");
    let max_chars = 6000usize;
    let trimmed = if content.len() > max_chars {
        format!("{}\n\n… (truncated, {} chars total)", &content[..max_chars], content.len())
    } else {
        content
    };

    Ok(format!("URL: {}\nTitle: {}\nFocus: {}\n\n{}", url, title, focus, trimmed))
}

// ── Notes / Task Management ───────────────────────────────────────────────────

pub fn read_notes(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .unwrap_or("TODO.md");

    let path = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        cwd.join(path_str)
    };

    if !path.exists() {
        return Ok(format!("No notes file at {}. Create one or specify a path.", path.display()));
    }

    let content = std::fs::read_to_string(&path)?;

    // Parse tasks: lines starting with - [ ] or - [x]
    let mut pending = 0usize;
    let mut done = 0usize;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("- [ ]") || t.starts_with("* [ ]") { pending += 1; }
        if t.starts_with("- [x]") || t.starts_with("- [X]") || t.starts_with("* [x]") { done += 1; }
    }

    let summary = if pending + done > 0 {
        format!("\nTask summary: {} pending, {} done\n", pending, done)
    } else {
        String::new()
    };

    Ok(format!("Notes: {}{}\n{}", path.display(), summary, content))
}

pub fn write_note(params: &Value, cwd: &Path) -> Result<String> {
    let path_str = params.get("path").and_then(|v| v.as_str())
        .unwrap_or("TODO.md");
    let content = params.get("content").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("write_note: 'content' required"))?;
    let append = params.get("append").and_then(|v| v.as_bool()).unwrap_or(false);

    let path = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        cwd.join(path_str)
    };

    if append {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", content)?;
        Ok(format!("Appended to {}", path.display()))
    } else {
        std::fs::write(&path, content)?;
        Ok(format!("Written to {}", path.display()))
    }
}

// ── System Monitoring ─────────────────────────────────────────────────────────

pub fn system_status(_params: &Value) -> Result<String> {
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem  = sys.total_memory() / 1024 / 1024;
    let used_mem   = sys.used_memory()  / 1024 / 1024;
    let free_mem   = total_mem - used_mem;
    let cpu_count  = sys.cpus().len();
    let cpu_usage: f32 = sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / cpu_count as f32;

    // Top 5 processes by memory
    let mut procs: Vec<_> = sys.processes().values().collect();
    procs.sort_by(|a, b| b.memory().cmp(&a.memory()));
    let top_procs: String = procs.iter().take(5).map(|p| {
        format!("  {} (PID {}) — {:.0} MB RAM, {:.1}% CPU",
            p.name().to_string_lossy(), p.pid(), p.memory() / 1024 / 1024, p.cpu_usage())
    }).collect::<Vec<_>>().join("\n");

    // Disk usage — filter out WSL virtual/overlay mounts and tiny pseudo-filesystems
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let disk_info: String = disks.iter().filter(|d| {
        let mount = d.mount_point().to_string_lossy();
        let total_gb = d.total_space() / 1024 / 1024 / 1024;
        total_gb >= 1
            && !mount.contains("wslg")
            && !mount.contains("wsl/lib")
            && !mount.contains("wsl/driver")
            && !mount.contains("versions.txt")
            && !mount.contains("modules")
    }).map(|d| {
        let total = d.total_space() / 1024 / 1024 / 1024;
        let free  = d.available_space() / 1024 / 1024 / 1024;
        let used  = total - free;
        let pct   = if total > 0 { used * 100 / total } else { 0 };
        format!("  {} — {} GB used / {} GB total  ({}% full)",
            d.mount_point().display(), used, total, pct)
    }).collect::<Vec<_>>().join("\n");

    Ok(format!(
        "System Status\n\
         CPU: {:.1}% usage across {} cores\n\
         RAM: {} MB used / {} MB total ({} MB free)\n\
         \nTop processes by memory:\n{}\n\
         \nDisk usage:\n{}",
        cpu_usage, cpu_count,
        used_mem, total_mem, free_mem,
        top_procs,
        disk_info
    ))
}

pub fn check_ports(params: &Value) -> Result<String> {
    let ports: Vec<u16> = params.get("ports")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u16)).collect())
        .unwrap_or_else(|| vec![80, 443, 3000, 5000, 8000, 8080, 8443, 11434, 18080]);

    let mut results = vec![];
    for port in &ports {
        let in_use = std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_err();
        results.push(format!("  :{} — {}", port,
            if in_use { "IN USE" } else { "free" }));
    }

    Ok(format!("Port status:\n{}", results.join("\n")))
}
