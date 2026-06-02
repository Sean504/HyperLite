/// Startup splash + first-run setup wizard.
///
/// Flow:
///   1. Splash — press ENTER
///   2a. If models already exist in ~/.hyperlite/models/ → Done
///   2b. No models → hardware summary + model picker
///   3. Download selected models directly from HuggingFace (no Ollama needed)
///   Done

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use tokio::sync::mpsc as tmpsc;

use crate::hardware::HardwareInfo;
use crate::providers::LocalModel;

// ── Models directory ──────────────────────────────────────────────────────────

/// On ARM64 Linux (RPi5), dirs::home_dir() can return wrong results when the
/// process is run as root or via sudo. Read the real home from /etc/passwd
/// using the actual UID so models are always found regardless of how the app
/// was launched.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub fn real_home_dir() -> PathBuf {
    // Try passwd lookup by real UID first
    let uid = unsafe { libc::getuid() };
    if let Ok(contents) = std::fs::read_to_string("/etc/passwd") {
        for line in contents.lines() {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 6 {
                if let Ok(entry_uid) = fields[2].parse::<u32>() {
                    if entry_uid == uid {
                        let home = PathBuf::from(fields[5]);
                        if home.exists() {
                            return home;
                        }
                    }
                }
            }
        }
    }
    // Fall back to dirs::home_dir() or /root for root, /home/<user> otherwise
    dirs::home_dir().unwrap_or_else(|| {
        if uid == 0 { PathBuf::from("/root") } else { PathBuf::from(".") }
    })
}

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
pub fn real_home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn models_dir() -> PathBuf {
    real_home_dir().join(".hyperlite").join("models")
}

pub fn ensure_models_dir() -> anyhow::Result<PathBuf> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns true if any .gguf files exist in the models directory.
pub fn has_local_models() -> bool {
    let dir = models_dir();
    if !dir.exists() { return false; }
    std::fs::read_dir(&dir)
        .map(|mut rd| rd.any(|e| {
            e.ok().and_then(|e| {
                let n = e.file_name();
                let s = n.to_string_lossy().to_string();
                if s.ends_with(".gguf") { Some(()) } else { None }
            }).is_some()
        }))
        .unwrap_or(false)
}

// ── Inference runtime ─────────────────────────────────────────────────────────

/// Path where HyperLite stores the downloaded llamafile fallback runtime.
pub fn runtime_path() -> PathBuf {
    let filename = if cfg!(windows) { "llamafile.exe" } else { "llamafile" };
    real_home_dir().join(".hyperlite").join(filename)
}

/// Returns true if any usable inference runtime is reachable.
pub fn has_runtime() -> bool {
    runtime_path().exists()
        || find_bundled_llama_server().is_some()
        || which::which("ollama").is_ok()
        || which::which("llama-server").is_ok()
        || which::which("llama-cpp-server").is_ok()
        || which::which("llamafile").is_ok()
}

/// Find llama-server in ~/.hyperlite/ or any extracted subdirectory.
/// Returns the full path including the directory so the caller can set LD_LIBRARY_PATH.
pub fn find_bundled_llama_server() -> Option<PathBuf> {
    let base = real_home_dir().join(".hyperlite");
    let direct = base.join("llama-server");
    if direct.exists() { return Some(direct); }
    // Check subdirectories — extraction puts everything in llama-b{build}/
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let candidate = entry.path().join("llama-server");
                if candidate.exists() { return Some(candidate); }
            }
        }
    }
    None
}


/// Build the runtime install job per OS:
/// - Linux: Ollama (CUDA auto-detected, works on WSL2 and native)
/// - macOS: brew install llama.cpp (Metal auto-detected, no sudo)
/// - Windows: GitHub API latest release
/// - Fallback: llamafile
fn runtime_download_job() -> DownloadJob {
    let os = std::env::consts::OS;

    // Linux — Ollama handles CUDA automatically on both native Linux and WSL2
    if os == "linux" {
        return DownloadJob {
            name:        "Ollama (GPU-accelerated inference)".to_string(),
            url:         String::new(), filename: String::new(), is_runtime: true,
            needs_sudo:  true,
            install_cmd: Some(vec![
                "sudo".into(), "-S".into(), "sh".into(), "-c".into(),
                "curl -fsSL https://ollama.com/install.sh | sh".into(),
            ]),
        };
    }

    // macOS — brew handles Metal automatically, no sudo needed
    if which::which("brew").is_ok() {
        return DownloadJob {
            name:        "llama-server via brew".to_string(),
            url:         String::new(), filename: String::new(), is_runtime: true,
            needs_sudo:  false,
            install_cmd: Some(vec!["brew".into(), "install".into(), "llama.cpp".into()]),
        };
    }

    // Windows / anything else — GitHub API picks the right build
    let has_nvidia = which::which("nvidia-smi").is_ok();
    let has_rocm   = which::which("rocm-smi").is_ok();
    let arch       = std::env::consts::ARCH;
    let accel      = if has_nvidia { "cuda" } else if has_rocm { "rocm" } else { "cpu" };

    DownloadJob {
        name:        format!("llama-server ({} / {} {})", accel, os, arch),
        url:         format!("GITHUB_LATEST:{}:{}:{}:ggerganov/llama.cpp", os, arch, accel),
        filename:    "llama-server-archive".to_string(),
        is_runtime:  true,
        install_cmd: None,
        needs_sudo:  false,
    }
}

fn llamafile_download_job() -> DownloadJob {
    let filename = if cfg!(windows) { "llamafile.exe" } else { "llamafile" };
    DownloadJob {
        name:        "llamafile  (inference runtime)".to_string(),
        url:         "https://github.com/Mozilla-Ocho/llamafile/releases/download/0.9.2/llamafile-0.9.2".to_string(),
        filename:    filename.to_string(),
        is_runtime:  true,
        install_cmd: None,
        needs_sudo:  false,
    }
}

// ── Download / install progress events ───────────────────────────────────────

#[derive(Debug)]
enum DlEvent {
    Status(String),
    Progress { total: u64, completed: u64 },
    Done(String),   // display name — finished
    Failed(String), // error message
}


// ── Pending download job ──────────────────────────────────────────────────────

#[derive(Clone)]
struct DownloadJob {
    name:        String,
    url:         String,
    filename:    String,
    is_runtime:  bool,
    install_cmd: Option<Vec<String>>,
    needs_sudo:  bool,  // if true, prompt user for password before running install_cmd
}

// ── Recommended models ────────────────────────────────────────────────────────

