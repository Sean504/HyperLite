/// Tool System
///
/// Tools the model can call during inference.
/// Two invocation modes:
///   1. Native function calling — model emits JSON tool_calls (OpenAI format)
///   2. Tag-based parsing — model emits <tool>...</tool> blocks (any model)
///
/// Available tools:
///   search       — DuckDuckGo web search (no API key required)
///   read_file    — Read file contents from disk
///   write_file   — Write/create a file on disk
///   edit_file    — Search & replace within a file
///   list_dir     — List directory contents
///   grep         — Search file contents with regex
///   glob         — Find files by pattern
///   shell        — Execute shell command (with permission gate)
///   http_fetch   — Fetch a URL and return its content
///   calc         — Simple calculator expression evaluator

pub mod search;
pub mod files;
pub mod shell;
pub mod http;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Tool Definition ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name:        &'static str,
    pub description: &'static str,
    pub parameters:  &'static str,  // JSON Schema string
    pub requires_permission: bool,
}

/// All tool definitions — injected into system prompt for tag-based models.
pub static ALL_TOOLS: &[ToolDef] = &[
    ToolDef {
        name:        "make_plan",
        description: "Declare a multi-step plan before executing. Call this FIRST for any task that needs 3+ tool calls. List each step you will take. After calling this, immediately start executing step 1 without waiting.",
        parameters:  r#"{"type":"object","properties":{"title":{"type":"string","description":"Short task title"},"steps":{"type":"array","items":{"type":"string"},"description":"Ordered list of steps you will execute"}},"required":["steps"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "search",
        description: "Search the web using DuckDuckGo. Returns titles, snippets, and URLs.",
        parameters:  r#"{"type":"object","properties":{"query":{"type":"string","description":"The search query"}},"required":["query"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "read_file",
        description: "Read the contents of a file. Returns the file content as text.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"File path to read"}, "start_line":{"type":"integer"}, "end_line":{"type":"integer"}},"required":["path"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "write_file",
        description: "Write content to a file, creating it if it doesn't exist.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
        requires_permission: true,
    },
    ToolDef {
        name:        "edit_file",
        description: "Search and replace text in a file.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"}},"required":["path","old_text","new_text"]}"#,
        requires_permission: true,
    },
    ToolDef {
        name:        "list_dir",
        description: "List the contents of a directory.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"Directory path, default is current directory"}}}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "grep",
        description: "Search for a pattern in files. Returns matching lines with file paths and line numbers.",
        parameters:  r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"file_glob":{"type":"string"}},"required":["pattern"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "glob",
        description: "Find files matching a glob pattern.",
        parameters:  r#"{"type":"object","properties":{"pattern":{"type":"string"},"base_dir":{"type":"string"}},"required":["pattern"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "create_dir",
        description: "Create a directory (and any missing parent directories).",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"Directory path to create"}},"required":["path"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "delete_file",
        description: "Delete a file or directory. Use with caution.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"File or directory path to delete"}},"required":["path"]}"#,
        requires_permission: true,
    },
    ToolDef {
        name:        "move_file",
        description: "Move or rename a file or directory.",
        parameters:  r#"{"type":"object","properties":{"from":{"type":"string","description":"Source path"},"to":{"type":"string","description":"Destination path"}},"required":["from","to"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "copy_file",
        description: "Copy a file or directory to a new location.",
        parameters:  r#"{"type":"object","properties":{"from":{"type":"string","description":"Source path"},"to":{"type":"string","description":"Destination path"}},"required":["from","to"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "append_file",
        description: "Append text to the end of a file, creating it if it doesn't exist.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"File path"},"content":{"type":"string","description":"Text to append"}},"required":["path","content"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "file_info",
        description: "Get metadata about a file or directory: type, size, modified date.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"File or directory path"}},"required":["path"]}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "tree",
        description: "Show a recursive directory tree. Use to understand project structure quickly.",
        parameters:  r#"{"type":"object","properties":{"path":{"type":"string","description":"Root directory (default: current)"},"max_depth":{"type":"integer","description":"How deep to recurse (default: 3)"}}}"#,
        requires_permission: false,
    },
    ToolDef {
        name:        "shell",
        description: "Run a shell command and return stdout/stderr. Always ask the user before running destructive commands.",
        parameters:  r#"{"type":"object","properties":{"command":{"type":"string"},"working_dir":{"type":"string"}},"required":["command"]}"#,
        requires_permission: true,
    },
    ToolDef {
        name:        "http_fetch",
        description: "Fetch the content of a URL. Returns the text content of the page.",
        parameters:  r#"{"type":"object","properties":{"url":{"type":"string"},"extract_text":{"type":"boolean","default":true}},"required":["url"]}"#,
        requires_permission: false,
    },
];

