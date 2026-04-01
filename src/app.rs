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

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveDialog {
    None,
    SessionList,
    ModelPicker,
    Help,
    CommandPalette,
    ThemePicker,
    FolderInput,
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
    pub spinner:        Spinner,
    pub last_token_count: Option<u32>,

    // Providers / models
    pub provider_registry: Arc<ProviderRegistry>,
    pub available_models:  Vec<LocalModel>,
    pub current_model:     String,
    pub model_picker_tab:      usize,
    pub command_palette_tab:   usize,

    // Hardware
    pub hardware:       HardwareInfo,

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

    // Toast
    pub toast: Option<crate::ui::components::toast::Toast>,

    // Reqwest client (shared for streaming)
    pub http_client: reqwest::Client,

    // Event sender for streaming tasks → main loop
    pub event_tx: mpsc::UnboundedSender<Event>,

    // Agentic tool loop safety counter (reset on each user message)
    pub tool_iterations: u8,

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
        Event::StreamText(text) => {
            // Filter chat-template tokens that some models leak (e.g. Qwen2.5)
            let filtered = text
                .replace("<|im_end|>", "")
                .replace("<|endoftext|>", "");
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
                };
                let _ = crate::db::insert_message(&app.db, &msg);
                app.messages.push(msg);
                if !tool_calls.is_empty() {
                    app.pending_tool_calls = tool_calls;
                }
            }
            app.scroll_to_bottom();
        }
        Event::StreamError(err) => {
            app.streaming = false;
            app.streaming_buf.clear();
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
        Event::ModelDownloadDone { model } => {
            app.model_dl_active = None;
            app.model_dl_bytes_done  = 0;
            app.model_dl_bytes_total = 0;
            app.model_dl_speed_bps   = 0.0;
            app.model_refresh_pending = true;
            app.push_toast(crate::ui::components::toast::Toast::success(
                format!("Downloaded: {}", model)
            ));
        }
        Event::ModelDownloadFailed { model: _, error } => {
            app.model_dl_active = None;
            app.push_toast(crate::ui::components::toast::Toast::error(
                format!("Download failed: {}", error)
            ));
        }
        Event::Quit => return true,
        _ => {}
    }
    false
}

async fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> anyhow::Result<bool> {
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
        Quit => return Ok(true),

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
        RedoMessage   => {}
        CopyLastMessage => copy_last_message(app),
        CompactSession  => {}

        ParentSession | NextChild | PrevChild => {}

        ModelPicker    => open_dialog(app, ActiveDialog::ModelPicker),
        CycleModelNext => cycle_model(app, 1),
        CycleModelPrev => cycle_model(app, -1),
        CycleFavoriteNext | CycleFavoritePrev => {}
        AgentPicker    => {}

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
        Help           => open_dialog(app, ActiveDialog::Help),
        StatusView     => {}
        OpenFolder     => open_folder_browser(app),

        ExternalEditor => open_external_editor(app).await?,
        ThemePicker    => open_dialog(app, ActiveDialog::ThemePicker),
        ThemeCycleNext => cycle_theme(app, 1),
        ThemeCyclePrev => cycle_theme(app, -1),

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
                        apply_folder(app, path.clone());
                        app.folder_browser_path = path.clone();
                        app.folder_browser_entries = load_dir_entries(&path);
                        app.dialog_selected_idx = 0;
                        close_dialog(app);
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
                        apply_folder(app, path);
                        close_dialog(app);
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
    app.project_context_active = app.project_ctx.as_ref().map(|c| c.is_git).unwrap_or(false);
    app.push_toast(crate::ui::components::toast::Toast::success(format!("Opened: {}", name)));
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
                        || e.display.to_lowercase().contains(&q)
                        || e.name.contains(q.as_str()))
                    .collect();
                if let Some(entry) = entries.get(app.dialog_selected_idx) {
                    let already = app.available_models.iter()
                        .any(|m| m.name.starts_with(entry.name.split(':').next().unwrap_or(entry.name)));
                    if already {
                        app.push_toast(crate::ui::components::toast::Toast::success(
                            format!("{} is already installed", entry.display)
                        ));
                    } else if app.model_dl_active.is_none() {
                        start_model_download(app, entry.name.to_string());
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
                    let name = app.available_models.iter().find(|m| &m.id == id)
                        .map(|m| m.name.clone()).unwrap_or_default();
                    app.current_model = id.clone();
                    app.push_toast(crate::ui::components::toast::Toast::success(
                        format!("Model: {}", name)
                    ));
                }
                close_dialog(app);
            }
        }
        ActiveDialog::ThemePicker => {
            let names = crate::ui::theme::all_names();
            if let Some(name) = names.get(app.dialog_selected_idx) {
                app.theme = crate::ui::theme::get(name);
                app.config.theme = name.to_string();
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
                Some("Cycle Model Next")   => cycle_model(app, 1),
                Some("Toggle Sidebar")     => app.sidebar_open = !app.sidebar_open,
                Some("Toggle Thinking")    => app.show_thinking = !app.show_thinking,
                Some("Toggle Tool Details")=> app.show_tool_details = !app.show_tool_details,
                Some("Toggle Conceal")     => app.concealed = !app.concealed,
                Some("Pick Theme")         => open_dialog(app, ActiveDialog::ThemePicker),
                Some("Open in Editor")     => open_external_editor(app).await?,
                Some("Copy Last Response") => copy_last_message(app),
                Some("Undo Last Message")  => undo_message(app).await?,
                Some("Help")               => open_dialog(app, ActiveDialog::Help),
                Some("Open Folder")        => open_folder_browser(app),
                Some("Quit")               => return Ok(()),
                _ => {}
            }
        }
        ActiveDialog::FolderInput => {} // handled in handle_dialog_key
        _ => close_dialog(app),
    }
    Ok(())
}

