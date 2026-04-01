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
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
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

pub fn models_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hyperlite")
        .join("models")
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

/// Path where HyperLite stores the bundled llamafile runtime.
pub fn runtime_path() -> PathBuf {
    let filename = if cfg!(windows) { "llamafile.exe" } else { "llamafile" };
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hyperlite")
        .join(filename)
}

/// Returns true if a usable inference runtime is available.
pub fn has_runtime() -> bool {
    if runtime_path().exists() { return true; }
    which::which("llama-server").is_ok()
        || which::which("llama-cpp-server").is_ok()
        || which::which("llamafile").is_ok()
}

/// Download job for the llamafile runtime binary.
fn runtime_download_job() -> DownloadJob {
    DownloadJob {
        name:       "llamafile  (inference runtime)".to_string(),
        url:        "https://github.com/Mozilla-Ocho/llamafile/releases/download/0.9.2/llamafile-0.9.2".to_string(),
        filename:   if cfg!(windows) { "llamafile.exe" } else { "llamafile" }.to_string(),
        is_runtime: true,
    }
}

// ── Download progress events ──────────────────────────────────────────────────

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
    name:       String,  // display name shown in UI
    url:        String,  // download URL
    filename:   String,  // destination filename
    is_runtime: bool,    // if true, save to ~/.hyperlite/ and chmod+x
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
        hf_repo: "bartowski/Phi-4-mini-instruct-GGUF",
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
        hf_repo: "Qwen/Qwen2.5-14B-Instruct-GGUF",
        hf_file: "qwen2.5-14b-instruct-q4_k_m.gguf",
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
        hf_repo: "Qwen/Qwen2.5-32B-Instruct-GGUF",
        hf_file: "qwen2.5-32b-instruct-q4_k_m.gguf",
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
    step:           SetupStep,
    hardware:       HardwareInfo,
    models:         Vec<RecommendedModel>,
    selected:       Vec<bool>,
    list_idx:       usize,
    runtime_needed: bool,
    download_queue: Vec<DownloadJob>,
    current_dl:     Option<DownloadJob>,
    dl_progress:    f64,
    dl_log:         Vec<String>,
    dl_done:        Vec<String>,
    dl_failed:      Vec<String>,
    dl_status_msg:  String,
    dl_bytes_total: u64,
    dl_bytes_done:  u64,
    dl_speed_bps:   f64,
    dl_start:       Option<Instant>,
    dl_rx:          Option<tmpsc::UnboundedReceiver<DlEvent>>,
    http_client:    reqwest::Client,
    splash_tick:    u8,
    error_msg:      Option<String>,
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

        let selected = vec![false; models.len()];

        Self {
            step: SetupStep::Splash,
            hardware,
            models,
            selected,
            list_idx: 0,
            runtime_needed: !has_runtime(),
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
            http_client,
            splash_tick: 0,
            error_msg: None,
        }
    }

    fn selected_jobs(&self) -> Vec<DownloadJob> {
        self.models.iter().zip(self.selected.iter())
            .filter_map(|(m, &sel)| if sel {
                Some(DownloadJob {
                    name:       m.display.to_string(),
                    url:        m.hf_url(),
                    filename:   m.hf_file.to_string(),
                    is_runtime: false,
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
        terminal.draw(|f| render(f, &mut state))?;
        state.splash_tick = state.splash_tick.wrapping_add(1);

        // ── Splash: advance on Enter ───────────────────────────────────────────
        if state.step == SetupStep::Splash {
            if event::poll(Duration::from_millis(80))? {
                let ev = event::read()?;
                if let Event::Key(key) = ev {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('q') {
                        std::process::exit(0);
                    }
                    if key.code == KeyCode::Enter || key.code == KeyCode::Char(' ') {
                        if !existing_models.is_empty() || has_local_models() {
                            if state.runtime_needed {
                                // Have models but no runtime — download it now
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
            if state.current_dl.is_none() && !state.download_queue.is_empty() {
                start_next_download(&mut state);
            }
            poll_download_progress(&mut state);
            if state.download_queue.is_empty() && state.current_dl.is_none() {
                state.step = SetupStep::Done;
            }
        }

        // ── Input ──────────────────────────────────────────────────────────────
        if !event::poll(Duration::from_millis(80))? { continue; }
        let ev = event::read()?;
        let Event::Key(key) = ev else { continue };

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
                        if state.runtime_needed {
                            jobs.push(runtime_download_job());
                        }
                        if !jobs.is_empty() {
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
    state.dl_status_msg  = "connecting…".to_string();
    state.dl_start       = Some(Instant::now());
    state.dl_log.push(format!("Downloading {}…", job.name));

    let (tx, rx) = tmpsc::unbounded_channel::<DlEvent>();
    state.dl_rx = Some(rx);

    let client     = state.http_client.clone();
    let url        = job.url.clone();
    let filename   = job.filename.clone();
    let name       = job.name.clone();
    let is_runtime = job.is_runtime;
    state.current_dl = Some(job);

    tokio::spawn(async move {
        // Runtimes go to ~/.hyperlite/, models go to ~/.hyperlite/models/
        let dest = if is_runtime {
            let base = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".hyperlite");
            if let Err(e) = tokio::fs::create_dir_all(&base).await {
                let _ = tx.send(DlEvent::Failed(format!("Could not create dir: {}", e)));
                return;
            }
            base.join(&filename)
        } else {
            match crate::startup::ensure_models_dir() {
                Ok(d)  => d.join(&filename),
                Err(e) => { let _ = tx.send(DlEvent::Failed(format!("Could not create models dir: {}", e))); return; }
            }
        };

        // Open destination file
        let mut file = match tokio::fs::File::create(&dest).await {
            Ok(f)  => f,
            Err(e) => { let _ = tx.send(DlEvent::Failed(format!("File create failed: {}", e))); return; }
        };

        let _ = tx.send(DlEvent::Status("connecting…".to_string()));

        let resp = match client.get(&url)
            .header("User-Agent", "HyperLite/0.1")
            .send().await
        {
            Ok(r)  => r,
            Err(e) => { let _ = tx.send(DlEvent::Failed(e.to_string())); return; }
        };

        if !resp.status().is_success() {
            let _ = tx.send(DlEvent::Failed(format!("HTTP {} from {}", resp.status(), url)));
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

        // Make runtime executable on Unix
        #[cfg(unix)]
        if is_runtime {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&dest) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&dest, perms);
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
                state.current_dl    = None;
                state.dl_rx         = None;
                start_next_download(state);
                return;
            }
            Ok(DlEvent::Failed(e)) => {
                let name = state.current_dl.as_ref().map(|j| j.name.clone()).unwrap_or_default();
                state.dl_failed.push(name.clone());
                state.dl_log.push(format!("✗ {} — {}", name, e));
                state.current_dl = None;
                state.dl_rx      = None;
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
        SetupStep::Done        => {}
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
        .constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    let hw_note = if state.hardware.cpu_only {
        format!("CPU only  ·  {} GB RAM", state.hardware.memory.total_mb / 1024)
    } else {
        format!("{:.0} GB VRAM  ·  {} GB RAM",
            state.hardware.best_vram_mb as f32 / 1024.0,
            state.hardware.memory.total_mb / 1024)
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(
                format!("  Filtered for your hardware: {}  ", hw_note),
                Style::default().fg(muted),
            )]),
            Line::from(vec![Span::styled(
                "  Space → toggle  ·  ↑↓/jk → navigate  ·  Enter → download  ·  Esc → skip",
                Style::default().fg(dim),
            )]),
        ]),
        chunks[0],
    );

    // Runtime status line
    let (rt_icon, rt_text, rt_col) = if state.runtime_needed {
        ("↓", "  llamafile will be downloaded automatically  (inference runtime)", orange)
    } else {
        ("✓", "  Inference runtime found", green)
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
    } else if sel_count == 0 && state.runtime_needed {
        (orange, "  Enter to download llamafile runtime  ·  Esc to skip (if you already have models)".to_string())
    } else if sel_count == 0 {
        (muted, "  No models selected  ·  Enter or Esc to skip (if you already have models)".to_string())
    } else if state.runtime_needed {
        (green, format!("  {} model(s) + llamafile runtime  ·  Enter to download", sel_count))
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
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let panel = centered_rect(72, 26, area);
    f.render_widget(Clear, panel);

    let total_queued = state.dl_done.len() + state.dl_failed.len()
        + state.current_dl.is_some() as usize + state.download_queue.len();
    let finished = state.dl_done.len() + state.dl_failed.len();

    let title = if state.current_dl.is_some() {
        format!(" Downloading  {}/{} ", finished + 1, total_queued)
    } else {
        " Downloads Complete ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(purple))
        .title(Line::from(vec![Span::styled(title, Style::default().fg(purple).add_modifier(Modifier::BOLD))]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    if let Some(ref job) = state.current_dl {
        let pct     = (state.dl_progress * 100.0).min(100.0) as u64;
        let spinner = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][(state.splash_tick as usize / 2) % 10];

        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(teal)),
            Span::styled(job.name.clone(), Style::default().fg(white).add_modifier(Modifier::BOLD)),
            Span::styled("  —  ", Style::default().fg(dim)),
            Span::styled(state.dl_status_msg.clone(), Style::default().fg(muted)),
        ])), chunks[0]);

        let bar_w  = chunks[2].width.saturating_sub(10) as usize;
        let filled = (bar_w as f64 * state.dl_progress).round() as usize;
        let empty  = bar_w.saturating_sub(filled);
        let bar    = format!("  [{}{}] {:>3}%", "█".repeat(filled), "░".repeat(empty), pct);
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(teal)),
        ])), chunks[2]);

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
        } else { "calculating…".to_string() };

        let size_info = if state.dl_bytes_total > 0 {
            format!("  {} / {}    {}    ETA {}",
                fmt_bytes(state.dl_bytes_done), fmt_bytes(state.dl_bytes_total),
                speed_str, eta_str)
        } else {
            format!("  {}    {}", speed_str, state.dl_status_msg)
        };
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(size_info, Style::default().fg(muted)),
        ])), chunks[3]);

    } else if state.dl_done.is_empty() && state.dl_failed.is_empty() {
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("  Preparing download…", Style::default().fg(muted)),
        ])), chunks[0]);
    } else {
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("  ✓  All downloads complete!", Style::default().fg(green).add_modifier(Modifier::BOLD)),
        ])), chunks[0]);
    }

    if !state.download_queue.is_empty() {
        let queued: String = state.download_queue.iter()
            .map(|j| j.name.as_str())
            .collect::<Vec<_>>().join("  ·  ");
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("  up next  ", Style::default().fg(dim)),
            Span::styled(queued, Style::default().fg(muted)),
        ])), chunks[4]);
    }

    let log_start  = state.dl_log.len().saturating_sub(chunks[5].height as usize);
    let log_lines: Vec<Line<'static>> = state.dl_log[log_start..].iter().map(|l| {
        let color = if l.contains('✓') { green }
            else if l.contains('✗')    { red   }
            else if l.contains("…")    { teal  }
            else                       { dim   };
        Line::from(vec![Span::styled(format!("  {}", l), Style::default().fg(color))])
    }).collect();
    f.render_widget(Paragraph::new(log_lines), chunks[5]);

    let hint = if !state.dl_done.is_empty() || !state.dl_failed.is_empty() {
        "  Esc / s → skip remaining and launch"
    } else {
        "  downloading from HuggingFace — please wait…"
    };
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled(hint, Style::default().fg(dim)),
    ])), chunks[6]);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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
