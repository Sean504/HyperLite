/// Startup splash + first-run setup wizard.
///
/// Flow:
///   1. Splash — press ENTER
///   2a. If models already exist → Done (straight to chat)
///   2b. If Ollama not running  → OllamaSetup (start / install / skip)
///   3. Hardware summary
///   4. Model selection (user MUST pick at least one or already have models)
///   5. Download progress
///   Done — only reachable when ≥1 model exists

use std::io;
use std::time::{Duration, Instant};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use tokio::sync::mpsc as tmpsc;

use crate::hardware::HardwareInfo;
use crate::providers::LocalModel;

// ── Download progress events ──────────────────────────────────────────────────

#[derive(Debug)]
enum DlEvent {
    Status(String),                         // e.g. "pulling manifest"
    Progress { total: u64, completed: u64 },// real byte counts from Ollama
    Done(String),                           // model name — finished
    Failed(String),                         // model name — error
}

// ── Recommended models catalogue ─────────────────────────────────────────────

#[derive(Clone)]
struct RecommendedModel {
    ollama_name: &'static str,
    display:     &'static str,
    desc:        &'static str,
    vram_mb:     u64,
    ram_mb:      u64,
    tags:        &'static [&'static str],
}

const RECOMMENDED_MODELS: &[RecommendedModel] = &[
    RecommendedModel { ollama_name: "smollm2:1.7b",        display: "SmolLM2 1.7B",         desc: "Tiny but capable. Runs anywhere.",                  vram_mb: 1200,  ram_mb: 3000,  tags: &["Fast","CPU-OK"]          },
    RecommendedModel { ollama_name: "phi4-mini",            display: "Phi-4 Mini 3.8B",       desc: "Microsoft's punchy small model. Great reasoning.",   vram_mb: 2500,  ram_mb: 6000,  tags: &["Coding","Reasoning"]      },
    RecommendedModel { ollama_name: "qwen2.5:3b",           display: "Qwen2.5 3B",            desc: "Alibaba multilingual workhorse.",                    vram_mb: 2200,  ram_mb: 5000,  tags: &["Writing","Coding"]        },
    RecommendedModel { ollama_name: "llama3.2:3b",          display: "Llama 3.2 3B",          desc: "Meta's small Llama. Solid all-rounder.",             vram_mb: 2400,  ram_mb: 6000,  tags: &["Writing","Coding"]        },
    RecommendedModel { ollama_name: "qwen2.5-coder:7b",     display: "Qwen2.5-Coder 7B",      desc: "Best-in-class code model at 7B.",                   vram_mb: 5000,  ram_mb: 10000, tags: &["Coding","Best-Code"]      },
    RecommendedModel { ollama_name: "mistral:7b",           display: "Mistral 7B",            desc: "Fast and strong general-purpose.",                   vram_mb: 5000,  ram_mb: 10000, tags: &["Writing","Fast"]          },
    RecommendedModel { ollama_name: "llama3.1:8b",          display: "Llama 3.1 8B",          desc: "Meta's flagship 8B. Excellent instruction following.",vram_mb: 5500, ram_mb: 12000, tags: &["Writing","ToolUse"]       },
    RecommendedModel { ollama_name: "qwen2.5:14b",          display: "Qwen2.5 14B",           desc: "Great balance of capability and size.",              vram_mb: 10000, ram_mb: 20000, tags: &["Writing","Reasoning"]     },
    RecommendedModel { ollama_name: "deepseek-r1:14b",      display: "DeepSeek-R1 14B",       desc: "Chain-of-thought reasoning model.",                  vram_mb: 10000, ram_mb: 20000, tags: &["Reasoning","Math"]        },
    RecommendedModel { ollama_name: "qwen2.5-coder:14b",    display: "Qwen2.5-Coder 14B",     desc: "Top coding model for mid-range GPUs.",               vram_mb: 10000, ram_mb: 20000, tags: &["Coding","Best-Code"]      },
    RecommendedModel { ollama_name: "qwen2.5:32b",          display: "Qwen2.5 32B",           desc: "Frontier-level quality for high-end GPUs.",          vram_mb: 22000, ram_mb: 40000, tags: &["Writing","Reasoning"]     },
    RecommendedModel { ollama_name: "deepseek-r1:32b",      display: "DeepSeek-R1 32B",       desc: "Best open reasoning model.",                         vram_mb: 22000, ram_mb: 40000, tags: &["Reasoning","Best-Reason"]  },
    RecommendedModel { ollama_name: "llama3.3:70b",         display: "Llama 3.3 70B",         desc: "Near-frontier. Needs 40 GB+ VRAM or large RAM.",     vram_mb: 45000, ram_mb: 80000, tags: &["Writing","Coding"]        },
];

// ── Setup wizard state ────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SetupStep {
    // ── Flow A: everything OK ─────────────────────────────────────────────────
    Splash,   // brief logo splash → Done (user never sees setup)

    // ── Flow B: setup required ────────────────────────────────────────────────
    // Reached only when preflight fails (Ollama missing/not running / no models)
    SetupChecks,   // animated preflight checklist, runs automatically
    SetupInstall,  // Ollama not installed — install flow
    SetupStart,    // Ollama installed but not running — start it
    SetupModels,   // Pick + download at least one model
    Downloading,

    Done,
}

#[derive(Clone, PartialEq)]
enum OllamaAction {
    Idle,
    PreFlight,      // Running dependency checks (shows checklist)
    EnterPassword,  // Collecting sudo password (skipped if already root)
    Starting,       // Running `ollama serve`
    Installing,     // Running install script
}