/// Build the full system prompt for an agentic coding session.
/// Combines project context + working directory + comprehensive tool docs.
pub fn build_coding_system_prompt(
    cwd: &std::path::Path,
    project_ctx: Option<&crate::project::ProjectContext>,
) -> String {
    let mut out = String::new();

    out.push_str("You are HyperLite, an expert AI coding assistant with direct filesystem access.\n");
    out.push_str(&format!("Working directory: {}\n\n", cwd.display()));

    if let Some(ctx) = project_ctx {
        out.push_str(&crate::project::build_system_prefix(ctx));
        out.push_str("\n\n");
    }

    out.push_str(r#"## Tool Use

CRITICAL RULES:
1. To call a tool, output ONLY the raw XML block below — no markdown code fences, no backticks, no extra wrapping.
2. After emitting a <tool_call> block, STOP your response immediately. Do NOT write any text after the closing </tool_call> tag.
3. Do NOT predict or guess the output — wait. The system will execute the tool and send you a <tool_result> block.
4. When you receive a <tool_result>, read it and continue your response naturally.
5. NEVER output file contents as plain chat text. To create or save a file, ALWAYS use write_file. To show a user what you wrote, use read_file after writing.
6. JSON parameters MUST use \\n for newlines inside string values — never embed literal newlines in JSON strings.

Tool call format (emit this raw, no ``` wrapping):

<tool_call>
<name>tool_name</name>
<parameters>{"key": "value"}</parameters>
</tool_call>

Tool result format (what you will receive back):

<tool_result>
<name>tool_name</name>
<status>ok</status>
<output>...result...</output>
</tool_result>

One tool call per response. Wait for the result before calling another tool.

---

### read_file
Read a file. Always read before editing to see current content.
```
<tool_call>
<name>read_file</name>
<parameters>{"path": "src/main.rs"}</parameters>
</tool_call>
```
Optional: `"start_line": 10, "end_line": 50` to read a range.

### write_file
Create a new file or completely overwrite an existing one.
```
<tool_call>
<name>write_file</name>
<parameters>{"path": "src/utils.rs", "content": "// full file content here\n"}</parameters>
</tool_call>
```

### edit_file
Search and replace text in a file. The `old_text` must match exactly (whitespace included).
```
<tool_call>
<name>edit_file</name>
<parameters>{"path": "src/main.rs", "old_text": "fn hello() {}", "new_text": "fn hello() {\n    println!(\"hi\");\n}"}</parameters>
</tool_call>
```

### list_dir
List directory contents with sizes.
```
<tool_call>
<name>list_dir</name>
<parameters>{"path": "."}</parameters>
</tool_call>
```

### glob
Find files matching a pattern.
```
<tool_call>
<name>glob</name>
<parameters>{"pattern": "**/*.rs", "base_dir": "src"}</parameters>
</tool_call>
```

### grep
Search file contents with regex.
```
<tool_call>
<name>grep</name>
<parameters>{"pattern": "fn main", "path": "src", "file_glob": "*.rs"}</parameters>
</tool_call>
```

### create_dir
Create a directory (parents created automatically).
```
<tool_call>
<name>create_dir</name>
<parameters>{"path": "src/components/ui"}</parameters>
</tool_call>
```

### delete_file
Delete a file or directory tree.
```
<tool_call>
<name>delete_file</name>
<parameters>{"path": "old_file.rs"}</parameters>
</tool_call>
```

### move_file
Move or rename a file or directory.
```
<tool_call>
<name>move_file</name>
<parameters>{"from": "src/old_name.rs", "to": "src/new_name.rs"}</parameters>
</tool_call>
```

### shell
Run a shell command (30s timeout). Use for builds, tests, git, package managers.
```
<tool_call>
<name>shell</name>
<parameters>{"command": "cargo build 2>&1", "working_dir": "."}</parameters>
</tool_call>
```

### search
Search the web.
```
<tool_call>
<name>search</name>
<parameters>{"query": "rust tokio async channel example"}</parameters>
</tool_call>
```

### http_fetch
Fetch a URL (docs, APIs, etc.).
```
<tool_call>
<name>http_fetch</name>
<parameters>{"url": "https://docs.rs/tokio/latest/tokio/"}</parameters>
</tool_call>
```

---

## Coding Guidelines

- **Read before editing** — always read a file's current content before making changes
- **Explore first** — use `list_dir` and `glob` to understand the project structure before diving in
- **Verify after edits** — read the file back to confirm your changes applied correctly
- **Build and test** — after making code changes, run `shell` to build/test and fix any errors
- **Explain your work** — describe what you're doing and why before executing tools
- **Targeted edits** — use `edit_file` for small changes, `write_file` only when creating or fully replacing
- **Handle errors** — if a tool returns an error, diagnose and fix it rather than giving up
"#);

    out
}

// ── Built-in agent definitions ────────────────────────────────────────────────

pub struct BuiltinAgent {
    pub id:          &'static str,
    pub name:        &'static str,
    pub description: &'static str,
    pub allowed_tools: Option<&'static [&'static str]>,
}

pub static BUILTIN_AGENTS: &[BuiltinAgent] = &[
    BuiltinAgent {
        id:          "general",
        name:        "General",
        description: "Conversational assistant with full tool access. Best for chat, analysis, and general questions.",
        allowed_tools: None, // all tools
    },
    BuiltinAgent {
        id:          "build",
        name:        "Build",
        description: "Expert coding agent with full filesystem and shell access. Best for writing, editing, and building code.",
        allowed_tools: None, // all tools
    },
    BuiltinAgent {
        id:          "plan",
        name:        "Plan",
        description: "Read-only exploration agent. Can read and search files but will ask before writing or running commands.",
        allowed_tools: Some(&["make_plan", "read_file", "list_dir", "tree", "grep", "glob", "file_info", "search", "http_fetch"]),
    },
];

pub fn get_builtin_agent(id: &str) -> Option<&'static BuiltinAgent> {
    BUILTIN_AGENTS.iter().find(|a| a.id == id)
}

/// Build the system prompt for the currently active agent.
pub fn build_agent_system_prompt(
    cwd: &std::path::Path,
    project_ctx: Option<&crate::project::ProjectContext>,
    agent_id: &str,
    custom_agents: &[crate::db::AgentRow],
) -> String {
    // Check custom agents first (they override built-ins if same id)
    if let Some(custom) = custom_agents.iter().find(|a| a.id == agent_id) {
        let allowed: Option<Vec<&str>> = custom.allowed_tools.as_ref().map(|s| {
            s.split(',').map(|t| t.trim()).collect()
        });
        return build_prompt_with_config(
            cwd, project_ctx,
            custom.system.as_deref(),
            allowed.as_deref(),
            agent_id == "plan",
        );
    }

    // Built-in agents
    let is_plan = agent_id == "plan";
    let custom_system = match agent_id {
        "build" => Some("You are HyperLite Build — an expert software engineer with direct filesystem and shell access. Focus on writing correct, efficient code. Always read files before editing. Build and test after changes."),
        "plan"  => Some("You are HyperLite Plan — a read-only analysis agent. You can read files, search code, and explore the project. You CANNOT write files or run shell commands. Provide detailed analysis and plans. If the user asks you to make changes, explain what changes are needed but do not execute them."),
        _       => None,
    };

    let allowed = get_builtin_agent(agent_id).and_then(|a| a.allowed_tools);
    build_prompt_with_config(cwd, project_ctx, custom_system, allowed, is_plan)
}

fn build_prompt_with_config(
    cwd: &std::path::Path,
    project_ctx: Option<&crate::project::ProjectContext>,
    custom_system: Option<&str>,
    allowed_tools: Option<&[&str]>,
    is_plan: bool,
) -> String {
    let mut out = String::new();

    if let Some(sys) = custom_system {
        out.push_str(sys);
        out.push_str("\n");
    } else {
        out.push_str("You are HyperLite, an expert AI coding assistant with direct filesystem access.\n");
    }
    out.push_str(&format!("Working directory: {}\n\n", cwd.display()));

    if let Some(ctx) = project_ctx {
        out.push_str(&crate::project::build_system_prefix(ctx));
        out.push_str("\n\n");
    }

    if is_plan {
        out.push_str("## Mode: Plan (Read-Only)\nYou are in plan mode. You may read and search files. You may NOT write files, edit files, delete files, or run shell commands. Describe changes needed but do not execute them.\n\n");
    }

    // Build tool documentation for allowed tools only
    let tool_names: Vec<&str> = if let Some(allowed) = allowed_tools {
        allowed.to_vec()
    } else {
        ALL_TOOLS.iter().map(|t| t.name).collect()
    };

    out.push_str(r#"## Tool Use

CRITICAL RULES:
1. To call a tool, output ONLY the raw XML block below — no markdown fences, no backticks, no extra wrapping.
2. After emitting a <tool_call> block, STOP immediately. Do NOT write text after </tool_call>.
3. Do NOT predict or guess output — wait for the <tool_result> the system sends back.
4. After receiving a <tool_result>, immediately call the next tool if there are more steps. Do NOT summarize between steps.
5. NEVER output file contents as chat text. Use write_file to create files. Use read_file to confirm what was written.
6. JSON parameters MUST use \n for newlines — never embed literal newlines inside JSON strings.

Tool call format (emit raw, no ``` wrapping):

<tool_call>
<name>tool_name</name>
<parameters>{"key": "value"}</parameters>
</tool_call>

One tool call per message. After the result arrives, call the next tool immediately.

## Multi-Step Tasks

For tasks requiring 3 or more tool calls:
1. Call `make_plan` FIRST with every step you intend to take.
2. Then execute step 1 immediately (call the tool — do not describe it).
3. After each <tool_result>, call the NEXT tool right away — no commentary between steps.
4. Only write a summary response AFTER all steps are complete.

This ensures every step is informed by the actual result of the previous step.

---

"#);

    // Emit docs for each allowed tool
    for tool in ALL_TOOLS {
        if tool_names.contains(&tool.name) {
            out.push_str(&format!("### {}\n{}\n\n", tool.name, tool.description));
        }
    }

    out.push_str(r#"---

## Coding Guidelines

- **Read before editing** — always read a file's current content before making changes
- **Explore first** — use `tree` or `list_dir` and `glob` to understand project structure quickly
- **Verify after edits** — read the file back to confirm your changes applied correctly
- **Build and test** — after making code changes, run `shell` to build/test and fix errors
- **Targeted edits** — use `edit_file` for small changes, `write_file` only when creating or fully replacing
- **Appending** — use `append_file` to add to an existing file without overwriting it
- **Copying** — use `copy_file` to duplicate files or directories
- **Check before acting** — use `file_info` to verify a file exists and check its size before reading
"#);

    out
}

/// Build the tool use section for a system prompt (tag-based mode for models
/// that don't support native function calling).
pub fn build_tool_prompt(tools: &[&str]) -> String {
    let selected: Vec<&ToolDef> = ALL_TOOLS.iter()
        .filter(|t| tools.contains(&t.name))
        .collect();

    if selected.is_empty() { return String::new(); }

    let mut lines = vec![];
    lines.push("## Available Tools".to_string());
    lines.push("You can use tools by outputting XML-style tool calls. The user's system will execute them and provide results.".to_string());
    lines.push("Format:".to_string());
    lines.push("```".to_string());
    lines.push("<tool_call>".to_string());
    lines.push("<name>tool_name</name>".to_string());
    lines.push("<parameters>{\"key\": \"value\"}</parameters>".to_string());
    lines.push("</tool_call>".to_string());
    lines.push("```".to_string());
    lines.push(String::new());
    lines.push("### Tools:".to_string());

    for tool in selected {
        lines.push(format!("**{}**: {}", tool.name, tool.description));
    }

    lines.join("\n")
}

// ── Tool Call (parsed from model output) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id:         String,
    pub name:       String,
    pub parameters: serde_json::Value,
    pub source:     ToolCallSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallSource {
    /// Model emitted native JSON function call (OpenAI format)
    Native,
    /// Parsed from <tool_call> XML in model output
    TagBased,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name:    String,
    pub output:  String,
    pub error:   Option<String>,
    pub is_error: bool,
}

// ── Tag-based parser ──────────────────────────────────────────────────────────

/// Parse <tool_call>...</tool_call> blocks from model output text.
/// Handles two fence patterns models commonly emit:
///   1. ```[lang]\n<tool_call>...</tool_call>\n```  (full XML in fence)
///   2. ```[lang]\n<name>...</name>\n<parameters>...</parameters>\n```  (inner XML, no wrapper)
pub fn unwrap_fenced_tool_calls(text: &str) -> String {
    let mut out  = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(fence_start) = rest.find("```") {
        let before      = &rest[..fence_start];
        let after_fence = &rest[fence_start + 3..];

        // Skip optional language tag (ascii letters only)
        let lang_end  = after_fence.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(0);
        let after_lang = &after_fence[lang_end..];
        // Require a newline right after the lang tag
        let after_nl = if after_lang.starts_with('\n') {
            &after_lang[1..]
        } else if after_lang.starts_with("\r\n") {
            &after_lang[2..]
        } else {
            // Not a newline-terminated fence header — keep as-is
            out.push_str(before);
            out.push_str("```");
            rest = after_fence;
            continue;
        };

        // Find closing ```
        if let Some(close_pos) = after_nl.find("\n```") {
            let inner = &after_nl[..close_pos];
            let after_close = &after_nl[close_pos + 4..]; // skip \n```

            let inner_trimmed = inner.trim();

            // Pattern 1: fence wraps a full <tool_call>...</tool_call>
            if inner_trimmed.contains("<tool_call>") && inner_trimmed.contains("</tool_call>") {
                // Emit inner as-is (the existing parser will find <tool_call>)
                out.push_str(before);
                out.push_str(inner);
                rest = after_close;
                continue;
            }

            // Pattern 2: fence contains <name>...</name> [<parameters>...</parameters>]
            // without the <tool_call> wrapper — synthesize it
            if inner_trimmed.contains("<name>") && inner_trimmed.contains("</name>") {
                out.push_str(before);
                out.push_str("<tool_call>\n");
                out.push_str(inner_trimmed);
                out.push_str("\n</tool_call>");
                rest = after_close;
                continue;
            }

            // Pattern 3: fence contains raw JSON object with "name" + "arguments"/"parameters"
            // e.g. {"name": "write_file", "arguments": {"path": "...", "content": "..."}}
            // Convert to XML tool_call format
            if inner_trimmed.starts_with('{') {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner_trimmed)
                    .or_else(|_| serde_json::from_str(&repair_json(inner_trimmed)))
                    .or_else(|_| serde_json::from_str(&expand_js_repeat(inner_trimmed)))
                    .or_else(|_| serde_json::from_str(&repair_json(&expand_js_repeat(inner_trimmed))))
                {
                    let name = v.get("name").or_else(|| v.get("tool")).and_then(|n| n.as_str());
                    let args = v.get("arguments").or_else(|| v.get("parameters"));
                    if let Some(name) = name {
                        let params_str = args
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| "{}".to_string());
                        out.push_str(before);
                        out.push_str("<tool_call>\n<name>");
                        out.push_str(name);
                        out.push_str("</name>\n<parameters>");
                        out.push_str(&params_str);
                        out.push_str("</parameters>\n</tool_call>");
                        rest = after_close;
                        continue;
                    }
                }
            }
        }

        // Not a tool call fence — keep it as-is
        out.push_str(before);
        out.push_str("```");
        rest = after_fence;
    }
    out.push_str(rest);
    out
}

