/// HyperLite Local Provider System
///
/// Supports every major local inference backend and model format.
/// No cloud APIs — fully offline, fully private.
///
/// Backends supported:
///   • Native GGUF     (direct llama.cpp, ~/.hyperlite/models/)
///   • llama.cpp       (GGUF, GGML legacy)       localhost:8080
///   • LM Studio       (GGUF, EXL2)              localhost:1234
///   • KoboldCpp       (GGUF, GGML, legacy)      localhost:5001
///   • text-gen-webui  (GGUF, GPTQ, AWQ, EXL2, safetensors, .bin, ONNX)
///   • LocalAI         (GGUF, GPTQ, safetensors, ONNX, Whisper, DALL-E)
///   • Jan.ai          (GGUF)                    localhost:1337
///   • llamafile       (GGUF self-contained)     localhost:8080
///   • vLLM            (safetensors, GPTQ, AWQ)  localhost:8000
///   • GPT4All         (GGUF)                    localhost:4891
///   • Direct GGUF     (spawn llama-server from .gguf file)
///
/// Model formats:
///   GGUF, GGML (legacy), GPTQ, AWQ, EXL2, SafeTensors, PyTorch .bin,
///   ONNX, MLX, llamafile, GGUF v1/v2/v3

pub mod openai_compat;
pub mod native;
pub mod llamacpp;
pub mod kobold;
pub mod lmstudio;
pub mod localai;
pub mod textgen;
pub mod jan;
pub mod llamafile;
pub mod vllm;
pub mod gpt4all;
pub mod direct;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;

// ── Model Format Enum ─────────────────────────────────────────────────────────

/// All model file formats HyperLite can work with (via appropriate backends).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelFormat {
    /// .gguf — current llama.cpp format (v1/v2/v3)
    Gguf,
    /// .bin (ggml tensors) — legacy llama.cpp format pre-GGUF
    GgmlLegacy,
    /// GPTQ quantization — GPU quantized, HuggingFace
    Gptq,
    /// AWQ (Activation-aware Weight Quantization)
    Awq,
    /// ExLlamaV2 format (.safetensors + .exl2)
    Exl2,
    /// .safetensors — HuggingFace standard
    SafeTensors,
    /// .bin (PyTorch) — legacy HuggingFace format
    PyTorchBin,
    /// .onnx — cross-platform ONNX
    Onnx,
    /// Apple MLX format
    Mlx,
    /// .llamafile — self-contained GGUF + inference runtime
    Llamafile,
    /// Any format the backend handles that we don't enumerate
    Unknown,
}

impl ModelFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "gguf"        => ModelFormat::Gguf,
            "ggml"        => ModelFormat::GgmlLegacy,
            "safetensors" => ModelFormat::SafeTensors,
            "onnx"        => ModelFormat::Onnx,
            "llamafile"   => ModelFormat::Llamafile,
            "bin"         => ModelFormat::PyTorchBin, // ambiguous, but common
            "exl2"        => ModelFormat::Exl2,
            _             => ModelFormat::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ModelFormat::Gguf        => "GGUF",
            ModelFormat::GgmlLegacy  => "GGML (legacy)",
            ModelFormat::Gptq        => "GPTQ",
            ModelFormat::Awq         => "AWQ",
            ModelFormat::Exl2        => "EXL2",
            ModelFormat::SafeTensors => "SafeTensors",
            ModelFormat::PyTorchBin  => "PyTorch .bin",
            ModelFormat::Onnx        => "ONNX",
            ModelFormat::Mlx         => "MLX",
            ModelFormat::Llamafile   => "llamafile",
            ModelFormat::Unknown     => "unknown",
        }
    }
}

// ── Backend Enum ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendKind {
    Native,        // direct GGUF load from ~/.hyperlite/models/ (no HTTP)
    LlamaCpp,
    LmStudio,
    KoboldCpp,
    TextGenWebui,
    LocalAI,
    Jan,
    Llamafile,
    Vllm,
    Gpt4All,
    DirectGguf,
    OpenAICompat,  // generic fallback
}