/// Results of the pre-flight dependency scan.
#[derive(Clone)]
struct PreFlight {
    has_curl:    Option<bool>,   // curl or wget found
    has_sudo:    Option<bool>,   // sudo found (or already root)
    is_root:     bool,           // running as uid 0 — no password needed
    pkg_manager: &'static str,   // "apt-get" | "dnf" | "yum" | "pacman" | ""
    use_wget:    bool,           // fall back to wget if no curl
}

impl PreFlight {
    fn run() -> Self {
        let has_curl  = which::which("curl").is_ok();
        let use_wget  = !has_curl && which::which("wget").is_ok();
        let is_root   = unsafe { libc::getuid() } == 0;
        let has_sudo  = is_root || which::which("sudo").is_ok();

        let pkg_manager = if which::which("apt-get").is_ok()  { "apt-get" }
            else if which::which("dnf").is_ok()               { "dnf"     }
            else if which::which("yum").is_ok()               { "yum"     }
            else if which::which("pacman").is_ok()            { "pacman"  }
            else if which::which("zypper").is_ok()            { "zypper"  }
            else                                              { ""        };

        PreFlight {
            has_curl:    Some(has_curl || use_wget),
            has_sudo:    Some(has_sudo),
            is_root,
            pkg_manager,
            use_wget,
        }
    }

    fn all_ok(&self) -> bool {
        self.has_curl == Some(true) && self.has_sudo == Some(true)
    }

    /// Build the shell command to run under sudo (or sh if already root).
    /// Installs curl + zstd (required by the Ollama installer) if missing,
    /// then runs the Ollama installer.
    fn build_install_cmd(&self) -> String {
        // zstd is required by the Ollama install script for binary extraction
        let deps_install = match self.pkg_manager {
            "apt-get" => {
                let mut pkgs = vec!["zstd"];
                if self.has_curl == Some(false) { pkgs.push("curl"); }
                format!("apt-get install -y {}; ", pkgs.join(" "))
            }
            "dnf"    => {
                let mut pkgs = vec!["zstd"];
                if self.has_curl == Some(false) { pkgs.push("curl"); }
                format!("dnf install -y {}; ", pkgs.join(" "))
            }
            "yum"    => {
                let mut pkgs = vec!["zstd"];
                if self.has_curl == Some(false) { pkgs.push("curl"); }
                format!("yum install -y {}; ", pkgs.join(" "))
            }
            "pacman" => {
                let mut pkgs = vec!["zstd"];
                if self.has_curl == Some(false) { pkgs.push("curl"); }
                format!("pacman -S --noconfirm {}; ", pkgs.join(" "))
            }
            "zypper" => {
                let mut pkgs = vec!["zstd"];
                if self.has_curl == Some(false) { pkgs.push("curl"); }
                format!("zypper install -y {}; ", pkgs.join(" "))
            }
            _ => String::new(),
        };

        let fetch = if self.use_wget {
            "wget -qO- https://ollama.com/install.sh | sh"
        } else {
            "curl -fsSL https://ollama.com/install.sh | sh"
        };

        format!("{}{}", deps_install, fetch)
    }
}

struct SetupState {
    step:           SetupStep,
    hardware:       HardwareInfo,
    models:         Vec<RecommendedModel>,
    selected:       Vec<bool>,
    list_idx:       usize,
    download_queue: Vec<String>,
    current_dl:     Option<String>,
    dl_progress:    f64,
    dl_log:         Vec<String>,
    dl_done:        Vec<String>,
    dl_failed:      Vec<String>,
    // real-time download progress
    dl_status_msg:  String,
    dl_bytes_total: u64,
    dl_bytes_done:  u64,
    dl_speed_bps:   f64,     // smoothed bytes/sec
    dl_start:       Option<Instant>,
    dl_rx:          Option<tmpsc::UnboundedReceiver<DlEvent>>,
    http_client:    reqwest::Client,
    ollama_url:     String,
    splash_tick:    u8,
    error_msg:      Option<String>,
    ollama_present: bool,
    ollama_installed: bool,
    ollama_action:  OllamaAction,
    action_started: Option<Instant>,
    action_log:     Vec<String>,
    needs_install:  bool,
    password_buf:   String,
    preflight:      Option<PreFlight>,
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
        let ollama_installed = which::which("ollama").is_ok();

