/// Central application state and event dispatch loop.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use crossterm::event::{self as ct_event, Event as CtEvent, KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tui_textarea::TextArea;

use crate::config::Config;
use crate::db::Db;
use crate::event::Event;
use crate::hardware::HardwareInfo;
use crate::keybinds::{Action, Keybinds};
use crate::providers::{ChatMessage, GenerationParams, LocalModel, ProviderRegistry, StreamEvent};
use crate::session::message::{Message, Part, PermissionRequest, Role, Session, TextPart};
use crate::ui::components::spinner::Spinner;
use crate::ui::theme::Theme;

/// One file the agent proposes to change, as shown in the review screen.
#[derive(Debug, Clone)]
pub struct ChangeEntry {
    pub call:       crate::tools::ToolCall,   // re-executed on apply (fresh disk read)
    pub file_path:  String,
    pub tool_name:  String,                   // "write_file" | "edit_file"
    pub is_new:     bool,                      // file did not exist before
    pub diff_lines: Vec<crate::session::message::DiffLine>,
    pub added:      usize,
    pub removed:    usize,
    pub status:     ChangeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeStatus { Pending, Applied, Skipped }

/// A batch of proposed file changes awaiting review.
#[derive(Debug, Clone)]
pub struct Changeset {
    pub entries:         Vec<ChangeEntry>,
    pub remaining_calls: Vec<crate::tools::ToolCall>, // non-edit calls to run after review
    pub results:         Vec<String>,                 // <tool_result> blocks accumulated on resolve
}

impl Changeset {
    pub fn total_added(&self)   -> usize { self.entries.iter().map(|e| e.added).sum() }
    pub fn total_removed(&self) -> usize { self.entries.iter().map(|e| e.removed).sum() }
    pub fn pending_count(&self) -> usize { self.entries.iter().filter(|e| e.status == ChangeStatus::Pending).count() }
    pub fn first_pending(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.status == ChangeStatus::Pending)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReviewView { Overview, File }

/// Session-wide record of an applied/skipped change, shown in the sidebar cockpit.
#[derive(Debug, Clone)]
pub struct ChangeLogEntry {
    pub file_path: String,
    pub added:     usize,
    pub removed:   usize,
    pub status:    ChangeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveDialog {
    None,
    SessionList,
    ModelPicker,
    Help,
    CommandPalette,
    ThemePicker,
    FolderInput,
    AgentPicker,
    AgentEditor,
    DraftPicker,
    GitConfirm,    // shown on folder open when git repo detected — opt into git agent
    IndexConfirm,  // shown after git confirm — ask if user wants to RAG index
    RagSearch,     // text input for searching an index manually
    BwrapInstall,  // bubblewrap install progress dialog
    MemoryInput,   // text input for saving a new memory fact
    GitToken,      // PAT setup guide + masked token input triggered by auth failures
    PenTestAuth,        // full-screen authorization gate (typewriter animation + AUTHORIZED input)
    PenTestPreflight,   // tool inventory check + workflow availability
    PenTestToolSelector, // interactive selection of missing tools to install
    PenTestInstall,     // streaming install progress + sudo prompt
    PenTestSetup,       // engagement setup form (target, depth, exclusions)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivePrompt {
    Input,
    Permission,
    Rename,   // textarea holds new session title; submit applies it
}

pub struct App {
    // Core
    pub config:         Config,
    pub db:             Db,
    pub keybinds:       Keybinds,
    pub theme:          &'static Theme,

    // Session
    pub session_id:     String,
    pub sessions:       Vec<Session>,
    pub messages:       Vec<Message>,

    // Input
    pub textarea:       TextArea<'static>,
    pub input_history:  Vec<String>,
    pub history_idx:    Option<usize>,
    pub placeholder_idx:   usize,
    pub cursor_blink_on:   bool,
    pub cursor_blink_tick: u8,

    // Streaming
    pub streaming:      bool,
    pub streaming_buf:  String,
    pub stream_status:  String,   // phase label shown in input border ("Starting server…", etc.)
    pub spinner:        Spinner,
    pub last_token_count: Option<u32>,

    // Providers / models
    pub provider_registry: Arc<ProviderRegistry>,
    pub available_models:  Vec<LocalModel>,
    pub current_model:     String,
    pub model_picker_tab:      usize,
    pub command_palette_tab:   usize,
    pub help_tab:              usize,

    // Hardware
    pub hardware:       HardwareInfo,
    // Live system stats for the sidebar gauges (refreshed ~1 Hz in tick())
    pub live_sys:         sysinfo::System,
    pub live_cpu_pct:     f32,
    pub live_ram_used_mb: u64,
    pub live_stat_tick:   u8,
    // Frame counter for sprite/mascot animation (80ms per tick)
    pub anim_tick:        usize,

    // Project context
    pub project_context_active: bool,
    pub project_ctx:    Option<crate::project::ProjectContext>,

    // Working directory (root for all file tool operations)
    pub working_dir:          std::path::PathBuf,
    // Folder browser dialog
    pub folder_input_buf:      String,
    pub folder_browser_path:   std::path::PathBuf,
    pub folder_browser_entries: Vec<String>,
    // Pending tool calls parsed from model output
    pub pending_tool_calls:   Vec<crate::tools::ToolCall>,

    // Scroll
    pub scroll_offset:       usize,
    pub scroll_stick_bottom: bool,
    pub show_scrollbar:      bool,

    // UI state
    pub active_dialog:       ActiveDialog,
    pub active_prompt:       ActivePrompt,
    pub show_tool_details:   bool,
    pub show_thinking:       bool,
    pub sidebar_open:        bool,
    pub concealed:           bool,

    // Dialogs shared state
    pub dialog_search_query:  String,
    pub dialog_selected_idx:  usize,

    // Permission
    pub pending_permission:  Option<PermissionRequest>,

    // Diff approval — pending write waiting for user to approve/discard
    // Full-screen change reviewer
    pub changeset:     Option<Changeset>,
    pub review_open:   bool,          // reviewer owns the screen
    pub review_view:   ReviewView,
    pub review_cursor: usize,         // selected file in overview
    pub review_scroll: usize,         // scroll offset in file view
    pub changes_log:   Vec<ChangeLogEntry>, // session-wide applied/skipped history

    // Sandbox mode — shell commands run inside bwrap isolation
    pub sandbox_enabled:   bool,
    pub bwrap_install_log: Vec<String>,
    pub bwrap_installing:  bool,
    pub bwrap_sudo_prompt: bool,
    pub bwrap_sudo_input:  String,

    // Git token setup — triggered when push/pull returns [AUTH_REQUIRED:host]
    pub git_token_input: String,
    pub git_token_host:  String,

    // Pen test mode — active engagement UI
    pub pentest_mode:        bool,
    pub pentest_engagement:  Option<crate::pentest::EngagementSpec>,
    pub pentest_phase:       crate::pentest::EngagementPhase,
    pub pentest_hosts:       Vec<crate::pentest::PentestHost>,
    pub pentest_selected_host: usize,
    pub pentest_scan_progress: crate::pentest::ScanProgress,
    pub pentest_raw_output:  Vec<String>,
    pub pentest_raw_tab:     bool,   // false = structured view, true = raw output
    pub pentest_evidence:    Vec<String>,

    // Pen test mode — setup form
    pub pentest_setup_target:     String,
    pub pentest_setup_exclusions: String,
    pub pentest_setup_field:      usize,
    pub pentest_setup_depth:      crate::pentest::Depth,

    // Pen test mode — auth gate
    pub pentest_auth_phase:  crate::pentest::AuthPhase,
    pub pentest_auth_tick:   u8,
    pub pentest_auth_input:  String,
    pub pentest_auth_flash:  u8,

    // Pen test mode — environment + inventory
    pub pentest_env:          Option<crate::pentest::EnvironmentReport>,
    pub pentest_inventory:    crate::pentest::ToolInventory,
    pub pentest_inv_complete: bool,

    // Pen test mode — tool selector
    pub pentest_selector_items:  Vec<(String, bool)>,  // (tool_name, selected)
    pub pentest_selector_cursor: usize,
    pub pentest_install_sudo_prompt: bool,
    pub pentest_install_sudo_input:  String,
    // Golang dependency confirmation (shown when Go-dependent tools selected + Go missing)
    pub pentest_golang_confirm:  bool,
    pub pentest_golang_tools:    Vec<String>,  // tools that need Go

    // Pen test mode — install progress
    pub pentest_install_log:  Vec<String>,
    pub pentest_installing:   bool,

    // Toast
    pub toast: Option<crate::ui::components::toast::Toast>,

    // Reqwest client (shared for streaming)
    pub http_client: reqwest::Client,

    // Event sender for streaming tasks → main loop
    pub event_tx: mpsc::UnboundedSender<Event>,

    // Agentic tool loop safety counter (reset on each user message)
    pub tool_iterations: u8,
    // Set when model described an action but called no tools — triggers enforcer re-prompt
    pub tool_enforcer_pending: bool,
    // Active multi-step plan declared by make_plan tool
    pub active_plan:  Vec<String>,
    pub plan_step:    usize,
    // Rolling history of recent tool calls for sidebar display: (name, is_error)
    pub tool_history: Vec<(String, bool)>,

    // Agent system
    pub current_agent:   String,             // "general" | "build" | "plan" | custom id
    pub custom_agents:   Vec<crate::db::AgentRow>,

    // Redo stack — each entry is a Vec of messages removed by one undo
    pub undo_stack:      Vec<Vec<crate::session::message::Message>>,

    // Draft stash
    pub drafts:          Vec<crate::db::DraftRow>,

    // Agent editor form fields
    pub agent_editor_name:   String,
    pub agent_editor_desc:   String,
    pub agent_editor_system: tui_textarea::TextArea<'static>,
    pub agent_editor_field:  usize,  // 0=name, 1=desc, 2=system
    pub agent_editor_id:     Option<String>,  // Some(id) = editing, None = new

    // In-dialog model download state
    pub model_dl_active:     Option<String>,  // model name currently downloading
    pub model_dl_bytes_done: u64,
    pub model_dl_bytes_total: u64,
    pub model_dl_speed_bps:  f64,
    pub model_refresh_pending: bool,          // re-query models after download
}

impl App {
    pub fn is_streaming(&self) -> bool { self.streaming }

    pub fn current_model_name(&self) -> String {
        self.available_models.iter()
            .find(|m| m.id == self.current_model)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| self.current_model.clone())
    }

