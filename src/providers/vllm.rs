/// vLLM backend
///
/// High-throughput GPU inference. Best for safetensors, GPTQ, AWQ, EXL2.
/// Default port: 8000. Full OpenAI-compatible API.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use super::{openai_compat, BackendKind, ChatMessage, GenerationParams, LocalModel, LocalProvider, StreamEvent};

pub struct VllmProvider { client: Client, base_url: String }

impl VllmProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

#[async_trait]
impl LocalProvider for VllmProvider {
    fn id(&self)       -> &str { "vllm" }
    fn name(&self)     -> &str { "vLLM" }
    fn kind(&self)     -> BackendKind { BackendKind::Vllm }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client.get(format!("{}/health", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        openai_compat::list_models(&self.client, &self.base_url, BackendKind::Vllm)
            .await.map(|m| m.into_iter().map(|mut x| {
                x.format = openai_compat::guess_format_from_id(&x.id);
                x
            }).collect())
    }

    async fn chat_stream(&self, messages: &[ChatMessage], model: &str, params: &GenerationParams) -> Result<mpsc::Receiver<StreamEvent>> {
        openai_compat::stream_chat(&self.client, &self.base_url, BackendKind::Vllm, messages, model, params).await
    }
}