#[derive(Clone)]
struct RecommendedModel {
    display:  &'static str,
    desc:     &'static str,
    hf_repo:  &'static str,   // e.g. "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF"
    hf_file:  &'static str,   // e.g. "qwen2.5-coder-7b-instruct-q4_k_m.gguf"
    vram_mb:  u64,
    ram_mb:   u64,
    tags:     &'static [&'static str],
}

impl RecommendedModel {
    fn hf_url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/main/{}", self.hf_repo, self.hf_file)
    }
}

const RECOMMENDED_MODELS: &[RecommendedModel] = &[
    RecommendedModel {
        display: "SmolLM2 1.7B",
        desc:    "Tiny but capable. Runs anywhere.",
        hf_repo: "HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF",
        hf_file: "smollm2-1.7b-instruct-q4_k_m.gguf",
        vram_mb: 1200, ram_mb: 3000, tags: &["Fast", "CPU-OK"],
    },
    RecommendedModel {
        display: "Phi-4 Mini 3.8B",
        desc:    "Microsoft's punchy small model. Great reasoning.",
        hf_repo: "unsloth/Phi-4-mini-instruct-GGUF",
        hf_file: "Phi-4-mini-instruct-Q4_K_M.gguf",
        vram_mb: 2500, ram_mb: 6000, tags: &["Coding", "Reasoning"],
    },
    RecommendedModel {
        display: "Qwen2.5 3B",
        desc:    "Alibaba multilingual workhorse.",
        hf_repo: "Qwen/Qwen2.5-3B-Instruct-GGUF",
        hf_file: "qwen2.5-3b-instruct-q4_k_m.gguf",
        vram_mb: 2200, ram_mb: 5000, tags: &["Writing", "Coding"],
    },
    RecommendedModel {
        display: "Llama 3.2 3B",
        desc:    "Meta's small Llama. Solid all-rounder.",
        hf_repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",
        hf_file: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        vram_mb: 2400, ram_mb: 6000, tags: &["Writing", "Coding"],
    },
    RecommendedModel {
        display: "Qwen2.5-Coder 7B",
        desc:    "Best-in-class code model at 7B.",
        hf_repo: "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF",
        hf_file: "qwen2.5-coder-7b-instruct-q4_k_m.gguf",
        vram_mb: 5000, ram_mb: 10000, tags: &["Coding", "Best-Code"],
    },
    RecommendedModel {
        display: "Mistral 7B",
        desc:    "Fast and strong general-purpose.",
        hf_repo: "bartowski/Mistral-7B-Instruct-v0.3-GGUF",
        hf_file: "Mistral-7B-Instruct-v0.3-Q4_K_M.gguf",
        vram_mb: 5000, ram_mb: 10000, tags: &["Writing", "Fast"],
    },
    RecommendedModel {
        display: "Llama 3.1 8B",
        desc:    "Meta's flagship 8B. Excellent instruction following.",
        hf_repo: "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF",
        hf_file: "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
        vram_mb: 5500, ram_mb: 12000, tags: &["Writing", "ToolUse"],
    },
    RecommendedModel {
        display: "Qwen2.5 14B",
        desc:    "Great balance of capability and size.",
        hf_repo: "bartowski/Qwen2.5-14B-Instruct-GGUF",
        hf_file: "Qwen2.5-14B-Instruct-Q4_K_M.gguf",
        vram_mb: 10000, ram_mb: 20000, tags: &["Writing", "Reasoning"],
    },
    RecommendedModel {
        display: "DeepSeek-R1 14B",
        desc:    "Chain-of-thought reasoning model.",
        hf_repo: "bartowski/DeepSeek-R1-Distill-Qwen-14B-GGUF",
        hf_file: "DeepSeek-R1-Distill-Qwen-14B-Q4_K_M.gguf",
        vram_mb: 10000, ram_mb: 20000, tags: &["Reasoning", "Math"],
    },
    RecommendedModel {
        display: "Qwen2.5-Coder 14B",
        desc:    "Top coding model for mid-range GPUs.",
        hf_repo: "Qwen/Qwen2.5-Coder-14B-Instruct-GGUF",
        hf_file: "qwen2.5-coder-14b-instruct-q4_k_m.gguf",
        vram_mb: 10000, ram_mb: 20000, tags: &["Coding", "Best-Code"],
    },
    RecommendedModel {
        display: "Qwen2.5 32B",
        desc:    "Frontier-level quality for high-end GPUs.",
        hf_repo: "bartowski/Qwen2.5-32B-Instruct-GGUF",
        hf_file: "Qwen2.5-32B-Instruct-Q4_K_M.gguf",
        vram_mb: 22000, ram_mb: 40000, tags: &["Writing", "Reasoning"],
    },
    RecommendedModel {
        display: "DeepSeek-R1 32B",
        desc:    "Best open reasoning model.",
        hf_repo: "bartowski/DeepSeek-R1-Distill-Qwen-32B-GGUF",
        hf_file: "DeepSeek-R1-Distill-Qwen-32B-Q4_K_M.gguf",
        vram_mb: 22000, ram_mb: 40000, tags: &["Reasoning", "Best-Reason"],
    },
    RecommendedModel {
        display: "Llama 3.3 70B",
        desc:    "Near-frontier. Needs 40 GB+ VRAM or large RAM.",
        hf_repo: "bartowski/Llama-3.3-70B-Instruct-GGUF",
        hf_file: "Llama-3.3-70B-Instruct-Q4_K_M.gguf",
        vram_mb: 45000, ram_mb: 80000, tags: &["Writing", "Coding"],
    },
];

// ── Setup state ───────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SetupStep {
    Splash,
    SetupModels,
    Downloading,
    Done,
}

struct SetupState {
    step:              SetupStep,
    hardware:          HardwareInfo,
    models:            Vec<RecommendedModel>,
    selected:          Vec<bool>,
    list_idx:          usize,
    runtime_needed:    bool,
    // Sudo password prompt
    sudo_prompt:       bool,    // waiting for user to enter password
    sudo_input:        String,  // password being typed (never displayed)
    // File download state
    download_queue:    Vec<DownloadJob>,
    current_dl:        Option<DownloadJob>,
    dl_progress:       f64,
    dl_log:            Vec<String>,
    dl_done:           Vec<String>,
    dl_failed:         Vec<String>,
    dl_status_msg:     String,
    dl_bytes_total:    u64,
    dl_bytes_done:     u64,
    dl_speed_bps:      f64,
    dl_start:          Option<Instant>,
    dl_rx:             Option<tmpsc::UnboundedReceiver<DlEvent>>,
    dl_total_queued:   usize,
    dl_complete_ticks: u8,
    http_client:       reqwest::Client,
    splash_tick:       u8,
    error_msg:         Option<String>,
}