    pub fn current_backend_name(&self) -> String {
        self.available_models.iter()
            .find(|m| m.id == self.current_model)
            .map(|m| m.backend.display_name().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn project_name(&self) -> String {
        self.project_ctx.as_ref()
            .and_then(|ctx| ctx.root.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string())
    }

    pub fn textarea_is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    pub fn scroll_by(&mut self, delta: i64) {
        self.scroll_stick_bottom = false;
        if delta < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_add(delta as usize);
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_stick_bottom = true;
        self.scroll_offset = usize::MAX;
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_stick_bottom = false;
        self.scroll_offset = 0;
    }

    pub fn push_toast(&mut self, toast: crate::ui::components::toast::Toast) {
        self.toast = Some(toast);
    }

    pub fn tick(&mut self) {
        self.spinner.tick();
        if let Some(t) = &mut self.toast {
            if !t.tick() { self.toast = None; }
        }
        // Blink cursor every ~400ms (80ms tick × 5)
        self.cursor_blink_tick = self.cursor_blink_tick.wrapping_add(1);
        if self.cursor_blink_tick % 5 == 0 {
            self.cursor_blink_on = !self.cursor_blink_on;
        }

        self.anim_tick = self.anim_tick.wrapping_add(1);

        // Live CPU/RAM stats for the sidebar gauges — every ~1s (80ms × 12)
        self.live_stat_tick = self.live_stat_tick.wrapping_add(1);
        if self.live_stat_tick % 12 == 0 {
            self.live_sys.refresh_memory();
            self.live_sys.refresh_cpu_usage();
            self.live_ram_used_mb = self.live_sys.used_memory() / 1024 / 1024;
            self.live_cpu_pct     = self.live_sys.global_cpu_usage();
        }

        // Auth gate typewriter animation
        if self.active_dialog == ActiveDialog::PenTestAuth {
            use crate::pentest::{AuthPhase, AUTH_CONTENT_LINES, AUTH_LINES_PER_TICK};
            match &self.pentest_auth_phase {
                AuthPhase::BlackOut => {
                    self.pentest_auth_tick += 1;
                    if self.pentest_auth_tick >= 2 {
                        self.pentest_auth_phase = AuthPhase::Revealing;
                        self.pentest_auth_tick = 0;
                    }
                }
                AuthPhase::Revealing => {
                    self.pentest_auth_tick += 1;
                    let revealed = (self.pentest_auth_tick as u16 * AUTH_LINES_PER_TICK as u16)
                        .min(AUTH_CONTENT_LINES as u16) as u8;
                    if revealed >= AUTH_CONTENT_LINES {
                        self.pentest_auth_phase = AuthPhase::AwaitInput;
                    }
                }
                AuthPhase::FlashWrong | AuthPhase::FlashCorrect => {
                    self.pentest_auth_flash = self.pentest_auth_flash.saturating_sub(1);
                    if self.pentest_auth_flash == 0 {
                        if self.pentest_auth_phase == AuthPhase::FlashCorrect {
                            // Transition to pre-flight
                            self.pentest_auth_phase = AuthPhase::AwaitInput;
                            self.pentest_auth_input.clear();
                            open_dialog(self, ActiveDialog::PenTestPreflight);
                            launch_pentest_inventory(self);
                        } else {
                            self.pentest_auth_phase = AuthPhase::AwaitInput;
                            self.pentest_auth_input.clear();
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// ── Main run loop ─────────────────────────────────────────────────────────────

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    mut app: App,
    mut event_rx: mpsc::UnboundedReceiver<Event>,
) -> anyhow::Result<()> {
    use crate::ui;

    let tick_rate = Duration::from_millis(80);

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Drain internal events (streaming chunks, etc.) — non-blocking
        while let Ok(ev) = event_rx.try_recv() {
            if handle_internal_event(&mut app, ev) {
                return Ok(());
            }
        }

        // Execute pending tool calls from the model's last response
        if !app.pending_tool_calls.is_empty() && !app.streaming {
            execute_pending_tools(&mut app).await?;
        }

        // Tool enforcer: model described but didn't act — re-prompt it
        if app.tool_enforcer_pending && !app.streaming {
            app.tool_enforcer_pending = false;
            fire_tool_enforcer(&mut app).await?;
        }

        // Refresh model list after a download completes
        if app.model_refresh_pending {
            app.model_refresh_pending = false;
            app.available_models = app.provider_registry.all_models().await;
        }

        // Poll for terminal events
        if ct_event::poll(tick_rate)? {
            match ct_event::read()? {
                CtEvent::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                    if handle_key(&mut app, key).await? {
                        break;
                    }
                }
                CtEvent::Resize(_, _) => {}
                _ => {}
            }
        }

        app.tick();
    }

    Ok(())
}

/// Handle an internal event from the mpsc channel (streaming/tool results).
/// Returns true if the app should quit.
fn handle_internal_event(app: &mut App, ev: Event) -> bool {
    match ev {
        Event::StreamStatus(s) => {
            app.stream_status = s;
        }
        Event::StreamText(text) => {
            app.stream_status.clear();
            // Filter chat-template tokens that models sometimes leak
            let filtered = text
                .replace("<|im_end|>", "")        // Qwen / Mistral
                .replace("<|endoftext|>", "")      // GPT-2 style
                .replace("<|eot_id|>", "")         // Llama-3
                .replace("<|end_of_text|>", "")    // Llama-3
                .replace("<|start_header_id|>", "") // Llama-3
                .replace("<|end_header_id|>", ""); // Llama-3
            app.streaming_buf.push_str(&filtered);

            // Stop early if model generates the next user turn
            if let Some(pos) = app.streaming_buf.find("<|im_start|>") {
                app.streaming_buf.truncate(pos);
                let _ = app.event_tx.send(Event::StreamDone { duration_ms: 0 });
            }
            // Stop as soon as we have a complete tool call so the model can't
            // hallucinate the tool results — we'll execute and re-submit.
            else if app.streaming_buf.contains("</tool_call>") {
                if let Some(tc_end) = app.streaming_buf.rfind("</tool_call>") {
                    let after = tc_end + "</tool_call>".len();
                    // Also skip the closing ``` fence if it directly follows
                    let fence_end = {
                        let tail = &app.streaming_buf[after..];
                        let ws_len = tail.len() - tail.trim_start().len();
                        let trimmed = &tail[ws_len..];
                        if trimmed.starts_with("```") {
                            after + ws_len + 3
                        } else {
                            after
                        }
                    };
                    app.streaming_buf.truncate(fence_end);
                }
                let _ = app.event_tx.send(Event::StreamDone { duration_ms: 0 });
            }
            // Pattern 3: model emits a JSON code fence with {"name":..., "arguments":...}
            // Detect a complete fence: ``` + newline + {...} + newline + ```
            else if is_complete_json_tool_fence(&app.streaming_buf) {
                // Truncate at end of the closing fence
                if let Some(end) = find_json_fence_end(&app.streaming_buf) {
                    app.streaming_buf.truncate(end);
                }
                let _ = app.event_tx.send(Event::StreamDone { duration_ms: 0 });
            }
        }
        Event::StreamReasoning(_r) => {
            // Could render reasoning separately — ignore for now
        }
        Event::StreamDone { duration_ms } => {
            app.streaming = false;
            app.stream_status.clear();
            let raw = std::mem::take(&mut app.streaming_buf);
            if !raw.is_empty() {
                // Normalize: unwrap any code-fence wrappers around <tool_call> blocks
                let normalized = crate::tools::unwrap_fenced_tool_calls(&raw);
                // Strip any incomplete <tool_call> open tags (model stopped mid-generation)
                let normalized = strip_incomplete_tool_tags(&normalized);
                // Parse tool calls — we keep the XML in the saved text so the LLM
                // has full context of what it called when we re-submit history.
                // The renderer hides <tool_call> blocks from display.
                let (_, tool_calls) = crate::tools::parse_tool_calls(&normalized);
                let msg = Message {
                    id:          ulid::Ulid::new().to_string(),
                    session_id:  app.session_id.clone(),
                    role:        Role::Assistant,
                    parts:       vec![Part::Text(TextPart::new(&normalized))],
                    model:       Some(app.current_model.clone()),
                    duration_ms: Some(duration_ms),
                    created_at:  chrono::Utc::now().timestamp(),
                    hidden:      false,
                };
                let _ = crate::db::insert_message(&app.db, &msg);
                app.messages.push(msg);
                if !tool_calls.is_empty() {
                    app.pending_tool_calls = tool_calls;
                } else {
                    // Tool enforcer: fire when model stopped without calling tools but should continue.
                    //
                    // Case A: active plan has remaining steps — must continue regardless.
                    let plan_has_more = !app.active_plan.is_empty()
                        && app.plan_step < app.active_plan.len();

                    // Case B: model described an action — or dumped a code block —
                    // without ever calling a tool.
                    let has_code_block = normalized.contains("```");
                    let has_passive = ["I'll ", "I will ", "I can ", "Let me ", "I would ",
                                       "Here's ", "Here is ", "Sure,", "Sure!"]
                        .iter().any(|p| normalized.contains(p));

                    // Always evaluate against the REAL user request: skip our own
                    // hidden enforcer nudges and tool-result messages. Otherwise the
                    // injected nudge becomes "last user text" and has_action collapses,
                    // so the enforcer only fires once and gives up.
                    let last_user_text = app.messages.iter().rev()
                        .filter(|m| m.role == Role::User && !m.hidden)
                        .find(|m| !m.text_content().starts_with("<tool_result>"))
                        .map(|m| m.text_content())
                        .unwrap_or_default();
                    let has_action = ["write", "create", "make", "edit", "fix", "build", "save",
                                      "add", "implement", "delete", "remove", "update",
                                      "generate", "script", "program", "file"]
                        .iter().any(|k| last_user_text.to_lowercase().contains(k));

                    // Cap enforced retries so a stubborn model can't flash endlessly.
                    let under_retry_cap = app.tool_iterations < 3;

                    if plan_has_more || (under_retry_cap && (has_passive || has_code_block) && has_action) {
                        app.tool_enforcer_pending = true;
                    }
                }
            }
            app.scroll_to_bottom();
        }
        Event::StreamError(err) => {
            app.streaming = false;
            app.streaming_buf.clear();
            app.stream_status.clear();
            app.push_toast(crate::ui::components::toast::Toast::error(err));
        }
        Event::PermissionRequest(req) => {
            app.pending_permission = Some(req);
            app.active_prompt = ActivePrompt::Permission;
        }
        Event::CompactDone { summary, session_id } => {
            app.streaming = false;
            if app.session_id == session_id {
                // Delete all existing messages and replace with summary pseudo-message
                let _ = crate::db::delete_all_messages(&app.db, &session_id);
                app.messages.clear();
                let msg = Message {
                    id:          ulid::Ulid::new().to_string(),
                    session_id:  session_id.clone(),
                    role:        Role::Assistant,
                    parts:       vec![Part::Text(TextPart::new(&format!(
                        "**[Compacted session summary]**\n\n{}", summary
                    )))],
                    model:       Some(app.current_model.clone()),
                    duration_ms: None,
                    created_at:  chrono::Utc::now().timestamp(),
                    hidden:      false,
                };
                let _ = crate::db::insert_message(&app.db, &msg);
                app.messages.push(msg);
                app.scroll_to_bottom();
                app.push_toast(crate::ui::components::toast::Toast::success("Session compacted"));
            }
        }
        Event::ModelDownloadProgress { model: _, bytes_done, bytes_total } => {
            let prev = app.model_dl_bytes_done;
            app.model_dl_bytes_done  = bytes_done;
            app.model_dl_bytes_total = bytes_total;
            let instant = (bytes_done.saturating_sub(prev)) as f64 / 0.08;
            app.model_dl_speed_bps = app.model_dl_speed_bps * 0.85 + instant * 0.15;
        }
        Event::ModelDownloadDone { model, filename } => {
            app.model_dl_active = None;
            app.model_dl_bytes_done  = 0;
            app.model_dl_bytes_total = 0;
            app.model_dl_speed_bps   = 0.0;
            app.model_refresh_pending = true;
            app.push_toast(crate::ui::components::toast::Toast::success(
                format!("Downloaded: {}", model)
            ));
            // Register with Ollama if it's running — same logic as first-run setup.
            // If Ollama isn't available this is a no-op and DirectGgufProvider handles it.
            if which::which("ollama").is_ok() {
                let model_path = crate::startup::models_dir().join(&filename);
                let model_name = filename
                    .trim_end_matches(".gguf")
                    .replace(['.', ' '], "-")
                    .to_lowercase();
                let path_str = model_path.to_string_lossy().to_string();
                tokio::spawn(async move {
                    let modelfile = format!("FROM {}\nPARAMETER num_ctx 16384", path_str);
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
        Event::ModelDownloadFailed { model: _, error } => {
            app.model_dl_active = None;
            app.push_toast(crate::ui::components::toast::Toast::error(
                format!("Download failed: {}", error)
            ));
        }
        Event::ToastMsg { text, is_error } => {
            if is_error {
                app.push_toast(crate::ui::components::toast::Toast::error(text));
            } else {
                app.push_toast(crate::ui::components::toast::Toast::success(text));
            }
        }
        Event::PenTestHostDiscovered(ip) => {
            // Ping sweep confirmed host live — add placeholder entry
            if !app.pentest_hosts.iter().any(|h| h.ip == ip) {
                app.pentest_hosts.push(crate::pentest::PentestHost {
                    ip,
                    hostname: None,
                    ports: vec![],
                    os_guess: None,
                });
            }
        }
        Event::PenTestHostComplete(host) => {
            // Service scan completed for a host — update or insert
            if let Some(existing) = app.pentest_hosts.iter_mut().find(|h| h.ip == host.ip) {
                *existing = *host;
            } else {
                app.pentest_hosts.push(*host);
            }
        }
        Event::PenTestScanProgress { percent, found, speed, eta, command } => {
            app.pentest_scan_progress.percent     = percent;
            app.pentest_scan_progress.hosts_found = found;
            app.pentest_scan_progress.speed       = speed;
            app.pentest_scan_progress.eta         = eta;
            if !command.is_empty() {
                app.pentest_scan_progress.command = command;
            }
            app.pentest_scan_progress.running = true;
        }
        Event::PenTestRawLine(line) => {
            app.pentest_raw_output.push(line);
            if app.pentest_raw_output.len() > 2000 { app.pentest_raw_output.remove(0); }
        }
        Event::PenTestEvidenceLine { timestamp, text } => {
            app.pentest_evidence.push(format!("[{}] {}", timestamp, text));
            if app.pentest_evidence.len() > 500 { app.pentest_evidence.remove(0); }
        }
        Event::PenTestReconComplete => {
            app.pentest_scan_progress.running  = false;
            app.pentest_scan_progress.percent  = 100;
            app.pentest_phase = crate::pentest::EngagementPhase::Complete;
            app.push_toast(crate::ui::components::toast::Toast::success(
                format!("Recon complete — {} hosts found", app.pentest_hosts.len())
            ));
        }
        Event::PenTestToolChecked { name, available, path } => {
            let status = if available {
                crate::pentest::ToolStatus::Available { path: path.unwrap_or_default() }
            } else {
                crate::pentest::ToolStatus::Missing
            };
            app.pentest_inventory.statuses.insert(name, status);
        }
        Event::PenTestInventoryComplete => {
            app.pentest_inv_complete = true;
        }
        Event::PenTestInstallLine(line) => {
            app.pentest_install_log.push(line);
            if app.pentest_install_log.len() > 400 { app.pentest_install_log.remove(0); }
        }
        Event::PenTestBatchInstallDone(success) => {
            app.pentest_installing = false;
            let msg = if success {
                "Install complete — rechecking inventory…"
            } else {
                "Some installs failed — see log for details"
            };
            app.push_toast(crate::ui::components::toast::Toast::info(msg));
            // Go back to pre-flight and rerun the inventory
            open_dialog(app, ActiveDialog::PenTestPreflight);
            launch_pentest_inventory(app);
        }
        Event::BwrapInstallLine(line) => {
            app.bwrap_install_log.push(line);
            if app.bwrap_install_log.len() > 200 { app.bwrap_install_log.remove(0); }
        }
        Event::BwrapInstallDone(success) => {
            app.bwrap_installing = false;
            if success {
                app.bwrap_install_log.push("✓ bubblewrap installed successfully!".to_string());
                app.bwrap_install_log.push("  Press Esc to close and then enable sandbox.".to_string());
            } else {
                app.bwrap_install_log.push("✗ Installation failed.".to_string());
                app.bwrap_install_log.push("  Try manually: sudo apt install bubblewrap".to_string());
            }
        }
        Event::Quit => return true,
        _ => {}
    }
    false
}

async fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> anyhow::Result<bool> {
    // Full-screen change reviewer owns all keys while open
    if app.review_open {
        return handle_review_key(app, key).await;
    }
    // Deferred changeset — Ctrl+R reopens the reviewer
    if app.changeset.is_some() && !app.review_open
        && key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        app.review_open = true;
        app.review_view = ReviewView::Overview;
        return Ok(false);
    }

    // Quit is always global — works even with dialogs/permission prompts open
    if let Some(Action::Quit) = app.keybinds.resolve(&key).cloned() {
        let _ = crate::config::save(&app.config);
        return Ok(true);
    }

    // Dialog key handling
    if app.active_dialog != ActiveDialog::None {
        return handle_dialog_key(app, key).await;
    }
    // Permission prompt
    if app.active_prompt == ActivePrompt::Permission {
        return handle_permission_key(app, key).await;
    }

    // Pass printable chars / navigation to textarea when no ctrl/alt
    let has_modifier = key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    if !has_modifier {
        match key.code {
            KeyCode::Char(_)
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End => {
                app.textarea.input(key);
                return Ok(false);
            }
            _ => {}
        }
    }

    let action = app.keybinds.resolve(&key).cloned();
    if let Some(action) = action {
        return dispatch_action(app, action).await;
    }

    Ok(false)
}

async fn dispatch_action(app: &mut App, action: Action) -> anyhow::Result<bool> {
    use Action::*;
    match action {
        Quit => {
            let _ = crate::config::save(&app.config);
            return Ok(true);
        }

        Submit => {
            if !app.is_streaming() {
                submit_message(app).await?;
            }
        }
        Newline       => { app.textarea.insert_newline(); }
        ClearInput    => { app.textarea = TextArea::default(); }
        PasteClipboard => paste_clipboard(app),
        HistoryPrev   => cycle_history(app, -1),
        HistoryNext   => cycle_history(app, 1),

        ScrollDown     => app.scroll_by(3),
        ScrollUp       => app.scroll_by(-3),
        ScrollHalfDown => app.scroll_by(15),
        ScrollHalfUp   => app.scroll_by(-15),
        ScrollPageDown => app.scroll_by(30),
        ScrollPageUp   => app.scroll_by(-30),
        ScrollTop      => app.scroll_to_top(),
        ScrollBottom   => app.scroll_to_bottom(),

        ScrollMsgPrev  => app.scroll_by(-20),
        ScrollMsgNext  => app.scroll_by(20),
        ScrollLastUser => app.scroll_to_bottom(),

        NewSession    => new_session(app).await?,
        SessionList   => open_dialog(app, ActiveDialog::SessionList),
        DeleteSession => delete_session(app).await?,
        RenameSession => {}
        ForkSession   => fork_session(app).await?,
        UndoMessage   => undo_message(app).await?,
        RedoMessage   => redo_message(app).await?,
        CopyLastMessage => copy_last_message(app),
        CompactSession  => {}

        ParentSession | NextChild | PrevChild => {}

        ModelPicker    => open_dialog(app, ActiveDialog::ModelPicker),
        CycleModelNext => cycle_model(app, 1),
        CycleModelPrev => cycle_model(app, -1),
        CycleFavoriteNext | CycleFavoritePrev => {}
        AgentPicker    => open_dialog(app, ActiveDialog::AgentPicker),

        ToggleThinking    => app.show_thinking = !app.show_thinking,
        ToggleSidebar     => app.sidebar_open  = !app.sidebar_open,
        ToggleToolDetails => app.show_tool_details = !app.show_tool_details,
        ToggleConceal     => app.concealed = !app.concealed,
        ToggleScrollbar   => app.show_scrollbar = !app.show_scrollbar,
        ToggleTerminalTitle => {}

        CommandPalette => {
            app.command_palette_tab = 0;
            open_dialog(app, ActiveDialog::CommandPalette);
        }
        Help           => { app.help_tab = 0; open_dialog(app, ActiveDialog::Help); }
        StatusView     => {}
        OpenFolder     => open_folder_browser(app),
        StashDraft     => stash_draft(app)?,
        PopDraft       => open_dialog(app, ActiveDialog::DraftPicker),

        ExternalEditor => open_external_editor(app).await?,
        ThemePicker    => open_dialog(app, ActiveDialog::ThemePicker),
        ThemeCycleNext => cycle_theme(app, 1),
        ThemeCyclePrev => cycle_theme(app, -1),

        PenTestMode    => enter_pentest_mode(app),

        Interrupt => {
            if app.is_streaming() {
                app.streaming = false;
                app.streaming_buf.clear();
                app.push_toast(crate::ui::components::toast::Toast::warning("Generation interrupted"));
            }
        }
    }
    Ok(false)
}

async fn handle_dialog_key(app: &mut App, key: crossterm::event::KeyEvent) -> anyhow::Result<bool> {
    // FolderInput — visual directory browser
    if app.active_dialog == ActiveDialog::FolderInput {
        match key.code {
            KeyCode::Esc => {
                app.folder_input_buf.clear();
                close_dialog(app);
            }
            KeyCode::Up => {
                if app.folder_input_buf.is_empty() {
                    app.dialog_selected_idx = app.dialog_selected_idx.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if app.folder_input_buf.is_empty() {
                    let max = app.folder_browser_entries.len().saturating_sub(1);
                    if app.dialog_selected_idx < max {
                        app.dialog_selected_idx += 1;
                    }
                }
            }
            KeyCode::Enter => {
                // If user typed a path manually, navigate to it / select it
                if !app.folder_input_buf.is_empty() {
                    let path_str = app.folder_input_buf.trim().to_string();
                    app.folder_input_buf.clear();
                    let path = std::path::PathBuf::from(&path_str);
                    if path.is_dir() {
                        // apply_folder opens IndexConfirm — don't close_dialog here
                        apply_folder(app, path);
                    } else {
                        app.push_toast(crate::ui::components::toast::Toast::error(
                            format!("Not a directory: {}", path_str)
                        ));
                    }
                    return Ok(false);
                }
                // Browser navigation
                let entry = app.folder_browser_entries.get(app.dialog_selected_idx).cloned();
                match entry.as_deref() {
                    Some("[ ✓ Select this folder ]") => {
                        let path = app.folder_browser_path.clone();
                        // apply_folder opens IndexConfirm — don't close_dialog here
                        apply_folder(app, path);
                    }
                    Some("..") => {
                        if let Some(parent) = app.folder_browser_path.parent().map(|p| p.to_path_buf()) {
                            app.folder_browser_path = parent.clone();
                            app.folder_browser_entries = load_dir_entries(&parent);
                            app.dialog_selected_idx = 0;
                        }
                    }
                    Some(name) => {
                        let new_path = app.folder_browser_path.join(name);
                        if new_path.is_dir() {
                            app.folder_browser_path = new_path.clone();
                            app.folder_browser_entries = load_dir_entries(&new_path);
                            app.dialog_selected_idx = 0;
                        }
                    }
                    None => {}
                }
            }
            KeyCode::Backspace => {
                if !app.folder_input_buf.is_empty() {
                    app.folder_input_buf.pop();
                } else {
                    // Backspace in browser = go to parent
                    if let Some(parent) = app.folder_browser_path.parent().map(|p| p.to_path_buf()) {
                        app.folder_browser_path = parent.clone();
                        app.folder_browser_entries = load_dir_entries(&parent);
                        app.dialog_selected_idx = 0;
                    }
                }
            }
            KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                app.folder_input_buf.push(c);
            }
            _ => {}
        }
        return Ok(false);
    }

    // RagSearch — type a query, press Enter to search the current folder's index
    if app.active_dialog == ActiveDialog::RagSearch {
        match key.code {
            KeyCode::Esc => close_dialog(app),
            KeyCode::Enter => {
                let query = app.dialog_search_query.trim().to_string();
                close_dialog(app);
                if !query.is_empty() {
                    let dir = app.working_dir.to_string_lossy().to_string();
                    let db  = app.db.clone();
                    let tx  = app.event_tx.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            let params = serde_json::json!({ "query": query, "top_k": 5 });
                            // Scope to current working dir index only
                            let conn = db.lock().unwrap();
                            match crate::rag::store::get_index_for_dir(&conn, &dir) {
                                Ok(Some(idx)) => {
                                    drop(conn);
                                    let p = serde_json::json!({ "query": params["query"], "index_name": idx.name, "top_k": 5 });
                                    crate::tools::rag::search_index(&p, &db)
                                }
                                _ => Err(anyhow::anyhow!("No index found for this folder. Use 'Index Folder' first.")),
                            }
                        }).await;
                        match result {
                            Ok(Ok(text))  => { let _ = tx.send(crate::event::Event::ToastMsg { text, is_error: false }); }
                            Ok(Err(e))    => { let _ = tx.send(crate::event::Event::ToastMsg { text: e.to_string(), is_error: true }); }
                            Err(e)        => { let _ = tx.send(crate::event::Event::ToastMsg { text: e.to_string(), is_error: true }); }
                        }
                    });
                }
            }
            KeyCode::Backspace => { app.dialog_search_query.pop(); }
            KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                app.dialog_search_query.push(c);
            }
            _ => {}
        }
        return Ok(false);
    }

    // GitConfirm — opt into git agent on folder open
    if app.active_dialog == ActiveDialog::GitConfirm {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.project_context_active = true;
                app.push_toast(crate::ui::components::toast::Toast::success("Git agent enabled. Branch, status and diff will be included in every prompt."));
                open_dialog(app, ActiveDialog::IndexConfirm);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                open_dialog(app, ActiveDialog::IndexConfirm);
            }
            _ => {}
        }
        return Ok(false);
    }

    // MemoryInput — type a fact to save to persistent memory
    if app.active_dialog == ActiveDialog::MemoryInput {
        match key.code {
            KeyCode::Esc => close_dialog(app),
            KeyCode::Enter => {
                let content = app.dialog_search_query.trim().to_string();
                close_dialog(app);
                if !content.is_empty() {
                    let sid    = app.session_id.clone();
                    let result = { let conn = app.db.lock().unwrap(); crate::memory::save(&conn, &content, "general", Some(&sid)) };
                    match result {
                        Ok(_)  => app.push_toast(crate::ui::components::toast::Toast::success("Memory saved.")),
                        Err(e) => app.push_toast(crate::ui::components::toast::Toast::error(format!("Failed to save memory: {}", e))),
                    }
                }
            }
            KeyCode::Backspace => { app.dialog_search_query.pop(); }
            KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                app.dialog_search_query.push(c);
            }
            _ => {}
        }
        return Ok(false);
    }

    // BwrapInstall — sudo password prompt + install progress
    if app.active_dialog == ActiveDialog::BwrapInstall {
        if app.bwrap_sudo_prompt {
            match key.code {
                KeyCode::Enter => {
                    app.bwrap_sudo_prompt = false;
                    app.bwrap_install_log.push("Starting installation…".to_string());
                    let tx = app.event_tx.clone();
                    let password = app.bwrap_sudo_input.clone();
                    app.bwrap_sudo_input.clear();
                    tokio::spawn(async move {
                        stream_bwrap_install(tx, password).await;
                    });
                }
                KeyCode::Backspace => { app.bwrap_sudo_input.pop(); }
                KeyCode::Esc => {
                    app.bwrap_installing = false;
                    app.bwrap_sudo_prompt = false;
                    close_dialog(app);
                }
                KeyCode::Char(c) => { app.bwrap_sudo_input.push(c); }
                _ => {}
            }
        } else if !app.bwrap_installing {
            if key.code == KeyCode::Esc {
                close_dialog(app);
            }
        }
        return Ok(false);
    }

    // GitToken — PAT setup guide + masked input; triggered by auth failures in git_push/git_pull
    if app.active_dialog == ActiveDialog::GitToken {
        match key.code {
            KeyCode::Backspace => { app.git_token_input.pop(); }
            KeyCode::Esc => {
                app.git_token_input.clear();
                close_dialog(app);
            }
            KeyCode::Enter if !app.git_token_input.is_empty() => {
                let token = app.git_token_input.clone();
                let host  = app.git_token_host.clone();
                app.git_token_input.clear();
                close_dialog(app);

                // Store synchronously — fast, and must complete before the retry reaches git
                let store_result = tokio::task::block_in_place(|| {
                    crate::tools::git::store_git_token(&host, &token)
                });

                match store_result {
                    Ok(_) => {
                        app.push_toast(crate::ui::components::toast::Toast::success(
                            "Git token saved — retrying now…"
                        ));
                        // Inject a synthetic user message so the model retries automatically
                        let retry_text = "Token saved. Please retry the git operation that just failed.";
                        let mut retry_ta = tui_textarea::TextArea::default();
                        retry_ta.insert_str(retry_text);
                        app.textarea = retry_ta;
                        submit_message(app).await?;
                    }
                    Err(e) => {
                        app.push_toast(crate::ui::components::toast::Toast::error(
                            format!("Failed to save token: {}", e)
                        ));
                    }
                }
            }
            KeyCode::Char(c) => { app.git_token_input.push(c); }
            _ => {}
        }
        return Ok(false);
    }

    // PenTestAuth — full-screen authorization gate
    if app.active_dialog == ActiveDialog::PenTestAuth {
        use crate::pentest::AuthPhase;
        // Only accept input once the content has fully revealed
        if app.pentest_auth_phase == AuthPhase::AwaitInput {
            match key.code {
                KeyCode::Backspace => { app.pentest_auth_input.pop(); }
                KeyCode::Esc => {
                    app.pentest_auth_input.clear();
                    app.pentest_auth_phase = AuthPhase::BlackOut;
                    app.pentest_auth_tick  = 0;
                    close_dialog(app);
                }
                KeyCode::Enter => {
                    let input = app.pentest_auth_input.trim().to_uppercase();
                    if input == "AUTHORIZED" {
                        app.pentest_auth_phase = AuthPhase::FlashCorrect;
                        app.pentest_auth_flash = 4;
                    } else {
                        app.pentest_auth_phase = AuthPhase::FlashWrong;
                        app.pentest_auth_flash = 3;
                    }
                }
                KeyCode::Char(c) if app.pentest_auth_input.len() < 20 => {
                    app.pentest_auth_input.push(c);
                }
                _ => {}
            }
        } else if app.pentest_auth_phase == AuthPhase::BlackOut
               || app.pentest_auth_phase == AuthPhase::Revealing {
            // Allow Esc to cancel even during animation
            if key.code == KeyCode::Esc {
                app.pentest_auth_phase = AuthPhase::BlackOut;
                app.pentest_auth_tick  = 0;
                close_dialog(app);
            }
        }
        return Ok(false);
    }

    // PenTestPreflight — tool inventory + workflow availability
    if app.active_dialog == ActiveDialog::PenTestPreflight {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.pentest_inv_complete = false;
                app.pentest_inventory    = crate::pentest::ToolInventory::default();
                close_dialog(app);
            }
            // Enter or i — if tools are missing open the selector, otherwise go to setup
            KeyCode::Enter | KeyCode::Char('i') if app.pentest_inv_complete => {
                let missing = app.pentest_inventory.missing_tools();
                if missing.is_empty() {
                    // All tools ready — open engagement setup form
                    app.pentest_setup_target.clear();
                    app.pentest_setup_exclusions.clear();
                    app.pentest_setup_field = 0;
                    app.pentest_setup_depth = crate::pentest::Depth::SafeActive;
                    open_dialog(app, ActiveDialog::PenTestSetup);
                } else {
                    app.pentest_selector_items = missing.iter()
                        .map(|&n| (n.to_string(), true))
                        .collect();
                    app.pentest_selector_cursor = 0;
                    app.pentest_install_sudo_prompt = false;
                    app.pentest_install_sudo_input.clear();
                    open_dialog(app, ActiveDialog::PenTestToolSelector);
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    // PenTestToolSelector — interactive selection of tools to install
    if app.active_dialog == ActiveDialog::PenTestToolSelector {
        // Golang dependency confirmation (shown before sudo prompt)
        if app.pentest_golang_confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    // Add golang-go to the selected install list
                    let already = app.pentest_selector_items.iter().any(|(n, _)| n == "golang-go");
                    if !already {
                        app.pentest_selector_items.insert(0, ("golang-go".to_string(), true));
                    }
                    app.pentest_golang_confirm = false;
                    app.pentest_install_sudo_prompt = true;
                    app.pentest_install_sudo_input.clear();
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    // Skip Go-dependent tools — deselect them
                    let skip: Vec<String> = app.pentest_golang_tools.drain(..).collect();
                    for (name, sel) in &mut app.pentest_selector_items {
                        if skip.contains(name) { *sel = false; }
                    }
                    app.pentest_golang_confirm = false;
                    // Only show sudo if something is still selected
                    let any = app.pentest_selector_items.iter().any(|(_, s)| *s);
                    if any {
                        app.pentest_install_sudo_prompt = true;
                        app.pentest_install_sudo_input.clear();
                    }
                }
                KeyCode::Esc => {
                    app.pentest_golang_confirm = false;
                }
                _ => {}
            }
            return Ok(false);
        }

        if app.pentest_install_sudo_prompt {
            match key.code {
                KeyCode::Backspace => { app.pentest_install_sudo_input.pop(); }
                KeyCode::Esc => {
                    app.pentest_install_sudo_prompt = false;
                    app.pentest_install_sudo_input.clear();
                }
                KeyCode::Enter if !app.pentest_install_sudo_input.is_empty() => {
                    let selected: Vec<String> = app.pentest_selector_items.iter()
                        .filter(|(_, sel)| *sel)
                        .map(|(n, _)| n.clone())
                        .collect();
                    if selected.is_empty() {
                        app.pentest_install_sudo_prompt = false;
                        return Ok(false);
                    }
                    let password = app.pentest_install_sudo_input.clone();
                    let env = app.pentest_env.clone().unwrap();
                    app.pentest_install_sudo_input.clear();
                    app.pentest_install_sudo_prompt = false;
                    app.pentest_install_log.clear();
                    app.pentest_installing = true;
                    open_dialog(app, ActiveDialog::PenTestInstall);
                    let tx = app.event_tx.clone();
                    tokio::spawn(async move {
                        stream_pentest_install(tx, password, selected, env).await;
                    });
                }
                KeyCode::Char(c) => { app.pentest_install_sudo_input.push(c); }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => {
                    open_dialog(app, ActiveDialog::PenTestPreflight);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if app.pentest_selector_cursor > 0 {
                        app.pentest_selector_cursor -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = app.pentest_selector_items.len();
                    if app.pentest_selector_cursor + 1 < len {
                        app.pentest_selector_cursor += 1;
                    }
                }
                KeyCode::Char(' ') => {
                    let idx = app.pentest_selector_cursor;
                    if let Some(item) = app.pentest_selector_items.get_mut(idx) {
                        item.1 = !item.1;
                    }
                }
                KeyCode::Char('a') => {
                    for item in &mut app.pentest_selector_items { item.1 = true; }
                }
                KeyCode::Char('n') => {
                    for item in &mut app.pentest_selector_items { item.1 = false; }
                }
                KeyCode::Enter => {
                    let any_selected = app.pentest_selector_items.iter().any(|(_, s)| *s);
                    if !any_selected { return Ok(false); }

                    // Check if any selected tools need Go and Go isn't installed
                    let go_missing = which::which("go").is_err();
                    if go_missing {
                        let go_tools: Vec<String> = app.pentest_selector_items.iter()
                            .filter(|(_, sel)| *sel)
                            .filter_map(|(name, _)| {
                                crate::pentest::tool_def(name)
                                    .filter(|d| d.go_pkg.is_some())
                                    .map(|_| name.clone())
                            })
                            .collect();
                        if !go_tools.is_empty() {
                            app.pentest_golang_tools   = go_tools;
                            app.pentest_golang_confirm = true;
                            return Ok(false);
                        }
                    }
                    app.pentest_install_sudo_prompt = true;
                    app.pentest_install_sudo_input.clear();
                }
                _ => {}
            }
        }
        return Ok(false);
    }

    // PenTestInstall — streaming install log
    if app.active_dialog == ActiveDialog::PenTestInstall {
        if !app.pentest_installing && key.code == KeyCode::Esc {
            open_dialog(app, ActiveDialog::PenTestPreflight);
        }
        return Ok(false);
    }

    // PenTestSetup — engagement target entry form
    if app.active_dialog == ActiveDialog::PenTestSetup {
        match key.code {
            KeyCode::Esc => { open_dialog(app, ActiveDialog::PenTestPreflight); }
            KeyCode::Tab | KeyCode::Down => {
                app.pentest_setup_field = (app.pentest_setup_field + 1) % 3;
            }
            KeyCode::Up => {
                app.pentest_setup_field = app.pentest_setup_field.saturating_sub(1);
            }
            KeyCode::Left if app.pentest_setup_field == 2 => {
                app.pentest_setup_depth = match app.pentest_setup_depth {
                    crate::pentest::Depth::SafeActive => crate::pentest::Depth::ReconOnly,
                    crate::pentest::Depth::Full       => crate::pentest::Depth::SafeActive,
                    _                                 => crate::pentest::Depth::ReconOnly,
                };
            }
            KeyCode::Right if app.pentest_setup_field == 2 => {
                app.pentest_setup_depth = match app.pentest_setup_depth {
                    crate::pentest::Depth::ReconOnly  => crate::pentest::Depth::SafeActive,
                    crate::pentest::Depth::SafeActive => crate::pentest::Depth::Full,
                    _                                 => crate::pentest::Depth::Full,
                };
            }
            KeyCode::Backspace => {
                match app.pentest_setup_field {
                    0 => { app.pentest_setup_target.pop(); }
                    1 => { app.pentest_setup_exclusions.pop(); }
                    _ => {}
                }
            }
            KeyCode::Enter => {
                if app.pentest_setup_target.trim().is_empty() {
                    app.push_toast(crate::ui::components::toast::Toast::error("Target required"));
                    return Ok(false);
                }
                start_engagement(app);
            }
            KeyCode::Char(c) => {
                match app.pentest_setup_field {
                    0 => app.pentest_setup_target.push(c),
                    1 => app.pentest_setup_exclusions.push(c),
                    _ => {}
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    // Pen test mode TUI navigation (when fully in pen test mode, no dialog)
    if app.pentest_mode && app.active_dialog == ActiveDialog::None {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.pentest_selected_host = app.pentest_selected_host.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = app.pentest_hosts.len().saturating_sub(1);
                if app.pentest_selected_host < max {
                    app.pentest_selected_host += 1;
                }
            }
            KeyCode::Tab => {
                app.pentest_raw_tab = !app.pentest_raw_tab;
            }
            // Ctrl+P exits pen test mode back to chat
            _ => if let Some(action) = app.keybinds.resolve(&key).cloned() {
                if action == crate::keybinds::Action::PenTestMode {
                    app.pentest_mode = false;
                    app.push_toast(crate::ui::components::toast::Toast::info("Exited pen test mode"));
                } else if action == crate::keybinds::Action::Quit {
                    return Ok(true);
                }
            }
        }
        return Ok(false);
    }

    // IndexConfirm — ask user whether to index the just-opened folder
    if app.active_dialog == ActiveDialog::IndexConfirm {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                close_dialog(app);
                let dir = app.working_dir.clone();
                let db  = app.db.clone();
                let tx  = app.event_tx.clone();
                app.push_toast(crate::ui::components::toast::Toast::info("Indexing folder… (downloads ~22 MB embedding model on first use)"));
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        crate::tools::rag::index_dir(
                            &serde_json::Value::Object(Default::default()),
                            &dir,
                            &db,
                        )
                    }).await;
                    match result {
                        Ok(Ok(msg))  => { let _ = tx.send(crate::event::Event::ToastMsg { text: msg,                               is_error: false }); }
                        Ok(Err(e))   => { let _ = tx.send(crate::event::Event::ToastMsg { text: format!("Index error: {}", e),    is_error: true  }); }
                        Err(e)       => { let _ = tx.send(crate::event::Event::ToastMsg { text: format!("Index error: {}", e),    is_error: true  }); }
                    }
                });
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                close_dialog(app);
            }
            _ => {}
        }
        return Ok(false);
    }

    // AgentEditor — full keyboard takeover for the form
    if app.active_dialog == ActiveDialog::AgentEditor {
        match key.code {
            KeyCode::Esc => close_dialog(app),
            KeyCode::Tab => {
                app.agent_editor_field = (app.agent_editor_field + 1) % 3;
            }
            KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                save_agent_editor(app)?;
                close_dialog(app);
            }
            _ => {
                if app.agent_editor_field == 2 {
                    app.agent_editor_system.input(key);
                } else {
                    // Name / description — basic char input
                    match key.code {
                        KeyCode::Char(c) => {
                            if app.agent_editor_field == 0 { app.agent_editor_name.push(c); }
                            else { app.agent_editor_desc.push(c); }
                        }
                        KeyCode::Backspace => {
                            if app.agent_editor_field == 0 { app.agent_editor_name.pop(); }
                            else { app.agent_editor_desc.pop(); }
                        }
                        _ => {}
                    }
                }
            }
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Esc   => close_dialog(app),
        KeyCode::Enter => confirm_dialog(app).await?,
        KeyCode::Up    => {
            app.dialog_selected_idx = app.dialog_selected_idx.saturating_sub(1);
        }
        KeyCode::Down  => {
            app.dialog_selected_idx += 1;
        }
        KeyCode::Tab   => {
            if app.active_dialog == ActiveDialog::ModelPicker {
                app.model_picker_tab = (app.model_picker_tab + 1) % 4;
                app.dialog_selected_idx = 0;
            } else if app.active_dialog == ActiveDialog::CommandPalette {
                app.command_palette_tab = (app.command_palette_tab + 1) % 4;
                app.dialog_selected_idx = 0;
                app.dialog_search_query.clear();
            } else if app.active_dialog == ActiveDialog::Help {
                app.help_tab = (app.help_tab + 1) % 4;
            }
        }
        KeyCode::Left if app.active_dialog == ActiveDialog::Help => {
            app.help_tab = (app.help_tab + 3) % 4;
        }
        KeyCode::Right if app.active_dialog == ActiveDialog::Help => {
            app.help_tab = (app.help_tab + 1) % 4;
        }
        // Ctrl+D = delete highlighted session in SessionList
        KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL
            && app.active_dialog == ActiveDialog::SessionList =>
        {
            let query = app.dialog_search_query.to_lowercase();
            let filtered_ids: Vec<String> = app.sessions.iter()
                .filter(|s| query.is_empty() || s.title.to_lowercase().contains(&query))
                .map(|s| s.id.clone())
                .collect();
            if let Some(target_id) = filtered_ids.get(app.dialog_selected_idx).cloned() {
                if app.sessions.len() <= 1 {
                    app.push_toast(crate::ui::components::toast::Toast::warning("Cannot delete the last session"));
                } else {
                    crate::db::delete_session(&app.db, &target_id)?;
                    app.sessions.retain(|s| s.id != target_id);
                    if app.session_id == target_id {
                        if let Some(s) = app.sessions.first() {
                            let id = s.id.clone();
                            load_session(app, id)?;
                        }
                        close_dialog(app);
                    } else {
                        let new_len = filtered_ids.len().saturating_sub(1);
                        if app.dialog_selected_idx >= new_len && new_len > 0 {
                            app.dialog_selected_idx = new_len - 1;
                        }
                    }
                    app.push_toast(crate::ui::components::toast::Toast::success("Session deleted"));
                }
            }
        }

        // 'd' = delete in AgentPicker (custom agents) and DraftPicker
        KeyCode::Char('d') if key.modifiers == KeyModifiers::NONE => {
            if app.active_dialog == ActiveDialog::AgentPicker {
                let builtin_len = crate::tools::BUILTIN_AGENTS.len();
                let idx = app.dialog_selected_idx;
                if idx >= builtin_len && idx < builtin_len + app.custom_agents.len() {
                    let agent_id = app.custom_agents[idx - builtin_len].id.clone();
                    crate::db::delete_agent(&app.db, &agent_id)?;
                    app.custom_agents.retain(|a| a.id != agent_id);
                    if app.current_agent == agent_id { app.current_agent = "general".to_string(); }
                    app.dialog_selected_idx = app.dialog_selected_idx.saturating_sub(1);
                    app.push_toast(crate::ui::components::toast::Toast::info("Agent deleted"));
                }
            } else if app.active_dialog == ActiveDialog::DraftPicker && !app.drafts.is_empty() {
                let idx = app.dialog_selected_idx.min(app.drafts.len().saturating_sub(1));
                let draft_id = app.drafts[idx].id.clone();
                crate::db::delete_draft(&app.db, &draft_id)?;
                app.drafts.remove(idx);
                app.dialog_selected_idx = app.dialog_selected_idx.saturating_sub(1);
                app.push_toast(crate::ui::components::toast::Toast::info("Draft deleted"));
            } else {
                app.dialog_search_query.push('d');
                app.dialog_selected_idx = 0;
            }
        }
        KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
            app.dialog_search_query.push(c);
            app.dialog_selected_idx = 0;
        }
        KeyCode::Backspace => {
            app.dialog_search_query.pop();
            app.dialog_selected_idx = 0;
        }
        _ => {}
    }
    Ok(false)
}

