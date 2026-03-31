/// HTTP fetch tool — fetches a URL and extracts readable text.

use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;

const MAX_CONTENT: usize = 8000;  // chars

pub async fn fetch(params: &Value, client: &Client) -> Result<String> {
    let url = params.get("url").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("http_fetch: 'url' required"))?;

    let extract_text = params.get("extract_text")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; HyperLite/0.1)")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    let status = resp.status();
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = resp.text().await?;

    // If HTML and extract_text is true, pull readable text
    if extract_text && content_type.contains("text/html") {
        let text = extract_readable_text(&body);
        let truncated = if text.len() > MAX_CONTENT {
            format!("{}…\n\n[Content truncated at {} chars]", &text[..MAX_CONTENT], text.len())
        } else {
            text
        };
        return Ok(format!("URL: {}\nStatus: {}\n\n{}", url, status, truncated));
    }

    // Plain text / JSON / other
    let truncated = if body.len() > MAX_CONTENT {
        format!("{}…\n\n[Truncated at {} chars]", &body[..MAX_CONTENT], body.len())
    } else {
        body
    };

    Ok(format!("URL: {}\nStatus: {}\nContent-Type: {}\n\n{}", url, status, content_type, truncated))
}

/// Extract human-readable text from HTML, stripping tags/scripts/style.
fn extract_readable_text(html: &str) -> String {
    let document = Html::parse_document(html);

    // Remove script/style elements
    let text_selector = Selector::parse("body").ok();
    let skip_selector = Selector::parse("script, style, nav, footer, header, aside, noscript").ok();

    let body = match text_selector.and_then(|s| document.select(&s).next()) {
        Some(b) => b,
        None    => return extract_text_naive(html),
    };

    // Collect text using element references (scraper ElementRef API)
    let mut result = String::new();
    let p_sel = Selector::parse("p, h1, h2, h3, h4, h5, li").ok();
    let skip_tags = ["script", "style", "nav", "footer", "header", "aside", "noscript"];

    // Walk block elements for structure
    if let Some(ref ps) = p_sel {
        for elem in document.select(ps) {
            // Skip if inside a skip-tag ancestor
            let in_skip = elem.ancestors().any(|a| {
                a.value().as_element()
                    .map(|e| skip_tags.contains(&e.name()))
                    .unwrap_or(false)
            });
            if in_skip { continue; }
            let text = elem.text().collect::<String>();
            let t = text.trim();
            if !t.is_empty() {
                result.push_str(t);
                result.push('\n');
            }
        }
    }

    // Fallback: just grab body text if nothing found
    if result.trim().is_empty() {
        result = body.text().collect::<Vec<_>>().join(" ");
    }

    // Clean up excessive whitespace
    result.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_text_naive(html: &str) -> String {
    // Simple regex-free tag stripper for fallback
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => { in_tag = true; result.push(' '); }
            '>' => { in_tag = false; }
            c if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