impl SetupState {
    fn new(hardware: HardwareInfo, http_client: reqwest::Client) -> Self {
        let budget = if hardware.cpu_only {
            hardware.memory.total_mb / 2
        } else {
            hardware.best_vram_mb
        };

        let models: Vec<RecommendedModel> = RECOMMENDED_MODELS.iter()
            .filter(|m| {
                if hardware.cpu_only {
                    m.ram_mb <= hardware.memory.total_mb
                } else {
                    m.vram_mb <= budget + budget / 4
                }
            })
            .cloned()
            .collect();

        let selected    = vec![false; models.len()];
        Self {
            step: SetupStep::Splash,
            hardware,
            models,
            selected,
            list_idx: 0,
            runtime_needed: !has_runtime(),
            sudo_prompt: false,
            sudo_input: String::new(),
            download_queue: vec![],
            current_dl: None,
            dl_progress: 0.0,
            dl_log: vec![],
            dl_done: vec![],
            dl_failed: vec![],
            dl_status_msg: String::new(),
            dl_bytes_total: 0,
            dl_bytes_done: 0,
            dl_speed_bps: 0.0,
            dl_start: None,
            dl_rx: None,
            dl_total_queued: 0,
            dl_complete_ticks: 0,
            http_client,
            splash_tick: 0,
            error_msg: None,
        }
    }

    fn selected_jobs(&self) -> Vec<DownloadJob> {
        self.models.iter().zip(self.selected.iter())
            .filter_map(|(m, &sel)| if sel {
                Some(DownloadJob {
                    name:        m.display.to_string(),
                    url:         m.hf_url(),
                    filename:    m.hf_file.to_string(),
                    is_runtime:  false,
                    install_cmd: None,
                    needs_sudo:  false,
                })
            } else {
                None
            })
            .collect()
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run_startup(
    terminal:        &mut Terminal<CrosstermBackend<io::Stdout>>,
    hardware:        HardwareInfo,
    existing_models: &[LocalModel],
    http_client:     reqwest::Client,
) -> anyhow::Result<()> {
    let _ = ensure_models_dir();
    let mut state = SetupState::new(hardware, http_client);

    loop {
        // Exit BEFORE drawing so we never show a blank frame
        if state.step == SetupStep::Done { return Ok(()); }

        terminal.draw(|f| render(f, &mut state))?;
        state.splash_tick = state.splash_tick.wrapping_add(1);

        // ── Splash: advance on Enter ───────────────────────────────────────────
        if state.step == SetupStep::Splash {
            if event::poll(Duration::from_millis(80))? {
                let ev = event::read()?;
                if let Event::Key(key) = ev {
                    if key.kind != KeyEventKind::Press { continue; }
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('q') {
                        std::process::exit(0);
                    }
                    if key.code == KeyCode::Enter || key.code == KeyCode::Char(' ') {
                        if !existing_models.is_empty() || has_local_models() {
                            if state.runtime_needed {
                                state.download_queue = vec![runtime_download_job()];
                                state.step = SetupStep::Downloading;
                            } else {
                                state.step = SetupStep::Done;
                            }
                        } else {
                            state.step = SetupStep::SetupModels;
                        }
                    }
                }
            }
            continue;
        }

        // ── Drive downloads ────────────────────────────────────────────────────
        if state.step == SetupStep::Downloading {
            // Record total files at start for the overall progress bar
            let in_flight = state.dl_done.len() + state.dl_failed.len()
                + state.current_dl.is_some() as usize + state.download_queue.len();
            if state.dl_total_queued == 0 && in_flight > 0 {
                state.dl_total_queued = in_flight;
            }

            if state.current_dl.is_none() && !state.download_queue.is_empty() {
                start_next_download(&mut state);
            }
            poll_download_progress(&mut state);

            // Show completion screen for ~1.5s before exiting
            if state.download_queue.is_empty() && state.current_dl.is_none() {
                if state.dl_complete_ticks >= 20 {
                    state.step = SetupStep::Done;
                } else {
                    state.dl_complete_ticks += 1;
                }
            }
        }

        // ── Input ──────────────────────────────────────────────────────────────
        if !event::poll(Duration::from_millis(80))? { continue; }
        let ev = event::read()?;
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press { continue; }

        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('q') {
            std::process::exit(0);
        }

        match state.step.clone() {
            SetupStep::SetupModels => {
                let can_proceed = !existing_models.is_empty()
                    || has_local_models()
                    || !state.dl_done.is_empty();
                match key.code {
                    KeyCode::Up   | KeyCode::Char('k') => {
                        state.list_idx = state.list_idx.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.list_idx + 1 < state.models.len() { state.list_idx += 1; }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(s) = state.selected.get_mut(state.list_idx) { *s = !*s; }
                    }
                    KeyCode::Enter => {
                        let mut jobs = state.selected_jobs();
                        if !jobs.is_empty() {
                            if state.runtime_needed {
                                jobs.push(runtime_download_job());
                            }
                            state.download_queue = jobs;
                            state.step = SetupStep::Downloading;
                        } else if can_proceed {
                            state.step = SetupStep::Done;
                        } else {
                            state.error_msg = Some("Select at least one model to download.".to_string());
                        }
                    }
                    KeyCode::Esc => {
                        if can_proceed { state.step = SetupStep::Done; }
                        else { state.error_msg = Some("You need at least one model to continue.".to_string()); }
                    }
                    _ => {}
                }
            }

            SetupStep::Downloading => {
                // Handle sudo password input
                if state.sudo_prompt {
                    match key.code {
                        KeyCode::Enter => {
                            state.sudo_prompt = false;
                            start_next_download(&mut state);
                        }
                        KeyCode::Backspace => { state.sudo_input.pop(); }
                        KeyCode::Char(c) => { state.sudo_input.push(c); }
                        _ => {}
                    }
                    continue;
                }
                if (key.code == KeyCode::Char('s') || key.code == KeyCode::Esc)
                    && (!existing_models.is_empty() || !state.dl_done.is_empty() || has_local_models())
                {
                    state.step = SetupStep::Done;
                }
            }

            SetupStep::Done => { return Ok(()); }
            _ => {}
        }
    }
}

// ── Download driver ───────────────────────────────────────────────────────────

fn start_next_download(state: &mut SetupState) {
    if state.current_dl.is_some() || state.download_queue.is_empty() { return; }

    let job = state.download_queue.remove(0);
    state.dl_progress    = 0.0;
    state.dl_bytes_total = 0;
    state.dl_bytes_done  = 0;
    state.dl_speed_bps   = 0.0;
    state.dl_start       = Some(Instant::now());

    let (tx, rx) = tmpsc::unbounded_channel::<DlEvent>();
    state.dl_rx = Some(rx);

    // Package manager install — run subprocess instead of HTTP download
    if let Some(ref cmd) = job.install_cmd {
        // If this job needs sudo and we don't have a password yet, pause and prompt
        if job.needs_sudo && state.sudo_input.is_empty() {
            state.sudo_prompt = true;
            state.download_queue.insert(0, job); // put it back at the front
            return;
        }
        state.dl_status_msg = "installing…".to_string();
        state.dl_log.push(format!("Installing {}…", job.name));
        state.dl_log.push("  This will take 2–5 minutes. Downloading and configuring the runtime.".to_string());
        state.dl_log.push("  You can leave this running — HyperLite will launch when done.".to_string());
        let cmd = cmd.clone();
        let name = job.name.clone();
        let password = if job.needs_sudo { Some(state.sudo_input.clone()) } else { None };
        state.current_dl = Some(job);
        tokio::spawn(async move {
            use tokio::io::{AsyncWriteExt, AsyncBufReadExt, BufReader};
            let mut child = match tokio::process::Command::new(&cmd[0])
                .args(&cmd[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => { let _ = tx.send(DlEvent::Failed(e.to_string())); return; }
            };
            // Pipe password to sudo -S stdin then close it
            if let (Some(pw), Some(mut stdin)) = (password, child.stdin.take()) {
                let _ = stdin.write_all(format!("{}\n", pw).as_bytes()).await;
                // stdin drops here, closing it so the child doesn't wait for more input
            }
            // Stream stdout and stderr as status lines
            if let Some(stdout) = child.stdout.take() {
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let clean = strip_ansi(line.trim());
                        if !clean.is_empty() {
                            let _ = tx2.send(DlEvent::Status(clean));
                        }
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let clean = strip_ansi(line.trim());
                        if !clean.is_empty() {
                            let _ = tx2.send(DlEvent::Status(clean));
                        }
                    }
                });
            }
            match child.wait().await {
                Ok(s) if s.success() => { let _ = tx.send(DlEvent::Done(name)); }
                Ok(s) => { let _ = tx.send(DlEvent::Failed(format!("exited {}", s))); }
                Err(e) => { let _ = tx.send(DlEvent::Failed(e.to_string())); }
            }
        });
        return;
    }