async fn handle_permission_key(app: &mut App, key: crossterm::event::KeyEvent) -> anyhow::Result<bool> {
    let req_id = app.pending_permission.as_ref().map(|r| r.id.clone()).unwrap_or_default();
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let _ = app.event_tx.send(Event::PermissionGranted { request_id: req_id });
            app.active_prompt = ActivePrompt::Input;
            app.pending_permission = None;
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            // "Allow always" — same event for now
            let _ = app.event_tx.send(Event::PermissionGranted { request_id: req_id });
            app.active_prompt = ActivePrompt::Input;
            app.pending_permission = None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            let _ = app.event_tx.send(Event::PermissionDenied { request_id: req_id });
            app.active_prompt = ActivePrompt::Input;
            app.pending_permission = None;
        }
        _ => {}
    }
    Ok(false)
}

/// Detect whether the streaming buffer contains a complete JSON tool fence.
/// Pattern: ```[lang]\n{"name": "...", "arguments": {...}}\n```
fn is_complete_json_tool_fence(buf: &str) -> bool {
    find_json_fence_end(buf).is_some()
}

/// Find the byte offset just past the closing ``` of a JSON tool fence.
fn find_json_fence_end(buf: &str) -> Option<usize> {
    let fence_start = buf.find("```")?;
    let after_fence = &buf[fence_start + 3..];
    // Skip optional lang tag
    let lang_end = after_fence.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(0);
    let after_lang = &after_fence[lang_end..];
    let after_nl = if after_lang.starts_with('\n') {
        &after_lang[1..]
    } else if after_lang.starts_with("\r\n") {
        &after_lang[2..]
    } else {
        return None;
    };

    // Find closing ```
    let close_pos = after_nl.find("\n```")?;
    let inner = after_nl[..close_pos].trim();

    // Must be JSON with a "name" key pointing to a known tool
    if !inner.starts_with('{') { return None; }
    let v: serde_json::Value = serde_json::from_str(inner)
        .or_else(|_| serde_json::from_str(&crate::tools::repair_json_pub(inner)))
        .ok()?;
    let name = v.get("name").or_else(|| v.get("tool")).and_then(|n| n.as_str())?;
    if !crate::tools::ALL_TOOLS.iter().any(|t| t.name == name) { return None; }

    // Compute absolute end position
    let inner_offset = fence_start + 3 + lang_end + 1; // past the opening ``` + lang + \n
    let end = inner_offset + close_pos + 4; // +4 for \n```
    Some(end)
}

