/// LocalAI backend
///
/// Broadest format support of any local server:
/// GGUF, GGML, GPTQ, safetensors, ONNX, Whisper (audio), StableDiffusion.
/// Also supports GPU offloading, multiple backends (llama.cpp, bark, whisper).
///
/// Default port: 8080
/// API: Full OpenAI-compatible

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct LocalAIProvider {
    client:   Client,
    base_url: String,
}

impl LocalAIProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

#[async_trait]
impl LocalProvider for LocalAIProvider {
    fn id(&self)       -> &str { "localai" }
    fn name(&self)     -> &str { "LocalAI" }
    fn kind(&self)     -> BackendKind { BackendKind::LocalAI }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/readyz", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        openai_compat::list_models(&self.client, &self.base_url, BackendKind::LocalAI)
            .await
            .map(|models| models.into_iter().map(|mut m| {
                m.format = openai_compat::guess_format_from_id(&m.id);
                m
            }).collect())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        openai_compat::stream_chat(
            &self.client, &self.base_url, BackendKind::LocalAI,
            messages, model, params,
        ).await
    }
}
