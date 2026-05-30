/// Ollama backend
///
/// Default port: 11434
/// API: OpenAI-compatible (/v1/chat/completions, /v1/models)
/// Supports: GGUF and any format Ollama manages

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use std::path::PathBuf;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, StreamEvent,
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
        openai_compat::list_models(&self.client, &self.base_url, BackendKind::Ollama).await
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        openai_compat::stream_chat(
            &self.client,
            &self.base_url,
            BackendKind::Ollama,
            messages,
            model,
            params,
        ).await
    }

    async fn load_model(&self, _path: &PathBuf) -> Result<LocalModel> {
        anyhow::bail!("Ollama manages its own models. Use 'ollama pull <model>' to add models.")
    }
}