impl BackendKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            BackendKind::Native        => "Native (llama.cpp)",
            BackendKind::LlamaCpp      => "llama.cpp",
            BackendKind::LmStudio      => "LM Studio",
            BackendKind::KoboldCpp     => "KoboldCpp",
            BackendKind::TextGenWebui  => "text-generation-webui",
            BackendKind::LocalAI       => "LocalAI",
            BackendKind::Jan           => "Jan.ai",
            BackendKind::Llamafile     => "llamafile",
            BackendKind::Vllm          => "vLLM",
            BackendKind::Gpt4All       => "GPT4All",
            BackendKind::DirectGguf    => "Direct GGUF",
            BackendKind::OpenAICompat  => "OpenAI-compat",
        }
    }

    /// Default port for this backend
    pub fn default_port(&self) -> u16 {
        match self {
            BackendKind::Native        => 0,  // no port — in-process
            BackendKind::LlamaCpp      => 8080,
            BackendKind::LmStudio      => 1234,
            BackendKind::KoboldCpp     => 5001,
            BackendKind::TextGenWebui  => 5000,
            BackendKind::LocalAI       => 8080,
            BackendKind::Jan           => 1337,
            BackendKind::Llamafile     => 8080,
            BackendKind::Vllm          => 8000,
            BackendKind::Gpt4All       => 4891,
            BackendKind::DirectGguf    => 8080,
            BackendKind::OpenAICompat  => 8080,
        }
    }

    /// Which model formats this backend can serve
    pub fn supported_formats(&self) -> Vec<ModelFormat> {
        match self {
            BackendKind::Native => vec![
                ModelFormat::Gguf,
            ],
            BackendKind::LlamaCpp => vec![
                ModelFormat::Gguf,
                ModelFormat::GgmlLegacy,
            ],
            BackendKind::LmStudio => vec![
                ModelFormat::Gguf,
                ModelFormat::Exl2,
            ],
            BackendKind::KoboldCpp => vec![
                ModelFormat::Gguf,
                ModelFormat::GgmlLegacy,
            ],
            BackendKind::TextGenWebui => vec![
                ModelFormat::Gguf,
                ModelFormat::Gptq,
                ModelFormat::Awq,
                ModelFormat::Exl2,
                ModelFormat::SafeTensors,
                ModelFormat::PyTorchBin,
                ModelFormat::Onnx,
            ],
            BackendKind::LocalAI => vec![
                ModelFormat::Gguf,
                ModelFormat::GgmlLegacy,
                ModelFormat::Gptq,
                ModelFormat::SafeTensors,
                ModelFormat::Onnx,
            ],
            BackendKind::Jan => vec![
                ModelFormat::Gguf,
            ],
            BackendKind::Llamafile => vec![
                ModelFormat::Llamafile,
                ModelFormat::Gguf,
            ],
            BackendKind::Vllm => vec![
                ModelFormat::SafeTensors,
                ModelFormat::Gptq,
                ModelFormat::Awq,
                ModelFormat::Exl2,
            ],
            BackendKind::Gpt4All => vec![
                ModelFormat::Gguf,
            ],
            BackendKind::DirectGguf => vec![
                ModelFormat::Gguf,
                ModelFormat::GgmlLegacy,
            ],
            BackendKind::OpenAICompat  => vec![ModelFormat::Unknown],
        }
    }
}

// ── LocalModel ────────────────────────────────────────────────────────────────