/// Remove empty code fences like "```xml\n\n```" left after tool call tags are stripped.
/// Remove any trailing incomplete <tool_call> open tag (model stopped mid-generation).
fn strip_incomplete_tool_tags(text: &str) -> String {
    let mut s = text.to_string();
    // Remove bare <tool_call> with no matching </tool_call>
    while let Some(open) = s.find("<tool_call>") {
        if !s[open..].contains("</tool_call>") {
            s.truncate(open);
            break;
        }
        // Move past this complete one
        let close = s[open..].find("</tool_call>").unwrap();
        let after = open + close + "</tool_call>".len();
        // Check if there's another one after
        if !s[after..].contains("<tool_call>") { break; }
        // otherwise loop will find it
        break; // only need one pass
    }
    s.trim_end().to_string()
}

fn strip_empty_fences(text: &str) -> String {
    // First: strip any code fence block that contained a <tool_call> (the tool XML was
    // already extracted; only the fence wrapper + leftover attribute tags remain).
    // Matches: ```[lang]\n ... ```  where the content contains <name> or <parameters> remnants
    let re_tool_fence = regex::Regex::new(r"(?s)```[a-z]*\n(?:[^`]*?)```\n?").unwrap();
    let text = re_tool_fence.replace_all(text, |caps: &regex::Captures| {
        let inner = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        // Only remove the fence if it looks like tool call debris
        if inner.contains("<name>") || inner.contains("<parameters>") || inner.contains("<tool_call>") {
            String::new()
        } else {
            inner.to_string()
        }
    });
    // Second pass: remove any remaining empty fences (``` ... ```) with only whitespace
    let re_empty = regex::Regex::new(r"(?m)```[a-z]*\n\s*```\n?").unwrap();
    re_empty.replace_all(&text, "").trim_matches('\n').to_string()
}

/// Read sorted subdirectory names from a path.
/// Returns a list starting with "[ ✓ Select this folder ]" then ".." (if parent exists),
/// then all non-hidden subdirectories alphabetically.
fn load_dir_entries(path: &std::path::Path) -> Vec<String> {
    let mut entries = vec!["[ ✓ Select this folder ]".to_string()];
    if path.parent().is_some() {
        entries.push("..".to_string());
    }
    if let Ok(rd) = std::fs::read_dir(path) {
        let mut dirs: Vec<String> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with('.'))
            .collect();
        dirs.sort();
        entries.extend(dirs);
    }
    entries
}

fn open_folder_browser(app: &mut App) {
    app.folder_input_buf.clear();
    app.folder_browser_path = app.working_dir.clone();
    app.folder_browser_entries = load_dir_entries(&app.working_dir.clone());
    open_dialog(app, ActiveDialog::FolderInput);
}

fn apply_folder(app: &mut App, path: std::path::PathBuf) {
    let name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    app.working_dir = path.clone();
    app.project_ctx = Some(crate::project::scan(&path));
    app.push_toast(crate::ui::components::toast::Toast::success(format!("Opened: {}", name)));
    // If git repo, ask about git agent first — then IndexConfirm follows
    let is_git = app.project_ctx.as_ref().map(|c| c.is_git).unwrap_or(false);
    if is_git {
        open_dialog(app, ActiveDialog::GitConfirm);
    } else {
        open_dialog(app, ActiveDialog::IndexConfirm);
    }
}

fn open_dialog(app: &mut App, dialog: ActiveDialog) {
    app.active_dialog = dialog;
    app.dialog_search_query.clear();
    app.dialog_selected_idx = 0;
}

fn close_dialog(app: &mut App) {
    app.active_dialog = ActiveDialog::None;
    app.dialog_search_query.clear();
}

async fn confirm_dialog(app: &mut App) -> anyhow::Result<()> {
    match app.active_dialog.clone() {
        ActiveDialog::SessionList => {
            let query = app.dialog_search_query.to_lowercase();
            let filtered_ids: Vec<String> = app.sessions.iter()
                .filter(|s| query.is_empty() || s.title.to_lowercase().contains(&query))
                .map(|s| s.id.clone())
                .collect();
            if let Some(id) = filtered_ids.get(app.dialog_selected_idx) {
                let id = id.clone();
                load_session(app, id)?;
            }
            close_dialog(app);
        }
        ActiveDialog::ModelPicker => {
            if app.model_picker_tab == 3 {
                // Download tab — trigger download of selected model
                let q = app.dialog_search_query.to_lowercase();
                let entries: Vec<_> = crate::ui::dialogs::model_picker::DOWNLOADABLE.iter()
                    .filter(|e| q.is_empty()
                        || e.display.to_lowercase().contains(&q))
                    .collect();
                if let Some(entry) = entries.get(app.dialog_selected_idx) {
                    let models_dir = dirs::home_dir()
                        .map(|h| h.join(".hyperlite").join("models"))
                        .unwrap_or_default();
                    let already = models_dir.join(entry.hf_file).exists();
                    if already {
                        app.push_toast(crate::ui::components::toast::Toast::success(
                            format!("{} is already installed", entry.display)
                        ));
                    } else if app.model_dl_active.is_none() {
                        start_model_download(
                            app,
                            entry.hf_file.to_string(),
                            entry.hf_url(),
                            entry.display.to_string(),
                        );
                    }
                }
                // Don't close — user can watch progress
            } else {
                let query = app.dialog_search_query.to_lowercase();
                let filtered: Vec<String> = app.available_models.iter()
                    .filter(|m| query.is_empty() || m.name.to_lowercase().contains(&query))
                    .map(|m| m.id.clone())
                    .collect();
                if let Some(id) = filtered.get(app.dialog_selected_idx) {
                    let id = id.clone();
                    let name = app.available_models.iter().find(|m| m.id == id)
                        .map(|m| m.name.clone()).unwrap_or_default();
                    close_dialog(app);

                    let is_same = app.current_model == id;
                    let has_history = !app.messages.is_empty();

                    if !is_same && has_history {
                        // Compact with the old model before switching so the new model
                        // gets a clean summary rather than a raw history it may not understand.
                        app.push_toast(crate::ui::components::toast::Toast::info(
                            format!("Compacting session before switching to {}…", name)
                        ));
                        compact_session(app).await?;
                    }
                    app.current_model = id;
                    if !is_same {
                        app.push_toast(crate::ui::components::toast::Toast::success(
                            format!("Switched to {}", name)
                        ));
                    }
                } else {
                    close_dialog(app);
                }
            }
        }
        ActiveDialog::ThemePicker => {
            let names = crate::ui::theme::all_names();
            if let Some(name) = names.get(app.dialog_selected_idx) {
                app.theme = crate::ui::theme::get(name);
                app.config.theme = name.to_string();
                let _ = crate::config::save(&app.config);
            }
            close_dialog(app);
        }
        ActiveDialog::CommandPalette => {
            // Dispatch the selected command to its actual action (scoped to current tab)
            let commands = crate::ui::dialogs::command::commands_for_tab(app.command_palette_tab);
            let query = app.dialog_search_query.to_lowercase();
            let filtered: Vec<_> = commands.iter()
                .filter(|c| query.is_empty()
                    || c.label.to_lowercase().contains(&query)
                    || c.desc.to_lowercase().contains(&query))
                .collect();
            let label = filtered
                .get(app.dialog_selected_idx.min(filtered.len().saturating_sub(1)))
                .map(|c| c.label);
            close_dialog(app);
            match label {
                Some("New Session")        => new_session(app).await?,
                Some("Switch Session")     => open_dialog(app, ActiveDialog::SessionList),
                Some("Fork Session")       => fork_session(app).await?,
                Some("Delete Session")     => delete_session(app).await?,
                Some("Rename Session")     => begin_rename_session(app),
                Some("Compact Session")    => compact_session(app).await?,
                Some("Switch Model")       => open_dialog(app, ActiveDialog::ModelPicker),
                Some("Switch Agent")       => open_dialog(app, ActiveDialog::AgentPicker),
                Some("Stash Draft")        => stash_draft(app)?,
                Some("Pop Draft")          => open_dialog(app, ActiveDialog::DraftPicker),
                Some("Cycle Model Next")   => cycle_model(app, 1),
                Some("Toggle Sidebar")     => app.sidebar_open = !app.sidebar_open,
                Some("Toggle Thinking")    => app.show_thinking = !app.show_thinking,
                Some("Toggle Tool Details")=> app.show_tool_details = !app.show_tool_details,
                Some("Toggle Conceal")     => app.concealed = !app.concealed,
                Some("Pick Theme")         => open_dialog(app, ActiveDialog::ThemePicker),
                Some("Open in Editor")     => open_external_editor(app).await?,
                Some("Copy Last Response") => copy_last_message(app),
                Some("Copy Last Code")     => copy_last_code_block(app),
                Some("Undo Last Message")  => undo_message(app).await?,
                Some("Help")               => open_dialog(app, ActiveDialog::Help),
                Some("Open Folder")        => open_folder_browser(app),
                Some("Enable Git Context")  => {
                    if app.project_ctx.as_ref().map(|c| c.is_git).unwrap_or(false) {
                        app.project_context_active = true;
                        app.push_toast(crate::ui::components::toast::Toast::success("Git context enabled — branch, status and diff injected into prompts."));
                    } else {
                        app.push_toast(crate::ui::components::toast::Toast::warning("Not a git repository."));
                    }
                }
                Some("Disable Git Context") => {
                    app.project_context_active = false;
                    app.push_toast(crate::ui::components::toast::Toast::info("Git context disabled."));
                }
                Some("Enable Sandbox") => {
                    if crate::tools::shell::bwrap_available() {
                        app.sandbox_enabled = true;
                        app.push_toast(crate::ui::components::toast::Toast::success("Sandbox enabled — shell commands isolated via bwrap."));
                    } else {
                        app.bwrap_install_log.clear();
                        app.bwrap_installing = true;
                        app.bwrap_sudo_prompt = true;
                        app.bwrap_sudo_input.clear();
                        app.bwrap_install_log.push("bubblewrap not found — need sudo to install.".to_string());
                        app.active_dialog = ActiveDialog::BwrapInstall;
                    }
                }
                Some("Disable Sandbox") => {
                    app.sandbox_enabled = false;
                    app.push_toast(crate::ui::components::toast::Toast::info("Sandbox disabled — shell runs directly."));
                }
                Some("Index Folder")       => open_dialog(app, ActiveDialog::IndexConfirm),
                Some("Search Index")       => open_dialog(app, ActiveDialog::RagSearch),
                Some("Clear Index")        => {
                    let dir = app.working_dir.to_string_lossy().to_string();
                    let db  = app.db.clone();
                    let conn = db.lock().unwrap();
                    match crate::rag::store::get_index_for_dir(&conn, &dir) {
                        Ok(Some(idx)) => {
                            drop(conn);
                            let conn2 = db.lock().unwrap();
                            let _ = crate::rag::store::delete_index(&conn2, &idx.name);
                            app.push_toast(crate::ui::components::toast::Toast::success("Index cleared for this folder"));
                        }
                        _ => {
                            drop(conn);
                            app.push_toast(crate::ui::components::toast::Toast::info("No index found for this folder"));
                        }
                    }
                }
                Some("List Indexes")       => {
                    let db = app.db.clone();
                    let conn = db.lock().unwrap();
                    match crate::rag::store::list_indexes(&conn) {
                        Ok(indexes) if indexes.is_empty() => {
                            app.push_toast(crate::ui::components::toast::Toast::info("No indexes yet. Open a folder and index it first."));
                        }
                        Ok(indexes) => {
                            let lines: Vec<String> = indexes.iter().map(|i| {
                                format!("{} — {} files · {} chunks", i.root_path, i.file_count, i.chunk_count)
                            }).collect();
                            app.push_toast(crate::ui::components::toast::Toast::info(lines.join("\n")));
                        }
                        Err(e) => {
                            app.push_toast(crate::ui::components::toast::Toast::error(format!("Error: {}", e)));
                        }
                    }
                }
                Some("Save Memory")        => open_dialog(app, ActiveDialog::MemoryInput),
                Some("View Memory")        => {
                    let result = { let conn = app.db.lock().unwrap(); crate::memory::list_all(&conn) };
                    match result {
                        Ok(mems) if mems.is_empty() => {
                            app.push_toast(crate::ui::components::toast::Toast::info("No memories saved yet. Use 'Save Memory' to add one."));
                        }
                        Ok(mems) => {
                            let lines: Vec<String> = mems.iter()
                                .map(|m| format!("[{}] {}", m.category, m.content))
                                .collect();
                            app.push_toast(crate::ui::components::toast::Toast::info(lines.join("\n")));
                        }
                        Err(e) => { app.push_toast(crate::ui::components::toast::Toast::error(format!("Error: {}", e))); }
                    }
                }
                Some("Clear Memory")       => {
                    let result = { let conn = app.db.lock().unwrap(); crate::memory::clear_all(&conn) };
                    match result {
                        Ok(n)  => { app.push_toast(crate::ui::components::toast::Toast::success(format!("Cleared {} memories.", n))); }
                        Err(e) => { app.push_toast(crate::ui::components::toast::Toast::error(format!("Error: {}", e))); }
                    }
                }
                Some("Quit")               => return Ok(()),
                _ => {}
            }
        }
        ActiveDialog::FolderInput => {} // handled in handle_dialog_key

        ActiveDialog::AgentPicker => {
            let total = crate::tools::BUILTIN_AGENTS.len() + app.custom_agents.len() + 1;
            let idx = app.dialog_selected_idx.min(total.saturating_sub(1));
            let builtin_len = crate::tools::BUILTIN_AGENTS.len();
            let custom_len  = app.custom_agents.len();
            if idx < builtin_len {
                let agent_id = crate::tools::BUILTIN_AGENTS[idx].id.to_string();
                let agent_name = crate::tools::BUILTIN_AGENTS[idx].name.to_string();
                app.current_agent = agent_id;
                app.push_toast(crate::ui::components::toast::Toast::success(
                    format!("Agent: {}", agent_name)
                ));
                close_dialog(app);
            } else if idx < builtin_len + custom_len {
                let agent = app.custom_agents[idx - builtin_len].clone();
                app.current_agent = agent.id.clone();
                app.push_toast(crate::ui::components::toast::Toast::success(
                    format!("Agent: {}", agent.name)
                ));
                close_dialog(app);
            } else {
                // "New Agent…" selected
                app.agent_editor_name.clear();
                app.agent_editor_desc.clear();
                app.agent_editor_system = tui_textarea::TextArea::default();
                app.agent_editor_field = 0;
                app.agent_editor_id = None;
                open_dialog(app, ActiveDialog::AgentEditor);
            }
        }

        ActiveDialog::AgentEditor => {
            // Ctrl+S saves — Enter handled via key handler; this is fallback
            save_agent_editor(app)?;
            close_dialog(app);
        }

        ActiveDialog::DraftPicker => {
            if !app.drafts.is_empty() {
                let idx = app.dialog_selected_idx.min(app.drafts.len().saturating_sub(1));
                let content = app.drafts[idx].content.clone();
                app.textarea = tui_textarea::TextArea::default();
                for ch in content.chars() {
                    if ch == '\n' { app.textarea.insert_newline(); }
                    else { app.textarea.insert_char(ch); }
                }
            }
            close_dialog(app);
        }

        _ => close_dialog(app),
    }
    Ok(())
}