    state.dl_status_msg = "connecting…".to_string();
    state.dl_log.push(format!("Downloading {}…", job.name));

    let client = reqwest::Client::builder()
        .user_agent("HyperLite")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_else(|_| state.http_client.clone());
    let url        = job.url.clone();
    let filename   = job.filename.clone();
    let name       = job.name.clone();
    let is_runtime = job.is_runtime;
    state.current_dl = Some(job);

    tokio::spawn(async move {
        // Resolve GITHUB_LATEST sentinel → real download URL via GitHub API
        let (resolved_url, resolved_filename) = if url.starts_with("GITHUB_LATEST:") {
            let _ = tx.send(DlEvent::Status("resolving latest release…".to_string()));
            // Format: GITHUB_LATEST:os:arch:accel:owner/repo
            let parts: Vec<&str> = url.splitn(5, ':').collect();
            let (os, arch, accel, repo) = if parts.len() == 5 {
                (parts[1], parts[2], parts[3], parts[4])
            } else {
                let _ = tx.send(DlEvent::Failed("bad GITHUB_LATEST format".to_string())); return;
            };

            let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
            let api_resp = match client.get(&api_url).send().await {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(DlEvent::Failed(format!("GitHub API: {}", e))); return; }
            };
            if !api_resp.status().is_success() {
                let _ = tx.send(DlEvent::Failed(format!("GitHub API HTTP {}", api_resp.status()))); return;
            }
            let json: serde_json::Value = match api_resp.json().await {
                Ok(j) => j,
                Err(e) => { let _ = tx.send(DlEvent::Failed(format!("GitHub API parse: {}", e))); return; }
            };

            let assets = match json["assets"].as_array() {
                Some(a) => a.clone(),
                None => { let _ = tx.send(DlEvent::Failed("no assets in release".to_string())); return; }
            };

            // Pick the best asset for this OS/arch/accel
            // Asset names use "x64" (not "x86_64"), "arm64" for aarch64.
            // Linux has no CUDA build — NVIDIA uses vulkan build instead.
            // Ubuntu builds work on Debian and all Debian-based distros.
            // Windows has separate cuda-12.x and cuda-13.x builds; pick cuda-12 for widest compat.
            let asset = assets.iter().find(|a| {
                let n = a["name"].as_str().unwrap_or("").to_lowercase();
                match (os, arch, accel) {
                    ("linux",   "x86_64",  "cuda") => n.contains("ubuntu") && n.contains("vulkan") && n.contains("x64") && !n.contains("arm"),
                    ("linux",   "x86_64",  "rocm") => n.contains("ubuntu") && n.contains("rocm")   && n.contains("x64") && !n.contains("arm"),
                    ("linux",   "x86_64",  _     ) => n.contains("ubuntu") && !n.contains("vulkan") && !n.contains("rocm") && !n.contains("openvino") && n.contains("x64") && !n.contains("arm"),
                    ("linux",   "aarch64", _     ) => n.contains("ubuntu") && n.contains("arm64")  && !n.contains("vulkan"),
                    ("macos",   "aarch64", _     ) => n.contains("macos")  && n.contains("arm64"),
                    ("macos",   "x86_64",  _     ) => n.contains("macos")  && n.contains("x64"),
                    ("windows", "x86_64",  "cuda") => n.contains("win")    && n.contains("cuda-12") && n.contains("x64"),
                    ("windows", "x86_64",  "rocm") => n.contains("win")    && n.contains("hip")    && n.contains("x64"),
                    ("windows", "x86_64",  _     ) => n.contains("win")    && n.contains("cpu")    && n.contains("x64"),
                    _ => false,
                }
            });

            match asset {
                Some(a) => {
                    let dl_url  = a["browser_download_url"].as_str().unwrap_or("").to_string();
                    let dl_name = a["name"].as_str().unwrap_or("llama-server-archive").to_string();
                    if dl_url.is_empty() {
                        let _ = tx.send(DlEvent::Failed("no download URL in asset".to_string())); return;
                    }
                    let _ = tx.send(DlEvent::Status(format!("found: {}", dl_name)));
                    (dl_url, dl_name)
                }
                None => {
                    let _ = tx.send(DlEvent::Failed("no matching asset for this platform — falling back".to_string())); return;
                }
            }
        } else {
            (url, filename)
        };

        // Runtimes go to ~/.hyperlite/, models go to ~/.hyperlite/models/
        let dest = if is_runtime {
            let base = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".hyperlite");
            if let Err(e) = tokio::fs::create_dir_all(&base).await {
                let _ = tx.send(DlEvent::Failed(format!("Could not create dir: {}", e)));
                return;
            }
            base.join(&resolved_filename)
        } else {
            match crate::startup::ensure_models_dir() {
                Ok(d)  => d.join(&resolved_filename),
                Err(e) => { let _ = tx.send(DlEvent::Failed(format!("Could not create models dir: {}", e))); return; }
            }
        };

