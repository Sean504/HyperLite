/// Ollama backend
///
/// Uses Ollama's native REST API (/api/chat streaming + /api/tags model list).
/// Also handles Ollama's custom response format which differs from OpenAI.
///
/// Default port: 11434
/// Formats: GGUF (Ollama stores all models as GGUF internally)
/// API docs: https://github.com/ollama/ollama/blob/main/docs/api.md

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;

use super::{
    BackendKind, ChatMessage, GenerationParams, LocalModel, LocalProvider,
    ModelFormat, StreamEvent,
};

pub struct OllamaProvider {
    client:   Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

// ── Ollama API types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaChatRequest {
    model:    String,
    messages: Vec<OllamaMessage>,
    stream:   bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options:  Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaMessage {
    role:    String,
    content: String,
}

#[derive(Serialize, Default)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature:    Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p:          Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k:          Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict:    Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed:           Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop:           Vec<String>,
}

#[derive(Deserialize, Debug)]
struct OllamaChatResponse {
    message:            Option<OllamaResponseMessage>,
    done:               bool,
    #[serde(default)]
    total_duration:     Option<u64>,   // nanoseconds
    #[serde(default)]
    eval_count:         Option<u32>,   // tokens generated
    #[serde(default)]
    error:              Option<String>,
}

#[derive(Deserialize, Debug)]
struct OllamaResponseMessage {
    role:    String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct OllamaTagList {
    models: Vec<OllamaModelInfo>,
}

#[derive(Deserialize, Debug)]
struct OllamaModelInfo {
    name:          String,
    #[serde(default)]
    size:          Option<u64>,
    details:       Option<OllamaModelDetails>,
    #[serde(default)]
    modified_at:   Option<String>,
}

#[derive(Deserialize, Debug)]
struct OllamaModelDetails {
    parameter_size:       Option<String>,
    quantization_level:   Option<String>,
    family:               Option<String>,
    families:             Option<Vec<String>>,
    format:               Option<String>,
}

// ── Pull request/response ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaPullRequest {
    name:   String,
    stream: bool,
}

// ── Provider implementation ───────────────────────────────────────────────────

#[async_trait]
impl LocalProvider for OllamaProvider {
    fn id(&self)       -> &str { "ollama" }
    fn name(&self)     -> &str { "Ollama" }
    fn kind(&self)     -> BackendKind { BackendKind::Ollama }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Ollama /api/tags returned {}", resp.status());
        }

        let tag_list: OllamaTagList = resp.json().await?;

        Ok(tag_list.models.into_iter().map(|m| {
            let details = m.details.as_ref();
            let param_count = details
                .and_then(|d| d.parameter_size.clone())
                .map(|s| s.to_uppercase());
            let quantization = details
                .and_then(|d| d.quantization_level.clone())
                .map(|s| s.to_uppercase());
            let tags = details
                .and_then(|d| d.families.clone())
                .unwrap_or_else(|| {
                    details.and_then(|d| d.family.clone())
                        .map(|f| vec![f])
                        .unwrap_or_default()
                });

            LocalModel {
                id:           m.name.clone(),
                name:         super::openai_compat::pretty_name(&m.name),
                backend:      BackendKind::Ollama,
                format:       ModelFormat::Gguf,
                path:         None,
                size_bytes:   m.size,
                context_len:  None,
                param_count,
                quantization,
                tags,
            }
        }).collect())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let url = format!("{}/api/chat", self.base_url);

        let oai_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage { role: m.role.clone(), content: m.content.clone() })
            .collect();

        let options = OllamaOptions {
            temperature:    params.temperature,
            top_p:          params.top_p,
            top_k:          params.top_k,
            num_predict:    params.max_tokens,
            seed:           params.seed,
            repeat_penalty: params.repeat_penalty,
            stop:           params.stop.clone(),
        };

        let body = OllamaChatRequest {
            model:    model.to_string(),
            messages: oai_messages,
            stream:   true,
            options:  Some(options),
        };

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned {}: {}", status, text);
        }

        let (tx, rx) = mpsc::channel::<StreamEvent>(256);
        let start = std::time::Instant::now();

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buf = String::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c)  => c,
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                        return;
                    }
                };

                buf.push_str(&String::from_utf8_lossy(&chunk));

                // Ollama sends one JSON object per line
                while let Some(newline) = buf.find('\n') {
                    let line = buf[..newline].trim().to_string();
                    buf = buf[newline + 1..].to_string();

                    if line.is_empty() { continue; }

                    match serde_json::from_str::<OllamaChatResponse>(&line) {
                        Ok(resp) => {
                            if let Some(err) = resp.error {
                                let _ = tx.send(StreamEvent::Error(err)).await;
                                return;
                            }
                            if let Some(msg) = resp.message {
                                if !msg.content.is_empty() {
                                    let _ = tx.send(StreamEvent::Text(msg.content)).await;
                                }
                            }
                            if resp.done {
                                let ms = resp.total_duration
                                    .map(|ns| ns / 1_000_000)
                                    .unwrap_or_else(|| start.elapsed().as_millis() as u64);
                                let _ = tx.send(StreamEvent::Done {
                                    duration_ms:      ms,
                                    tokens_generated: resp.eval_count,
                                }).await;
                                return;
                            }
                        }
                        Err(_) => { /* skip malformed line */ }
                    }
                }
            }

            let ms = start.elapsed().as_millis() as u64;
            let _ = tx.send(StreamEvent::Done { duration_ms: ms, tokens_generated: None }).await;
        });

        Ok(rx)
    }

    /// Pull a model from the Ollama registry by name (e.g. "llama3:8b").
    /// This is a special Ollama feature — not part of the standard trait,
    /// but callable by the UI when needed.
    async fn load_model(&self, path: &PathBuf) -> Result<LocalModel> {
        // For Ollama, "path" is interpreted as a model name/tag to pull
        let name = path.to_string_lossy().to_string();
        let url  = format!("{}/api/pull", self.base_url);

        let _resp = self.client
            .post(&url)
            .json(&OllamaPullRequest { name: name.clone(), stream: false })
            .send()
            .await?;

        Ok(LocalModel {
            id:           name.clone(),
            name:         super::openai_compat::pretty_name(&name),
            backend:      BackendKind::Ollama,
            format:       ModelFormat::Gguf,
            path:         None,
            size_bytes:   None,
            context_len:  None,
            param_count:  None,
            quantization: None,
            tags:         vec![],
        })
    }
}