// ── Session operations ────────────────────────────────────────────────────────

/// Fix 3 + 4 — build GenerationParams tuned for the current user request.
/// • temperature 0.1 when the message is an action request (reduces prose/narration)
/// • </tool_call> stop sequence so generation halts as soon as a tool call closes
fn action_params(user_text: &str) -> GenerationParams {
    let is_action = ["write", "create", "make", "edit", "fix", "build",
                     "add", "implement", "delete", "remove", "update"]
        .iter().any(|k| user_text.to_lowercase().contains(k));
    let mut params = GenerationParams::default();
    params.stop = vec!["</tool_call>".to_string()];
    if is_action {
        params.temperature = Some(0.1);
    }
    params
}

async fn submit_message(app: &mut App) -> anyhow::Result<()> {
    // Rename mode: textarea holds new title, not a chat message
    if app.active_prompt == ActivePrompt::Rename {
        let title: String = app.textarea.lines().join(" ").trim().to_string();
        if !title.is_empty() {
            if let Some(s) = app.sessions.iter_mut().find(|s| s.id == app.session_id) {
                s.title = title.clone();
                let _ = crate::db::update_session_title(&app.db, &app.session_id, &title);
            }
            app.push_toast(crate::ui::components::toast::Toast::success(
                format!("Renamed to: {}", title)
            ));
        }
        app.textarea = tui_textarea::TextArea::default();
        app.active_prompt = ActivePrompt::Input;
        return Ok(());
    }

    let text: String = app.textarea.lines().join("\n").trim().to_string();
    if text.is_empty() { return Ok(()); }

    app.input_history.push(text.clone());
    app.history_idx = None;
    app.textarea = TextArea::default();
    app.scroll_to_bottom();

    let user_msg = Message::new_user(&app.session_id, &text);
    crate::db::insert_message(&app.db, &user_msg)?;
    app.messages.push(user_msg);

    app.streaming = true;
    app.streaming_buf.clear();
    app.tool_iterations = 0;
    app.tool_enforcer_pending = false;
    app.active_plan.clear();
    app.plan_step = 0;
    app.tool_history.clear();
    app.undo_stack.clear(); // new message invalidates redo history

    // Build system prompt with full tool documentation
    let system = crate::tools::build_agent_system_prompt(
        &app.working_dir,
        if app.project_context_active { app.project_ctx.as_ref() } else { None },
        &app.current_agent,
        &app.custom_agents,
    );

    // Build chat messages for provider
    let mut chat: Vec<ChatMessage> = vec![
        ChatMessage { role: "system".to_string(), content: system },
    ];

    // Inject persistent memories into system prompt
    if let Some(user_query) = app.messages.last().map(|m| m.text_content()) {
        if !user_query.is_empty() {
            // Persistent memory (cross-session facts)
            if let Some(mem_ctx) = crate::memory::build_context(&app.db, &user_query) {
                chat.push(ChatMessage {
                    role:    "system".to_string(),
                    content: mem_ctx,
                });
            }
            // RAG context scoped to current working directory
            let working_dir = app.working_dir.to_string_lossy().to_string();
            if let Some(rag_ctx) = crate::tools::rag::retrieve_context(&user_query, &app.db, &working_dir, 5) {
                chat.push(ChatMessage {
                    role:    "system".to_string(),
                    content: rag_ctx,
                });
            }
        }
    }

    for msg in &app.messages {
        let role = match msg.role {
            Role::User      => "user",
            Role::Assistant => "assistant",
        };
        let content = msg.text_content();
        if !content.is_empty() {
            chat.push(ChatMessage { role: role.to_string(), content });
        }
    }

    let model_id    = app.current_model.clone();
    let tx          = app.event_tx.clone();
    let http_client = app.http_client.clone();

    // ── Native in-process inference (only when feature is compiled in) ──────────
    #[cfg(feature = "native-inference")]
    if model_id.starts_with("native:") {
        let native_models = crate::providers::native::discover_models();
        if let Some(nm) = native_models.iter().find(|m| m.id == model_id) {
            let nm       = nm.clone();
            let prompt   = format_chat_as_prompt(&chat);
            let max_tok  = 4096u32;

            tokio::spawn(async move {
                let start = std::time::Instant::now();
                let mut rx = crate::providers::native::inference::stream_generate(
                    &nm, prompt, max_tok,
                ).await;
                while let Some(ev) = rx.recv().await {
                    match ev {
                        StreamEvent::Text(t) => { let _ = tx.send(Event::StreamText(t)); }
                        StreamEvent::Done { .. } => {
                            let ms = start.elapsed().as_millis() as u64;
                            let _ = tx.send(Event::StreamDone { duration_ms: ms });
                            break;
                        }
                        StreamEvent::Error(e) => {
                            let _ = tx.send(Event::StreamError(e));
                            break;
                        }
                        _ => {}
                    }
                }
            });
            return Ok(());
        }
    }

    // ── DirectGguf path — spawn llama-server for the model file ─────────────
    let model_backend = app.available_models.iter()
        .find(|m| m.id == model_id)
        .map(|m| m.backend.clone());

    if model_backend == Some(crate::providers::BackendKind::DirectGguf) {
        let params   = action_params(&text);
        let client   = http_client.clone();
        let hardware = app.hardware.clone();
        tokio::spawn(async move {
            // Quick probe to show a meaningful status label during server startup
            let alive = client
                .get("http://127.0.0.1:18080/health")
                .timeout(std::time::Duration::from_millis(300))
                .send().await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !alive {
                let _ = tx.send(Event::StreamStatus("  Starting inference server…".to_string()));
            }

            let provider = crate::providers::direct::DirectGgufProvider::new(client, hardware);
            match crate::providers::LocalProvider::chat_stream(&provider, &chat, &model_id, &params).await {
                Ok(mut rx) => {
                    let _ = tx.send(Event::StreamStatus("  Generating…".to_string()));
                    let start = std::time::Instant::now();
                    while let Some(ev) = rx.recv().await {
                        match ev {
                            StreamEvent::Text(t)      => { let _ = tx.send(Event::StreamText(t)); }
                            StreamEvent::Reasoning(r) => { let _ = tx.send(Event::StreamReasoning(r)); }
                            StreamEvent::Done { .. } => {
                                let ms = start.elapsed().as_millis() as u64;
                                let _ = tx.send(Event::StreamDone { duration_ms: ms });
                                break;
                            }
                            StreamEvent::Error(e) => {
                                let _ = tx.send(Event::StreamError(e));
                                break;
                            }
                        }
                    }
                }
                Err(e) => { let _ = tx.send(Event::StreamError(e.to_string())); }
            }
        });
        return Ok(());
    }

    // ── HTTP path ────────────────────────────────────────────────────────────
    let (base_url, backend_kind) = app.provider_registry
        .backend_for_model(&model_id, &app.available_models);

    if base_url.is_empty() {
        app.streaming = false;
        app.push_toast(crate::ui::components::toast::Toast::error("No backend available"));
        return Ok(());
    }

    // Fix 3+4: tune temperature and stop sequences based on user intent
    let params = action_params(&text);

    tokio::spawn(async move {
        match crate::providers::stream_for_backend(
            &http_client,
            &base_url,
            backend_kind,
            &chat,
            &model_id,
            &params,
        ).await {
            Ok(mut rx) => {
                let start = std::time::Instant::now();
                while let Some(ev) = rx.recv().await {
                    match ev {
                        StreamEvent::Text(t)      => { let _ = tx.send(Event::StreamText(t)); }
                        StreamEvent::Reasoning(r) => { let _ = tx.send(Event::StreamReasoning(r)); }
                        StreamEvent::Done { .. } => {
                            let ms = start.elapsed().as_millis() as u64;
                            let _ = tx.send(Event::StreamDone { duration_ms: ms });
                            break;
                        }
                        StreamEvent::Error(e) => {
                            let _ = tx.send(Event::StreamError(e));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Event::StreamError(e.to_string()));
            }
        }
    });

    Ok(())
}

// ── Agentic tool execution loop ───────────────────────────────────────────────

/// Execute all pending tool calls from the model's last response, then
/// feed results back to the model and continue streaming.
/// Capped at 15 iterations per user turn to prevent infinite loops.
async fn execute_pending_tools(app: &mut App) -> anyhow::Result<()> {
    const MAX_ITERATIONS: u8 = 25;

    if app.tool_iterations >= MAX_ITERATIONS {
        app.pending_tool_calls.clear();
        app.push_toast(crate::ui::components::toast::Toast::warning(
            format!("Tool limit ({} calls) reached — stopping agent loop", MAX_ITERATIONS)
        ));
        return Ok(());
    }

    let calls = std::mem::take(&mut app.pending_tool_calls);
    if calls.is_empty() { return Ok(()); }

    app.tool_iterations += 1;
    app.streaming = true; // keep UI in "busy" state while tools run

    let mut result_parts: Vec<String> = Vec::new();

    // Determine which tools are allowed by current agent
    let plan_mode_tools: Option<Vec<String>> = {
        // Check custom agent first
        let custom = app.custom_agents.iter().find(|a| a.id == app.current_agent);
        if let Some(c) = custom {
            c.allowed_tools.as_ref().map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        } else {
            crate::tools::get_builtin_agent(&app.current_agent)
                .and_then(|a| a.allowed_tools)
                .map(|v| v.iter().map(|s| s.to_string()).collect())
        }
    };

    let mut i = 0;
    while i < calls.len() {
        let call = &calls[i];
        // Plan-mode: block write/shell tools
        if let Some(ref allowed) = plan_mode_tools {
            if !allowed.contains(&call.name) {
                app.push_toast(crate::ui::components::toast::Toast::warning(
                    format!("Plan mode: '{}' is not allowed. Switch to Build agent to execute.", call.name)
                ));
                let blocked_result = format!(
                    "<tool_result>\n<name>{}</name>\n<status>error</status>\n<output>\nBlocked by Plan mode. This agent cannot run '{}'. The user must switch to the Build agent.\n</output>\n</tool_result>",
                    call.name, call.name
                );
                result_parts.push(blocked_result);
                i += 1;
                continue;
            }
        }

        // ── Change review for write_file / edit_file ──────────────────────────
        // Batch a run of consecutive edit calls into one reviewable changeset and
        // hand off to the full-screen reviewer. Non-edit calls resume after review.
        if call.name == "write_file" || call.name == "edit_file" {
            let mut entries: Vec<ChangeEntry> = Vec::new();
            let mut j = i;
            while j < calls.len() && (calls[j].name == "write_file" || calls[j].name == "edit_file") {
                if let Some(entry) = build_change_entry(&calls[j], &app.working_dir) {
                    entries.push(entry);
                }
                j += 1;
            }
            if entries.is_empty() {
                i = j;
                continue;
            }
            let remaining: Vec<_> = calls[j..].to_vec();
            app.changeset = Some(Changeset {
                entries,
                remaining_calls: remaining,
                results: Vec::new(),
            });
            app.review_open   = true;
            app.review_view   = ReviewView::Overview;
            app.review_cursor = 0;
            app.review_scroll = 0;
            app.scroll_stick_bottom = true;
            app.streaming = false;
            return Ok(());
        }

        // Show plan progress in the toast if a plan is active
        let toast_label = if !app.active_plan.is_empty() && call.name != "make_plan" {
            app.plan_step += 1;
            let step_label = app.active_plan.get(app.plan_step.saturating_sub(1))
                .map(|s| format!(": {}", s))
                .unwrap_or_default();
            format!("⚙  {} [step {}/{}{}]", call.name, app.plan_step, app.active_plan.len(), step_label)
        } else {
            format!("⚙  {}", call.name)
        };
        app.push_toast(crate::ui::components::toast::Toast::info(toast_label));

        // Execute — route shell through sandbox if enabled
        let result = if call.name == "shell" && app.sandbox_enabled {
            let r = crate::tools::shell::execute_sandboxed(&call.parameters, &app.working_dir).await;
            match r {
                Ok(o)  => crate::tools::ToolResult { call_id: call.id.clone(), name: call.name.clone(), output: o, error: None, is_error: false },
                Err(e) => crate::tools::ToolResult { call_id: call.id.clone(), name: call.name.clone(), output: String::new(), error: Some(e.to_string()), is_error: true },
            }
        } else {
            crate::tools::execute(call, &app.working_dir, &app.http_client, &app.db).await
        };

        let (status, content) = if result.is_error {
            ("error", result.error.unwrap_or_else(|| "Unknown error".to_string()))
        } else {
            ("ok", result.output)
        };

        // Detect git auth failures and open the token setup dialog
        if content.contains("[AUTH_REQUIRED:") {
            if let Some(start) = content.find("[AUTH_REQUIRED:") {
                let rest = &content[start + "[AUTH_REQUIRED:".len()..];
                if let Some(end) = rest.find(']') {
                    app.git_token_host = rest[..end].to_string();
                    open_dialog(app, ActiveDialog::GitToken);
                }
            }
        }

        // If make_plan was called, store the steps in app state
        if call.name == "make_plan" && status == "ok" {
            if let Some(steps) = call.parameters.get("steps").and_then(|v| v.as_array()) {
                app.active_plan = steps.iter()
                    .filter_map(|s| s.as_str().map(|t| t.to_string()))
                    .collect();
                app.plan_step = 0;
                // Clear history when a new plan starts
                app.tool_history.clear();
            }
        }

        // Record in rolling tool history (skip make_plan itself — the plan panel handles it)
        if call.name != "make_plan" {
            app.tool_history.push((call.name.clone(), status == "error"));
            if app.tool_history.len() > 12 {
                app.tool_history.remove(0);
            }
        }

        result_parts.push(format!(
            "<tool_result>\n<name>{}</name>\n<status>{}</status>\n<output>\n{}\n</output>\n</tool_result>",
            call.name, status, content.trim()
        ));
        i += 1;
    }

    let results_text = result_parts.join("\n\n");

    // Store tool results as a user message so they appear in history
    let tool_msg = Message::new_user(&app.session_id, &results_text);
    crate::db::insert_message(&app.db, &tool_msg)?;
    app.messages.push(tool_msg);

    app.streaming_buf.clear();

    // Re-build the full chat and send back to the model
    let system = crate::tools::build_agent_system_prompt(
        &app.working_dir,
        if app.project_context_active { app.project_ctx.as_ref() } else { None },
        &app.current_agent,
        &app.custom_agents,
    );

    let mut chat: Vec<ChatMessage> = vec![
        ChatMessage { role: "system".to_string(), content: system },
    ];
    for msg in &app.messages {
        let role = match msg.role {
            Role::User      => "user",
            Role::Assistant => "assistant",
        };
        let content = msg.text_content();
        if !content.is_empty() {
            chat.push(ChatMessage { role: role.to_string(), content });
        }
    }

    // If a multi-step plan is active and there are steps left, inject an explicit
    // continuation directive into the model context (NOT saved to DB / not shown in UI).
    // This prevents the model from stopping after each step and waiting.
    if !app.active_plan.is_empty() && app.plan_step < app.active_plan.len() {
        let next_step = app.active_plan[app.plan_step].clone();
        let remaining = app.active_plan.len() - app.plan_step;
        let tool_hint = suggest_tool_for_step(&next_step);
        chat.push(ChatMessage {
            role: "user".to_string(),
            content: format!(
                "[Step {}/{} of plan — {} step(s) left]\n\
                 Next: {}.\n\
                 Please use the `{}` tool for this step.",
                app.plan_step + 1,
                app.active_plan.len(),
                remaining,
                next_step,
                tool_hint,
            ),
        });
    }

    let model_id    = app.current_model.clone();
    let tx          = app.event_tx.clone();
    let http_client = app.http_client.clone();

    // Keep low temperature and stop sequences for the follow-up turn
    let last_user_text = app.messages.iter().rev()
        .find(|m| m.role == Role::User && !m.text_content().starts_with("<tool_result>"))
        .map(|m| m.text_content())
        .unwrap_or_default();
    let params = action_params(&last_user_text);

    // DirectGguf path for tool follow-ups
    let model_backend = app.available_models.iter()
        .find(|m| m.id == model_id)
        .map(|m| m.backend.clone());

    if model_backend == Some(crate::providers::BackendKind::DirectGguf) {
        let client   = http_client.clone();
        let hardware = app.hardware.clone();
        tokio::spawn(async move {
            // Server should already be up from initial message, but show status just in case
            let alive = client
                .get("http://127.0.0.1:18080/health")
                .timeout(std::time::Duration::from_millis(300))
                .send().await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !alive {
                let _ = tx.send(Event::StreamStatus("  Starting inference server…".to_string()));
            }

            let provider = crate::providers::direct::DirectGgufProvider::new(client, hardware);
            match crate::providers::LocalProvider::chat_stream(&provider, &chat, &model_id, &params).await {
                Ok(mut rx) => {
                    let _ = tx.send(Event::StreamStatus("  Generating…".to_string()));
                    let start = std::time::Instant::now();
                    while let Some(ev) = rx.recv().await {
                        match ev {
                            StreamEvent::Text(t)      => { let _ = tx.send(Event::StreamText(t)); }
                            StreamEvent::Reasoning(r) => { let _ = tx.send(Event::StreamReasoning(r)); }
                            StreamEvent::Done { .. } => {
                                let ms = start.elapsed().as_millis() as u64;
                                let _ = tx.send(Event::StreamDone { duration_ms: ms });
                                break;
                            }
                            StreamEvent::Error(e) => {
                                let _ = tx.send(Event::StreamError(e));
                                break;
                            }
                        }
                    }
                }
                Err(e) => { let _ = tx.send(Event::StreamError(e.to_string())); }
            }
        });
        return Ok(());
    }

    let (base_url, backend_kind) = app.provider_registry
        .backend_for_model(&model_id, &app.available_models);

    if base_url.is_empty() {
        app.streaming = false;
        app.push_toast(crate::ui::components::toast::Toast::error("No backend for follow-up"));
        return Ok(());
    }

    tokio::spawn(async move {
        match crate::providers::stream_for_backend(
            &http_client,
            &base_url,
            backend_kind,
            &chat,
            &model_id,
            &params,
        ).await {
            Ok(mut rx) => {
                let start = std::time::Instant::now();
                while let Some(ev) = rx.recv().await {
                    match ev {
                        StreamEvent::Text(t) => { let _ = tx.send(Event::StreamText(t)); }
                        StreamEvent::Reasoning(r) => { let _ = tx.send(Event::StreamReasoning(r)); }
                        StreamEvent::Done { .. } => {
                            let ms = start.elapsed().as_millis() as u64;
                            let _ = tx.send(Event::StreamDone { duration_ms: ms });
                            break;
                        }
                        StreamEvent::Error(e) => {
                            let _ = tx.send(Event::StreamError(e));
                            break;
                        }
                    }
                }
            }
            Err(e) => { let _ = tx.send(Event::StreamError(e.to_string())); }
        }
    });

    Ok(())
}

/// Map a plan step description to the most appropriate tool name.
/// Used to inject a concrete tool hint into plan continuation messages.
fn suggest_tool_for_step(step: &str) -> &'static str {
    let s = step.to_lowercase();
    if s.contains("write") || s.contains("save") || s.contains("output") || s.contains("create file")
        || s.contains("summary") || s.contains("guide") || s.contains("report") || s.contains("document") {
        "write_file"
    } else if s.contains("batch") || s.contains("scan") || s.contains("multiple files")
        || s.contains("all files") || s.contains("each file") {
        "batch_read"
    } else if s.contains("tree") || s.contains("structure") || s.contains("layout") || s.contains("map") {
        "tree"
    } else if s.contains("grep") || s.contains("search") || s.contains("find pattern") || s.contains("look for") {
        "grep"
    } else if s.contains("glob") || s.contains("list all") || s.contains("find files") {
        "glob"
    } else if s.contains("read") || s.contains("open") || s.contains("examine") || s.contains("inspect")
        || s.contains("look at") || s.contains("check") || s.contains("review") || s.contains("analyze") {
        "batch_read"
    } else if s.contains("list") || s.contains("dir") || s.contains("directory") || s.contains("folder") {
        "list_dir"
    } else if s.contains("shell") || s.contains("run") || s.contains("execute") || s.contains("build") || s.contains("test") {
        "shell"
    } else {
        "read_file"
    }
}

/// Fix 1 — Tool enforcer: model said it would act but issued no tool calls.
/// Inject a short correction message and re-stream with low temperature.
/// Pull a likely target filename (e.g. "test.py") out of a free-text request.
fn extract_target_filename(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || "`\"'(),:;".contains(c))
        .map(|t| t.trim_matches(|c: char| c == '`' || c == '.'))
        .find(|t| {
            match t.rfind('.') {
                Some(dot) if dot > 0 && dot + 1 < t.len() => {
                    let ext  = &t[dot + 1..];
                    let stem = &t[..dot];
                    ext.len() <= 5
                        && ext.chars().all(|c| c.is_ascii_alphanumeric())
                        && stem.chars().all(|c| c.is_ascii_alphanumeric() || "_-/.".contains(c))
                }
                _ => false,
            }
        })
        .map(|s| s.to_string())
}

