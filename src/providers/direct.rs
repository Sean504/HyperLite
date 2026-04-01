/// Direct GGUF / model file provider
///
/// Scans user-configured model directories for model files of any format.
/// Spawns the best available local runtime (llama-server, llamafile, etc.)
/// to serve the selected file on a dynamic port.
///
/// This is the "offline, no-daemon" mode — point at a .gguf file and go.
///
/// Supported file extensions and their formats:
///   .gguf         → GGUF (current llama.cpp)
///   .ggml         → GGML legacy
///   .bin          → PyTorch or GGML (ambiguous, detected by content)
///   .safetensors  → SafeTensors (HuggingFace)
///   .gguf.part*   → Split GGUF (multi-file, reassembled by llama.cpp)
///   .llamafile    → self-contained llamafile executable

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;
use walkdir::WalkDir;

use super::{
    openai_compat, BackendKind, ChatMessage, GenerationParams,
    LocalModel, LocalProvider, ModelFormat, StreamEvent,
};

/// Default directories to scan for model files
const DEFAULT_MODEL_DIRS: &[&str] = &[
    "~/.hyperlite/models",   // HyperLite's own download directory (always first)
    "~/.cache/huggingface",
    "~/models",
    "~/Models",
    "~/lm-studio/models",
    "/opt/models",
];

/// File extensions we recognize as model files
const MODEL_EXTENSIONS: &[&str] = &[
    "gguf", "ggml", "bin", "safetensors", "llamafile",
    "onnx", "exl2",
];

pub struct DirectGgufProvider {
    client:      Client,
    model_dirs:  Vec<PathBuf>,
    /// Active spawned process (base_url → process)
    active_port: u16,
}

impl DirectGgufProvider {
    pub fn new(client: Client) -> Self {
        let model_dirs = DEFAULT_MODEL_DIRS
            .iter()
            .filter_map(|dir| {
                let expanded = expand_tilde(dir);
                let path = PathBuf::from(&expanded);
                if path.exists() { Some(path) } else { None }
            })
            .collect();

        Self { client, model_dirs, active_port: 18080 }
    }

    pub fn with_model_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.model_dirs = dirs;
        self
    }

    /// Scan all configured directories for model files.
    pub fn scan_model_files(&self) -> Vec<LocalModel> {
        let mut models = vec![];

        for dir in &self.model_dirs {
            for entry in WalkDir::new(dir)
                .follow_links(true)
                .max_depth(5)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path().to_path_buf();
                let ext = path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !MODEL_EXTENSIONS.contains(&ext.as_str()) {
                    continue;
                }

                // Skip split GGUF files (only show the first part or the merged)
                let filename = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                if filename.contains(".gguf-split-") && !filename.ends_with("-00001-of-") {
                    continue;
                }

                let format = ModelFormat::from_extension(&ext);
                let stem = path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(filename.clone());

                let size = path.metadata().ok().map(|m| m.len());

                models.push(LocalModel {
                    id:           path.to_string_lossy().to_string(),
                    name:         openai_compat::pretty_name(&stem),
                    backend:      BackendKind::DirectGguf,
                    format,
                    path:         Some(path.clone()),
                    size_bytes:   size,
                    context_len:  None,
                    param_count:  openai_compat::extract_param_count(&stem),
                    quantization: openai_compat::extract_quantization(&stem),
                    tags:         vec![stem],
                });
            }
        }

        // Sort by size descending (largest first = probably most capable)
        models.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        models
    }

    /// Spawn the best available runtime for the given model file.
    /// Priority: llama-server > llamafile > error
    async fn spawn_runtime(
        &self,
        path:    &PathBuf,
        port:    u16,
    ) -> Result<tokio::process::Child> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // llamafile: run it directly
        if ext == "llamafile" {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = path.metadata()?.permissions();
                perms.set_mode(perms.mode() | 0o755);
                std::fs::set_permissions(path, perms)?;
            }
            return Ok(tokio::process::Command::new(path)
                .arg("--port").arg(port.to_string())
                .arg("--nobrowser")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?);
        }

        // GGUF/GGML: check bundled llamafile first, then PATH
        let bundled = crate::startup::runtime_path();
        let runtime_bin = if bundled.exists() {
            bundled
        } else {
            which::which("llama-server")
                .or_else(|_| which::which("llama-cpp-server"))
                .or_else(|_| which::which("llamafile"))
                .map_err(|_| anyhow::anyhow!(
                    "No inference runtime found. Restart HyperLite to download llamafile automatically."
                ))?
        };

        // llamafile needs --server to run as an HTTP server; llama-server is already a server
        let is_llamafile = runtime_bin.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("llamafile"))
            .unwrap_or(false);

        let mut cmd = tokio::process::Command::new(&runtime_bin);
        cmd.arg("--model").arg(path)
           .arg("--port").arg(port.to_string())
           .arg("--host").arg("127.0.0.1")
           .arg("--ctx-size").arg("4096")
           .arg("--n-gpu-layers").arg("0")
           .stdout(std::process::Stdio::null())
           .stderr(std::process::Stdio::null());
        if is_llamafile {
            cmd.arg("--server");
        }
        Ok(cmd.spawn()?)
    }
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), rest);
        }
    }
    path.to_string()
}

