/// text-generation-webui (oobabooga) backend
///
/// The widest model format support via extensions:
/// GGUF, GPTQ, AWQ, EXL2, safetensors (.bin + .safetensors),
/// ONNX, 8-bit/4-bit bitsandbytes, DeepSpeed, AutoGPTQ, AQLM, HQQ.
///
/// Requires the "openai" extension to be enabled in the webui.
/// Default port: 5000 (webui) / 5000 (API via --api flag)
/// OpenAI-compat API at: http://localhost:5000/v1/

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct TextGenProvider {
    client:   Client,
    base_url: String,
}

impl TextGenProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

#[derive(Deserialize)]
struct TextGenModel {
    model_name: String,
}

#[async_trait]
impl LocalProvider for TextGenProvider {
    fn id(&self)       -> &str { "textgen" }
    fn name(&self)     -> &str { "text-generation-webui" }
    fn kind(&self)     -> BackendKind { BackendKind::TextGenWebui }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        // Try the OpenAI compat endpoint first
        self.client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        // Try OpenAI compat first
        if let Ok(models) = openai_compat::list_models(
            &self.client, &self.base_url, BackendKind::TextGenWebui,
        ).await {
            if !models.is_empty() {
                return Ok(models.into_iter().map(|mut m| {
                    m.format = detect_textgen_format(&m.id);
                    m
                }).collect());
            }
        }

        // Fall back to text-gen-webui's native /api/v1/model endpoint
        let url = format!("{}/api/v1/model", self.base_url);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let info: TextGenModel = resp.json().await?;
        let format = detect_textgen_format(&info.model_name);

        Ok(vec![LocalModel {
            id:           info.model_name.clone(),
            name:         openai_compat::pretty_name(&info.model_name),
            backend:      BackendKind::TextGenWebui,
            format,
            path:         None,
            size_bytes:   None,
            context_len:  None,
            param_count:  openai_compat::extract_param_count(&info.model_name),
            quantization: openai_compat::extract_quantization(&info.model_name),
            tags:         vec![],
        }])
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        openai_compat::stream_chat(
            &self.client, &self.base_url, BackendKind::TextGenWebui,
            messages, model, params,
        ).await
    }
}

/// Detect the likely format from a text-generation-webui model name.
/// These models live in model directories, not single files.
fn detect_textgen_format(name: &str) -> ModelFormat {
    let lower = name.to_lowercase();
    if lower.contains("gptq") || lower.contains("-gptq") { return ModelFormat::Gptq; }
    if lower.contains("awq")  || lower.contains("-awq")  { return ModelFormat::Awq; }
    if lower.contains("exl2") || lower.contains("-exl2") { return ModelFormat::Exl2; }
    if lower.contains(".gguf")                           { return ModelFormat::Gguf; }
    if lower.contains(".ggml")                           { return ModelFormat::GgmlLegacy; }
    // Default for HuggingFace-style model dirs is safetensors
    ModelFormat::SafeTensors
}