        Self {
            step: SetupStep::Splash,
            hardware,
            models,
            selected,
            list_idx: 0,
            download_queue: vec![],
            current_dl: None,
            dl_progress: 0.0,
            dl_log: vec![],
            dl_done: vec![],
            dl_failed: vec![],
            dl_status_msg: String::new(),
            dl_bytes_total: 0,
            dl_bytes_done:  0,
            dl_speed_bps:   0.0,
            dl_start:       None,
            dl_rx:          None,
            http_client,
            ollama_url: "http://localhost:11434".to_string(),
            splash_tick: 0,
            error_msg: None,
            ollama_present: false,
            ollama_installed,
            ollama_action: OllamaAction::Idle,
            action_started: None,
            action_log: vec![],
            needs_install: false,
            password_buf: String::new(),
            preflight: None,
        }
    }

    fn selected_names(&self) -> Vec<String> {
        self.models.iter().zip(self.selected.iter())
            .filter_map(|(m, &sel)| if sel { Some(m.ollama_name.to_string()) } else { None })
            .collect()
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run_startup(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    hardware:       HardwareInfo,
    existing_models: &[LocalModel],
    ollama_present: bool,
    http_client:    reqwest::Client,
) -> anyhow::Result<()> {
    let mut state = SetupState::new(hardware, http_client.clone());
    state.ollama_present = ollama_present;

    loop {
        terminal.draw(|f| render(f, &mut state))?;
        state.splash_tick = state.splash_tick.wrapping_add(1);

        // ── Flow A: Splash — run preflight silently, decide which flow ─────────
        if state.step == SetupStep::Splash {
            // Run preflight once, automatically, while the splash is showing
            if state.preflight.is_none() {
                let pf = PreFlight::run();
                state.ollama_installed = which::which("ollama").is_ok()
                    || std::path::Path::new("/usr/local/bin/ollama").exists();
                state.preflight = Some(pf);
            }

            if event::poll(Duration::from_millis(80))? {
                let ev = event::read()?;
                if let Event::Key(key) = ev {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('q') {
                        std::process::exit(0);
                    }
                    if key.code == KeyCode::Enter || key.code == KeyCode::Char(' ') {
                        if !existing_models.is_empty() && state.ollama_present {
                            // ── Flow A: everything ready ──────────────────────
                            state.step = SetupStep::Done;
                        } else {
                            // ── Flow B: setup required ────────────────────────
                            state.step = SetupStep::SetupChecks;
                        }
                    }
                }
            }
            continue;
        }

        // ── Flow B: SetupChecks — animated checklist, auto-advances ───────────
        if state.step == SetupStep::SetupChecks {
            // Auto-advance after one pass of rendering the checklist (~500ms)
            if state.splash_tick > 8 {
                if !state.ollama_installed {
                    state.step = SetupStep::SetupInstall;
                } else if !state.ollama_present {
                    state.step = SetupStep::SetupStart;
                } else {
                    state.step = SetupStep::SetupModels;
                }
            }
            if event::poll(Duration::from_millis(80))? {
                let ev = event::read()?;
                if let Event::Key(key) = ev {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('q') {
                        std::process::exit(0);
                    }
                }
            }
            continue;
        }

        // ── Install trigger (blocks, drops to terminal) ────────────────────────
        if state.needs_install {
            state.needs_install = false;
            let password = state.password_buf.clone();
            state.password_buf.clear();

            let pf = state.preflight.clone().unwrap_or_else(PreFlight::run);
            let cmd = pf.build_install_cmd();

            crossterm::terminal::disable_raw_mode()?;
            crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
            println!("\n  Installing Ollama…\n  {}\n  {}\n", cmd, "─".repeat(60));

            let mut child = if pf.is_root {
                std::process::Command::new("sh").args(["-c", &cmd])
                    .stdin(std::process::Stdio::null()).spawn()
            } else {
                std::process::Command::new("sudo").args(["-S", "sh", "-c", &cmd])
                    .stdin(std::process::Stdio::piped()).spawn()
            };
            if let Ok(ref mut c) = child {
                if let Some(mut stdin) = c.stdin.take() {
                    use std::io::Write;
                    let _ = writeln!(stdin, "{}", password);
                }
            }
            let _ = child.and_then(|mut c| c.wait());
            println!();

            crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
            crossterm::terminal::enable_raw_mode()?;
            terminal.clear()?;

            let found = which::which("ollama").is_ok()
                || std::path::Path::new("/usr/local/bin/ollama").exists()
                || std::path::Path::new("/usr/bin/ollama").exists();

            if found {
                state.ollama_installed = true;
                state.action_log.push("  ✓ Ollama installed".to_string());
                state.action_log.push("  → Starting ollama serve…".to_string());
                let _ = std::process::Command::new("ollama").arg("serve")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null()).spawn();
                state.ollama_action  = OllamaAction::Starting;
                state.action_started = Some(Instant::now());
            } else {
                state.ollama_action = OllamaAction::Idle;
                state.error_msg = Some("Binary not found — check terminal output above, then retry.".to_string());
            }
        }

        // ── Poll Ollama while starting/installing ──────────────────────────────
        if matches!(state.ollama_action, OllamaAction::Starting | OllamaAction::Installing)
            && state.splash_tick % 10 == 0
        {
            state.ollama_present = check_ollama(&state.http_client, &state.ollama_url).await;
            if state.ollama_present {
                state.action_log.push("  ✓ Ollama is running!".to_string());
                state.ollama_action = OllamaAction::Idle;
                state.step = SetupStep::SetupModels;
            } else if let Some(t) = state.action_started {
                if t.elapsed().as_secs() > 60 {
                    state.error_msg = Some("Timed out — run `ollama serve` manually then press Enter.".to_string());
                    state.ollama_action  = OllamaAction::Idle;
                    state.action_started = None;
                }
            }
        }

        // ── Drive downloads ────────────────────────────────────────────────────
        if state.step == SetupStep::Downloading {
            // Kick off first download if nothing running yet
            if state.current_dl.is_none() && !state.download_queue.is_empty() {
                start_next_download(&mut state);
            }
            // Drain progress events from background task
            poll_download_progress(&mut state);
            // Advance when everything is done
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

            // ── SetupInstall: Ollama not installed ────────────────────────────
            SetupStep::SetupInstall => match state.ollama_action.clone() {
                OllamaAction::EnterPassword => match key.code {
                    KeyCode::Enter     => { state.needs_install = true; }
                    KeyCode::Esc       => { state.ollama_action = OllamaAction::Idle; state.password_buf.clear(); }
                    KeyCode::Backspace => { state.password_buf.pop(); }
                    KeyCode::Char(c)   => { state.password_buf.push(c); }
                    _ => {}
                },
                OllamaAction::Idle => match key.code {
                    KeyCode::Enter | KeyCode::Char('i') => {
                        state.error_msg = None;
                        let pf = PreFlight::run();
                        if pf.is_root {
                            state.preflight     = Some(pf);
                            state.needs_install = true;
                        } else if pf.has_sudo == Some(true) {
                            state.preflight     = Some(pf);
                            state.ollama_action = OllamaAction::EnterPassword;
                            state.password_buf.clear();
                        } else {
                            state.error_msg = Some("sudo not available — install Ollama manually.".to_string());
                        }
                    }
                    KeyCode::Char('s') | KeyCode::Esc => { state.step = SetupStep::SetupModels; }
                    _ => {}
                },
                _ => {}
            },

            // ── SetupStart: Ollama installed but not running ──────────────────
            SetupStep::SetupStart => match key.code {
                KeyCode::Enter | KeyCode::Char('s') => {
                    let _ = std::process::Command::new("ollama").arg("serve")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null()).spawn();
                    state.ollama_action  = OllamaAction::Starting;
                    state.action_started = Some(Instant::now());
                    state.action_log.push("  → Starting `ollama serve`…".to_string());
                }
                KeyCode::Char('k') | KeyCode::Esc => { state.step = SetupStep::SetupModels; }
                _ => {}
            },

            // ── SetupModels: pick + download models ───────────────────────────
            SetupStep::SetupModels => {
                let can_proceed = !existing_models.is_empty() || !state.dl_done.is_empty();
                match key.code {
                    KeyCode::Up   | KeyCode::Char('k') => { state.list_idx = state.list_idx.saturating_sub(1); }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.list_idx + 1 < state.models.len() { state.list_idx += 1; }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(s) = state.selected.get_mut(state.list_idx) { *s = !*s; }
                    }
                    KeyCode::Enter => {
                        let names = state.selected_names();
                        if !names.is_empty() {
                            state.download_queue = names;
                            state.step = SetupStep::Downloading;
                        } else if can_proceed {
                            state.step = SetupStep::Done;
                        } else {
                            state.error_msg = Some("Select at least one model.".to_string());
                        }
                    }
                    KeyCode::Esc => {
                        if can_proceed { state.step = SetupStep::Done; }
                        else { state.error_msg = Some("You need at least one model.".to_string()); }
                    }
                    _ => {}
                }
            }

            // ── Downloading ───────────────────────────────────────────────────
            SetupStep::Downloading => {
                if (key.code == KeyCode::Char('s') || key.code == KeyCode::Esc)
                    && (!existing_models.is_empty() || !state.dl_done.is_empty())
                {
                    state.step = SetupStep::Done;
                }
            }

            SetupStep::Done => { return Ok(()); }

            // Splash / SetupChecks handled above with `continue`
            _ => {}
        }
    }
}

