/// LM Studio backend
///
/// Full OpenAI-compatible API. Supports GGUF and EXL2 formats.
/// Default port: 1234

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use std::path::PathBuf;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct LmStudioProvider {
    client:   Client,
    base_url: String,
}

impl LmStudioProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

#[async_trait]
impl LocalProvider for LmStudioProvider {
    fn id(&self)       -> &str { "lmstudio" }
    fn name(&self)     -> &str { "LM Studio" }
    fn kind(&self)     -> BackendKind { BackendKind::LmStudio }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        openai_compat::list_models(&self.client, &self.base_url, BackendKind::LmStudio)
            .await
            .map(|models| models.into_iter().map(|mut m| {
                m.format = if m.id.to_lowercase().contains("exl2") {
                    ModelFormat::Exl2
                } else {
                    ModelFormat::Gguf
                };
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
            &self.client, &self.base_url, BackendKind::LmStudio,
            messages, model, params,
        ).await
    }
}
