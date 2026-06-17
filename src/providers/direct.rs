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
use crate::hardware::HardwareInfo;

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
    active_port: u16,
    hardware:    HardwareInfo,
}

impl DirectGgufProvider {
    pub fn new(client: Client, hardware: HardwareInfo) -> Self {
        let model_dirs = DEFAULT_MODEL_DIRS
            .iter()
            .map(|dir| PathBuf::from(expand_tilde(dir)))
            .collect();

        Self { client, model_dirs, active_port: 18080, hardware }
    }

    pub fn with_model_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.model_dirs = dirs;
        self
    }

    /// Scan all configured directories for model files.
    /// If Ollama is running, skips GGUFs that are already registered there
    /// to avoid showing duplicates in the model picker.
    pub fn scan_model_files(&self) -> Vec<LocalModel> {
        let mut models = vec![];

        // Fetch Ollama's registered model names once (blocking, best-effort)
        let ollama_names: std::collections::HashSet<String> = if which::which("ollama").is_ok() {
            std::process::Command::new("ollama")
                .arg("list")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|text| {
                    text.lines().skip(1) // skip header
                        .filter_map(|l| l.split_whitespace().next())
                        .map(|name| name.trim_end_matches(":latest").to_string())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            std::collections::HashSet::new()
        };

        // Always include the canonical HyperLite models dir, even if it wasn't
        // present when this provider was created.
        let canonical = crate::startup::models_dir();
        let extra = std::iter::once(&canonical);

        let platform_extras = extra_model_dirs();
        let all_dirs: Vec<&PathBuf> = self.model_dirs.iter()
            .chain(extra)
            .chain(platform_extras.iter())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for dir in all_dirs {
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

                // Skip if Ollama already has this model registered
                let ollama_name = stem.replace(['.', ' '], "-").to_lowercase();
                if ollama_names.contains(&ollama_name) {
                    continue;
                }

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

        // llamafile: Cosmopolitan APE polyglot (PE+ELF+shell-script in one file).
        // - Windows: native PE binary — run directly.
        // - Linux/macOS: run via `sh` so the shell strips the PE header before exec,
        //   bypassing WSL binfmt_misc (which would intercept the PE header and try to
        //   run it under Wine/Windows, where Linux paths don't exist).
        if ext == "llamafile" {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = path.metadata()?.permissions();
                perms.set_mode(perms.mode() | 0o755);
                std::fs::set_permissions(path, perms)?;
            }

            #[cfg(unix)]
            return Ok(tokio::process::Command::new("sh")
                .arg(path)
                .arg("--port").arg(port.to_string())
                .arg("--nobrowser")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()?);

            #[cfg(not(unix))]
            return Ok(tokio::process::Command::new(path)
                .arg("--port").arg(port.to_string())
                .arg("--nobrowser")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()?);
        }

        // GGUF/GGML: prefer system llama-server (typically CUDA-enabled) over bundled llamafile.
        // System builds compiled with -DGGML_CUDA=ON give full GPU offload; bundled llamafile
        // may lack CUDA support in WSL environments.
        let bundled_llamafile = crate::startup::runtime_path();
        let bundled_llama_server = crate::startup::find_bundled_llama_server();
        let runtime_bin = which::which("llama-server")
            .or_else(|_| which::which("llama-cpp-server"))
            .unwrap_or_else(|_| {
                if let Some(p) = bundled_llama_server.clone() { p }
                else if bundled_llamafile.exists() { bundled_llamafile }
                else { which::which("llamafile").unwrap_or_else(|_| PathBuf::from("llamafile")) }
            });

        // llamafile needs --server to run as an HTTP server; llama-server is already a server
        let is_llamafile = runtime_bin.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("llamafile"))
            .unwrap_or(false);

        // On Linux/macOS, run llamafile via `sh` to bypass WSL binfmt_misc PE interception.
        // On Windows, llamafile is a native PE binary — run it directly.
        #[cfg(unix)]
        let mut cmd = if is_llamafile {
            let mut c = tokio::process::Command::new("sh");
            c.arg(&runtime_bin);
            c
        } else {
            tokio::process::Command::new(&runtime_bin)
        };
        #[cfg(not(unix))]
        let mut cmd = tokio::process::Command::new(&runtime_bin);
        let p = self.hardware.optimal_inference_params();

        // Set LD_LIBRARY_PATH to the runtime's directory so shared libs (.so files)
        // from extracted llama.cpp archives are found at startup.
        if let Some(dir) = runtime_bin.parent() {
            let existing = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
            let lib_path = if existing.is_empty() {
                dir.to_string_lossy().to_string()
            } else {
                format!("{}:{}", dir.to_string_lossy(), existing)
            };
            cmd.env("LD_LIBRARY_PATH", lib_path);
        }

        cmd.arg("--model").arg(path)
           .arg("--port").arg(port.to_string())
           .arg("--host").arg("127.0.0.1")
           .arg("--ctx-size").arg(p.ctx_size.to_string())
           .arg("--batch-size").arg(p.batch_size.to_string())
           .arg("--threads").arg(p.threads.to_string())
           .arg("--n-gpu-layers").arg(p.n_gpu_layers.to_string())
           .stdout(std::process::Stdio::null())
           .stderr(std::process::Stdio::piped());
        if p.flash_attn {
            cmd.arg("--flash-attn");
        }
        if is_llamafile {
            cmd.arg("--server").arg("--nobrowser");
        }
        Ok(cmd.spawn()?)
    }
}