// ── Download driver ───────────────────────────────────────────────────────────

/// Start the next queued download as a background task.
/// Returns immediately; progress arrives via `state.dl_rx`.
fn start_next_download(state: &mut SetupState) {
    if state.current_dl.is_some() || state.download_queue.is_empty() {
        return;
    }
    let name = state.download_queue.remove(0);
    state.current_dl    = Some(name.clone());
    state.dl_progress   = 0.0;
    state.dl_bytes_total = 0;
    state.dl_bytes_done  = 0;
    state.dl_speed_bps   = 0.0;
    state.dl_status_msg  = "connecting…".to_string();
    state.dl_start       = Some(Instant::now());
    state.dl_log.push(format!("Pulling {}…", name));

    let (tx, rx) = tmpsc::unbounded_channel::<DlEvent>();
    state.dl_rx = Some(rx);

    let client = state.http_client.clone();
    let url    = format!("{}/api/pull", state.ollama_url);

    tokio::spawn(async move {
        let body = serde_json::json!({ "name": name, "stream": true });
        let resp = match client.post(&url).json(&body).send().await {
            Ok(r)  => r,
            Err(e) => { let _ = tx.send(DlEvent::Failed(e.to_string())); return; }
        };

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf    = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c)  => c,
                Err(e) => { let _ = tx.send(DlEvent::Failed(e.to_string())); return; }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));

            // Each newline-delimited JSON object is one event
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if line.is_empty() { continue; }

                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    let status = v["status"].as_str().unwrap_or("").to_string();
                    let total     = v["total"].as_u64().unwrap_or(0);
                    let completed = v["completed"].as_u64().unwrap_or(0);

                    if total > 0 {
                        let _ = tx.send(DlEvent::Progress { total, completed });
                    } else if !status.is_empty() {
                        let _ = tx.send(DlEvent::Status(status.clone()));
                    }

                    if status == "success" {
                        let _ = tx.send(DlEvent::Done(String::new()));
                        return;
                    }
                }
            }
        }
        // Stream ended without explicit "success" — treat as done
        let _ = tx.send(DlEvent::Done(String::new()));
    });
}

/// Drain the progress channel and update state. Non-blocking.
fn poll_download_progress(state: &mut SetupState) {
    let name = match state.current_dl.clone() { Some(n) => n, None => return };
    let rx   = match state.dl_rx.as_mut()     { Some(r) => r, None => return };
    let now  = Instant::now();

    loop {
        match rx.try_recv() {
            Ok(DlEvent::Status(s)) => {
                state.dl_status_msg = s.clone();
                state.dl_log.push(format!("  {}", s));
                if state.dl_log.len() > 40 { state.dl_log.remove(0); }
            }
            Ok(DlEvent::Progress { total, completed }) => {
                let prev_done = state.dl_bytes_done;
                state.dl_bytes_total = total;
                state.dl_bytes_done  = completed;
                state.dl_progress    = completed as f64 / total as f64;
                state.dl_status_msg  = "downloading…".to_string();

                // Smoothed speed (exponential moving average)
                if let Some(start) = state.dl_start {
                    let elapsed = start.elapsed().as_secs_f64().max(0.001);
                    let instant_speed = (completed.saturating_sub(prev_done)) as f64 / 0.08; // ~80ms tick
                    state.dl_speed_bps = state.dl_speed_bps * 0.85 + instant_speed * 0.15;
                    let _ = elapsed;
                }
            }
            Ok(DlEvent::Done(_)) => {
                state.dl_progress    = 1.0;
                state.dl_status_msg  = "complete".to_string();
                state.dl_done.push(name.clone());
                state.dl_log.push(format!("✓ {} ready", name));
                state.current_dl = None;
                state.dl_rx      = None;
                // Start the next one immediately
                start_next_download(state);
                return;
            }
            Ok(DlEvent::Failed(e)) => {
                state.dl_failed.push(name.clone());
                state.dl_log.push(format!("✗ {} — {}", name, e));
                state.current_dl = None;
                state.dl_rx      = None;
                start_next_download(state);
                return;
            }
            Err(_) => break, // channel empty or closed
        }
    }
    let _ = now;
}