// ── Provider implementation ───────────────────────────────────────────────────

#[async_trait]
impl LocalProvider for DirectGgufProvider {
    fn id(&self)       -> &str { "direct" }
    fn name(&self)     -> &str { "Direct (file scan)" }
    fn kind(&self)     -> BackendKind { BackendKind::DirectGguf }
    fn base_url(&self) -> &str { "http://localhost:18080" }

    async fn health_check(&self) -> bool {
        // "direct" is always available — it just scans the filesystem
        true
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>> {
        Ok(self.scan_model_files())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        model:    &str,
        params:   &GenerationParams,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        // model ID is the file path for direct provider
        let path = PathBuf::from(model);
        if !path.exists() {
            anyhow::bail!("Model file not found: {}", path.display());
        }

        let port = self.active_port;
        let base_url = format!("http://127.0.0.1:{}", port);

        // Check if a server is already running on this port
        let alive = self.client
            .get(format!("{}/health", base_url))
            .timeout(std::time::Duration::from_millis(300))
            .send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        if !alive {
            // Spawn the runtime — process runs independently after this returns
            self.spawn_runtime(&path, port).await?;

            // Wait up to 90s for the server to become ready.
            // llamafile/llama-server can take a while to memory-map a large model.
            let mut ready = false;
            for i in 0..180 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                // Try /health first; fall back to /v1/models (some builds differ)
                let health_ok = self.client
                    .get(format!("{}/health", base_url))
                    .timeout(std::time::Duration::from_millis(400))
                    .send().await
                    .map(|r| r.status().is_success() || r.status().as_u16() == 503)
                    .unwrap_or(false);
                let models_ok = if !health_ok {
                    self.client
                        .get(format!("{}/v1/models", base_url))
                        .timeout(std::time::Duration::from_millis(400))
                        .send().await
                        .map(|r| r.status().is_success())
                        .unwrap_or(false)
                } else { false };

                if health_ok || models_ok {
                    ready = true;
                    break;
                }
                // After 10s without connection, assume process failed to start
                if i == 20 {
                    let _ = self.client.get(format!("{}/health", base_url))
                        .send().await;  // one final probe
                }
            }
            if !ready {
                anyhow::bail!(
                    "llamafile/llama-server did not start on {} within 90s. \
                     The model may be too large for available memory.",
                    base_url
                );
            }
        }

        // Now use OpenAI compat to stream
        let stem = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        openai_compat::stream_chat(
            &self.client,
            &base_url,
            BackendKind::DirectGguf,
            messages,
            &stem,
            params,
        ).await
    }

    async fn load_model(&self, path: &PathBuf) -> Result<LocalModel> {
        if !path.exists() {
            anyhow::bail!("File not found: {}", path.display());
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let format = ModelFormat::from_extension(ext);
        let stem = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(LocalModel {
            id:           path.to_string_lossy().to_string(),
            name:         openai_compat::pretty_name(&stem),
            backend:      BackendKind::DirectGguf,
            format,
            path:         Some(path.clone()),
            size_bytes:   path.metadata().ok().map(|m| m.len()),
            context_len:  None,
            param_count:  openai_compat::extract_param_count(&stem),
            quantization: openai_compat::extract_quantization(&stem),
            tags:         vec![],
        })
    }
}