        // Open destination file
        let mut file = match tokio::fs::File::create(&dest).await {
            Ok(f)  => f,
            Err(e) => { let _ = tx.send(DlEvent::Failed(format!("File create failed: {}", e))); return; }
        };

        let _ = tx.send(DlEvent::Status("connecting…".to_string()));

        let resp = match client.get(&resolved_url).send().await {
            Ok(r)  => r,
            Err(e) => { let _ = tx.send(DlEvent::Failed(e.to_string())); return; }
        };

        if !resp.status().is_success() {
            let _ = tx.send(DlEvent::Failed(format!("HTTP {} from {}", resp.status(), resolved_url)));
            return;
        }

        let total = resp.content_length().unwrap_or(0);
        if total > 0 {
            let _ = tx.send(DlEvent::Progress { total, completed: 0 });
        }

        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;
        let mut stream    = resp.bytes_stream();
        let mut completed = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c)  => c,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&dest).await;
                    let _ = tx.send(DlEvent::Failed(e.to_string()));
                    return;
                }
            };
            if let Err(e) = file.write_all(&chunk).await {
                let _ = tokio::fs::remove_file(&dest).await;
                let _ = tx.send(DlEvent::Failed(format!("Write error: {}", e)));
                return;
            }
            completed += chunk.len() as u64;
            if total > 0 {
                let _ = tx.send(DlEvent::Progress { total, completed });
            }
        }

        let _ = file.flush().await;
        drop(file);

        // Runtime post-processing
        #[cfg(unix)]
        if is_runtime {
            use std::os::unix::fs::PermissionsExt;
            let dest_str = dest.to_string_lossy().to_string();
            let is_archive = dest_str.ends_with(".tar.gz") || dest_str.ends_with(".zip");

            if is_archive {
                // Extract the archive and find llama-server inside it.
                let hyperlite_dir = dest.parent().unwrap_or(&dest).to_path_buf();
                let _ = tx.send(DlEvent::Status("extracting…".to_string()));

                let extract_result = if dest_str.ends_with(".tar.gz") {
                    tokio::process::Command::new("tar")
                        .args(["-xzf", &dest_str, "-C", &hyperlite_dir.to_string_lossy()])
                        .output().await
                } else {
                    tokio::process::Command::new("unzip")
                        .args(["-o", &dest_str, "-d", &hyperlite_dir.to_string_lossy()])
                        .output().await
                };

                // Remove the archive regardless of extraction result
                let _ = tokio::fs::remove_file(&dest).await;

                if extract_result.map(|o| o.status.success()).unwrap_or(false) {
                    // Find llama-server in the extracted files and move it to ~/.hyperlite/
                    let find = tokio::process::Command::new("find")
                        .args([&hyperlite_dir.to_string_lossy().to_string(), "-name", "llama-server", "-type", "f"])
                        .output().await;

                    if let Ok(out) = find {
                        let found = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if !found.is_empty() {
                            let server_bin = hyperlite_dir.join("llama-server");
                            let _ = tokio::fs::rename(&found, &server_bin).await;
                            if let Ok(meta) = std::fs::metadata(&server_bin) {
                                let mut perms = meta.permissions();
                                perms.set_mode(0o755);
                                let _ = std::fs::set_permissions(&server_bin, perms);
                            }
                        }
                    }
                }
            } else {
                // Single binary (llamafile) — just chmod
                if let Ok(meta) = std::fs::metadata(&dest) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&dest, perms);
                }
            }
        }

        let _ = tx.send(DlEvent::Done(name));
    });
}


/// Drain progress channel and update state. Non-blocking.
fn poll_download_progress(state: &mut SetupState) {
    if state.current_dl.is_none() { return; }
    let rx = match state.dl_rx.as_mut() { Some(r) => r, None => return };

    loop {
        match rx.try_recv() {
            Ok(DlEvent::Status(s)) => {
                state.dl_status_msg = s.clone();
                state.dl_log.push(format!("  {}", s));
                if state.dl_log.len() > 40 { state.dl_log.remove(0); }
            }
            Ok(DlEvent::Progress { total, completed }) => {
                let prev_done        = state.dl_bytes_done;
                state.dl_bytes_total = total;
                state.dl_bytes_done  = completed;
                state.dl_progress    = completed as f64 / total as f64;
                state.dl_status_msg  = "downloading…".to_string();

                let instant_speed = (completed.saturating_sub(prev_done)) as f64 / 0.08;
                state.dl_speed_bps = state.dl_speed_bps * 0.85 + instant_speed * 0.15;
            }
            Ok(DlEvent::Done(name)) => {
                state.dl_progress   = 1.0;
                state.dl_status_msg = "complete".to_string();
                state.dl_done.push(name.clone());
                state.dl_log.push(format!("✓ {} ready", name));

                // If Ollama is now available and this was a model (not runtime),
                // register the GGUF with Ollama so it can serve it
                let finished = state.current_dl.take();
                if let Some(job) = &finished {
                    if !job.is_runtime && which::which("ollama").is_ok() {
                        let model_path = crate::startup::models_dir().join(&job.filename);
                        let model_name = job.filename
                            .trim_end_matches(".gguf")
                            .replace(['.', ' '], "-")
                            .to_lowercase();
                        state.dl_log.push(format!("Registering {} with Ollama…", model_name));
                        let path_str = model_path.to_string_lossy().to_string();
                        tokio::spawn(async move {
                            // Ollama requires a Modelfile with a FROM directive
                            let modelfile = format!("FROM {}", path_str);
                            let modelfile_path = format!("/tmp/Modelfile-{}", model_name);
                            let _ = tokio::fs::write(&modelfile_path, modelfile).await;
                            let _ = tokio::process::Command::new("ollama")
                                .args(["create", &model_name, "-f", &modelfile_path])
                                .output()
                                .await;
                            let _ = tokio::fs::remove_file(&modelfile_path).await;
                        });
                    }
                }

                state.dl_rx = None;
                start_next_download(state);
                return;
            }
            Ok(DlEvent::Failed(e)) => {
                let failed = state.current_dl.take().unwrap();
                state.dl_failed.push(failed.name.clone());
                state.dl_log.push(format!("✗ {} — {}", failed.name, e));
                state.dl_rx = None;
                // If a package manager install failed, fall back to llamafile
                if failed.install_cmd.is_some() {
                    state.dl_log.push("Falling back to llamafile…".to_string());
                    state.download_queue.insert(0, llamafile_download_job());
                    state.dl_failed.pop(); // don't count the fallback as a permanent failure
                }
                start_next_download(state);
                return;
            }
            Err(_) => break,
        }
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render(f: &mut ratatui::Frame, state: &mut SetupState) {
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(8, 8, 18))),
        area,
    );
    match state.step {
        SetupStep::Splash      => render_splash(f, area, state),
        SetupStep::SetupModels => render_model_select(f, area, state),
        SetupStep::Downloading => render_downloading(f, area, state),
        SetupStep::Done        => render_launching(f, area),
    }
}