/// Returns (clean_text_without_tool_calls, tool_calls)
pub fn parse_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    // Normalize: unwrap any code-fence wrappers around tool calls
    let normalized = unwrap_fenced_tool_calls(text);
    let text = normalized.as_str();

    let mut calls = vec![];
    let mut clean = String::new();
    let mut rest = text;

    while let Some(start) = rest.find("<tool_call>") {
        clean.push_str(&rest[..start]);
        rest = &rest[start + "<tool_call>".len()..];

        if let Some(end) = rest.find("</tool_call>") {
            let inner = &rest[..end];
            rest = &rest[end + "</tool_call>".len()..];

            if let Some(call) = parse_single_tool_call(inner) {
                calls.push(call);
            }
        }
    }
    clean.push_str(rest);

    (clean, calls)
}

/// Repair JSON that contains literal (unescaped) control characters inside strings.
/// Models often emit file content with real newlines instead of \\n escapes.
/// Public alias for use from app.rs JSON fence detection.
pub fn repair_json_pub(s: &str) -> String { repair_json(s) }

fn repair_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 64);
    let mut in_string = false;
    let mut escape_next = false;
    for c in s.chars() {
        if escape_next {
            out.push(c);
            escape_next = false;
            continue;
        }
        match c {
            '\\' if in_string => { out.push(c); escape_next = true; }
            '"' => { out.push(c); in_string = !in_string; }
            '\n' if in_string => out.push_str("\\n"),
            '\r' if in_string => out.push_str("\\r"),
            '\t' if in_string => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Expand JS-style string `.repeat(N)` expressions that models sometimes emit.
/// e.g.  `"hello\n".repeat(3)`  →  `"hello\nhello\nhello\n"`
fn expand_js_repeat(s: &str) -> String {
    let mut out = s.to_string();
    // Keep replacing until no more matches (handles chained or multiple repeats)
    loop {
        // Find pattern:  "..." .repeat( N )
        // We scan manually to correctly handle escaped quotes inside the string.
        let bytes = out.as_bytes();
        let mut found = false;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'"' { i += 1; continue; }
            // Scan to end of JSON string literal
            let str_start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' { i += 2; continue; }
                if bytes[i] == b'"' { i += 1; break; }
                i += 1;
            }
            let str_end = i; // exclusive, points past closing "
            // Skip whitespace
            let mut j = str_end;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') { j += 1; }
            // Look for .repeat(
            if bytes.get(j..j+8) != Some(b".repeat(") { continue; }
            j += 8;
            // Parse number
            let num_start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() { j += 1; }
            let num_end = j;
            // Skip whitespace then expect )
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') { j += 1; }
            if bytes.get(j) != Some(&b')') { continue; }
            j += 1;
            // We have a match: out[str_start..str_end] .repeat( N )
            let count: usize = std::str::from_utf8(&bytes[num_start..num_end])
                .ok().and_then(|s| s.parse().ok()).unwrap_or(1);
            // Extract the raw string content (without surrounding quotes)
            let raw = &out[str_start+1..str_end-1];
            // Unescape \n \t \r for repetition, then re-escape
            let unescaped = raw.replace("\\n", "\n").replace("\\t", "\t").replace("\\r", "\r");
            let repeated  = unescaped.repeat(count);
            let reescaped = repeated.replace('\\', "\\\\").replace('"', "\\\"")
                .replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
            let replacement = format!("\"{}\"", reescaped);
            out = format!("{}{}{}", &out[..str_start], replacement, &out[j..]);
            found = true;
            break; // restart from beginning
        }
        if !found { break; }
    }
    out
}

