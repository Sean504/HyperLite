/// OpenAI-compatible streaming client.
///
/// Shared by: llama.cpp, LM Studio, LocalAI, Jan, llamafile, vLLM, GPT4All,
/// and any other server implementing the OpenAI chat completions API.
///
/// Endpoint: POST /v1/chat/completions  (streaming SSE)
/// Model list: GET /v1/models

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{ChatMessage, GenerationParams, LocalModel, StreamEvent, BackendKind, ModelFormat};

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model:       &'a str,
    messages:    &'a [OaiMessage],
    stream:      bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p:       Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens:  Option<u32>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    stop:        Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed:        Option<i64>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    tools:       Vec<OaiTool>,
}

#[derive(Serialize, Clone)]
struct OaiTool {
    #[serde(rename = "type")]
    kind:     &'static str,
    function: OaiFunctionDef,
}

#[derive(Serialize, Clone)]
struct OaiFunctionDef {
    name:        &'static str,
    description: &'static str,
    parameters:  serde_json::Value,
}

#[derive(Serialize)]
struct OaiMessage {
    role:    String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct ChatChunk {
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage:   Option<Usage>,
}

#[derive(Deserialize, Debug)]
struct ChunkChoice {
    delta:         Delta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct Delta {
    #[serde(default)]
    content:    Option<String>,
    #[serde(default)]
    reasoning:  Option<String>,     // Qwen, DeepSeek-R1
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>, // native function calling
}

/// Accumulated native tool call from streaming deltas
#[derive(Deserialize, Debug, Default, Clone)]
struct ToolCallDelta {
    index:    Option<usize>,
    #[serde(default)]
    function: FunctionDelta,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct FunctionDelta {
    #[serde(default)]
    name:      Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Usage {
    completion_tokens: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct ModelList {
    data: Vec<OaiModel>,
}

#[derive(Deserialize, Debug)]
struct OaiModel {
    id:      String,
    #[serde(default)]
    owned_by: Option<String>,
}

// ── Core streaming function ───────────────────────────────────────────────────

/// Build the OaiTool list from our static ToolDef registry.
fn build_oai_tools() -> Vec<OaiTool> {
    crate::tools::ALL_TOOLS
        .iter()
        .map(|t| OaiTool {
            kind: "function",
            function: OaiFunctionDef {
                name:        t.name,
                description: t.description,
                parameters:  serde_json::from_str(t.parameters)
                    .unwrap_or(serde_json::json!({"type":"object","properties":{}})),
            },
        })
        .collect()
}

/// Send a streaming chat request to any OpenAI-compatible endpoint.
/// Returns a channel receiver that yields `StreamEvent`s.
pub async fn stream_chat(
    client:      &Client,
    base_url:    &str,
    backend:     BackendKind,
    messages:    &[ChatMessage],
    model:       &str,
    params:      &GenerationParams,
) -> Result<mpsc::Receiver<StreamEvent>> {
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

    let oai_messages: Vec<OaiMessage> = messages
        .iter()
        .map(|m| OaiMessage { role: m.role.clone(), content: m.content.clone() })
        .collect();

    let tools = build_oai_tools();

    let body = ChatRequest {
        model,
        messages:    &oai_messages,
        stream:      true,
        temperature: params.temperature,
        top_p:       params.top_p,
        max_tokens:  params.max_tokens,
        stop:        params.stop.clone(),
        seed:        params.seed,
        tools,
    };

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Failed to connect to {}", url))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Backend returned {}: {}", status, body);
    }

    let (tx, rx) = mpsc::channel::<StreamEvent>(256);
    let start = std::time::Instant::now();

    tokio::spawn(async move {
        let mut stream = response.bytes_stream();
        let mut buf    = String::new();
        let mut tokens_generated: u32 = 0;
        // Accumulate native tool call deltas across chunks
        // Index → (name, arguments_so_far)
        let mut tool_acc: std::collections::HashMap<usize, (String, String)> = Default::default();

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c)  => c,
                Err(e) => { let _ = tx.send(StreamEvent::Error(e.to_string())).await; return; }
            };

            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline) = buf.find('\n') {
                let line = buf[..newline].trim().to_string();
                buf = buf[newline + 1..].to_string();

                if line.is_empty() || line.starts_with(':') { continue; }

                let data = if let Some(s) = line.strip_prefix("data: ") {
                    s.trim()
                } else {
                    continue;
                };

                if data == "[DONE]" {
                    // Emit any accumulated native tool calls as XML
                    for (_, (name, args)) in &tool_acc {
                        if !name.is_empty() {
                            let xml = format!(
                                "<tool_call>\n<name>{}</name>\n<parameters>{}</parameters>\n</tool_call>",
                                name,
                                if args.is_empty() { "{}" } else { args }
                            );
                            let _ = tx.send(StreamEvent::Text(xml)).await;
                        }
                    }
                    let ms = start.elapsed().as_millis() as u64;
                    let _ = tx.send(StreamEvent::Done {
                        duration_ms:      ms,
                        tokens_generated: if tokens_generated > 0 { Some(tokens_generated) } else { None },
                    }).await;
                    return;
                }

                match serde_json::from_str::<ChatChunk>(data) {
                    Ok(chunk) => {
                        if let Some(usage) = &chunk.usage {
                            if let Some(ct) = usage.completion_tokens { tokens_generated = ct; }
                        }
                        for choice in chunk.choices {
                            // Regular text content
                            if let Some(text) = choice.delta.content {
                                if !text.is_empty() {
                                    let _ = tx.send(StreamEvent::Text(text)).await;
                                }
                            }
                            // Reasoning (thinking tokens)
                            if let Some(r) = choice.delta.reasoning {
                                if !r.is_empty() {
                                    let _ = tx.send(StreamEvent::Reasoning(r)).await;
                                }
                            }
                            // Native tool call deltas — accumulate across chunks
                            for tc in choice.delta.tool_calls {
                                let idx = tc.index.unwrap_or(0);
                                let entry = tool_acc.entry(idx).or_insert_with(|| (String::new(), String::new()));
                                if let Some(n) = tc.function.name { entry.0.push_str(&n); }
                                if let Some(a) = tc.function.arguments { entry.1.push_str(&a); }
                            }
                            // finish_reason: "tool_calls" → emit accumulated calls + done
                            let fr = choice.finish_reason.as_deref().unwrap_or("");
                            if fr == "tool_calls" {
                                for (_, (name, args)) in &tool_acc {
                                    if !name.is_empty() {
                                        let xml = format!(
                                            "<tool_call>\n<name>{}</name>\n<parameters>{}</parameters>\n</tool_call>",
                                            name,
                                            if args.is_empty() { "{}" } else { args }
                                        );
                                        let _ = tx.send(StreamEvent::Text(xml)).await;
                                    }
                                }
                                tool_acc.clear();
                                let ms = start.elapsed().as_millis() as u64;
                                let _ = tx.send(StreamEvent::Done {
                                    duration_ms:      ms,
                                    tokens_generated: if tokens_generated > 0 { Some(tokens_generated) } else { None },
                                }).await;
                                return;
                            }
                            if fr == "stop" || fr == "length" {
                                let ms = start.elapsed().as_millis() as u64;
                                let _ = tx.send(StreamEvent::Done {
                                    duration_ms:      ms,
                                    tokens_generated: if tokens_generated > 0 { Some(tokens_generated) } else { None },
                                }).await;
                                return;
                            }
                        }
                    }
                    Err(_) => {} // skip unparseable SSE lines
                }
            }
        }

