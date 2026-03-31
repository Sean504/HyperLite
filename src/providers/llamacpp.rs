/// llama.cpp server backend (llama-server)
///
/// Supports GGUF and legacy GGML formats.
/// Can also spawn a llama-server process directly from a .gguf file path.
///
/// Default port: 8080
/// API: OpenAI-compatible + llama.cpp extensions
/// Supports: GGUF (v1/v2/v3), GGML legacy, llamafile

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::mpsc;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

pub struct LlamaCppProvider {
    client:   Client,
    base_url: String,
    /// Optional path to llama-server binary (for spawning)
    binary:   Option<PathBuf>,
}

impl LlamaCppProvider {
    pub fn new(client: Client, base_url: &str) -> Self {
        // Try to find llama-server in PATH
        let binary = which::which("llama-server")
            .or_else(|_| which::which("llama-cpp-server"))
            .or_else(|_| which::which("server"))  // common build output name
            .ok();

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            binary,
        }
    }

    pub fn with_binary(mut self, path: PathBuf) -> Self {
        self.binary = Some(path);
        self
    }

    /// Spawn llama-server from a GGUF file.
    /// Returns a child process handle (caller must manage lifetime).
    pub async fn spawn_from_file(
        &self,
        model_path: &PathBuf,
        port:       u16,
        ctx_size:   u32,
        gpu_layers: u32,
    ) -> Result<tokio::process::Child> {
        let binary = self.binary.as_ref()
            .ok_or_else(|| anyhow::anyhow!(
                "llama-server not found in PATH. Install llama.cpp first."
            ))?;

        let child = tokio::process::Command::new(binary)
            .arg("--model")
            .arg(model_path)
            .arg("--port")
            .arg(port.to_string())
            .arg("--ctx-size")
            .arg(ctx_size.to_string())
            .arg("--n-gpu-layers")
            .arg(gpu_layers.to_string())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--no-webui")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(child)
    }
}

// ── llama.cpp-specific model info ─────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct LlamaCppModelInfo {
    id:    String,
    #[serde(default)]
    meta: Option<LlamaCppMeta>,
}

#[derive(Deserialize, Debug)]
struct LlamaCppMeta {
    #[serde(rename = "general.name")]
    name:              Option<String>,
    #[serde(rename = "general.parameter_count")]
    parameter_count:   Option<u64>,
    #[serde(rename = "llama.context_length")]
    context_length:    Option<u32>,
    #[serde(rename = "general.quantization_version")]
    quantization_ver:  Option<u32>,
    #[serde(rename = "general.file_type")]
    file_type:         Option<u32>,
}

// llama.cpp file_type encoding → quantization name
fn file_type_to_quant(ft: u32) -> &'static str {
    match ft {
        0  => "F32",
        1  => "F16",
        2  => "Q4_0",
        3  => "Q4_1",
        6  => "Q5_0",
        7  => "Q5_1",
        8  => "Q8_0",
        9  => "Q8_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "Q8_K",
        20 => "IQ2_XXS",
        21 => "IQ2_XS",
        _  => "unknown",
    }
}

fn param_count_to_str(count: u64) -> String {
    if count >= 1_000_000_000 {
        format!("{:.0}B", count as f64 / 1_000_000_000.0)
    } else if count >= 1_000_000 {
        format!("{:.0}M", count as f64 / 1_000_000.0)
    } else {
        format!("{}", count)
    }
}

// ── Provider implementation ───────────────────────────────────────────────────

#[async_trait]
impl LocalProvider for LlamaCppProvider {
    fn id(&self)       -> &str { "llamacpp" }
    fn name(&self)     -> &str { "llama.cpp" }
    fn kind(&self)     -> BackendKind { BackendKind::LlamaCpp }
    fn base_url(&self) -> &str { &self.base_url }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/health", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        // llama.cpp server loads a single model; try extended /v1/models first
        match openai_compat::list_models(&self.client, &self.base_url, BackendKind::LlamaCpp).await {
            Ok(models) => {
                // Enrich with llama.cpp /v1/models/{id} metadata if available
                let enriched: Vec<LocalModel> = models.into_iter().map(|mut m| {
                    // llama.cpp typically exposes GGUF models
                    m.format = ModelFormat::Gguf;
                    m
                }).collect();
                Ok(enriched)
            }
            Err(_) => {
                // Fallback: query /props for current loaded model info
                self.get_loaded_model().await
            }
        }
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
            BackendKind::LlamaCpp,
            messages,
            model,
            params,
        ).await
    }

    /// Load a GGUF/GGML file by spawning a new llama-server instance.
    async fn load_model(&self, path: &PathBuf) -> Result<LocalModel> {
        if !path.exists() {
            anyhow::bail!("Model file not found: {}", path.display());
        }

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let format = ModelFormat::from_extension(ext);

        // Determine a free port (simplistic)
        let port = 8090u16;
        let _child = self.spawn_from_file(path, port, 4096, 0).await?;

        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let name = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "loaded-model".to_string());

        Ok(LocalModel {
            id:           name.clone(),
            name:         openai_compat::pretty_name(&name),
            backend:      BackendKind::LlamaCpp,
            format,
            path:         Some(path.clone()),
            size_bytes:   path.metadata().ok().map(|m| m.len()),
            context_len:  None,
            param_count:  openai_compat::extract_param_count(&name),
            quantization: openai_compat::extract_quantization(&name),
            tags:         vec![],
        })
    }
}

impl LlamaCppProvider {
    /// Query /props for the currently loaded model info
    async fn get_loaded_model(&self) -> Result<Vec<LocalModel>> {
        #[derive(Deserialize)]
        struct Props {
            model_path: Option<String>,
        }

        let url = format!("{}/props", self.base_url);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let props: Props = resp.json().await.unwrap_or(Props { model_path: None });
        let model_path = match props.model_path {
            Some(p) => p,
            None    => return Ok(vec![]),
        };

        let path = PathBuf::from(&model_path);
        let stem = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(model_path.clone());

        Ok(vec![LocalModel {
            id:           stem.clone(),
            name:         openai_compat::pretty_name(&stem),
            backend:      BackendKind::LlamaCpp,
            format:       ModelFormat::Gguf,
            path:         Some(path.clone()),
            size_bytes:   path.metadata().ok().map(|m| m.len()),
            context_len:  None,
            param_count:  openai_compat::extract_param_count(&stem),
            quantization: openai_compat::extract_quantization(&stem),
            tags:         vec![],
        }])
    }
}