async fn check_ollama(client: &reqwest::Client, base_url: &str) -> bool {
    client.get(format!("{}/api/tags", base_url))
        .timeout(Duration::from_secs(2))
        .send().await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn check_model_ready(client: &reqwest::Client, base_url: &str, model: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct TagsResp { models: Vec<serde_json::Value> }
    let Ok(resp) = client.get(format!("{}/api/tags", base_url))
        .timeout(Duration::from_secs(3)).send().await else { return false; };
    let Ok(tags) = resp.json::<TagsResp>().await else { return false; };
    tags.models.iter().any(|m| {
        m.get("name").and_then(|n| n.as_str())
            .map(|n| n.starts_with(model.split(':').next().unwrap_or(model)))
            .unwrap_or(false)
    })
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render(f: &mut ratatui::Frame, state: &mut SetupState) {
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(8, 8, 18))),
        area,
    );
    match state.step {
        SetupStep::Splash       => render_splash(f, area, state),
        SetupStep::SetupChecks  => render_setup_checks(f, area, state),
        SetupStep::SetupInstall => render_setup_install(f, area, state),
        SetupStep::SetupStart   => render_setup_start(f, area, state),
        SetupStep::SetupModels  => render_model_select(f, area, state),
        SetupStep::Downloading  => render_downloading(f, area, state),
        SetupStep::Done         => {}
    }
}

// ── Boot sequence screen (called from main() during initialisation) ───────────

/// A completed step shown in the boot checklist.
pub struct BootStep {
    pub ok:    bool,    // green ✓ vs yellow ⚠
    pub label: String,  // e.g. "3 models available (llama3.2, mistral)"
}

/// Drawn multiple times from main() as each init step completes.
/// `steps`   = already-finished steps (shown with ✓)
/// `current` = what is running right now (shown with spinner)
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

    // Logo — centred vertically in top portion of screen
    let logo_w = LOGO.iter().map(|l| unicode_width::UnicodeWidthStr::width(*l) as u16).max().unwrap_or(74);
    let top_half_h = inner.height / 2;
    let logo_y = inner.y + top_half_h.saturating_sub(LOGO.len() as u16 + 2) / 2;
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

    // Boot checklist — centred horizontally, below the logo
    let checklist_y = inner.y + top_half_h + 1;
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

    // Current step — spinner animation (static here since it's a one-shot draw,
    // but the caller can redraw with updated state for animation)
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

/// Public probe — used from main() so startup wizard receives the result
/// without a hidden second blank-screen delay.
pub async fn probe_ollama(client: &reqwest::Client) -> bool {
    check_ollama(client, "http://localhost:11434").await
}

// ── Splash ────────────────────────────────────────────────────────────────────

// Slant-style figlet logo
const LOGO: &[&str] = &[
    r"  ___ ___                             .____    .__  __         ",
    r" /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____  ",
    r"/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \ ",
    r"\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/ ",
    r" \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >",
    r"       \/  \/     |__|        \/              \/              \/ ",
];

fn render_splash(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);

    let pulse = if (state.splash_tick / 6) % 2 == 0 { purple } else { teal };

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
        // Alternate purple/teal per line for that angular two-tone look
        let color = if i % 2 == 0 { purple } else { teal };
        let p = Paragraph::new(Line::from(vec![
            Span::styled(*line, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]));
        f.render_widget(p, Rect { x: logo_x, y: logo_y + i as u16, width: logo_w.min(inner.width), height: 1 });
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

// ── Ollama setup ──────────────────────────────────────────────────────────────

// ── Setup: animated pre-flight checklist ─────────────────────────────────────

fn render_setup_checks(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let panel = centered_rect(60, 16, area);
    f.render_widget(Clear, panel);
    let block = Block::default().borders(Borders::ALL)
        .border_style(Style::default().fg(teal))
        .title(Line::from(vec![Span::styled(" Checking System ", Style::default().fg(teal).add_modifier(Modifier::BOLD))]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let pf = match &state.preflight { Some(p) => p, None => return };
    let spinner = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][(state.splash_tick as usize / 2) % 10];

    let chk = |ok: bool, label: &str| -> String {
        if ok { format!("  ✓  {}", label) } else { format!("  ✗  {}", label) }
    };
    let chk_color = |ok: bool| if ok { green } else { red };

    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    let curl_label  = if pf.use_wget { "wget  (curl absent, using wget)" } else { "curl" };
    let curl_ok     = pf.has_curl == Some(true);
    let sudo_ok     = pf.has_sudo == Some(true);
    let ollama_ok   = state.ollama_installed;
    let running_ok  = state.ollama_present;

    lines.push(Line::from(vec![Span::styled(chk(curl_ok,   curl_label),           Style::default().fg(chk_color(curl_ok)))]));
    lines.push(Line::from(vec![Span::styled(chk(sudo_ok,   "sudo"),               Style::default().fg(chk_color(sudo_ok)))]));
    lines.push(Line::from(vec![Span::styled(chk(ollama_ok, "ollama binary"),       Style::default().fg(chk_color(ollama_ok)))]));
    lines.push(Line::from(vec![Span::styled(chk(running_ok,"ollama running"),      Style::default().fg(chk_color(running_ok)))]));
    lines.push(Line::default());
    let pm = if pf.pkg_manager.is_empty() { "none detected" } else { pf.pkg_manager };
    lines.push(Line::from(vec![Span::styled(format!("  {}  package manager: {}", spinner, pm), Style::default().fg(muted))]));

    if let Some(ref err) = state.error_msg {
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(format!("  ✗  {}", err), Style::default().fg(red))]));
    }

    f.render_widget(Paragraph::new(lines), inner);
    let hint_y = panel.y + panel.height - 2;
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled("  scanning…", Style::default().fg(muted))])),
        Rect { x: panel.x + 1, y: hint_y, width: panel.width - 2, height: 1 },
    );
}