// ── Boot sequence (called from main during init) ──────────────────────────────

pub struct BootStep {
    pub ok:    bool,
    pub label: String,
}

pub fn render_booting(f: &mut ratatui::Frame, steps: &[BootStep], current: &str) {
    let area   = f.area();
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let yellow = ratatui::style::Color::Rgb(241, 250, 140);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);

    f.render_widget(
        Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(8, 8, 18))),
        area,
    );
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(purple))
        .style(Style::default().bg(ratatui::style::Color::Rgb(8, 8, 18)));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let logo_w   = LOGO.iter().map(|l| unicode_width::UnicodeWidthStr::width(*l) as u16).max().unwrap_or(74);
    let top_half = inner.height / 2;
    let logo_y   = inner.y + top_half.saturating_sub(LOGO.len() as u16 + 2) / 2;
    let logo_x   = inner.x + inner.width.saturating_sub(logo_w) / 2;

    for (i, line) in LOGO.iter().enumerate() {
        let color = if i % 2 == 0 { purple } else { teal };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(*line, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            ])),
            Rect { x: logo_x, y: logo_y + i as u16, width: logo_w.min(inner.width), height: 1 },
        );
    }

    let sub_y = logo_y + LOGO.len() as u16 + 1;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("terminal-native  ·  local-only  ·  blazing fast", Style::default().fg(teal)),
        ])).alignment(ratatui::layout::Alignment::Center),
        Rect { x: inner.x, y: sub_y, width: inner.width, height: 1 },
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(concat!("v", env!("CARGO_PKG_VERSION")), Style::default().fg(dim)),
        ])).alignment(ratatui::layout::Alignment::Center),
        Rect { x: inner.x, y: sub_y + 1, width: inner.width, height: 1 },
    );

    let checklist_y = inner.y + top_half + 1;
    let checklist_x = inner.x + 4;
    let checklist_w = inner.width.saturating_sub(8);

    for (i, step) in steps.iter().enumerate() {
        let (icon, color) = if step.ok { ("✓", green) } else { ("⚠", yellow) };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {}  ", icon), Style::default().fg(color)),
                Span::styled(step.label.clone(), Style::default().fg(if step.ok { teal } else { yellow })),
            ])),
            Rect { x: checklist_x, y: checklist_y + i as u16, width: checklist_w, height: 1 },
        );
    }

    if !current.is_empty() {
        let cur_y = checklist_y + steps.len() as u16;
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ⟳  ", Style::default().fg(purple)),
                Span::styled(current, Style::default().fg(muted)),
            ])),
            Rect { x: checklist_x, y: cur_y, width: checklist_w, height: 1 },
        );
    }
}

// ── Splash ────────────────────────────────────────────────────────────────────

const LOGO: &[&str] = &[
    r"  ___ ___                             .____    .__  __         ",
    r" /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____  ",
    r"/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \ ",
    r"\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/ ",
    r" \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >",
    r"       \/  \/     |__|        \/              \/             \/ ",
];

fn render_splash(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let pulse  = if (state.splash_tick / 6) % 2 == 0 { purple } else { teal };

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pulse))
        .style(Style::default().bg(ratatui::style::Color::Rgb(8, 8, 18)));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let logo_w = LOGO.iter().map(|l| unicode_width::UnicodeWidthStr::width(*l) as u16).max().unwrap_or(74);
    let logo_h = LOGO.len() as u16 + 6;
    let logo_y = inner.y + inner.height.saturating_sub(logo_h) / 2;
    let logo_x = inner.x + inner.width.saturating_sub(logo_w) / 2;

    for (i, line) in LOGO.iter().enumerate() {
        let color = if i % 2 == 0 { purple } else { teal };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(*line, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            ])),
            Rect { x: logo_x, y: logo_y + i as u16, width: logo_w.min(inner.width), height: 1 },
        );
    }

    let sub_y = logo_y + LOGO.len() as u16 + 1;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("terminal-native  ·  local-only  ·  blazing fast", Style::default().fg(teal)),
        ])).alignment(ratatui::layout::Alignment::Center),
        Rect { x: inner.x, y: sub_y, width: inner.width, height: 1 },
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(concat!("v", env!("CARGO_PKG_VERSION")), Style::default().fg(dim)),
        ])).alignment(ratatui::layout::Alignment::Center),
        Rect { x: inner.x, y: sub_y + 1, width: inner.width, height: 1 },
    );

    let blink_on = (state.splash_tick / 7) % 2 == 0;
    let enter_text = if blink_on { "  ▸  PRESS  ENTER  TO  LAUNCH  ◂  " } else { "" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(enter_text, Style::default().fg(green).add_modifier(Modifier::BOLD)),
        ])).alignment(ratatui::layout::Alignment::Center),
        Rect { x: inner.x, y: sub_y + 3, width: inner.width, height: 1 },
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(" Ctrl+Q quit ", Style::default().fg(dim))])),
        Rect { x: inner.x, y: inner.y + inner.height - 1, width: 16, height: 1 },
    );
}