async fn fire_tool_enforcer(app: &mut App) -> anyhow::Result<()> {
    if app.tool_iterations >= 15 { return Ok(()); }
    app.tool_iterations += 1;

    // Did the model just print code instead of saving it?
    let last_assistant_had_code = app.messages.iter().rev()
        .find(|m| m.role == Role::Assistant)
        .map(|m| m.text_content().contains("```"))
        .unwrap_or(false);

    // Build a targeted correction — if a plan is active, name the exact next step.
    let correction = if !app.active_plan.is_empty() && app.plan_step < app.active_plan.len() {
        let next = &app.active_plan[app.plan_step].clone();
        let tool_hint = suggest_tool_for_step(next);
        format!(
            "Please continue with step {}/{} of the plan: \"{}\". \
             Use the `{}` tool to complete this step.",
            app.plan_step + 1, app.active_plan.len(), next, tool_hint
        )
    } else if last_assistant_had_code {
        // The model wrote code but didn't save it — and small models often name the
        // tool after the FILE ("<name>test.py</name>"). Give an exact template.
        let fname = app.messages.iter().rev()
            .filter(|m| m.role == Role::User && !m.hidden)
            .find(|m| !m.text_content().starts_with("<tool_result>"))
            .and_then(|m| extract_target_filename(&m.text_content()))
            .unwrap_or_else(|| "the_file.py".to_string());
        format!(
            "You wrote code but did not save it. Reply with ONLY this tool call and nothing else. \
             The tool name must be exactly write_file — NOT the filename:\n\
             <tool_call>\n<name>write_file</name>\n<parameters>{{\"path\": \"{}\", \"content\": \"<full code here>\"}}</parameters>\n</tool_call>",
            fname
        )
    } else {
        "Please go ahead and call the appropriate tool to complete this task. Respond with only the tool call.".to_string()
    };
    let mut enforcer_msg = Message::new_user(&app.session_id, &correction);
    enforcer_msg.hidden = true;
    crate::db::insert_message(&app.db, &enforcer_msg)?;
    app.messages.push(enforcer_msg);

    app.streaming = true;
    app.streaming_buf.clear();

    let system = crate::tools::build_agent_system_prompt(
        &app.working_dir, if app.project_context_active { app.project_ctx.as_ref() } else { None }, &app.current_agent, &app.custom_agents,
    );
    let mut chat: Vec<ChatMessage> = vec![ChatMessage { role: "system".to_string(), content: system }];
    for msg in &app.messages {
        let role = match msg.role { Role::User => "user", Role::Assistant => "assistant" };
        let content = msg.text_content();
        if !content.is_empty() { chat.push(ChatMessage { role: role.to_string(), content }); }
    }

    let model_id    = app.current_model.clone();
    let tx          = app.event_tx.clone();
    let http_client = app.http_client.clone();

    let mut params = GenerationParams::default();
    params.temperature = Some(0.1);
    params.stop = vec!["</tool_call>".to_string()];

    // DirectGguf models need the same special path as submit_message /
    // execute_pending_tools — find_for_model falls back to the first HTTP
    // provider (LlamaCpp:8080) for path-based model IDs, which is wrong.
    let model_backend = app.available_models.iter()
        .find(|m| m.id == model_id)
        .map(|m| m.backend.clone());

    if model_backend == Some(crate::providers::BackendKind::DirectGguf) {
        let client   = http_client.clone();
        let hardware = app.hardware.clone();
        tokio::spawn(async move {
            let provider = crate::providers::direct::DirectGgufProvider::new(client, hardware);
            match crate::providers::LocalProvider::chat_stream(&provider, &chat, &model_id, &params).await {
                Ok(mut rx) => {
                    let start = std::time::Instant::now();
                    while let Some(ev) = rx.recv().await {
                        match ev {
                            StreamEvent::Text(t)      => { let _ = tx.send(Event::StreamText(t)); }
                            StreamEvent::Reasoning(r) => { let _ = tx.send(Event::StreamReasoning(r)); }
                            StreamEvent::Done { .. } => {
                                let _ = tx.send(Event::StreamDone { duration_ms: start.elapsed().as_millis() as u64 });
                                break;
                            }
                            StreamEvent::Error(e) => { let _ = tx.send(Event::StreamError(e)); break; }
                        }
                    }
                }
                Err(e) => { let _ = tx.send(Event::StreamError(e.to_string())); }
            }
        });
        return Ok(());
    }

    let (base_url, backend_kind) = app.provider_registry
        .backend_for_model(&model_id, &app.available_models);

    if base_url.is_empty() { app.streaming = false; return Ok(()); }

    tokio::spawn(async move {
        match crate::providers::stream_for_backend(
            &http_client, &base_url, backend_kind,
            &chat, &model_id, &params,
        ).await {
            Ok(mut rx) => {
                let start = std::time::Instant::now();
                while let Some(ev) = rx.recv().await {
                    match ev {
                        StreamEvent::Text(t)      => { let _ = tx.send(Event::StreamText(t)); }
                        StreamEvent::Reasoning(r) => { let _ = tx.send(Event::StreamReasoning(r)); }
                        StreamEvent::Done { .. } => {
                            let _ = tx.send(Event::StreamDone { duration_ms: start.elapsed().as_millis() as u64 });
                            break;
                        }
                        StreamEvent::Error(e) => { let _ = tx.send(Event::StreamError(e)); break; }
                    }
                }
            }
            Err(e) => { let _ = tx.send(Event::StreamError(e.to_string())); }
        }
    });
    Ok(())
}

/// Build a reviewable change entry from a write_file / edit_file call.
fn build_change_entry(call: &crate::tools::ToolCall, cwd: &std::path::Path) -> Option<ChangeEntry> {
    use crate::session::message::DiffLineKind;
    let (file_path, proposed, old_text, new_text) = extract_diff_info(call, cwd);
    let fp = file_path?;
    let existed = std::fs::read_to_string(&fp).map(|s| !s.is_empty()).unwrap_or(false);
    let diff_lines = compute_diff_lines(&fp, &proposed, old_text.as_deref(), new_text.as_deref());
    let added   = diff_lines.iter().filter(|d| d.kind == DiffLineKind::Added).count();
    let removed = diff_lines.iter().filter(|d| d.kind == DiffLineKind::Removed).count();
    Some(ChangeEntry {
        call:      call.clone(),
        file_path: fp,
        tool_name: call.name.clone(),
        is_new:    !existed,
        diff_lines,
        added,
        removed,
        status:    ChangeStatus::Pending,
    })
}