// ── Setup: install Ollama ─────────────────────────────────────────────────────

fn render_setup_install(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let border_col = match state.ollama_action { OllamaAction::Idle => orange, _ => teal };
    let panel = centered_rect(66, 22, area);
    f.render_widget(Clear, panel);
    let block = Block::default().borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Line::from(vec![Span::styled(" Install Ollama ", Style::default().fg(orange).add_modifier(Modifier::BOLD))]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    lines.push(Line::from(vec![Span::styled("  Ollama is not installed on this system.", Style::default().fg(white))]));
    lines.push(Line::default());

    // Show what will be installed
    if let Some(ref pf) = state.preflight {
        let cmd = pf.build_install_cmd();
        lines.push(Line::from(vec![Span::styled("  will run:", Style::default().fg(dim))]));
        lines.push(Line::from(vec![Span::styled(format!("  {}", cmd), Style::default().fg(teal))]));
        lines.push(Line::default());
    }

    match &state.ollama_action {
        OllamaAction::EnterPassword => {
            let masked  = "●".repeat(state.password_buf.len());
            let cursor  = if (state.splash_tick / 6) % 2 == 0 { "▌" } else { " " };
            lines.push(Line::from(vec![Span::styled("  sudo password", Style::default().fg(orange).add_modifier(Modifier::BOLD))]));
            lines.push(Line::from(vec![Span::styled("  ┌──────────────────────────────────────────┐", Style::default().fg(teal))]));
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(teal)),
                Span::styled(format!("{:<42}", format!("{}{}", masked, cursor)), Style::default().fg(white)),
                Span::styled(" │", Style::default().fg(teal)),
            ]));
            lines.push(Line::from(vec![Span::styled("  └──────────────────────────────────────────┘", Style::default().fg(teal))]));
            lines.push(Line::from(vec![Span::styled("  Enter to install  ·  Esc to cancel", Style::default().fg(dim))]));
        }
        OllamaAction::Starting | OllamaAction::Installing => {
            let dots = ".".repeat(((state.splash_tick / 5) % 4) as usize);
            lines.push(Line::from(vec![Span::styled(format!("  ◉  Installing{}", dots), Style::default().fg(purple))]));
            for log in state.action_log.iter().take(4) {
                let c = if log.contains('✓') { green } else { muted };
                lines.push(Line::from(vec![Span::styled(log.clone(), Style::default().fg(c))]));
            }
        }
        _ => {
            lines.push(Line::from(vec![Span::styled("  Enter  → install Ollama", Style::default().fg(teal))]));
            lines.push(Line::from(vec![Span::styled("  S / Esc → skip (use another backend)", Style::default().fg(muted))]));
        }
    }

    if let Some(ref err) = state.error_msg {
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(format!("  ✗  {}", err), Style::default().fg(red))]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Setup: start Ollama ───────────────────────────────────────────────────────