// ── Model selection ───────────────────────────────────────────────────────────

fn render_model_select(f: &mut ratatui::Frame, area: Rect, state: &mut SetupState) {
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let panel_h = (state.models.len() as u16 + 12).min(area.height.saturating_sub(4));
    let panel   = centered_rect(76, panel_h, area);
    f.render_widget(Clear, panel);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(teal))
        .title(Line::from(vec![
            Span::styled(" Download Models ", Style::default().fg(teal).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(1), Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    let hw_note = if state.hardware.cpu_only {
        format!("CPU only  ·  {} GB RAM", state.hardware.memory.total_mb / 1024)
    } else {
        format!("{:.0} GB VRAM  ·  {} GB RAM",
            state.hardware.best_vram_mb as f32 / 1024.0,
            state.hardware.memory.total_mb / 1024)
    };
    let models_path = models_dir().to_string_lossy().to_string();
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(
                format!("  Filtered for your hardware: {}  ", hw_note),
                Style::default().fg(muted),
            )]),
            Line::from(vec![Span::styled(
                format!("  Models saved to: {}", models_path),
                Style::default().fg(dim),
            )]),
            Line::from(vec![Span::styled(
                "  Space → toggle  ·  ↑↓/jk → navigate  ·  Enter → download  ·  Esc → skip",
                Style::default().fg(dim),
            )]),
        ]),
        chunks[0],
    );

    // Runtime status line — show what was detected/installed
    let (rt_icon, rt_text, rt_col) = if !state.runtime_needed {
        ("✓", "  Inference runtime ready".to_string(), green)
    } else {
        ("✓", "  Runtime installed in previous step".to_string(), green)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} {}", rt_icon, rt_text), Style::default().fg(rt_col)),
        ])),
        chunks[1],
    );

    if state.models.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  No models fit your hardware profile. Add a .gguf file to ~/.hyperlite/models/",
                Style::default().fg(orange),
            )])),
            chunks[2],
        );
    } else {
        let items: Vec<ListItem> = state.models.iter().enumerate().map(|(i, m)| {
            let checked = state.selected.get(i).copied().unwrap_or(false);
            let is_cur  = i == state.list_idx;
            let checkbox = if checked { "[✓] " } else { "[ ] " };
            let cb_col   = if checked { green } else { muted };
            let nm_col   = if is_cur { teal } else { white };
            let bg_col   = if is_cur { ratatui::style::Color::Rgb(26, 26, 48) } else { bg };
            let tags: String = m.tags.iter().map(|t| format!("#{} ", t)).collect();
            ListItem::new(vec![Line::from(vec![
                Span::styled(checkbox, Style::default().fg(cb_col)),
                Span::styled(format!("{:<22}", m.display),
                    Style::default().fg(nm_col).add_modifier(if is_cur { Modifier::BOLD } else { Modifier::empty() })),
                Span::styled(format!("  {:<38}", m.desc), Style::default().fg(muted)),
                Span::styled(tags, Style::default().fg(dim)),
            ])]).style(Style::default().bg(bg_col))
        }).collect();

        let mut ls = ListState::default();
        ls.select(Some(state.list_idx));
        f.render_stateful_widget(
            List::new(items).highlight_style(Style::default().bg(ratatui::style::Color::Rgb(26, 26, 48))),
            chunks[2], &mut ls,
        );
    }

    let sel_count = state.selected.iter().filter(|&&s| s).count();
    let (action_col, action_text) = if let Some(ref err) = state.error_msg {
        (red, format!("  ✗  {}", err))
    } else if sel_count == 0 {
        (muted, "  No models selected  ·  Enter or Esc to skip (if you already have models)".to_string())
    } else {
        (green, format!("  {} model(s) selected  ·  Enter to download from HuggingFace", sel_count))
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(action_text, Style::default().fg(action_col).add_modifier(Modifier::BOLD)),
        ])).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(dim))),
        chunks[3],
    );
}

// ── Download progress ─────────────────────────────────────────────────────────

