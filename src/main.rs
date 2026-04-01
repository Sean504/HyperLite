//! HyperLite — terminal-native local LLM chat client.
#![allow(unused_imports, dead_code, unused_variables, unused_mut, clippy::all)]

mod app;
mod config;
mod db;
mod event;
mod hardware;
mod keybinds;
mod models;
mod project;
mod providers;
mod session;
mod startup;
mod tools;
mod ui;

use std::io;
use std::sync::Arc;
use anyhow::Context;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tui_textarea::TextArea;

use crate::app::{ActiveDialog, ActivePrompt, App};
use crate::hardware::HardwareInfo;
use crate::keybinds::Keybinds;
use crate::providers::ProviderRegistry;
use crate::ui::components::spinner::Spinner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("hyperlite {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }
    run_app().await
}

async fn run_app() -> anyhow::Result<()> {
    // ── Config ────────────────────────────────────────────────────────────────
    let config = config::load(None).unwrap_or_default();

    // Check if a path was given as a CLI arg and chdir to it
    for arg in std::env::args().skip(1) {
        if !arg.starts_with('-') {
            let p = std::path::Path::new(&arg);
            if p.is_dir() {
                let _ = std::env::set_current_dir(p);
                break;
            }
        }
    }

    // ── Database ──────────────────────────────────────────────────────────────
    let data_dir = config::data_dir();
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("hyperlite.db");
    let db = db::open(&db_path).context("Failed to open database")?;

    // ── Sessions ──────────────────────────────────────────────────────────────
    let mut sessions = db::list_sessions(&db)?;
    let cwd_str = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // ── Hardware ──────────────────────────────────────────────────────────────
    let hardware = hardware::detect();

    // ── HTTP client (shared) ──────────────────────────────────────────────────
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // ── Provider registry ─────────────────────────────────────────────────────
    let registry = Arc::new(ProviderRegistry::with_defaults(http_client.clone()));

    // ── TUI setup — start immediately so we never show a blank terminal ───────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let _ = crossterm::execute!(io::stdout(), crossterm::terminal::SetTitle("HyperLite"));

    // ── Animated boot checklist — user sees exactly what is happening ─────────
    let mut boot_steps: Vec<startup::BootStep> = Vec::new();

    // Step 1: hardware (already detected above, show it)
    let hw_label = {
        let gpu_info = hardware.gpus.first()
            .map(|g| format!("  {}  {:.0} GB VRAM", g.name, g.vram_total_mb as f64 / 1024.0))
            .unwrap_or_else(|| "  CPU-only".to_string());
        format!("Hardware  —  {:.1} GB RAM{}", hardware.memory.total_mb as f64 / 1024.0, gpu_info)
    };
    boot_steps.push(startup::BootStep { ok: true, label: hw_label });
    terminal.draw(|f| startup::render_booting(f, &boot_steps, "Scanning for models…"))?;

    // Step 2: probe available models
    let pre_models = registry.all_models().await;
    let model_label = if pre_models.is_empty() {
        "No models found yet".to_string()
    } else {
        let names: Vec<_> = pre_models.iter().take(3).map(|m| m.name.as_str()).collect();
        let extra = if pre_models.len() > 3 { format!(" +{} more", pre_models.len() - 3) } else { String::new() };
        format!("{} model(s)  —  {}{}", pre_models.len(), names.join(", "), extra)
    };
    boot_steps.push(startup::BootStep { ok: true, label: model_label });
    terminal.draw(|f| startup::render_booting(f, &boot_steps, "Checking local model server…"))?;

    // Step 3: check local model server (Ollama)
    let ollama_present = startup::probe_ollama(&http_client).await;
    let ollama_label = if ollama_present {
        "Local AI server  —  running".to_string()
    } else {
        "Local AI server  —  not running".to_string()
    };
    boot_steps.push(startup::BootStep { ok: ollama_present, label: ollama_label });
    terminal.draw(|f| startup::render_booting(f, &boot_steps, ""))?;

    // ── Startup screen + optional setup wizard ────────────────────────────────
    // Startup runs BEFORE the App is built so that any newly pulled Ollama
    // models are included when we construct available_models below.
    startup::run_startup(
        &mut terminal,
        hardware.clone(),
        &pre_models,
        ollama_present,
        http_client.clone(),
    ).await?;

    // ── Re-query models (startup may have pulled new ones) ────────────────────
    // Draw the boot screen again so there's no blank flash between startup and chat
    terminal.draw(|f| startup::render_booting(f, &boot_steps, "Loading…"))?;
    let available_models = registry.all_models().await;

    // ── Pick current model ─────────────────────────────────────────────────────
    // If there's exactly one model always use it. If config has a remembered
    // model and it still exists use that. Otherwise fall back to first available.
    let current_model = if available_models.len() == 1 {
        available_models[0].id.clone()
    } else if !config.model.is_empty() && available_models.iter().any(|m| m.id == config.model) {
        config.model.clone()
    } else {
        available_models.first().map(|m| m.id.clone()).unwrap_or_default()
    };

    // ── Sessions (cont.) ──────────────────────────────────────────────────────
    // Sort newest-first so [0] is always the most recently used session.
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let (session_id, messages) = if sessions.is_empty() {
        let s = session::message::Session::new(&current_model, "local", &cwd_str);
        db::insert_session(&db, &s)?;
        let id = s.id.clone();
        sessions.push(s);
        (id, vec![])
    } else {
        let id = sessions[0].id.clone();
        let msgs = db::load_messages(&db, &id)?;
        (id, msgs)
    };

    // ── Project context ───────────────────────────────────────────────────────
    let project_ctx = std::env::current_dir().ok().map(|d| project::scan(&d));
    let project_context_active = project_ctx.as_ref().map(|c| c.is_git).unwrap_or(false);

    // ── Theme ─────────────────────────────────────────────────────────────────
    let theme = ui::theme::get(&config.theme);

    // ── Event channel ─────────────────────────────────────────────────────────
    let (event_tx, event_rx) = mpsc::unbounded_channel::<event::Event>();

    // ── Build App ─────────────────────────────────────────────────────────────
    let custom_agents = crate::db::list_agents(&db).unwrap_or_default();
    let drafts        = crate::db::list_drafts(&db).unwrap_or_default();

    let app = App {
        config,
        db,
        keybinds:  Keybinds::default_binds(),
        theme,

        session_id,
        sessions,
        messages,

        textarea:        TextArea::default(),
        input_history:   vec![],
        history_idx:     None,
        placeholder_idx:   0,
        cursor_blink_on:   true,
        cursor_blink_tick: 0,

        streaming:        false,
        streaming_buf:    String::new(),
        spinner:          Spinner::new(),
        last_token_count: None,

        provider_registry: registry,
        available_models,
        current_model,
        model_picker_tab:    0,
        command_palette_tab: 0,

        hardware,

        project_context_active,
        project_ctx,

        working_dir:       std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        folder_input_buf:       String::new(),
        folder_browser_path:    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        folder_browser_entries: vec![],
        pending_tool_calls: Vec::new(),

        scroll_offset:       0,
        scroll_stick_bottom: true,
        show_scrollbar:      true,

        active_dialog:     ActiveDialog::None,
        active_prompt:     ActivePrompt::Input,
        show_tool_details: false,
        show_thinking:     true,
        sidebar_open:      true,
        concealed:         false,

        dialog_search_query: String::new(),
        dialog_selected_idx: 0,

        pending_permission: None,
        toast:              None,

        http_client,
        event_tx,

        tool_iterations: 0,
        tool_enforcer_pending: false,
        active_plan:     Vec::new(),
        plan_step:       0,

        current_agent:   "general".to_string(),
        custom_agents,
        undo_stack:      Vec::new(),
        drafts,
        agent_editor_name:   String::new(),
        agent_editor_desc:   String::new(),
        agent_editor_system: tui_textarea::TextArea::default(),
        agent_editor_field:  0,
        agent_editor_id:     None,

        model_dl_active:      None,
        model_dl_bytes_done:  0,
        model_dl_bytes_total: 0,
        model_dl_speed_bps:   0.0,
        model_refresh_pending: false,
    };

    // ── Run ───────────────────────────────────────────────────────────────────
    let result = app::run(&mut terminal, app, event_rx).await;

    // ── Teardown ──────────────────────────────────────────────────────────────
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn print_help() {
    println!(
        "hyperlite {}\n\nTerminal-native local LLM chat client.\n\nUSAGE:\n    hyperlite [OPTIONS]\n\nOPTIONS:\n    -h, --help       Print this help\n    -V, --version    Print version\n\nKEYBINDINGS:\n    Enter            Send message\n    Alt+Enter        Insert newline\n    Ctrl+K           Command palette\n    Ctrl+M           Model picker\n    Ctrl+S           Session list\n    Ctrl+N           New session\n    Ctrl+\\           Toggle sidebar\n    ?                Help dialog\n    Ctrl+Q           Quit\n\nCONFIG:\n    ~/.config/hyperlite/settings.toml\n",
        env!("CARGO_PKG_VERSION")
    );
}