fn parse_single_tool_call(inner: &str) -> Option<ToolCall> {
    let name = extract_xml_tag(inner, "name")?;
    let params_str = extract_xml_tag(inner, "parameters").unwrap_or_else(|| "{}".to_string());
    let parameters = serde_json::from_str(&params_str)
        .or_else(|_| serde_json::from_str(&repair_json(&params_str)))
        .or_else(|_| serde_json::from_str(&expand_js_repeat(&params_str)))
        .or_else(|_| serde_json::from_str(&repair_json(&expand_js_repeat(&params_str))))
        .unwrap_or(serde_json::Value::Object(Default::default()));

    Some(ToolCall {
        id:         ulid::Ulid::new().to_string(),
        name,
        parameters,
        source:     ToolCallSource::TagBased,
    })
}

fn extract_xml_tag(text: &str, tag: &str) -> Option<String> {
    let open  = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)? + open.len();
    let end   = text.find(&close)?;
    Some(text[start..end].trim().to_string())
}

// ── Dispatcher ───────────────────────────────────────────────────────────────

/// Execute a tool call and return the result.
pub async fn execute(
    call:    &ToolCall,
    cwd:     &PathBuf,
    client:  &reqwest::Client,
) -> ToolResult {
    let result = match call.name.as_str() {
        "make_plan"  => files::make_plan(&call.parameters),
        "search"     => search::execute(&call.parameters).await,
        "read_file"  => files::read_file(&call.parameters, cwd),
        "write_file" => files::write_file(&call.parameters, cwd),
        "edit_file"  => files::edit_file(&call.parameters, cwd),
        "list_dir"   => files::list_dir(&call.parameters, cwd),
        "grep"       => files::grep(&call.parameters, cwd),
        "glob"       => files::glob_files(&call.parameters, cwd),
        "create_dir" => files::create_dir(&call.parameters, cwd),
        "delete_file"=> files::delete_file(&call.parameters, cwd),
        "move_file"  => files::move_file(&call.parameters, cwd),
        "copy_file"  => files::copy_file(&call.parameters, cwd),
        "append_file"=> files::append_file(&call.parameters, cwd),
        "file_info"  => files::file_info(&call.parameters, cwd),
        "tree"       => files::tree(&call.parameters, cwd),
        "shell"      => shell::execute(&call.parameters, cwd).await,
        "http_fetch" => http::fetch(&call.parameters, client).await,
        _            => Err(anyhow::anyhow!("Unknown tool: {}", call.name)),
    };

    match result {
        Ok(output) => ToolResult {
            call_id:  call.id.clone(),
            name:     call.name.clone(),
            output,
            error:    None,
            is_error: false,
        },
        Err(e) => ToolResult {
            call_id:  call.id.clone(),
            name:     call.name.clone(),
            output:   String::new(),
            error:    Some(e.to_string()),
            is_error: true,
        },
    }
}
