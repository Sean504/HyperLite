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
    StreamStatus(String),   // transient phase label ("Starting server…", "Loading model…", "")
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
    ModelDownloadDone     { model: String, filename: String },
    ModelDownloadFailed   { model: String, error: String },

    // Background task result toast (is_error=true → error style)
    ToastMsg { text: String, is_error: bool },

    // Bwrap install progress
    BwrapInstallLine(String),
    BwrapInstallDone(bool), // true = success

    // Pen test mode — tool inventory (one event per tool so UI updates live)
    PenTestToolChecked { name: String, available: bool, path: Option<String> },
    PenTestInventoryComplete,
    // Pen test mode — tool install progress (same streaming pattern as bwrap)
    PenTestInstallLine(String),
    PenTestBatchInstallDone(bool),  // true = all steps succeeded

    // Pen test mode — recon phase runtime
    PenTestHostDiscovered(String),    // IP confirmed live (ping sweep)
    PenTestHostComplete(Box<crate::pentest::PentestHost>), // full host with ports
    PenTestScanProgress { percent: u8, found: u32, speed: String, eta: String, command: String },
    PenTestRawLine(String),           // raw tool output for the raw-output tab
    PenTestEvidenceLine { timestamp: String, text: String },
    PenTestReconComplete,

    // App lifecycle
    Quit,
}