fn render_setup_start(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let border_col = match state.ollama_action { OllamaAction::Idle => orange, _ => teal };
    let panel = centered_rect(60, 16, area);
    f.render_widget(Clear, panel);
    let block = Block::default().borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Line::from(vec![Span::styled(" Start Ollama ", Style::default().fg(orange).add_modifier(Modifier::BOLD))]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    lines.push(Line::from(vec![Span::styled("  Ollama is installed but not running.", Style::default().fg(white))]));
    lines.push(Line::default());

    match &state.ollama_action {
        OllamaAction::Starting => {
            let dots = ".".repeat(((state.splash_tick / 5) % 4) as usize);
            lines.push(Line::from(vec![Span::styled(format!("  ◉  Starting ollama serve{}", dots), Style::default().fg(teal))]));
            for log in state.action_log.iter().take(3) {
                let c = if log.contains('✓') { green } else { muted };
                lines.push(Line::from(vec![Span::styled(log.clone(), Style::default().fg(c))]));
            }
        }
        _ => {
            lines.push(Line::from(vec![Span::styled("  Enter / S  → start ollama serve", Style::default().fg(teal))]));
            lines.push(Line::from(vec![Span::styled("  K / Esc    → skip (use another backend)", Style::default().fg(muted))]));
        }
    }

    if let Some(ref err) = state.error_msg {
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(format!("  ✗  {}", err), Style::default().fg(red))]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── (old render_ollama_setup kept as dead code placeholder) ──────────────────
#[allow(dead_code)]
fn render_ollama_setup(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let panel = centered_rect(68, 26, area);
    f.render_widget(Clear, panel);

    let border_color = match state.ollama_action {
        OllamaAction::Idle => orange,
        _                  => teal,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(vec![
            Span::styled(" Ollama Setup ", Style::default().fg(orange).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let mut lines: Vec<Line<'static>> = vec![Line::default()];

    lines.push(Line::from(vec![
        Span::styled("  ⚡  Ollama not detected", Style::default().fg(orange).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("  Ollama manages local model downloads and inference.", Style::default().fg(muted)),
    ]));
    lines.push(Line::default());

    // Options — grey out [1] if ollama not installed
    let (c1, label1) = if state.ollama_installed {
        (teal,   "  [1]  Start Ollama        it's installed, just not running  ")
    } else {
        (dim,    "  [1]  Start Ollama        (not installed — install first)   ")
    };
    lines.push(Line::from(vec![Span::styled(label1, Style::default().fg(c1))]));
    lines.push(Line::from(vec![Span::styled(
        "  [2]  Install Ollama      auto-detects curl/wget/pkg-manager, no shell drop",
        Style::default().fg(purple),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "  [3]  Skip                I use LM Studio / llama.cpp / another backend",
        Style::default().fg(muted),
    )]));
    lines.push(Line::default());

    match &state.ollama_action {
        OllamaAction::EnterPassword => {
            // Show preflight results and password input
            if let Some(ref pf) = state.preflight {
                let check = |ok: Option<bool>, label: &'static str| -> Line<'static> {
                    match ok {
                        Some(true)  => Line::from(vec![
                            Span::styled("  ✓ ", Style::default().fg(green)),
                            Span::styled(label, Style::default().fg(muted)),
                        ]),
                        Some(false) => Line::from(vec![
                            Span::styled("  ✗ ", Style::default().fg(red)),
                            Span::styled(label, Style::default().fg(red)),
                        ]),
                        None => Line::from(vec![Span::styled(format!("  ? {}", label), Style::default().fg(dim))]),
                    }
                };
                let fetcher = if pf.use_wget { "wget (curl not found, using wget)" } else { "curl" };
                lines.push(check(pf.has_curl, fetcher));
                lines.push(check(pf.has_sudo, "sudo"));
                let pm_label = if pf.pkg_manager.is_empty() { "package manager (none found)" } else { pf.pkg_manager };
                lines.push(Line::from(vec![
                    Span::styled("  ● ", Style::default().fg(teal)),
                    Span::styled(pm_label, Style::default().fg(muted)),
                    Span::styled(
                        if pf.has_curl == Some(false) { " — will install curl first" } else { " — ok" },
                        Style::default().fg(dim),
                    ),
                ]));
                lines.push(Line::default());
                let cmd_preview = pf.build_install_cmd();
                lines.push(Line::from(vec![
                    Span::styled("  cmd  ", Style::default().fg(dim)),
                    Span::styled(cmd_preview, Style::default().fg(teal)),
                ]));
                lines.push(Line::default());
            }
            let masked = "●".repeat(state.password_buf.len());
            let cursor  = if (state.splash_tick / 6) % 2 == 0 { "▌" } else { " " };
            lines.push(Line::from(vec![
                Span::styled("  sudo password  ", Style::default().fg(orange).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ┌─────────────────────────────────────┐", Style::default().fg(teal)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(teal)),
                Span::styled(format!("{:<37}", format!("{}{}", masked, cursor)), Style::default().fg(white)),
                Span::styled(" │", Style::default().fg(teal)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  └─────────────────────────────────────┘", Style::default().fg(teal)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Enter to install  ·  Esc to cancel", Style::default().fg(dim)),
            ]));
        }
        OllamaAction::Starting => {
            let dots = ".".repeat(((state.splash_tick / 5) % 4) as usize);
            lines.push(Line::from(vec![Span::styled(
                format!("  ◉  Starting Ollama{}", dots), Style::default().fg(teal))]));
        }
        OllamaAction::Installing => {
            let dots = ".".repeat(((state.splash_tick / 5) % 4) as usize);
            lines.push(Line::from(vec![Span::styled(
                format!("  ◉  Installing{}", dots), Style::default().fg(purple))]));
        }
        _ => {
            lines.push(Line::from(vec![Span::styled(
                "  ●  Press [2] to install", Style::default().fg(dim))]));
        }
    }

    // Action log
    for log_line in state.action_log.iter().take(4) {
        let color = if log_line.contains('✓') { green } else { muted };
        lines.push(Line::from(vec![Span::styled(log_line.clone(), Style::default().fg(color))]));
    }

    // Error
    if let Some(ref err) = state.error_msg {
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(
            format!("  ✗  {}", err),
            Style::default().fg(red),
        )]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Hardware summary (shown inline in SetupChecks now) ───────────────────────
#[allow(dead_code)]
fn render_hardware(f: &mut ratatui::Frame, area: Rect, state: &SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let panel = centered_rect(70, 28, area);
    f.render_widget(Clear, panel);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(purple))
        .title(Line::from(vec![
            Span::styled(" Hardware Detected ", Style::default().fg(purple).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let hw = &state.hardware;
    let mut lines: Vec<Line<'static>> = vec![Line::default()];

    lines.push(Line::from(vec![
        Span::styled("  CPU  ", Style::default().fg(teal).add_modifier(Modifier::BOLD)),
        Span::styled(hw.cpu.name.clone(), Style::default().fg(white)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ", Style::default()),
        Span::styled(
            format!("{} cores  ({} logical)", hw.cpu.physical_cores, hw.cpu.logical_cores),
            Style::default().fg(muted),
        ),
    ]));
    lines.push(Line::default());

    let ram_gb = hw.memory.total_mb / 1024;
    let unified = if hw.memory.is_unified { "  (unified)" } else { "" };
    lines.push(Line::from(vec![
        Span::styled("  RAM  ", Style::default().fg(teal).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{} GB{}", ram_gb, unified), Style::default().fg(white)),
    ]));
    lines.push(Line::default());

    if hw.gpus.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  GPU  ", Style::default().fg(teal).add_modifier(Modifier::BOLD)),
            Span::styled("No dedicated GPU — CPU inference only", Style::default().fg(orange)),
        ]));
    } else {
        for gpu in &hw.gpus {
            let vram_gb = gpu.vram_total_mb as f32 / 1024.0;
            lines.push(Line::from(vec![
                Span::styled("  GPU  ", Style::default().fg(teal).add_modifier(Modifier::BOLD)),
                Span::styled(gpu.name.clone(), Style::default().fg(white)),
                Span::styled(format!("  {:.1} GB VRAM", vram_gb), Style::default().fg(green)),
            ]));
        }
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("  ⚡   ", Style::default().fg(green)),
        Span::styled(hw.recommendation_line(), Style::default().fg(green).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::default());
    let (oc, om) = if state.ollama_present {
        (green, "✓ Ollama running — ready to download models")
    } else {
        (orange, "⚠ Ollama not running — models via other backends only")
    };
    lines.push(Line::from(vec![
        Span::styled("       ", Style::default()),
        Span::styled(om, Style::default().fg(oc)),
    ]));

    f.render_widget(Paragraph::new(lines), inner);

    let hint_y = panel.y + panel.height - 2;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter → pick models    Esc → skip to chat  ", Style::default().fg(muted)),
        ])),
        Rect { x: panel.x + 1, y: hint_y, width: panel.width - 2, height: 1 },
    );
}

// ── Model selection ───────────────────────────────────────────────────────────

fn render_model_select(f: &mut ratatui::Frame, area: Rect, state: &mut SetupState) {
    let purple = ratatui::style::Color::Rgb(189, 147, 249);
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let white  = ratatui::style::Color::Rgb(224, 223, 255);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);
    let dim    = ratatui::style::Color::Rgb(55,  55,  90);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let red    = ratatui::style::Color::Rgb(255,  85,  85);
    let bg     = ratatui::style::Color::Rgb(16,  16,  30);

    let panel_h = (state.models.len() as u16 + 12).min(area.height.saturating_sub(4));
    let panel = centered_rect(76, panel_h, area);
    f.render_widget(Clear, panel);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(teal))
        .title(Line::from(vec![
            Span::styled(" Model Setup ", Style::default().fg(teal).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1), Constraint::Length(3)])
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
                "  Space → toggle  ·  ↑↓/jk → navigate  ·  Enter → download / continue",
                Style::default().fg(dim),
            )]),
        ]),
        chunks[0],
    );

    if state.models.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  No models fit your hardware. Try: ollama pull llama3.2:3b",
                Style::default().fg(orange),
            )])),
            chunks[1],
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
            chunks[1], &mut ls,
        );
    }

    let sel_count = state.selected.iter().filter(|&&s| s).count();
    let (action_col, action_text) = if let Some(ref err) = state.error_msg {
        (red, format!("  ✗  {}", err))
    } else if sel_count == 0 {
        (muted, "  No models selected  ·  Enter to skip (if you already have models)".to_string())
    } else {
        (green, format!("  {} model(s) selected  ·  Enter to download", sel_count))
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(action_text, Style::default().fg(action_col).add_modifier(Modifier::BOLD)),
        ])).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(dim))),
        chunks[2],
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
            Constraint::Length(1), // model name + status
            Constraint::Length(1), // blank
            Constraint::Length(1), // progress bar (manual)
            Constraint::Length(1), // bytes + speed + eta
            Constraint::Length(1), // blank
            Constraint::Min(1),    // log
            Constraint::Length(1), // hint
        ])
        .split(inner);

    if let Some(ref name) = state.current_dl {
        let pct      = (state.dl_progress * 100.0).min(100.0) as u64;
        let spinner  = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][(state.splash_tick as usize / 2) % 10];

        // Row 0: model name + status
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(teal)),
            Span::styled(name.clone(), Style::default().fg(white).add_modifier(Modifier::BOLD)),
            Span::styled("  —  ", Style::default().fg(dim)),
            Span::styled(state.dl_status_msg.clone(), Style::default().fg(muted)),
        ])), chunks[0]);

        // Row 2: manual progress bar
        let bar_w  = chunks[2].width.saturating_sub(10) as usize;
        let filled = (bar_w as f64 * state.dl_progress).round() as usize;
        let empty  = bar_w.saturating_sub(filled);
        let bar    = format!("  [{}{}] {:>3}%",
            "█".repeat(filled), "░".repeat(empty), pct);
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(teal)),
        ])), chunks[2]);

        // Row 3: bytes done / total  ·  speed  ·  ETA
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
                fmt_bytes(state.dl_bytes_done),
                fmt_bytes(state.dl_bytes_total),
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

    // Queue overview
    if !state.download_queue.is_empty() {
        let queued: String = state.download_queue.iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>().join("  ·  ");
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("  up next  ", Style::default().fg(dim)),
            Span::styled(queued, Style::default().fg(muted)),
        ])), chunks[4]);
    }

    // Log (last few lines, newest at bottom)
    let log_start = state.dl_log.len().saturating_sub(chunks[5].height as usize);
    let log_lines: Vec<Line<'static>> = state.dl_log[log_start..].iter().map(|l| {
        let color = if l.contains('✓') { green }
            else if l.contains('✗')    { red   }
            else if l.contains("…")    { teal  }
            else                       { dim   };
        Line::from(vec![Span::styled(format!("  {}", l), Style::default().fg(color))])
    }).collect();
    f.render_widget(Paragraph::new(log_lines), chunks[5]);

    // Hint
    let hint = if !state.dl_done.is_empty() || !state.dl_failed.is_empty() {
        "  Esc / s → skip remaining and launch"
    } else {
        "  downloading — please wait…"
    };
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled(hint, Style::default().fg(dim)),
    ])), chunks[6]);
    let _ = orange;
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