/// A model available on a local backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    /// Unique identifier used in API calls (e.g. "llama3:8b", "mistral-7b-q4")
    pub id:           String,
    /// Human-readable display name
    pub name:         String,
    /// Which backend serves this model
    pub backend:      BackendKind,
    /// File format if known
    pub format:       ModelFormat,
    /// Model file path on disk (if known)
    pub path:         Option<PathBuf>,
    /// File size in bytes (if known)
    pub size_bytes:   Option<u64>,
    /// Context length in tokens (if reported by backend)
    pub context_len:  Option<u32>,
    /// Parameter count string (e.g. "7B", "13B", "70B")
    pub param_count:  Option<String>,
    /// Quantization description (e.g. "Q4_K_M", "Q8_0", "FP16")
    pub quantization: Option<String>,
    /// Extra tags/families from the backend
    pub tags:         Vec<String>,
}

impl LocalModel {
    pub fn display_size(&self) -> String {
        match self.size_bytes {
            Some(b) if b >= 1_073_741_824 => format!("{:.1} GB", b as f64 / 1_073_741_824.0),
            Some(b) if b >= 1_048_576     => format!("{:.0} MB", b as f64 / 1_048_576.0),
            Some(b)                       => format!("{} B", b),
            None                          => String::new(),
        }
    }

    pub fn subtitle(&self) -> String {
        let mut parts = vec![];
        if let Some(ref q) = self.quantization  { parts.push(q.as_str().to_string()); }
        if let Some(ref p) = self.param_count   { parts.push(p.clone()); }
        let size = self.display_size();
        if !size.is_empty() { parts.push(size); }
        parts.push(self.format.display_name().to_string());
        parts.join("  ·  ")
    }
}

// ── Chat types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role:    String,  // "user" | "assistant" | "system"
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    pub temperature:  Option<f32>,
    pub top_p:        Option<f32>,
    pub top_k:        Option<u32>,
    pub max_tokens:   Option<u32>,
    pub stop:         Vec<String>,
    pub seed:         Option<i64>,
    pub repeat_penalty: Option<f32>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            temperature:   Some(0.7),
            top_p:         Some(0.9),
            top_k:         Some(40),
            max_tokens:    Some(8192),
            stop:          vec![],
            seed:          None,
            repeat_penalty: Some(1.1),
        }
    }
}

// ── Stream Events ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum StreamEvent {
    /// A chunk of assistant text
    Text(String),
    /// A chunk of reasoning/thinking (supported by some models)
    Reasoning(String),
    /// Generation complete
    Done { duration_ms: u64, tokens_generated: Option<u32> },
    /// Error from the backend
    Error(String),
}

// ── Backend Health ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BackendStatus {
    pub kind:      BackendKind,
    pub base_url:  String,
    pub reachable: bool,
    pub version:   Option<String>,
    pub models:    Vec<LocalModel>,
    pub error:     Option<String>,
}

// ── Provider Trait ────────────────────────────────────────────────────────────

#[async_trait]
pub trait LocalProvider: Send + Sync {
    /// Short machine ID (e.g. "ollama", "llamacpp")
    fn id(&self) -> &str;

    /// Human display name
    fn name(&self) -> &str;

    /// Which backend kind this is
    fn kind(&self) -> BackendKind;

    /// Base URL of the local server
    fn base_url(&self) -> &str;

    /// Check if the server is running and reachable
    async fn health_check(&self) -> bool;

    /// List all models currently available on this backend
    async fn list_models(&self) -> Result<Vec<LocalModel>>;

    /// Stream a chat completion.
    /// Returns an mpsc::Receiver that yields StreamEvents until Done/Error.
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>>;

    /// Optional: load a model file from disk into this backend.
    /// Not all backends support this.
    async fn load_model(&self, _path: &PathBuf) -> Result<LocalModel> {
        anyhow::bail!("{} does not support loading models directly", self.name())
    }
}

// ── Registry ─────────────────────────────────────────────────────────────────

