/// GPT4All backend — Default port: 4891, OpenAI-compat, GGUF

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use super::{openai_compat, BackendKind, ChatMessage, GenerationParams, LocalModel, LocalProvider, ModelFormat, StreamEvent};

pub struct Gpt4AllProvider { client: Client, base_url: String }

impl Gpt4AllProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

#[async_trait]
impl LocalProvider for Gpt4AllProvider {
    fn id(&self)       -> &str { "gpt4all" }
    fn name(&self)     -> &str { "GPT4All" }
    fn kind(&self)     -> BackendKind { BackendKind::Gpt4All }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client.get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        openai_compat::list_models(&self.client, &self.base_url, BackendKind::Gpt4All)
            .await.map(|m| m.into_iter().map(|mut x| { x.format = ModelFormat::Gguf; x }).collect())
    }

    async fn chat_stream(&self, messages: &[ChatMessage], model: &str, params: &GenerationParams) -> Result<mpsc::Receiver<StreamEvent>> {
        openai_compat::stream_chat(&self.client, &self.base_url, BackendKind::Gpt4All, messages, model, params).await
    }
}