/// Locate the `ollama` binary. `which` misses it when PATH is sanitized
/// (e.g. inside Nix shells, or when `hl` is launched from a minimal env),
/// so fall back to common install locations before giving up.
fn find_ollama() -> Option<PathBuf> {
    if let Ok(p) = which::which("ollama") {
        return Some(p);
    }
    let home = crate::startup::real_home_dir();
    let candidates = [
        home.join(".local/bin/ollama"),
        home.join(".nix-profile/bin/ollama"),
        PathBuf::from("/usr/local/bin/ollama"),
        PathBuf::from("/usr/bin/ollama"),
        PathBuf::from("/run/current-system/sw/bin/ollama"), // NixOS
        PathBuf::from("/opt/ollama/bin/ollama"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = crate::startup::real_home_dir();
        return format!("{}/{}", home.display(), rest);
    }
    path.to_string()
}

/// On ARM64 Linux (RPi5), return extra directories to scan for models.
/// Covers cases where the app was run as a different user than the one who
/// downloaded the models (e.g. root vs normal user).
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub fn extra_model_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/root/.hyperlite/models"),
    ];
    // Scan all home directories under /home/
    if let Ok(entries) = std::fs::read_dir("/home") {
        for entry in entries.flatten() {
            let candidate = entry.path().join(".hyperlite").join("models");
            if candidate.exists() {
                dirs.push(candidate);
            }
        }
    }
    dirs
}

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
pub fn extra_model_dirs() -> Vec<PathBuf> {
    vec![]
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
        let path = PathBuf::from(model);
        if !path.exists() {
            anyhow::bail!("Model file not found: {}", path.display());
        }

        // If Ollama is running, try to route through it — it handles CUDA automatically.
        // Derive the Ollama model name from the GGUF filename (same as registration).
        // If Ollama returns 404 (model not yet registered) fall through to llama-server.
        if let Some(ollama_bin) = find_ollama() {
            let ollama_base = "http://127.0.0.1:11434";
            let health_ok = self.client
                .get(format!("{}/api/tags", ollama_base))
                .timeout(std::time::Duration::from_millis(500))
                .send().await
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            if health_ok {
                let model_name = path.file_stem()
                    .map(|s| s.to_string_lossy()
                        .replace(['.', ' '], "-")
                        .to_lowercase())
                    .unwrap_or_default();

                // Verify the model is actually registered before committing to Ollama
                let registered = async {
                    let resp = self.client
                        .get(format!("{}/api/tags", ollama_base))
                        .send().await.ok()?;
                    let j: serde_json::Value = resp.json().await.ok()?;
                    j["models"].as_array().map(|arr|
                        arr.iter().any(|m| m["name"].as_str().unwrap_or("").starts_with(&model_name))
                    )
                }.await.unwrap_or(false);

                // Model not registered yet — register it now, then use Ollama
                if !registered {
                    let path_str = path.to_string_lossy().to_string();
                    let modelfile = format!("FROM {}\nPARAMETER num_ctx 16384", path_str);
                    let modelfile_path = format!("/tmp/Modelfile-{}", model_name);
                    let _ = tokio::fs::write(&modelfile_path, &modelfile).await;
                    let _ = tokio::process::Command::new(&ollama_bin)
                        .args(["create", &model_name, "-f", &modelfile_path])
                        .output()
                        .await;
                    let _ = tokio::fs::remove_file(&modelfile_path).await;
                }

                return openai_compat::stream_chat(
                    &self.client,
                    ollama_base,
                    BackendKind::Ollama,
                    messages,
                    &model_name,
                    params,
                ).await;
            }
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
            let mut child = self.spawn_runtime(&path, port).await?;

            // Wait up to 90s for the server to become ready.
            // Poll every 500ms; if the process exits early we surface its stderr immediately.
            let mut ready = false;
            for _ in 0..180 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // Check whether the process has already died
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Read whatever stderr the process wrote
                        let mut err_text = String::new();
                        if let Some(stderr) = child.stderr.take() {
                            use tokio::io::AsyncReadExt;
                            let mut buf = Vec::new();
                            let mut reader = tokio::io::BufReader::new(stderr);
                            let _ = reader.read_to_end(&mut buf).await;
                            err_text = String::from_utf8_lossy(&buf).to_string();
                        }
                        // Surface the last few lines — most useful for diagnosing the crash
                        let tail: String = err_text.lines()
                            .filter(|l| !l.trim().is_empty())
                            .rev().take(4)
                            .collect::<Vec<_>>()
                            .into_iter().rev()
                            .collect::<Vec<_>>()
                            .join("\n");
                        if tail.is_empty() {
                            anyhow::bail!(
                                "Inference runtime exited immediately (exit {}). \
                                 Try running llamafile manually to see the error.",
                                status
                            );
                        } else {
                            anyhow::bail!("Inference runtime crashed:\n{}", tail);
                        }
                    }
                    Ok(None) => {} // still running — continue polling
                    Err(_)   => {} // can't determine status — keep going
                }

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
            }
            if !ready {
                anyhow::bail!(
                    "Inference server did not become ready within 90s. \
                     The model may be too large for available memory."
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