fn render_downloading(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    // Sudo password prompt overlay
    if state.sudo_prompt {
        let popup = centered_rect(50, 5, area);
        f.render_widget(ratatui::widgets::Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(orange))
            .title(Line::from(vec![Span::styled(" sudo password ", Style::default().fg(orange).add_modifier(Modifier::BOLD))]))
            .style(Style::default().bg(bg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let dots = "●".repeat(state.sudo_input.len());
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![Span::styled("  Enter your sudo password:", Style::default().fg(white))]),
                Line::from(vec![Span::styled(format!("  {}_", dots), Style::default().fg(teal).add_modifier(Modifier::BOLD))]),
            ]),
            inner,
        );
        return;
    }

    let all_done   = state.current_dl.is_none() && state.download_queue.is_empty();
    let total      = state.dl_total_queued;
    let finished   = state.dl_done.len() + state.dl_failed.len();

    let title_text = if all_done {
        " ✓  Setup Complete ".to_string()
    } else {
        format!(" Downloading  {}/{}  ", finished + 1, total.max(1))
    };
    let title_col = if all_done { green } else { purple };

    let panel = centered_rect(72, 26, area);
    f.render_widget(Clear, panel);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_col))
        .title(Line::from(vec![Span::styled(title_text, Style::default().fg(title_col).add_modifier(Modifier::BOLD))]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    // Layout:  overall-bar | current-name | file-bar | size/speed/eta | queue | log | hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 0: overall progress bar
            Constraint::Length(1), // 1: current file name + spinner
            Constraint::Length(1), // 2: current file progress bar
            Constraint::Length(1), // 3: size / speed / ETA
            Constraint::Length(1), // 4: queue / completion message
            Constraint::Min(1),    // 5: log
            Constraint::Length(1), // 6: hint
        ])
        .split(inner);

    // ── Overall progress bar ─────────────────────────────────────────────────
    if total > 0 {
        let overall_done = if all_done { total } else { finished };
        let frac  = overall_done as f64 / total as f64;
        let bar_w = chunks[0].width.saturating_sub(12) as usize;
        let fill  = (bar_w as f64 * frac).round() as usize;
        let empty = bar_w.saturating_sub(fill);
        let bar   = format!("  [{}{:>fill$}{}] {}/{}",
            "█".repeat(fill), "", "░".repeat(empty), overall_done, total,
            fill = 0);
        // Simpler format
        let bar = format!("  [{}{}] {}/{}  files",
            "█".repeat(fill), "░".repeat(empty), overall_done, total);
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(if all_done { green } else { muted })),
        ])), chunks[0]);
    }

    if all_done {
        // ── Completion state ─────────────────────────────────────────────────
        let spinner = ["◐","◓","◑","◒"][(state.splash_tick as usize / 3) % 4];
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(teal)),
            Span::styled("All downloads complete — launching HyperLite…", Style::default().fg(white).add_modifier(Modifier::BOLD)),
        ])), chunks[1]);

        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {} model{} downloaded{}",
                    state.dl_done.len(),
                    if state.dl_done.len() == 1 { "" } else { "s" },
                    if state.dl_failed.is_empty() { "" } else { "  (some failed — check log)" }),
                Style::default().fg(if state.dl_failed.is_empty() { green } else { orange }),
            ),
        ])), chunks[2]);

    } else if let Some(ref job) = state.current_dl {
        // ── Active download ──────────────────────────────────────────────────
        let spinner  = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][(state.splash_tick as usize / 2) % 10];
        let type_tag = if job.is_runtime { "[runtime]" } else { "[model]  " };
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(teal)),
            Span::styled(type_tag, Style::default().fg(dim)),
            Span::styled(format!("  {}", job.name), Style::default().fg(white).add_modifier(Modifier::BOLD)),
            Span::styled("  —  ", Style::default().fg(dim)),
            Span::styled(state.dl_status_msg.clone(), Style::default().fg(muted)),
        ])), chunks[1]);

        // For install jobs (no byte progress), show a patience message instead of a 0% bar
        if job.install_cmd.is_some() {
            let pulse = if (state.splash_tick / 8) % 2 == 0 { orange } else { muted };
            f.render_widget(Paragraph::new(Line::from(vec![
                Span::styled("  ⏳  Installing in the background — this takes 2–5 min, please wait…", Style::default().fg(pulse)),
            ])), chunks[2]);
        } else {
            let pct    = (state.dl_progress * 100.0).min(100.0) as u64;
            let bar_w  = chunks[2].width.saturating_sub(10) as usize;
            let filled = (bar_w as f64 * state.dl_progress).round() as usize;
            let empty  = bar_w.saturating_sub(filled);
            let bar    = format!("  [{}{}] {:>3}%", "█".repeat(filled), "░".repeat(empty), pct);
            f.render_widget(Paragraph::new(Line::from(vec![
                Span::styled(bar, Style::default().fg(teal)),
            ])), chunks[2]);
        }

        let fmt_bytes = |b: u64| -> String {
            if b >= 1_073_741_824 { format!("{:.2} GB", b as f64 / 1_073_741_824.0) }
            else if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) }
            else { format!("{} KB", b / 1024) }
        };
        let speed_str = if state.dl_speed_bps > 0.0 {
            let s = state.dl_speed_bps;
            if s >= 1_048_576.0 { format!("{:.1} MB/s", s / 1_048_576.0) }
            else { format!("{:.0} KB/s", s / 1024.0) }
        } else { "—".to_string() };

        let eta_str = if state.dl_speed_bps > 1024.0 && state.dl_bytes_total > state.dl_bytes_done {
            let remaining = state.dl_bytes_total - state.dl_bytes_done;
            let secs = remaining as f64 / state.dl_speed_bps;
            if secs < 60.0 { format!("{}s", secs as u64) }
            else { format!("{}m {}s", secs as u64 / 60, secs as u64 % 60) }
        } else { "—".to_string() };

        let size_info = if state.dl_bytes_total > 0 {
            format!("  {} / {}    {}    ETA {}",
                fmt_bytes(state.dl_bytes_done), fmt_bytes(state.dl_bytes_total),
                speed_str, eta_str)
        } else {
            format!("  connecting…    {}", state.dl_status_msg)
        };
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(size_info, Style::default().fg(muted)),
        ])), chunks[3]);

    } else {
        // ── Preparing first download ─────────────────────────────────────────
        let spinner = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][(state.splash_tick as usize / 2) % 10];
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(teal)),
            Span::styled("Preparing downloads…", Style::default().fg(muted)),
        ])), chunks[1]);
    }

    // ── Queue ────────────────────────────────────────────────────────────────
    if !state.download_queue.is_empty() && !all_done {
        let queued: String = state.download_queue.iter()
            .map(|j| j.name.as_str())
            .collect::<Vec<_>>().join("  ·  ");
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("  up next  ", Style::default().fg(dim)),
            Span::styled(queued, Style::default().fg(muted)),
        ])), chunks[4]);
    }

    // ── Log ──────────────────────────────────────────────────────────────────
    let log_start  = state.dl_log.len().saturating_sub(chunks[5].height as usize);
    let log_lines: Vec<Line<'static>> = state.dl_log[log_start..].iter().map(|l| {
        let color = if l.contains('✓') { green }
            else if l.contains('✗')    { red   }
            else if l.contains("…")    { teal  }
            else                       { dim   };
        Line::from(vec![Span::styled(format!("  {}", l), Style::default().fg(color))])
    }).collect();
    f.render_widget(Paragraph::new(log_lines), chunks[5]);

    // ── Hint ─────────────────────────────────────────────────────────────────
    let hint = if all_done {
        "  launching automatically…"
    } else if !state.dl_done.is_empty() || !state.dl_failed.is_empty() {
        "  Esc / s → skip remaining downloads and launch"
    } else {
        "  downloading — please wait…"
    };
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled(hint, Style::default().fg(dim)),
    ])), chunks[6]);
}

fn render_launching(f: &mut ratatui::Frame, area: Rect) {
    let teal = ratatui::style::Color::Rgb(0, 245, 212);
    let bg   = ratatui::style::Color::Rgb(8, 8, 18);
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Launching HyperLite…", Style::default().fg(teal)),
        ])),
        area,
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn strip_ansi(s: &str) -> String {
    let stripped = strip_ansi_escapes::strip(s.as_bytes());
    String::from_utf8(stripped).unwrap_or_else(|_| s.to_string())
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_w = (r.width * percent_x / 100).min(r.width.saturating_sub(4));
    let popup_h = height.min(r.height.saturating_sub(2));
    Rect {
        x: r.x + (r.width.saturating_sub(popup_w)) / 2,
        y: r.y + (r.height.saturating_sub(popup_h)) / 2,
        width:  popup_w,
        height: popup_h,
    }
}
