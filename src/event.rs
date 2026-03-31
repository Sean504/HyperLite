use crossterm::event::{KeyEvent, MouseEvent};
use crate::session::message::PermissionRequest;

/// All events that flow through the application event loop.
#[derive(Debug)]
pub enum Event {
    // Terminal input
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),

    // Tick for animations / toast expiry
    Tick,

    // Streaming LLM response chunks
    StreamText(String),
    StreamReasoning(String),
    StreamToolStart { call_id: String, name: String },
    StreamToolInput(String),   // partial JSON
    StreamToolEnd,
    StreamDone { duration_ms: u64 },
    StreamError(String),

    // Tool execution results
    ToolOutput { call_id: String, output: String },
    ToolError  { call_id: String, error: String },

    // Permission system
    PermissionRequest(PermissionRequest),
    PermissionGranted { request_id: String },
    PermissionDenied  { request_id: String },

    // Session compaction result
    CompactDone { summary: String, session_id: String },

    // Model download progress (from model-picker Download tab)
    ModelDownloadProgress { model: String, bytes_done: u64, bytes_total: u64 },
    ModelDownloadDone     { model: String },
    ModelDownloadFailed   { model: String, error: String },

    // App lifecycle
    Quit,
}