/// Holds all configured provider instances.
pub struct ProviderRegistry {
    providers: Vec<Box<dyn LocalProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self { providers: vec![] }
    }

    /// Register all default local backends with their standard ports.
    /// Backends that aren't running will show as offline — no error.
    pub fn with_defaults(client: reqwest::Client, hardware: crate::hardware::HardwareInfo) -> Self {
        use crate::providers::*;
        let mut reg = Self::new();

        reg.add(Box::new(llamacpp::LlamaCppProvider::new(
            client.clone(), "http://localhost:8080",
        )));
        reg.add(Box::new(lmstudio::LmStudioProvider::new(
            client.clone(), "http://localhost:1234",
        )));
        reg.add(Box::new(kobold::KoboldCppProvider::new(
            client.clone(), "http://localhost:5001",
        )));
        reg.add(Box::new(textgen::TextGenProvider::new(
            client.clone(), "http://localhost:5000",
        )));
        reg.add(Box::new(localai::LocalAIProvider::new(
            client.clone(), "http://localhost:8080",
        )));
        reg.add(Box::new(jan::JanProvider::new(
            client.clone(), "http://localhost:1337",
        )));
        reg.add(Box::new(vllm::VllmProvider::new(
            client.clone(), "http://localhost:8000",
        )));
        reg.add(Box::new(gpt4all::Gpt4AllProvider::new(
            client.clone(), "http://localhost:4891",
        )));
        reg.add(Box::new(direct::DirectGgufProvider::new(client.clone(), hardware)));

        reg
    }

    pub fn add(&mut self, provider: Box<dyn LocalProvider>) {
        self.providers.push(provider);
    }

    pub fn get(&self, id: &str) -> Option<&dyn LocalProvider> {
        self.providers.iter().find(|p| p.id() == id).map(|p| p.as_ref())
    }

    pub fn all(&self) -> &[Box<dyn LocalProvider>] {
        &self.providers
    }

    /// Poll all backends concurrently and return their status.
    pub async fn probe_all(&self) -> Vec<BackendStatus> {
        use futures::future::join_all;

        let futures: Vec<_> = self.providers.iter().map(|p| async move {
            let reachable = p.health_check().await;
            let (models, error) = if reachable {
                match p.list_models().await {
                    Ok(m)  => (m, None),
                    Err(e) => (vec![], Some(e.to_string())),
                }
            } else {
                (vec![], None)
            };

            BackendStatus {
                kind:      p.kind(),
                base_url:  p.base_url().to_string(),
                reachable,
                version:   None,
                models,
                error,
            }
        }).collect();

        join_all(futures).await
    }

    /// List all models across all reachable backends, including any GGUF files
    /// found in ~/.hyperlite/models/ for native (zero-HTTP) inference.
    pub async fn all_models(&self) -> Vec<LocalModel> {
        use futures::future::join_all;

        let futures: Vec<_> = self.providers.iter().map(|p| async move {
            if p.health_check().await {
                p.list_models().await.unwrap_or_default()
            } else {
                vec![]
            }
        }).collect();

        let mut models: Vec<LocalModel> = join_all(futures).await.into_iter().flatten().collect();

        // When native-inference is compiled in, prepend in-process GGUF models
        // (faster — no HTTP). They deduplicate against anything found by DirectGguf.
        // Without the feature, DirectGgufProvider already covers ~/.hyperlite/models/.
        #[cfg(feature = "native-inference")]
        {
            let native = native::discover_models();
            let native_names: std::collections::HashSet<_> =
                native.iter().map(|m| m.name.as_str()).collect();
            models.retain(|m| !native_names.contains(m.name.as_str()));
            let mut native_local: Vec<LocalModel> = native.iter().map(|m| m.to_local_model()).collect();
            native_local.extend(models);
            return native_local;
        }

        #[allow(unreachable_code)]
        models
    }

    /// Find a provider that can serve the given model ID.
    pub fn find_for_model(&self, model_id: &str) -> Option<&dyn LocalProvider> {
        // Try exact provider ID prefix first: "ollama/llama3" -> ollama
        if let Some(slash) = model_id.find('/') {
            let prefix = &model_id[..slash];
            if let Some(p) = self.get(prefix) {
                return Some(p);
            }
        }
        // Fall back to first provider registered
        self.providers.first().map(|p| p.as_ref())
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self { Self::new() }
}
