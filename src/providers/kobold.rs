/// KoboldCpp backend
///
/// KoboldCpp supports the widest range of LEGACY formats including very old
/// GGML variants, GGUF, and older model architectures (GPT-J, GPT-NeoX,
/// RWKV, Falcon, etc.) — making it the best choice for vintage/legacy models.
///
/// Default port: 5001
/// Supports: GGUF, GGML (all legacy variants), GPT-J, GPT-NeoX, RWKV, Falcon
/// API: KoboldCpp native + partial OpenAI compat at /v1/

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct KoboldCppProvider {
    client:   Client,
    base_url: String,
}

impl KoboldCppProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

// ── KoboldCpp API types ───────────────────────────────────────────────────────

/// KoboldCpp /api/v1/generate  (streaming via SSE on /api/extra/generate/stream)
#[derive(Serialize)]
struct KoboldGenRequest {
    prompt:           String,
    max_length:       u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature:      Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p:            Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k:            Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rep_pen:          Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequence:    Vec<String>,
    stream:           bool,
}

#[derive(Deserialize, Debug)]
struct KoboldStreamToken {
    token: String,
}

#[derive(Deserialize, Debug)]
struct KoboldModelInfo {
    result: String,
}

#[derive(Deserialize, Debug)]
struct KoboldVersion {
    result: String,
}

/// Convert a list of chat messages into a single prompt using ChatML format.
/// KoboldCpp uses raw completions, so we must format the conversation ourselves.
fn messages_to_chatml(messages: &[ChatMessage]) -> String {
    let mut prompt = String::new();
    for msg in messages {
        match msg.role.as_str() {
            "system"    => prompt.push_str(&format!("<|im_start|>system\n{}<|im_end|>\n", msg.content)),
            "user"      => prompt.push_str(&format!("<|im_start|>user\n{}<|im_end|>\n", msg.content)),
            "assistant" => prompt.push_str(&format!("<|im_start|>assistant\n{}<|im_end|>\n", msg.content)),
            _           => prompt.push_str(&format!("{}\n", msg.content)),
        }
    }
    prompt.push_str("<|im_start|>assistant\n");
    prompt
}

// ── Provider implementation ───────────────────────────────────────────────────

#[async_trait]
impl LocalProvider for KoboldCppProvider {
    fn id(&self)       -> &str { "koboldcpp" }
    fn name(&self)     -> &str { "KoboldCpp" }
    fn kind(&self)     -> BackendKind { BackendKind::KoboldCpp }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/api/v1/info/version", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        // First try OpenAI compat endpoint
        if let Ok(models) = openai_compat::list_models(
            &self.client, &self.base_url, BackendKind::KoboldCpp
        ).await {
            if !models.is_empty() {
                return Ok(models.into_iter().map(|mut m| {
                    m.format = openai_compat::guess_format_from_id(&m.id);
                    m
                }).collect());
            }
        }

        // Fall back to KoboldCpp native model endpoint
        let url = format!("{}/api/v1/model", self.base_url);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let info: KoboldModelInfo = resp.json().await?;
        let name = info.result;
        let format = openai_compat::guess_format_from_id(&name);

        Ok(vec![LocalModel {
            id:           name.clone(),
            name:         openai_compat::pretty_name(&name),
            backend:      BackendKind::KoboldCpp,
            format,
            path:         None,
            size_bytes:   None,
            context_len:  None,
            param_count:  openai_compat::extract_param_count(&name),
            quantization: openai_compat::extract_quantization(&name),
            tags:         vec![],
        }])
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        // Try OpenAI-compat endpoint first (KoboldCpp 1.60+ supports it)
        if let Ok(rx) = openai_compat::stream_chat(
            &self.client,
            &self.base_url,
            BackendKind::KoboldCpp,
            messages,
            model,
            params,
        ).await {
            return Ok(rx);
        }

        // Fall back to KoboldCpp streaming generate endpoint
        self.kobold_stream(messages, params).await
    }
}

impl KoboldCppProvider {
    async fn kobold_stream(
        &self,
        messages: &[ChatMessage],
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let url = format!("{}/api/extra/generate/stream", self.base_url);
        let prompt = messages_to_chatml(messages);

        let body = KoboldGenRequest {
            prompt,
            max_length:    params.max_tokens.unwrap_or(2048),
            temperature:   params.temperature,
            top_p:         params.top_p,
            top_k:         params.top_k,
            rep_pen:       params.repeat_penalty,
            stop_sequence: params.stop.clone(),
            stream:        true,
        };

        let response = self.client.post(&url).json(&body).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("KoboldCpp returned {}", response.status());
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

                while let Some(newline) = buf.find('\n') {
                    let line = buf[..newline].trim().to_string();
                    buf = buf[newline + 1..].to_string();

                    if line.is_empty() || !line.starts_with("data: ") { continue; }
                    let data = line["data: ".len()..].trim();

                    if data == "[DONE]" {
                        let ms = start.elapsed().as_millis() as u64;
                        let _ = tx.send(StreamEvent::Done { duration_ms: ms, tokens_generated: None }).await;
                        return;
                    }

                    if let Ok(tok) = serde_json::from_str::<KoboldStreamToken>(data) {
                        if !tok.token.is_empty() {
                            let _ = tx.send(StreamEvent::Text(tok.token)).await;
                        }
                    }
                }
            }

            let ms = start.elapsed().as_millis() as u64;
            let _ = tx.send(StreamEvent::Done { duration_ms: ms, tokens_generated: None }).await;
        });

        Ok(rx)
    }
}
