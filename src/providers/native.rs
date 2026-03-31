/// Native llama.cpp inference provider.
///
/// Bypasses Ollama's HTTP server entirely — models are loaded directly from the
/// GGUF blobs that Ollama (or any other tool) has already downloaded.  This
/// eliminates the HTTP round-trip and cross-process serialization overhead,
/// giving significantly lower TTFT (time-to-first-token) and higher throughput.
///
/// Model discovery order:
///   1. Ollama blob store  (~/.ollama/models/)
///   2. User-supplied GGUF directories in config (coming soon)
///
/// When the `native-inference` feature is disabled this module still compiles —
/// `discover_models()` returns model metadata but `stream_generate` is stubbed
/// (falls back to Ollama HTTP).

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tokio::sync::mpsc;
use serde::Deserialize;

use crate::providers::{LocalModel, BackendKind, StreamEvent};

// ── Model discovery ───────────────────────────────────────────────────────────

/// A GGUF model that has been found on disk.
#[derive(Debug, Clone)]
pub struct NativeModel {
    pub id:          String,   // e.g. "ollama:llama3.2:3b"
    pub name:        String,   // e.g. "llama3.2:3b"
    pub blob_path:   PathBuf,  // absolute path to the .gguf / blob file
    pub size_bytes:  u64,
    pub context_len: u32,
}

impl NativeModel {
    pub fn to_local_model(&self) -> LocalModel {
        LocalModel {
            id:           self.id.clone(),
            name:         self.name.clone(),
            backend:      BackendKind::OllamaNative,
            format:       crate::providers::ModelFormat::Gguf,
            path:         Some(self.blob_path.clone()),
            size_bytes:   Some(self.size_bytes),
            context_len:  Some(self.context_len),
            param_count:  None,
            quantization: None,
            tags:         vec!["native".to_string()],
        }
    }
}

/// Scan the Ollama model store and return all GGUF blobs we can load directly.
pub fn discover_models() -> Vec<NativeModel> {
    let mut models = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let manifest_root = home
            .join(".ollama")
            .join("models")
            .join("manifests")
            .join("registry.ollama.ai")
            .join("library");

        let blob_root = home.join(".ollama").join("models").join("blobs");

        models.extend(scan_ollama_manifests(&manifest_root, &blob_root));
    }

    models
}

// ── Ollama manifest parsing ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct OllamaManifest {
    layers: Vec<OllamaLayer>,
}

#[derive(Deserialize)]
struct OllamaLayer {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest:     String,
    size:       u64,
}

fn scan_ollama_manifests(manifest_root: &Path, blob_root: &Path) -> Vec<NativeModel> {
    let mut out = Vec::new();

    let Ok(model_dirs) = std::fs::read_dir(manifest_root) else { return out; };

    for entry in model_dirs.flatten() {
        let model_name_dir = entry.path();
        if !model_name_dir.is_dir() { continue; }

        let model_family = entry.file_name().to_string_lossy().to_string();

        // Each subdirectory is a tag (e.g. "latest", "3b", "7b-instruct", …)
        let Ok(tag_entries) = std::fs::read_dir(&model_name_dir) else { continue; };
        for tag_entry in tag_entries.flatten() {
            let tag = tag_entry.file_name().to_string_lossy().to_string();
            let manifest_path = tag_entry.path();

            let Ok(raw) = std::fs::read_to_string(&manifest_path) else { continue; };
            let Ok(manifest) = serde_json::from_str::<OllamaManifest>(&raw) else { continue; };

            // Find the model (GGUF) layer — media type contains "model"
            let model_layer = manifest.layers.iter().find(|l| {
                l.media_type.contains("model") || l.media_type.contains("gguf")
            });

            if let Some(layer) = model_layer {
                // Ollama blob filenames: replace ":" with "-" in the digest
                let blob_file = layer.digest.replace(':', "-");
                let blob_path = blob_root.join(&blob_file);

                if !blob_path.exists() { continue; }

                let display_name = if tag == "latest" {
                    model_family.clone()
                } else {
                    format!("{}:{}", model_family, tag)
                };

                out.push(NativeModel {
                    id:          format!("native:{}", display_name),
                    name:        display_name,
                    blob_path,
                    size_bytes:  layer.size,
                    context_len: context_len_from_size(layer.size),
                });
            }
        }
    }

    out
}

/// Rough heuristic: estimate context window from model file size.
fn context_len_from_size(bytes: u64) -> u32 {
    let gb = bytes as f64 / 1_073_741_824.0;
    match gb as u32 {
        0..=2  => 4096,
        3..=5  => 8192,
        6..=10 => 16384,
        _      => 32768,
    }
}

