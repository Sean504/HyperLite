/// llamafile backend
///
/// llamafile is a self-contained executable embedding a GGUF model +
/// llama.cpp inference engine in a single file. Run it and it spins up
/// an OpenAI-compatible server on port 8080.
///
/// Can also spawn a .llamafile from disk if found in PATH or user-specified path.
///
/// Default port: 8080
/// API: OpenAI-compatible

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct LlamafileProvider {
    client:   Client,
    base_url: String,
}

impl LlamafileProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        Self { client, base_url: base_url.trim_end_matches('/').to_string() }
    }
}

#[async_trait]
impl LocalProvider for LlamafileProvider {
    fn id(&self)       -> &str { "llamafile" }
    fn name(&self)     -> &str { "llamafile" }
    fn kind(&self)     -> BackendKind { BackendKind::Llamafile }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        openai_compat::list_models(&self.client, &self.base_url, BackendKind::Llamafile)
            .await.map(|m| m.into_iter().map(|mut x| {
                x.format = ModelFormat::Llamafile;
                x
            }).collect())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        openai_compat::stream_chat(
            &self.client, &self.base_url, BackendKind::Llamafile,
            messages, model, params,
        ).await
    }

    /// Spawn a .llamafile executable from disk.
    async fn load_model(&self, path: &PathBuf) -> Result<LocalModel> {
        if !path.exists() {
            anyhow::bail!("llamafile not found: {}", path.display());
        }

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = path.metadata()?.permissions();
            perms.set_mode(perms.mode() | 0o755);
            std::fs::set_permissions(path, perms)?;
        }

        let _child = tokio::process::Command::new(path)
            .arg("--port").arg("8080")
            .arg("--host").arg("127.0.0.1")
            .arg("--nobrowser")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        let stem = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "llamafile-model".to_string());

        Ok(LocalModel {
            id:           stem.clone(),
            name:         openai_compat::pretty_name(&stem),
            backend:      BackendKind::Llamafile,
            format:       ModelFormat::Llamafile,
            path:         Some(path.clone()),
            size_bytes:   path.metadata().ok().map(|m| m.len()),
            context_len:  None,
            param_count:  openai_compat::extract_param_count(&stem),
            quantization: openai_compat::extract_quantization(&stem),
            tags:         vec![],
        })
    }
}