/// Key handling for the full-screen change reviewer.
async fn handle_review_key(app: &mut App, key: crossterm::event::KeyEvent) -> anyhow::Result<bool> {
    use crossterm::event::KeyCode;

    // Quit is always honored
    if let Some(crate::keybinds::Action::Quit) = app.keybinds.resolve(&key).cloned() {
        let _ = crate::config::save(&app.config);
        return Ok(true);
    }

    let file_count = app.changeset.as_ref().map(|c| c.entries.len()).unwrap_or(0);
    if file_count == 0 { app.review_open = false; return Ok(false); }

    match app.review_view {
        ReviewView::Overview => match key.code {
            KeyCode::Up   | KeyCode::Char('k') => {
                app.review_cursor = app.review_cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.review_cursor + 1 < file_count { app.review_cursor += 1; }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                app.review_view = ReviewView::File;
                app.review_scroll = 0;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => { apply_changeset_all(app).await?; }
            KeyCode::Char('d') | KeyCode::Char('D') => { discard_changeset_all(app).await?; }
            KeyCode::Esc => { app.review_open = false; } // defer — Ctrl+R reopens
            _ => {}
        },
        ReviewView::File => match key.code {
            KeyCode::Up   | KeyCode::Char('k') => {
                app.review_scroll = app.review_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.review_scroll = app.review_scroll.saturating_add(1);
            }
            KeyCode::Char('[') | KeyCode::Left | KeyCode::Char('h') => {
                app.review_cursor = app.review_cursor.saturating_sub(1);
                app.review_scroll = 0;
            }
            KeyCode::Char(']') | KeyCode::Right | KeyCode::Char('l') => {
                if app.review_cursor + 1 < file_count { app.review_cursor += 1; app.review_scroll = 0; }
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let idx = app.review_cursor;
                apply_changeset_file(app, idx).await?;
                advance_review_cursor(app);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let idx = app.review_cursor;
                skip_changeset_file(app, idx);
                maybe_finalize_changeset(app).await?;
                advance_review_cursor(app);
            }
            KeyCode::Char('a') | KeyCode::Char('A') => { apply_changeset_all(app).await?; }
            KeyCode::Esc => { app.review_view = ReviewView::Overview; }
            _ => {}
        },
    }
    Ok(false)
}

/// After resolving a file, jump the cursor to the next still-pending file.
fn advance_review_cursor(app: &mut App) {
    if let Some(cs) = app.changeset.as_ref() {
        if let Some(next) = cs.first_pending() {
            app.review_cursor = next;
            app.review_scroll = 0;
        }
    }
}

/// Execute one file's write, record the receipt, mark it applied.
async fn apply_changeset_file(app: &mut App, idx: usize) -> anyhow::Result<()> {
    let call = {
        let Some(cs) = app.changeset.as_ref() else { return Ok(()); };
        let Some(entry) = cs.entries.get(idx) else { return Ok(()); };
        if entry.status != ChangeStatus::Pending { return Ok(()); }
        entry.call.clone()
    };

    let result = crate::tools::execute(&call, &app.working_dir, &app.http_client, &app.db).await;
    let is_error = result.is_error;
    let output   = if is_error { result.error.clone().unwrap_or_default() } else { result.output.clone() };

    let tool_result_str = format!(
        "<tool_result>\n<name>{}</name>\n<status>{}</status>\n<output>\n{}\n</output>\n</tool_result>",
        call.name, if is_error { "error" } else { "ok" }, output,
    );
    // The tool trace line on the message acts as the river receipt.
    append_diff_result_to_message(app, &call.id, &call.name, &tool_result_str, is_error);

    if let Some(cs) = app.changeset.as_mut() {
        if let Some(entry) = cs.entries.get_mut(idx) {
            entry.status = if is_error { ChangeStatus::Skipped } else { ChangeStatus::Applied };
            app.changes_log.push(ChangeLogEntry {
                file_path: entry.file_path.clone(),
                added:     entry.added,
                removed:   entry.removed,
                status:    entry.status,
            });
        }
        cs.results.push(tool_result_str);
    }

    maybe_finalize_changeset(app).await
}

/// Mark one file skipped and record a declined result for the model.
fn skip_changeset_file(app: &mut App, idx: usize) {
    if let Some(cs) = app.changeset.as_mut() {
        if let Some(entry) = cs.entries.get_mut(idx) {
            if entry.status != ChangeStatus::Pending { return; }
            entry.status = ChangeStatus::Skipped;
            let declined = format!(
                "<tool_result>\n<name>{}</name>\n<status>error</status>\n<output>\nUser skipped the proposed change to {}. Do not retry this write.\n</output>\n</tool_result>",
                entry.tool_name, entry.file_path,
            );
            app.changes_log.push(ChangeLogEntry {
                file_path: entry.file_path.clone(),
                added:     entry.added,
                removed:   entry.removed,
                status:    ChangeStatus::Skipped,
            });
            cs.results.push(declined);
        }
    }
}

/// Apply every still-pending file, then finalize.
async fn apply_changeset_all(app: &mut App) -> anyhow::Result<()> {
    loop {
        let next = app.changeset.as_ref().and_then(|c| c.first_pending());
        match next {
            Some(idx) => { apply_changeset_file(app, idx).await?; }
            None => break,
        }
        if app.changeset.is_none() { break; } // finalized mid-loop
    }
    Ok(())
}

/// Skip every still-pending file (decline the batch), then finalize.
async fn discard_changeset_all(app: &mut App) -> anyhow::Result<()> {
    let pending: Vec<usize> = app.changeset.as_ref()
        .map(|c| c.entries.iter().enumerate()
            .filter(|(_, e)| e.status == ChangeStatus::Pending)
            .map(|(i, _)| i).collect())
        .unwrap_or_default();
    for idx in pending { skip_changeset_file(app, idx); }
    maybe_finalize_changeset(app).await
}

/// If no files remain pending, push results back to the model and resume.
async fn maybe_finalize_changeset(app: &mut App) -> anyhow::Result<()> {
    let done = app.changeset.as_ref().map(|c| c.pending_count() == 0).unwrap_or(false);
    if !done { return Ok(()); }

    let Some(cs) = app.changeset.take() else { return Ok(()); };
    app.review_open = false;
    app.review_view = ReviewView::Overview;

    let applied = cs.entries.iter().filter(|e| e.status == ChangeStatus::Applied).count();
    let skipped = cs.entries.iter().filter(|e| e.status == ChangeStatus::Skipped).count();

    if applied > 0 {
        let a: usize = cs.entries.iter().filter(|e| e.status == ChangeStatus::Applied).map(|e| e.added).sum();
        let r: usize = cs.entries.iter().filter(|e| e.status == ChangeStatus::Applied).map(|e| e.removed).sum();
        app.push_toast(crate::ui::components::toast::Toast::success(
            format!("Applied {} file{} · +{} −{}", applied, if applied == 1 { "" } else { "s" }, a, r),
        ));
    } else if skipped > 0 {
        app.push_toast(crate::ui::components::toast::Toast::info(
            format!("Discarded {} change{}", skipped, if skipped == 1 { "" } else { "s" }),
        ));
    }

    // Resume any non-edit calls the model queued after the edits.
    app.pending_tool_calls = cs.remaining_calls;
    // Edits were declined → nudge the model to reconsider rather than re-write.
    app.tool_enforcer_pending = applied == 0 && skipped > 0 && app.pending_tool_calls.is_empty();
    app.streaming = false;
    Ok(())
}

/// Append a tool result for a diff call directly to the last assistant message's Tool part.
fn append_diff_result_to_message(app: &mut App, call_id: &str, name: &str, output: &str, is_error: bool) {
    use crate::session::message::{Part, ToolPart, ToolState};

    if let Some(msg) = app.messages.iter_mut().rev()
        .find(|m| m.role == crate::session::message::Role::Assistant)
    {
        // Find existing Tool part for this call and update it, or add a new one
        let existing = msg.parts.iter_mut().find_map(|p| {
            if let Part::Tool(tp) = p { if tp.call_id == call_id { return Some(tp); } }
            None
        });
        if let Some(tp) = existing {
            tp.state  = if is_error { ToolState::Error } else { ToolState::Complete };
            tp.output = if is_error { None } else { Some(output.to_string()) };
            tp.error  = if is_error { Some(output.to_string()) } else { None };
        } else {
            let mut tp = ToolPart::new(call_id, name);
            tp.state  = if is_error { ToolState::Error } else { ToolState::Complete };
            tp.output = if is_error { None } else { Some(output.to_string()) };
            tp.error  = if is_error { Some(output.to_string()) } else { None };
            msg.parts.push(Part::Tool(tp));
        }
        let _ = crate::db::update_message_parts(&app.db, msg);
    }
}

/// Extract file path + proposed content from a write_file or edit_file call.
fn extract_diff_info(
    call: &crate::tools::ToolCall,
    cwd: &std::path::Path,
) -> (Option<String>, String, Option<String>, Option<String>) {
    use std::path::PathBuf;

    let resolve = |p: &str| -> String {
        let path = if p.starts_with('~') {
            crate::startup::real_home_dir().join(p.trim_start_matches("~/"))
        } else {
            let pb = PathBuf::from(p);
            if pb.is_absolute() { pb } else { cwd.join(p) }
        };
        path.to_string_lossy().to_string()
    };

    if call.name == "write_file" {
        let path = call.parameters["path"].as_str().map(|p| resolve(p));
        let content = call.parameters["content"].as_str().unwrap_or("").to_string();
        (path, content, None, None)
    } else if call.name == "edit_file" {
        let path = call.parameters["path"].as_str().map(|p| resolve(p));
        let old = call.parameters["old_text"].as_str().unwrap_or("").to_string();
        let new = call.parameters["new_text"].as_str().unwrap_or("").to_string();
        let content = if let Some(ref p) = path {
            std::fs::read_to_string(p).unwrap_or_default()
                .replacen(&old, &new, 1)
        } else { new.clone() };
        (path, content, Some(old), Some(new))
    } else {
        (None, String::new(), None, None)
    }
}

/// Compute colored diff lines between current file and proposed content.
fn compute_diff_lines(
    file_path: &str,
    proposed:  &str,
    old_text:  Option<&str>,
    new_text:  Option<&str>,
) -> Vec<crate::session::message::DiffLine> {
    use similar::{TextDiff, ChangeTag};
    use crate::session::message::{DiffLine, DiffLineKind};

    let current = std::fs::read_to_string(file_path).unwrap_or_default();

    // For edit_file, only diff the changed region; for write_file, diff the whole file
    let (old_str, new_str) = if let (Some(old), Some(new)) = (old_text, new_text) {
        (old.to_string(), new.to_string())
    } else {
        (current.clone(), proposed.to_string())
    };

    let mut lines = vec![];
    let label = if current.is_empty() { "new file" } else { "modified" };
    lines.push(DiffLine { kind: DiffLineKind::Header, content: format!("── {} ──", label) });

    let diff = TextDiff::from_lines(old_str.as_str(), new_str.as_str());
    let mut eq_count = 0usize;

    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Delete => { eq_count = 0; DiffLineKind::Removed }
            ChangeTag::Insert => { eq_count = 0; DiffLineKind::Added }
            ChangeTag::Equal  => {
                eq_count += 1;
                if eq_count > 3 { continue; }
                DiffLineKind::Context
            }
        };
        let content = change.value().trim_end_matches('\n').to_string();
        lines.push(DiffLine { kind, content });
    }

    if lines.len() == 1 {
        lines.push(DiffLine { kind: DiffLineKind::Header, content: "(no visible changes)".to_string() });
    }

    lines
}


fn load_session(app: &mut App, session_id: String) -> anyhow::Result<()> {
    app.session_id = session_id.clone();
    app.messages = crate::db::load_messages(&app.db, &session_id)?;
    app.scroll_to_bottom();
    Ok(())
}

async fn new_session(app: &mut App) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let session = Session::new(&app.current_model, "local", &cwd);
    crate::db::insert_session(&app.db, &session)?;
    app.sessions.insert(0, session.clone());
    app.session_id = session.id;
    app.messages.clear();
    app.scroll_to_bottom();
    app.push_toast(crate::ui::components::toast::Toast::success("New session"));
    Ok(())
}

async fn delete_session(app: &mut App) -> anyhow::Result<()> {
    if app.sessions.len() <= 1 {
        app.push_toast(crate::ui::components::toast::Toast::warning("Cannot delete last session"));
        return Ok(());
    }
    crate::db::delete_session(&app.db, &app.session_id)?;
    app.sessions.retain(|s| s.id != app.session_id);
    if let Some(s) = app.sessions.first() {
        let id = s.id.clone();
        load_session(app, id)?;
    }
    app.push_toast(crate::ui::components::toast::Toast::success("Session deleted"));
    Ok(())
}

async fn fork_session(app: &mut App) -> anyhow::Result<()> {
    let parent_id = app.session_id.clone();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut new_session = Session::new(&app.current_model, "local", &cwd);
    let parent_title = app.sessions.iter()
        .find(|s| s.id == parent_id)
        .map(|s| s.title.as_str())
        .unwrap_or("session")
        .to_string();
    new_session.title = format!("Fork of {}", parent_title);
    crate::db::insert_session(&app.db, &new_session)?;
    for msg in app.messages.clone() {
        let mut m = msg.clone();
        m.session_id = new_session.id.clone();
        crate::db::insert_message(&app.db, &m)?;
    }
    app.sessions.insert(0, new_session.clone());
    app.session_id = new_session.id;
    app.push_toast(crate::ui::components::toast::Toast::success("Session forked"));
    Ok(())
}

async fn undo_message(app: &mut App) -> anyhow::Result<()> {
    let mut removed: Vec<Message> = Vec::new();

    if let Some(last) = app.messages.last() {
        if last.role == Role::Assistant {
            let ts = last.created_at;
            crate::db::delete_messages_from(&app.db, &app.session_id, ts)?;
            removed.push(app.messages.pop().unwrap());
        }
    }
    if let Some(last) = app.messages.last() {
        if last.role == Role::User {
            let ts = last.created_at;
            crate::db::delete_messages_from(&app.db, &app.session_id, ts)?;
            removed.push(app.messages.pop().unwrap());
        }
    }

    if !removed.is_empty() {
        removed.reverse(); // preserve original order for redo
        app.undo_stack.push(removed);
        app.push_toast(crate::ui::components::toast::Toast::info("Undid last exchange  (Ctrl+Y to redo)"));
    }
    Ok(())
}

async fn redo_message(app: &mut App) -> anyhow::Result<()> {
    if let Some(msgs) = app.undo_stack.pop() {
        for msg in &msgs {
            let mut restored = msg.clone();
            restored.session_id = app.session_id.clone();
            crate::db::insert_message(&app.db, &restored)?;
            app.messages.push(restored);
        }
        app.push_toast(crate::ui::components::toast::Toast::info("Redid last exchange"));
    }
    Ok(())
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn stash_draft(app: &mut App) -> anyhow::Result<()> {
    let text: String = app.textarea.lines().join("\n");
    let text = text.trim().to_string();
    if text.is_empty() {
        app.push_toast(crate::ui::components::toast::Toast::warning("Nothing to stash"));
        return Ok(());
    }
    let now = chrono::Utc::now();
    let label = now.format("%b %d %H:%M").to_string();
    let draft = crate::db::DraftRow {
        id:         ulid::Ulid::new().to_string(),
        label,
        content:    text,
        created_at: now.timestamp(),
    };
    crate::db::insert_draft(&app.db, &draft)?;
    app.drafts.insert(0, draft);
    app.textarea = tui_textarea::TextArea::default();
    app.push_toast(crate::ui::components::toast::Toast::success("Draft stashed  (Ctrl+Shift+D to restore)"));
    Ok(())
}

fn save_agent_editor(app: &mut App) -> anyhow::Result<()> {
    let name = app.agent_editor_name.trim().to_string();
    if name.is_empty() {
        app.push_toast(crate::ui::components::toast::Toast::error("Agent name is required"));
        return Ok(());
    }
    let system_text = app.agent_editor_system.lines().join("\n").trim().to_string();
    let now = chrono::Utc::now().timestamp();
    let agent = crate::db::AgentRow {
        id:            app.agent_editor_id.clone().unwrap_or_else(|| ulid::Ulid::new().to_string()),
        name:          name.clone(),
        description:   if app.agent_editor_desc.is_empty() { None } else { Some(app.agent_editor_desc.clone()) },
        model:         None,
        provider:      None,
        system:        if system_text.is_empty() { None } else { Some(system_text) },
        allowed_tools: None,
        created_at:    now,
    };
    crate::db::insert_agent(&app.db, &agent)?;
    // Refresh custom agents list
    if let Some(existing) = app.custom_agents.iter_mut().find(|a| a.id == agent.id) {
        *existing = agent.clone();
    } else {
        app.custom_agents.push(agent.clone());
    }
    app.push_toast(crate::ui::components::toast::Toast::success(format!("Agent '{}' saved", name)));
    Ok(())
}

fn paste_clipboard(app: &mut App) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        if let Ok(text) = cb.get_text() {
            for ch in text.chars() {
                if ch == '\n' { app.textarea.insert_newline(); }
                else { app.textarea.insert_char(ch); }
            }
        }
    }
}

fn cycle_history(app: &mut App, dir: i64) {
    if app.input_history.is_empty() { return; }
    let new_idx = match app.history_idx {
        None if dir < 0 => Some(app.input_history.len() - 1),
        None => return,
        Some(i) => {
            let ni = i as i64 + dir;
            if ni < 0 { None }
            else { Some((ni as usize).min(app.input_history.len() - 1)) }
        }
    };
    app.history_idx = new_idx;
    app.textarea = TextArea::default();
    if let Some(idx) = new_idx {
        let text = app.input_history[idx].clone();
        for ch in text.chars() {
            if ch == '\n' { app.textarea.insert_newline(); }
            else { app.textarea.insert_char(ch); }
        }
    }
}

fn copy_last_message(app: &mut App) {
    let text = app.messages.iter().rev()
        .find(|m| m.role == Role::Assistant)
        .map(|m| m.text_content())
        .unwrap_or_default();
    if text.is_empty() { return; }
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text);
        app.push_toast(crate::ui::components::toast::Toast::success("Copied to clipboard"));
    }
}

/// Copy the last fenced code block from the most recent assistant message.
fn copy_last_code_block(app: &mut App) {
    let text = app.messages.iter().rev()
        .find(|m| m.role == Role::Assistant)
        .map(|m| m.text_content())
        .unwrap_or_default();

    // Walk fenced ``` blocks, keep the last one's body
    let mut last: Option<String> = None;
    let mut in_block = false;
    let mut buf = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_block {
                last = Some(std::mem::take(&mut buf));
                in_block = false;
            } else {
                in_block = true;
                buf.clear();
            }
        } else if in_block {
            buf.push_str(line);
            buf.push('\n');
        }
    }

    match last {
        Some(code) if !code.trim().is_empty() => {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(code.trim_end().to_string());
                app.push_toast(crate::ui::components::toast::Toast::success("Code block copied"));
            }
        }
        _ => app.push_toast(crate::ui::components::toast::Toast::warning("No code block found")),
    }
}

/// Spawn a background task to download `model` via Ollama's /api/pull.
/// Download a GGUF model file directly from HuggingFace to ~/.hyperlite/models/.
/// Progress events arrive via `app.event_tx` as `Event::ModelDownload*`.
fn start_model_download(app: &mut App, filename: String, url: String, display: String) {
    let models_dir = crate::startup::models_dir();
    let tmp        = models_dir.join(format!("{}.part", &filename));

    // How many bytes are already on disk from a previous interrupted download
    let partial_bytes = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);

    app.model_dl_active      = Some(filename.clone());
    app.model_dl_bytes_done  = partial_bytes;   // start progress bar where we left off
    app.model_dl_bytes_total = 0;
    app.model_dl_speed_bps   = 0.0;

    // Use a separate client with no timeout — large model downloads can take hours.
    // The shared http_client has a 120-second timeout which would kill mid-download.
    let client = match reqwest::Client::builder()
        // No request timeout — large model downloads can take hours.
        // connect_timeout only limits the initial TCP handshake.
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            app.push_toast(crate::ui::components::toast::Toast::error(format!("Download client error: {}", e)));
            return;
        }
    };
    let tx = app.event_tx.clone();

    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        use futures::StreamExt;

        if let Err(e) = tokio::fs::create_dir_all(&models_dir).await {
            let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
            return;
        }

        let dest = models_dir.join(&filename);

        // Send Range header when resuming so we don't re-download what we have
        let mut req = client.get(&url);
        if partial_bytes > 0 {
            req = req.header("Range", format!("bytes={}-", partial_bytes));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
                return;
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let _ = tx.send(Event::ModelDownloadFailed {
                model: display, error: format!("HTTP {}", status)
            });
            return;
        }

        // 206 Partial Content = server honoured Range; 200 = no range support, full restart
        let resuming      = status.as_u16() == 206 && partial_bytes > 0;
        let content_len   = resp.content_length().unwrap_or(0);
        let total         = if resuming { partial_bytes + content_len } else { content_len };

        // Open file in append mode when resuming, fresh create otherwise
        let mut file = if resuming {
            match tokio::fs::OpenOptions::new().append(true).open(&tmp).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
                    return;
                }
            }
        } else {
            match tokio::fs::File::create(&tmp).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
                    return;
                }
            }
        };

        let mut stream = resp.bytes_stream();
        let mut done   = if resuming { partial_bytes } else { 0u64 };

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c)  => c,
                Err(e) => {
                    // Keep .part file intact — next attempt will resume from here
                    let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
                    return;
                }
            };
            if let Err(e) = file.write_all(&chunk).await {
                let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
                return;
            }
            done += chunk.len() as u64;
            let _ = tx.send(Event::ModelDownloadProgress {
                model: filename.clone(), bytes_done: done, bytes_total: total
            });
        }

        drop(file);
        if let Err(e) = tokio::fs::rename(&tmp, &dest).await {
            let _ = tx.send(Event::ModelDownloadFailed { model: display, error: e.to_string() });
            return;
        }

        let _ = tx.send(Event::ModelDownloadDone { model: display, filename: filename.clone() });
    });
}

fn cycle_model(app: &mut App, dir: i64) {
    if app.available_models.is_empty() { return; }
    let cur = app.available_models.iter().position(|m| m.id == app.current_model).unwrap_or(0);
    let n   = app.available_models.len() as i64;
    let nxt = ((cur as i64 + dir).rem_euclid(n)) as usize;
    app.current_model = app.available_models[nxt].id.clone();
    let name = app.available_models[nxt].name.clone();
    app.push_toast(crate::ui::components::toast::Toast::info(format!("Model: {}", name)));
}

fn cycle_theme(app: &mut App, dir: i64) {
    let names = crate::ui::theme::all_names();
    let cur = names.iter().position(|n| *n == app.config.theme.as_str()).unwrap_or(0);
    let n   = names.len() as i64;
    let nxt = ((cur as i64 + dir).rem_euclid(n)) as usize;
    let name = names[nxt];
    app.theme = crate::ui::theme::get(name);
    app.config.theme = name.to_string();
    let _ = crate::config::save(&app.config);
    app.push_toast(crate::ui::components::toast::Toast::info(format!("Theme: {}", name)));
}