        // Stream ended without [DONE]
        let ms = start.elapsed().as_millis() as u64;
        let _ = tx.send(StreamEvent::Done { duration_ms: ms, tokens_generated: None }).await;
    });

    Ok(rx)
}

// ── Model listing ─────────────────────────────────────────────────────────────

pub async fn list_models(
    client:   &Client,
    base_url: &str,
    backend:  BackendKind,
) -> Result<Vec<LocalModel>> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Cannot reach {}", url))?;

    if !resp.status().is_success() {
        anyhow::bail!("Model list returned {}", resp.status());
    }

    let list: ModelList = resp.json().await?;

    Ok(list.data.into_iter().map(|m| {
        // Try to guess format from name/path heuristics
        let format = guess_format_from_id(&m.id);
        let quantization = extract_quantization(&m.id);

        LocalModel {
            id:           m.id.clone(),
            name:         pretty_name(&m.id),
            backend:      backend.clone(),
            format,
            path:         None,
            size_bytes:   None,
            context_len:  None,
            param_count:  extract_param_count(&m.id),
            quantization,
            tags:         vec![],
        }
    }).collect())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Make a human-friendly name from a model ID like "llama3:8b-instruct-q4_K_M"
pub fn pretty_name(id: &str) -> String {
    // Strip path prefix if any
    let base = id.rsplit('/').next().unwrap_or(id);
    // Replace underscores/dashes with spaces, capitalize
    base.replace('_', " ").replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None    => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn guess_format_from_id(id: &str) -> ModelFormat {
    let lower = id.to_lowercase();
    if lower.contains(".gguf") || lower.ends_with("gguf") { return ModelFormat::Gguf; }
    if lower.contains(".ggml") || lower.ends_with("ggml") { return ModelFormat::GgmlLegacy; }
    if lower.contains("gptq")  { return ModelFormat::Gptq; }
    if lower.contains("awq")   { return ModelFormat::Awq; }
    if lower.contains("exl2")  { return ModelFormat::Exl2; }
    if lower.contains(".safetensors") { return ModelFormat::SafeTensors; }
    if lower.contains(".llamafile")   { return ModelFormat::Llamafile; }
    // Ollama-style IDs are usually GGUF under the hood
    ModelFormat::Gguf
}

pub fn extract_quantization(id: &str) -> Option<String> {
    // Match patterns like Q4_K_M, Q8_0, q4_0, IQ3_M, f16, fp16, bf16
    let re = regex::Regex::new(
        r"(?i)(Q\d+_[KI0-9]+(?:_[A-Z]+)?|IQ\d+_[A-Z]+|[bfBF]16|int8|int4|fp32)"
    ).ok()?;
    re.find(id).map(|m| m.as_str().to_uppercase())
}

pub fn extract_param_count(id: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?i)(\d+(?:\.\d+)?)[bB](?:[^a-zA-Z]|$)").ok()?;
    re.captures(id).map(|c| format!("{}B", &c[1]))
}
