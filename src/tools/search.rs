/// Web Search Tool
///
/// No API key required. Uses DuckDuckGo's HTML endpoint and parses results.
/// Falls back to a simple DuckDuckGo instant-answer API call.
///
/// For self-hosted search: configure SearXNG base URL in settings.

use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;

// Build a shared client lazily
fn search_client() -> Client {
    Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title:   String,
    pub url:     String,
    pub snippet: String,
}

/// Execute a web search and return formatted results.
pub async fn execute(params: &Value) -> Result<String> {
    let query = params.get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("search: 'query' parameter required"))?;

    let results = duckduckgo_search(query).await
        .unwrap_or_default();

    if results.is_empty() {
        return Ok(format!("No results found for: {}", query));
    }

    let mut output = format!("Search results for: {}\n\n", query);
    for (i, r) in results.iter().enumerate() {
        output.push_str(&format!("{}. **{}**\n   {}\n   {}\n\n",
            i + 1, r.title, r.snippet, r.url));
    }

    Ok(output)
}

/// DuckDuckGo HTML scraper — no API key needed, returns up to 10 results.
async fn duckduckgo_search(query: &str) -> Result<Vec<SearchResult>> {
    let encoded = url::form_urlencoded::byte_serialize(query.as_bytes())
        .collect::<String>();

    let client = search_client();

    // DuckDuckGo HTML search
    let url = format!("https://html.duckduckgo.com/html/?q={}", encoded);
    let resp = client.get(&url)
        .header("Accept-Language", "en-US,en;q=0.5")
        .send().await?;

    let html = resp.text().await?;
    let results = parse_ddg_html(&html);

    if !results.is_empty() {
        return Ok(results);
    }

    // Fallback: DuckDuckGo instant answer API
    duckduckgo_instant(query, &client).await
}

fn parse_ddg_html(html: &str) -> Vec<SearchResult> {
    let document = Html::parse_document(html);
    let mut results = vec![];

    // DuckDuckGo HTML result selectors
    let result_sel = Selector::parse(".result").ok();
    let title_sel  = Selector::parse(".result__title a").ok();
    let url_sel    = Selector::parse(".result__url").ok();
    let snip_sel   = Selector::parse(".result__snippet").ok();

    let (result_sel, title_sel, snip_sel) = match (result_sel, title_sel, snip_sel) {
        (Some(r), Some(t), Some(s)) => (r, t, s),
        _ => return results,
    };

    for result in document.select(&result_sel).take(10) {
        let title = result.select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let url = result.select(&title_sel)
            .next()
            .and_then(|e| e.value().attr("href"))
            .map(clean_ddg_url)
            .unwrap_or_default();

        let snippet = result.select(&snip_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        if !title.is_empty() && !url.is_empty() {
            results.push(SearchResult { title, url, snippet });
        }
    }

    results
}

/// Clean DuckDuckGo redirect URLs to actual destination URLs.
fn clean_ddg_url(href: &str) -> String {
    if href.starts_with("//duckduckgo.com/l/?uddg=") {
        // Decode the uddg parameter
        if let Some(pos) = href.find("uddg=") {
            let encoded = &href[pos + 5..];
            let decoded: String = url::form_urlencoded::parse(encoded.as_bytes())
                .flat_map(|(k, v)| if k == "uddg" { vec![v.to_string()] } else { vec![k.to_string()] })
                .collect();
            if !decoded.is_empty() { return decoded; }
        }
    }
    href.to_string()
}

/// DuckDuckGo Instant Answer API — returns fewer but reliable results.
async fn duckduckgo_instant(query: &str, client: &Client) -> Result<Vec<SearchResult>> {
    let encoded = url::form_urlencoded::byte_serialize(query.as_bytes())
        .collect::<String>();

    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
        encoded
    );

    let resp = client.get(&url).send().await?;
    let json: Value = resp.json().await?;

    let mut results = vec![];

    // Abstract / Instant Answer
    if let Some(abstract_text) = json.get("AbstractText").and_then(|v| v.as_str()) {
        if !abstract_text.is_empty() {
            let title = json.get("Heading")
                .and_then(|v| v.as_str())
                .unwrap_or(query)
                .to_string();
            let url = json.get("AbstractURL")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            results.push(SearchResult {
                title,
                url,
                snippet: abstract_text.chars().take(300).collect(),
            });
        }
    }

    // Related topics
    if let Some(topics) = json.get("RelatedTopics").and_then(|v| v.as_array()) {
        for topic in topics.iter().take(5) {
            if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    let url = topic.get("FirstURL")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    results.push(SearchResult {
                        title:   text.chars().take(80).collect::<String>(),
                        url,
                        snippet: text.chars().take(200).collect(),
                    });
                }
            }
        }
    }

    Ok(results)
}

/// Optional: SearXNG search (self-hosted, more results, better privacy).
/// Configure in settings: search.searxng_url = "http://localhost:8888"
pub async fn searxng_search(
    query:    &str,
    base_url: &str,
    client:   &Client,
) -> Result<Vec<SearchResult>> {
    let encoded = url::form_urlencoded::byte_serialize(query.as_bytes())
        .collect::<String>();

    let url = format!("{}/search?q={}&format=json", base_url.trim_end_matches('/'), encoded);
    let resp = client.get(&url).send().await?;
    let json: Value = resp.json().await?;

    let mut results = vec![];
    if let Some(items) = json.get("results").and_then(|v| v.as_array()) {
        for item in items.iter().take(10) {
            let title   = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let url     = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let snippet = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !title.is_empty() {
                results.push(SearchResult { title, url, snippet });
            }
        }
    }

    Ok(results)
}