fn begin_rename_session(app: &mut App) {
    // Pre-fill the textarea with the current session title so the user can edit it inline.
    // On next submit the text will become the new session title (handled in submit_message via
    // a one-shot rename flag — for now we just open the editor with a prefilled prompt).
    let title = app.sessions.iter()
        .find(|s| s.id == app.session_id)
        .map(|s| s.title.clone())
        .unwrap_or_default();
    app.textarea = tui_textarea::TextArea::default();
    for ch in title.chars() { app.textarea.insert_char(ch); }
    app.push_toast(crate::ui::components::toast::Toast::info(
        "Edit title above and press Enter to rename"
    ));
    app.active_prompt = ActivePrompt::Rename;
}

async fn compact_session(app: &mut App) -> anyhow::Result<()> {
    if app.messages.is_empty() {
        app.push_toast(crate::ui::components::toast::Toast::warning("No messages to compact"));
        return Ok(());
    }
    if app.is_streaming() {
        app.push_toast(crate::ui::components::toast::Toast::warning("Wait for generation to finish"));
        return Ok(());
    }

    // Build a compaction prompt from the full history
    let history: String = app.messages.iter().map(|m| {
        let role = match m.role { Role::User => "User", Role::Assistant => "Assistant" };
        format!("{}: {}\n", role, m.text_content())
    }).collect();

    let prompt = format!(
        "Summarize this conversation concisely in under 300 words, preserving all key decisions, \
         code, and facts. Write as a factual summary, not a dialogue.\n\n{}",
        history
    );

    let (base_url, backend_kind) = app.provider_registry
        .backend_for_model(&app.current_model, &app.available_models);
    if base_url.is_empty() {
        app.push_toast(crate::ui::components::toast::Toast::error("No backend for compaction"));
        return Ok(());
    }

    let model_id    = app.current_model.clone();
    let session_id  = app.session_id.clone();
    let tx          = app.event_tx.clone();
    let http_client = app.http_client.clone();
    let chat = vec![
        crate::providers::ChatMessage { role: "user".to_string(), content: prompt },
    ];

    app.streaming = true;
    app.streaming_buf.clear();
    app.push_toast(crate::ui::components::toast::Toast::info("Compacting session…"));

    tokio::spawn(async move {
        let params = crate::providers::GenerationParams::default();
        match crate::providers::stream_for_backend(
            &http_client, &base_url, backend_kind,
            &chat, &model_id, &params,
        ).await {
            Ok(mut rx) => {
                let start = std::time::Instant::now();
                let mut summary = String::new();
                while let Some(ev) = rx.recv().await {
                    match ev {
                        crate::providers::StreamEvent::Text(t) => { summary.push_str(&t); }
                        crate::providers::StreamEvent::Done { .. } => {
                            let ms = start.elapsed().as_millis() as u64;
                            let _ = tx.send(Event::CompactDone { summary, session_id });
                            break;
                        }
                        crate::providers::StreamEvent::Error(e) => {
                            let _ = tx.send(Event::StreamError(e));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => { let _ = tx.send(Event::StreamError(e.to_string())); }
        }
    });

    Ok(())
}

/// Convert a chat history into a single prompt string for llama.cpp.
///
/// Uses a generic instruction-following template that works with most GGUF models.
/// The system prompt is wrapped in <<SYS>> tags; user/assistant turns alternate as
/// [INST]…[/INST] pairs (Llama-2 chat format, widely supported).
fn format_chat_as_prompt(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    let mut system_injected = false;

    // Collect system prompt first
    let system = messages.iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    for msg in messages {
        match msg.role.as_str() {
            "system" => {} // handled below, injected into first user turn
            "user" => {
                if !system_injected && !system.is_empty() {
                    out.push_str(&format!(
                        "[INST] <<SYS>>\n{}\n<</SYS>>\n\n{} [/INST]",
                        system, msg.content
                    ));
                    system_injected = true;
                } else {
                    out.push_str(&format!("[INST] {} [/INST]", msg.content));
                }
            }
            "assistant" => {
                out.push_str(&format!(" {} </s><s>", msg.content));
            }
            _ => {}
        }
    }
    out
}

async fn open_external_editor(app: &mut App) -> anyhow::Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let tmp = std::env::temp_dir().join("hyperlite_input.md");
    let current = app.textarea.lines().join("\n");
    std::fs::write(&tmp, &current)?;

    crossterm::terminal::disable_raw_mode()?;
    let status = std::process::Command::new(&editor).arg(&tmp).status()?;
    crossterm::terminal::enable_raw_mode()?;

    if status.success() {
        let content = std::fs::read_to_string(&tmp)?;
        app.textarea = TextArea::default();
        for line in content.lines() {
            for ch in line.chars() { app.textarea.insert_char(ch); }
            app.textarea.insert_newline();
        }
    }
    Ok(())
}


/// Install bubblewrap using the available package manager, streaming output as events.
async fn stream_bwrap_install(tx: tokio::sync::mpsc::UnboundedSender<crate::event::Event>, password: String) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let attempts: &[(&str, &[&str], bool)] = &[
        ("apt-get", &["sudo", "-S", "apt-get", "install", "-y", "bubblewrap"], true),
        ("brew",    &["brew", "install", "bubblewrap"],                         false),
        ("pacman",  &["sudo", "-S", "pacman", "-S", "--noconfirm", "bubblewrap"], true),
        ("dnf",     &["sudo", "-S", "dnf", "install", "-y", "bubblewrap"],     true),
        ("zypper",  &["sudo", "-S", "zypper", "install", "-y", "bubblewrap"],  true),
    ];

    for (mgr, cmd, needs_sudo) in attempts {
        if which::which(mgr).is_ok() {
            let _ = tx.send(crate::event::Event::BwrapInstallLine(
                format!("Using {} to install bubblewrap…", mgr)
            ));

            let mut child = match tokio::process::Command::new(cmd[0])
                .args(&cmd[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(crate::event::Event::BwrapInstallLine(format!("Failed to start: {}", e)));
                    let _ = tx.send(crate::event::Event::BwrapInstallDone(false));
                    return;
                }
            };

            // Pipe password to sudo -S stdin
            if *needs_sudo && !password.is_empty() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(format!("{}\n", password).as_bytes()).await;
                }
            } else {
                drop(child.stdin.take());
            }

            if let Some(stdout) = child.stdout.take() {
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let clean = strip_ansi_simple(&line);
                        if !clean.trim().is_empty() {
                            let _ = tx2.send(crate::event::Event::BwrapInstallLine(clean));
                        }
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let clean = strip_ansi_simple(&line);
                        if !clean.trim().is_empty() {
                            let _ = tx2.send(crate::event::Event::BwrapInstallLine(clean));
                        }
                    }
                });
            }

            let success = match child.wait().await {
                Ok(s) => s.success(),
                Err(_) => false,
            };
            let _ = tx.send(crate::event::Event::BwrapInstallDone(success));
            return;
        }
    }

    let _ = tx.send(crate::event::Event::BwrapInstallLine("No supported package manager found.".to_string()));
    let _ = tx.send(crate::event::Event::BwrapInstallLine("Run manually: sudo apt install bubblewrap".to_string()));
    let _ = tx.send(crate::event::Event::BwrapInstallDone(false));
}

async fn stream_pentest_install(
    tx:       tokio::sync::mpsc::UnboundedSender<crate::event::Event>,
    password: String,
    tools:    Vec<String>,
    env:      crate::pentest::EnvironmentReport,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use crate::pentest::{tool_def, PackageManager, EnvironmentType};

    let is_kali = matches!(env.env_type, EnvironmentType::NativeKali | EnvironmentType::KaliWSL2);

    let mgr = match &env.package_manager {
        Some(PackageManager::Apt)    => "apt",
        Some(PackageManager::Dnf)    => "dnf",
        Some(PackageManager::Yum)    => "yum",
        Some(PackageManager::Pacman) => "pacman",
        _ => "apt",
    };

    // Classify each selected tool into an install strategy
    enum Strategy {
        AptPkg(String),
        Pip3Pkg(String),
        GoInstall(String),
        Special(String),
        ManualOnly(String),  // can't auto-install, show instructions
    }

    let mut steps: Vec<(String, Strategy)> = vec![];

    for name in &tools {
        // golang-go is a synthetic dependency, not in ALL_TOOLS
        if name == "golang-go" {
            steps.push((name.clone(), Strategy::AptPkg("golang-go".to_string())));
            continue;
        }

        let def = match tool_def(name) {
            Some(d) => d,
            None    => { steps.push((name.clone(), Strategy::ManualOnly(format!("unknown tool: {}", name)))); continue; }
        };

        // Determine the apt package name for this distro/package manager
        let apt_pkg = match &env.package_manager {
            Some(PackageManager::Apt)    => def.apt,
            Some(PackageManager::Dnf)    => def.dnf,
            Some(PackageManager::Yum)    => def.dnf,
            Some(PackageManager::Pacman) => def.pacman,
            _ => def.apt,
        };

        let can_use_apt = apt_pkg.is_some() && (is_kali || !def.apt_kali_only);

        if can_use_apt {
            steps.push((name.clone(), Strategy::AptPkg(apt_pkg.unwrap().to_string())));
        } else if let Some(go) = def.go_pkg {
            steps.push((name.clone(), Strategy::GoInstall(go.to_string())));
        } else if let Some(pip) = def.pip3 {
            steps.push((name.clone(), Strategy::Pip3Pkg(pip.to_string())));
        } else if let Some(special) = def.special {
            steps.push((name.clone(), Strategy::Special(special.to_string())));
        } else if def.apt_kali_only && apt_pkg.is_some() {
            // Kali-only tool with no Ubuntu alternative
            let msg = format!(
                "{} is only available in Kali repos. On Ubuntu, add the Kali repo or install manually.",
                name
            );
            steps.push((name.clone(), Strategy::ManualOnly(msg)));
        } else {
            steps.push((name.clone(), Strategy::ManualOnly(format!("no install method found for {} on this system", name))));
        }
    }

    let mut any_success = false;

    // Run each step individually — failures are isolated
    for (name, strategy) in &steps {
        match strategy {
            Strategy::ManualOnly(msg) => {
                let _ = tx.send(crate::event::Event::PenTestInstallLine(format!("⚠  {}", msg)));
            }
            Strategy::AptPkg(pkg) => {
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    format!("installing {}…  (sudo {} install -y {})", name, mgr, pkg)
                ));
                let ok = run_sudo_command(
                    &tx, &password,
                    &["sudo", "-S", mgr, "install", "-y", pkg]
                ).await;
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    if ok { format!("✓  {}", name) } else { format!("✗  {} — install failed (check log)", name) }
                ));
                if ok {
                    any_success = true;
                    // After golang installs, add ~/go/bin to PATH for subsequent go install calls
                    if name == "golang-go" {
                        let home = std::env::var("HOME").unwrap_or_default();
                        let go_bin = format!("{}/go/bin", home);
                        let current = std::env::var("PATH").unwrap_or_default();
                        if !current.contains(&go_bin) {
                            std::env::set_var("PATH", format!("{}:{}", go_bin, current));
                        }
                    }
                }
            }
            Strategy::Pip3Pkg(pkg) => {
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    format!("installing {} via pip3…", name)
                ));
                let ok = run_command(&tx, &["pip3", "install", "--user", pkg]).await;
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    if ok { format!("✓  {}", name) } else { format!("✗  {} pip3 install failed", name) }
                ));
                if ok { any_success = true; }
            }
            Strategy::GoInstall(pkg) => {
                // Check go is available
                let go_path = which::which("go").is_ok();
                if !go_path {
                    let _ = tx.send(crate::event::Event::PenTestInstallLine(
                        format!("⚠  {} requires Go — install Go first: sudo apt install golang", name)
                    ));
                    continue;
                }
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    format!("installing {} via go install…", name)
                ));
                let ok = run_command(&tx, &["go", "install", "-v", pkg]).await;
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    if ok { format!("✓  {} (may need to add ~/go/bin to PATH)", name) }
                    else  { format!("✗  {} go install failed", name) }
                ));
                if ok { any_success = true; }
            }
            Strategy::Special(cmd) => {
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    format!("installing {} via special method…", name)
                ));
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    format!("$ {}", cmd)
                ));
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.is_empty() { continue; }

                // Special commands may need sudo — pipe password if sudo is first token
                let ok = if parts[0] == "sudo" {
                    run_sudo_command(&tx, &password, &parts).await
                } else {
                    run_command(&tx, &parts).await
                };
                let _ = tx.send(crate::event::Event::PenTestInstallLine(
                    if ok { format!("✓  {}", name) } else { format!("✗  {} failed — see log", name) }
                ));
                if ok { any_success = true; }
            }
        }
    }

    let _ = tx.send(crate::event::Event::PenTestBatchInstallDone(any_success));
}

/// Run a non-sudo command, stream output, return success.
async fn run_command(
    tx:   &tokio::sync::mpsc::UnboundedSender<crate::event::Event>,
    args: &[&str],
) -> bool {
    use tokio::io::AsyncBufReadExt;
    if args.is_empty() { return false; }

    let mut child = match tokio::process::Command::new(args[0])
        .args(&args[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(crate::event::Event::PenTestInstallLine(format!("  error: {}", e)));
            return false;
        }
    };

    stream_child_output(&mut child, tx).await;
    child.wait().await.map(|s| s.success()).unwrap_or(false)
}

/// Run a sudo command with password piped to stdin.
async fn run_sudo_command(
    tx:       &tokio::sync::mpsc::UnboundedSender<crate::event::Event>,
    password: &str,
    args:     &[&str],
) -> bool {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    if args.is_empty() { return false; }

    let mut child = match tokio::process::Command::new(args[0])
        .args(&args[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(crate::event::Event::PenTestInstallLine(format!("  error: {}", e)));
            return false;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{}\n", password).as_bytes()).await;
    }

    stream_child_output(&mut child, tx).await;
    child.wait().await.map(|s| s.success()).unwrap_or(false)
}

async fn stream_child_output(
    child: &mut tokio::process::Child,
    tx:    &tokio::sync::mpsc::UnboundedSender<crate::event::Event>,
) {
    use tokio::io::AsyncBufReadExt;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(out) = stdout {
        let t = tx.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let clean = strip_ansi_simple(&line);
                if !clean.trim().is_empty() {
                    let _ = t.send(crate::event::Event::PenTestInstallLine(format!("  {}", clean)));
                }
            }
        });
    }
    if let Some(err) = stderr {
        let t = tx.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let clean = strip_ansi_simple(&line);
                if !clean.trim().is_empty() {
                    let _ = t.send(crate::event::Event::PenTestInstallLine(format!("  {}", clean)));
                }
            }
        });
    }
}

fn strip_ansi_simple(s: &str) -> String {
    let stripped = strip_ansi_escapes::strip(s.as_bytes());
    String::from_utf8(stripped).unwrap_or_else(|_| s.to_string())
}

// ── Pen test mode ─────────────────────────────────────────────────────────────

fn start_engagement(app: &mut App) {
    let exclusions: Vec<String> = app.pentest_setup_exclusions
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut spec = crate::pentest::EngagementSpec::default();
    spec.scope      = vec![app.pentest_setup_target.trim().to_string()];
    spec.exclusions = exclusions;
    spec.depth      = app.pentest_setup_depth.clone();

    let _ = spec.save();

    app.pentest_hosts.clear();
    app.pentest_raw_output.clear();
    app.pentest_evidence.clear();
    app.pentest_selected_host  = 0;
    app.pentest_raw_tab        = false;
    app.pentest_phase          = crate::pentest::EngagementPhase::Recon;
    app.pentest_scan_progress  = crate::pentest::ScanProgress {
        command: format!("nmap on {}", spec.scope.first().cloned().unwrap_or_default()),
        running: true,
        ..Default::default()
    };

    let spec_clone = spec.clone();
    let env        = app.pentest_env.clone().unwrap_or_else(|| crate::pentest::env::detect());
    let tx         = app.event_tx.clone();

    app.pentest_engagement = Some(spec);
    close_dialog(app);
    app.pentest_mode = true;

    tokio::spawn(async move {
        crate::pentest::runner::run_recon(tx, spec_clone, env).await;
    });
}

fn enter_pentest_mode(app: &mut App) {
    // Detect environment immediately (fast — reads /proc/version + /etc/os-release)
    let env = crate::pentest::env::detect();
    app.pentest_env = Some(env);

    // Reset auth gate state and open it
    app.pentest_auth_phase = crate::pentest::AuthPhase::BlackOut;
    app.pentest_auth_tick  = 0;
    app.pentest_auth_input.clear();
    app.pentest_auth_flash = 0;
    open_dialog(app, ActiveDialog::PenTestAuth);
}

fn launch_pentest_inventory(app: &mut App) {
    let env = match &app.pentest_env {
        Some(e) => e.clone(),
        None    => return,
    };
    let tx = app.event_tx.clone();

    // Mark all tools as "checking" so the pre-flight renders immediately
    app.pentest_inventory    = crate::pentest::ToolInventory::new_checking();
    app.pentest_inv_complete = false;

    tokio::spawn(async move {
        // Run checks in a blocking thread — just file existence, ~2ms per tool
        let results = tokio::task::spawn_blocking(move || {
            crate::pentest::ALL_TOOLS.iter().map(|def| {
                let status = crate::pentest::check_tool(def, &env);
                (def.name.to_string(), status)
            }).collect::<Vec<_>>()
        }).await.unwrap_or_default();

        for (name, status) in results {
            let available = status.is_available();
            let path = if let crate::pentest::ToolStatus::Available { path: p } = &status {
                Some(p.clone())
            } else {
                None
            };
            let _ = tx.send(crate::event::Event::PenTestToolChecked { name, available, path });
        }
        let _ = tx.send(crate::event::Event::PenTestInventoryComplete);
    });
}