// ── Session operations ────────────────────────────────────────────────────────

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

    // Build system prompt with full tool documentation
    let system = crate::tools::build_coding_system_prompt(
        &app.working_dir,
        app.project_ctx.as_ref(),
    );

    // Build chat messages for provider
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

    let model_id    = app.current_model.clone();
    let tx          = app.event_tx.clone();
    let http_client = app.http_client.clone();

    // ── Native inference path (zero HTTP overhead) ────────────────────────────
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
                            // Fallback notice — will drop to HTTP path on next send
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

    // ── HTTP / Ollama path ────────────────────────────────────────────────────
    let base_url_opt = app.provider_registry
        .find_for_model(&model_id)
        .map(|p| p.base_url().to_string());

    let base_url = match base_url_opt {
        Some(u) => u,
        None => {
            app.streaming = false;
            app.push_toast(crate::ui::components::toast::Toast::error("No backend available"));
            return Ok(());
        }
    };

    tokio::spawn(async move {
        let params = GenerationParams::default();
        match crate::providers::openai_compat::stream_chat(
            &http_client,
            &base_url,
            crate::providers::BackendKind::OpenAICompat,
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
    const MAX_ITERATIONS: u8 = 15;

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

    for call in &calls {
        // Toast so the user sees each tool firing
        app.push_toast(crate::ui::components::toast::Toast::info(
            format!("⚙  {}", call.name)
        ));

        // Execute (this is async for shell; synchronous for file ops)
        let result = crate::tools::execute(call, &app.working_dir, &app.http_client).await;

        let (status, content) = if result.is_error {
            ("error", result.error.unwrap_or_else(|| "Unknown error".to_string()))
        } else {
            ("ok", result.output)
        };

        // Use a human-readable format the model understands clearly
        result_parts.push(format!(
            "<tool_result>\n<name>{}</name>\n<status>{}</status>\n<output>\n{}\n</output>\n</tool_result>",
            call.name, status, content.trim()
        ));
    }

    let results_text = result_parts.join("\n\n");

    // Store tool results as a user message so they appear in history
    let tool_msg = Message::new_user(&app.session_id, &results_text);
    crate::db::insert_message(&app.db, &tool_msg)?;
    app.messages.push(tool_msg);

    app.streaming_buf.clear();

    // Re-build the full chat and send back to the model
    let system = crate::tools::build_coding_system_prompt(
        &app.working_dir,
        app.project_ctx.as_ref(),
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

    let model_id    = app.current_model.clone();
    let tx          = app.event_tx.clone();
    let http_client = app.http_client.clone();

    let base_url = app.provider_registry
        .find_for_model(&model_id)
        .map(|p| p.base_url().to_string())
        .unwrap_or_default();

    if base_url.is_empty() {
        app.streaming = false;
        app.push_toast(crate::ui::components::toast::Toast::error("No backend for follow-up"));
        return Ok(());
    }

    tokio::spawn(async move {
        let params = GenerationParams::default();
        match crate::providers::openai_compat::stream_chat(
            &http_client,
            &base_url,
            crate::providers::BackendKind::OpenAICompat,
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
    if let Some(last) = app.messages.last() {
        if last.role == Role::Assistant {
            let ts = last.created_at;
            crate::db::delete_messages_from(&app.db, &app.session_id, ts)?;
            app.messages.pop();
        }
    }
    if let Some(last) = app.messages.last() {
        if last.role == Role::User {
            let ts = last.created_at;
            crate::db::delete_messages_from(&app.db, &app.session_id, ts)?;
            app.messages.pop();
        }
    }
    app.push_toast(crate::ui::components::toast::Toast::info("Undid last exchange"));
    Ok(())
}

// ── Utilities ─────────────────────────────────────────────────────────────────

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

/// Spawn a background task to download `model` via Ollama's /api/pull.
/// Progress events arrive via `app.event_tx` as `Event::ModelDownload*`.
fn start_model_download(app: &mut App, model: String) {
    app.model_dl_active      = Some(model.clone());
    app.model_dl_bytes_done  = 0;
    app.model_dl_bytes_total = 0;
    app.model_dl_speed_bps   = 0.0;

    let client = app.http_client.clone();
    let tx     = app.event_tx.clone();
    let url    = "http://localhost:11434/api/pull".to_string();

    tokio::spawn(async move {
        let body = serde_json::json!({ "name": model, "stream": true });
        let resp = match client.post(&url).json(&body).send().await {
            Ok(r)  => r,
            Err(e) => {
                let _ = tx.send(Event::ModelDownloadFailed { model, error: e.to_string() });
                return;
            }
        };

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf    = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c)  => c,
                Err(e) => {
                    let _ = tx.send(Event::ModelDownloadFailed { model, error: e.to_string() });
                    return;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if line.is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    let status    = v["status"].as_str().unwrap_or("").to_string();
                    let total     = v["total"].as_u64().unwrap_or(0);
                    let completed = v["completed"].as_u64().unwrap_or(0);
                    if total > 0 {
                        let _ = tx.send(Event::ModelDownloadProgress {
                            model: model.clone(), bytes_done: completed, bytes_total: total
                        });
                    }
                    if status == "success" {
                        let _ = tx.send(Event::ModelDownloadDone { model });
                        return;
                    }
                }
            }
        }
        let _ = tx.send(Event::ModelDownloadDone { model });
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

    let base_url_opt = app.provider_registry
        .find_for_model(&app.current_model)
        .map(|p| p.base_url().to_string());
    let base_url = match base_url_opt {
        Some(u) => u,
        None => {
            app.push_toast(crate::ui::components::toast::Toast::error("No backend for compaction"));
            return Ok(());
        }
    };

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
        match crate::providers::openai_compat::stream_chat(
            &http_client, &base_url,
            crate::providers::BackendKind::OpenAICompat,
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
