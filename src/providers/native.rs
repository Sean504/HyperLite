/// Native llama.cpp inference provider.
///
/// Models are loaded directly from GGUF files in ~/.hyperlite/models/.
/// No HTTP round-trips, no external server dependency.
///
/// When the `native-inference` feature is disabled this module still compiles —
/// `discover_models()` returns model metadata but `stream_generate` is a stub.

use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::providers::{LocalModel, BackendKind, StreamEvent};

// ── Model discovery ───────────────────────────────────────────────────────────

/// A GGUF model that has been found on disk.
#[derive(Debug, Clone)]
pub struct NativeModel {
    pub id:          String,   // e.g. "native:qwen2.5-coder-3b-instruct-q4_k_m"
    pub name:        String,   // e.g. "qwen2.5-coder-3b-instruct-q4_k_m"
    pub blob_path:   PathBuf,  // absolute path to the .gguf file
    pub size_bytes:  u64,
    pub context_len: u32,
}

impl NativeModel {
    pub fn to_local_model(&self) -> LocalModel {
        LocalModel {
            id:           self.id.clone(),
            name:         self.name.clone(),
            backend:      BackendKind::Native,
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

/// Scan ~/.hyperlite/models/ for .gguf files and return them as NativeModels.
pub fn discover_models() -> Vec<NativeModel> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let models_dir = home.join(".hyperlite").join("models");

    let Ok(entries) = std::fs::read_dir(&models_dir) else { return vec![] };

    let mut models = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let size = meta.len();

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        models.push(NativeModel {
            id:          format!("native:{}", stem),
            name:        stem,
            blob_path:   path,
            size_bytes:  size,
            context_len: context_len_from_size(size),
        });
    }

    models
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

    /// Stream tokens from a GGUF model directly.
    ///
    /// Runs the decode loop on a blocking thread so it doesn't stall the
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
            let mut n_cur    = tokens.len() as i32;
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

                if let Ok(fragment) = llama_model.token_to_str(token, llama_cpp_2::StringEncoding::Utf8) {
                    let _ = tx.blocking_send(StreamEvent::Text(fragment));
                }

                batch.clear();
                batch.add(token, n_cur, &[0], true).ok();
                n_cur    += 1;
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

    pub async fn stream_generate(
        _model: &NativeModel,
        _prompt: String,
        _max_tokens: u32,
    ) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(1);
        let _ = tx.send(StreamEvent::Error(
            "native-inference feature not compiled in this build".to_string()
        )).await;
        rx
    }
}
