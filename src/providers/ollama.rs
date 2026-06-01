/// Ollama backend
///
/// Default port: 11434
/// Uses Ollama's native /api/chat endpoint (works with all versions).
/// Falls back gracefully — no OpenAI-compat required.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;

use super::{
    BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct OllamaProvider {
    client:   Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

// ── Ollama API types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model:    &'a str,
    messages: Vec<OllamaMessage<'a>>,
    stream:   bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options:  Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaMessage<'a> {
    role:    &'a str,
    content: &'a str,
}

#[derive(Serialize)]
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
    repeat_penalty: Option<f32>,
}

#[derive(Deserialize)]
struct OllamaChatChunk {
    message:       Option<OllamaChunkMessage>,
    done:          bool,
    eval_count:    Option<u32>,
}

#[derive(Deserialize)]
struct OllamaChunkMessage {
    content: String,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
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
            .get(format!("{}/api/tags", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        let url  = format!("{}/api/tags", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let tags: OllamaTagsResponse = resp.json().await?;

        Ok(tags.models.into_iter().map(|m| {
            let name = m.name.clone();
            LocalModel {
                id:           name.clone(),
                name:         name.clone(),
                backend:      BackendKind::Ollama,
                format:       ModelFormat::Gguf,
                path:         None,
                size_bytes:   None,
                context_len:  None,
                param_count:  None,
                quantization: None,
                tags:         vec![name],
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

        let body = OllamaChatRequest {
            model,
            messages: messages.iter().map(|m| OllamaMessage {
                role:    &m.role,
                content: &m.content,
            }).collect(),
            stream: true,
            options: Some(OllamaOptions {
                temperature:    params.temperature,
                top_p:          params.top_p,
                top_k:          params.top_k,
                num_predict:    params.max_tokens,
                repeat_penalty: params.repeat_penalty,
            }),
        };

        let resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text   = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned {}: {}", status, text);
        }

        let (tx, rx) = mpsc::channel::<StreamEvent>(256);

        tokio::spawn(async move {
            use futures::StreamExt;
            let start = std::time::Instant::now();
            let mut token_count = 0u32;
            let mut stream = resp.bytes_stream();

            let mut buf = String::new();

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b)  => b,
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete newline-delimited JSON lines
                while let Some(nl) = buf.find('\n') {
                    let line = buf[..nl].trim().to_string();
                    buf = buf[nl + 1..].to_string();

                    if line.is_empty() { continue; }

                    match serde_json::from_str::<OllamaChatChunk>(&line) {
                        Ok(chunk) => {
                            if let Some(msg) = &chunk.message {
                                if !msg.content.is_empty() {
                                    token_count += 1;
                                    let _ = tx.send(StreamEvent::Text(msg.content.clone())).await;
                                }
                            }
                            if chunk.done {
                                let duration_ms = start.elapsed().as_millis() as u64;
                                let tokens = chunk.eval_count.or(if token_count > 0 { Some(token_count) } else { None });
                                let _ = tx.send(StreamEvent::Done { duration_ms, tokens_generated: tokens }).await;
                                return;
                            }
                        }
                        Err(_) => {} // skip malformed lines
                    }
                }
            }

            let _ = tx.send(StreamEvent::Done {
                duration_ms:      start.elapsed().as_millis() as u64,
                tokens_generated: if token_count > 0 { Some(token_count) } else { None },
            }).await;
        });

        Ok(rx)
    }

    async fn load_model(&self, _path: &PathBuf) -> Result<LocalModel> {
        anyhow::bail!("Ollama manages its own models. Use 'ollama pull <model>' to add models.")
    }
}

/// Standalone streaming function usable outside the provider trait.
pub async fn stream_chat_ollama(
    client:   &Client,
    base_url: &str,
    messages: &[super::ChatMessage],
    model:    &str,
    params:   &super::GenerationParams,
) -> anyhow::Result<tokio::sync::mpsc::Receiver<super::StreamEvent>> {
    let p = OllamaProvider::new(client.clone(), base_url);
    p.chat_stream(messages, model, params).await
}
