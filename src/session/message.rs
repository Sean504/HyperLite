use serde::{Deserialize, Serialize};
use ulid::Ulid;

pub type SessionId = String;
pub type MessageId = String;
pub type PartId    = String;

// ── Session ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id:          SessionId,
    pub title:       String,
    pub model_id:    String,
    pub provider_id: String,
    pub cwd:         String,
    pub parent_id:   Option<SessionId>,
    pub created_at:  i64,
    pub updated_at:  i64,
}

impl Session {
    pub fn new(model_id: &str, provider_id: &str, cwd: &str) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id:          Ulid::new().to_string(),
            title:       "New Session".into(),
            model_id:    model_id.into(),
            provider_id: provider_id.into(),
            cwd:         cwd.into(),
            parent_id:   None,
            created_at:  now,
            updated_at:  now,
        }
    }

    pub fn is_default_title(&self) -> bool {
        self.title == "New Session" || self.title.is_empty()
    }
}

// ── Message ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id:          MessageId,
    pub session_id:  SessionId,
    pub role:        Role,
    pub parts:       Vec<Part>,
    pub model:       Option<String>,
    pub duration_ms: Option<u64>,
    pub created_at:  i64,
}

impl Message {
    pub fn new_user(session_id: &str, text: &str) -> Self {
        Self {
            id:          Ulid::new().to_string(),
            session_id:  session_id.into(),
            role:        Role::User,
            parts:       vec![Part::Text(TextPart::new(text))],
            model:       None,
            duration_ms: None,
            created_at:  chrono::Utc::now().timestamp(),
        }
    }

    pub fn new_assistant(session_id: &str) -> Self {
        Self {
            id:          Ulid::new().to_string(),
            session_id:  session_id.into(),
            role:        Role::Assistant,
            parts:       vec![],
            model:       None,
            duration_ms: None,
            created_at:  chrono::Utc::now().timestamp(),
        }
    }

    /// Return all text joined (for copy/export)
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.text.as_str()),
                _             => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Find the first TextPart index for streaming append
    pub fn first_text_idx(&self) -> Option<usize> {
        self.parts.iter().position(|p| matches!(p, Part::Text(_)))
    }
}

// ── Role ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

// ── Parts ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(ToolPart),
    File(FilePart),
}

// TextPart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    pub id:        PartId,
    pub text:      String,
    pub streaming: bool,
}

impl TextPart {
    pub fn new(text: &str) -> Self {
        Self { id: Ulid::new().to_string(), text: text.into(), streaming: false }
    }
    pub fn streaming(text: &str) -> Self {
        Self { id: Ulid::new().to_string(), text: text.into(), streaming: true }
    }
}

// ReasoningPart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPart {
    pub id:        PartId,
    pub text:      String,
    pub streaming: bool,
}

impl ReasoningPart {
    pub fn new() -> Self {
        Self { id: Ulid::new().to_string(), text: String::new(), streaming: true }
    }
}

impl Default for ReasoningPart {
    fn default() -> Self { Self::new() }
}

// ToolPart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPart {
    pub id:      PartId,
    pub call_id: String,
    pub name:    String,
    pub input:   serde_json::Value,
    pub state:   ToolState,
    pub output:  Option<String>,
    pub error:   Option<String>,
}

impl ToolPart {
    pub fn new(call_id: &str, name: &str) -> Self {
        Self {
            id:      Ulid::new().to_string(),
            call_id: call_id.into(),
            name:    name.into(),
            input:   serde_json::Value::Object(Default::default()),
            state:   ToolState::Pending,
            output:  None,
            error:   None,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self.name.as_str() {
            "bash" | "shell"        => "$",
            "read"                  => "→",
            "write"                 => "←",
            "edit"                  => "→",
            "glob"                  => "✱",
            "grep"                  => "✱",
            "list"                  => "→",
            "webfetch" | "fetch"    => "%",
            "websearch" | "search"  => "◈",
            "codesearch"            => "◇",
            "task" | "todowrite"    => "□",
            _                       => "⚙",
        }
    }

    pub fn pending_text(&self) -> &'static str {
        match self.name.as_str() {
            "bash" | "shell"        => "Running command...",
            "read"                  => "Reading file...",
            "write"                 => "Writing file...",
            "edit"                  => "Editing file...",
            "glob"                  => "Finding files...",
            "grep"                  => "Searching...",
            "list"                  => "Listing directory...",
            "webfetch" | "fetch"    => "Fetching URL...",
            "websearch" | "search"  => "Searching web...",
            "codesearch"            => "Searching code...",
            "task" | "todowrite"    => "Updating tasks...",
            _                       => "Running tool...",
        }
    }

    pub fn display_title(&self) -> String {
        let arg = match self.name.as_str() {
            "bash" | "shell" => self
                .input.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "read" | "write" | "edit" | "list" => self
                .input.get("path")
                .or_else(|| self.input.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "glob" => self
                .input.get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "grep" => self
                .input.get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "webfetch" | "fetch" => self
                .input.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "websearch" | "search" => self
                .input.get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => self.input.to_string(),
        };
        format!("{} {}", self.icon(), arg)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Pending,
    Running,
    Complete,
    Error,
    AwaitingPermission,
    Denied,
}

impl ToolState {
    pub fn is_done(&self) -> bool {
        matches!(self, ToolState::Complete | ToolState::Error | ToolState::Denied)
    }
}

// FilePart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    pub id:       PartId,
    pub filename: String,
    pub mime:     String,
    #[serde(with = "serde_bytes_base64")]
    pub data:     Vec<u8>,
}

mod serde_bytes_base64 {
    use serde::{Deserializer, Serializer, Deserialize};
    use std::io::Read;

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        // simple hex encoding to avoid base64 dep
        let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        s.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}

// ── Permission Request ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id:        String,
    pub tool:      String,
    pub title:     String,
    pub detail:    String,
    pub diff:      Option<String>,
    pub file_path: Option<String>,
}

impl PermissionRequest {
    pub fn new(tool: &str, detail: &str) -> Self {
        Self {
            id:        Ulid::new().to_string(),
            tool:      tool.into(),
            title:     format!("Allow {} to run?", tool),
            detail:    detail.into(),
            diff:      None,
            file_path: None,
        }
    }
}