// ── Inference ─────────────────────────────────────────────────────────────────

#[cfg(feature = "native-inference")]
pub mod inference {
    use super::*;
    use llama_cpp_2::{
        context::params::LlamaContextParams,
        llama_backend::LlamaBackend,
        llama_model::{AddBos, LlamaModel, LlamaModelParams},
        token::data_array::LlamaTokenDataArray,
    };

    /// Lazily-initialised per-process llama backend.
    static BACKEND: std::sync::OnceLock<LlamaBackend> = std::sync::OnceLock::new();

    fn backend() -> &'static LlamaBackend {
        BACKEND.get_or_init(|| {
            LlamaBackend::init().expect("Failed to initialise llama.cpp backend")
        })
    }

    /// Stream tokens from a GGUF model directly, bypassing Ollama's HTTP server.
    ///
    /// This runs the decode loop on a blocking thread so it doesn't stall the
    /// async runtime.  Tokens are sent over the returned channel.
    pub async fn stream_generate(
        model: &NativeModel,
        prompt: String,
        max_tokens: u32,
    ) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(256);
        let path       = model.blob_path.clone();
        let ctx_len    = model.context_len;

        tokio::task::spawn_blocking(move || {
            let bk = backend();

            let model_params = LlamaModelParams::default();
            let llama_model = match LlamaModel::load_from_file(bk, &path, &model_params) {
                Ok(m)  => m,
                Err(e) => {
                    let _ = tx.blocking_send(StreamEvent::Error(format!("Model load failed: {}", e)));
                    return;
                }
            };

            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZeroU32::new(ctx_len).unwrap_or(
                    std::num::NonZeroU32::new(4096).unwrap()
                ));
            let mut ctx = match llama_model.new_context(bk, ctx_params) {
                Ok(c)  => c,
                Err(e) => {
                    let _ = tx.blocking_send(StreamEvent::Error(format!("Context create failed: {}", e)));
                    return;
                }
            };

            let tokens = match llama_model.str_to_token(&prompt, AddBos::Always) {
                Ok(t)  => t,
                Err(e) => {
                    let _ = tx.blocking_send(StreamEvent::Error(format!("Tokenize failed: {}", e)));
                    return;
                }
            };

            // Evaluate the prompt
            use llama_cpp_2::llama_batch::LlamaBatch;
            let mut batch = LlamaBatch::new(512, 1);
            let last_idx = (tokens.len() - 1) as i32;
            for (i, token) in tokens.iter().enumerate() {
                let is_last = i as i32 == last_idx;
                batch.add(*token, i as i32, &[0], is_last).ok();
            }
            if ctx.decode(&mut batch).is_err() {
                let _ = tx.blocking_send(StreamEvent::Error("Prompt decode failed".to_string()));
                return;
            }

            // Sampling loop
            let mut n_cur   = tokens.len() as i32;
            let mut n_decode = 0u32;
            let eos_token    = llama_model.token_eos();

            loop {
                if n_decode >= max_tokens { break; }

                let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
                let mut arr    = LlamaTokenDataArray::from_iter(candidates, false);
                ctx.sample_temp(None, &mut arr, 0.8);
                ctx.sample_top_p(None, &mut arr, 0.9, 1);
                let token = ctx.sample_token(None, &mut arr);

                if token == eos_token { break; }

                // Decode token → UTF-8 fragment
                if let Ok(fragment) = llama_model.token_to_str(token, llama_cpp_2::StringEncoding::Utf8) {
                    let _ = tx.blocking_send(StreamEvent::Text(fragment));
                }

                batch.clear();
                batch.add(token, n_cur, &[0], true).ok();
                n_cur   += 1;
                n_decode += 1;

                if ctx.decode(&mut batch).is_err() { break; }
            }

            let _ = tx.blocking_send(StreamEvent::Done {
                duration_ms: 0,
                tokens_generated: Some(n_decode),
            });
        });

        rx
    }
}

// ── Stub when feature is off ──────────────────────────────────────────────────

#[cfg(not(feature = "native-inference"))]
pub mod inference {
    use super::*;

    /// Without the `native-inference` feature this just signals the caller to
    /// fall back to the Ollama HTTP provider.
    pub async fn stream_generate(
        _model: &NativeModel,
        _prompt: String,
        _max_tokens: u32,
    ) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(1);
        let _ = tx.send(StreamEvent::Error(
            "native-inference feature not compiled; falling back to Ollama HTTP".to_string()
        )).await;
        rx
    }
}
