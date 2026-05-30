# HyperLite — Documentation

> Terminal-native, offline-first AI chat client built in Rust. Fast, private, agentic. Runs on Linux, macOS, Windows, and Raspberry Pi 5.

---

## Table of Contents

1. [Overview](#overview)
2. [Philosophy](#philosophy)
3. [Architecture](#architecture)
4. [How Inference Works](#how-inference-works)
5. [Build Targets](#build-targets)
6. [First-Run Setup](#first-run-setup)
7. [Provider System](#provider-system)
8. [Tool System](#tool-system)
9. [Agent System](#agent-system)
10. [Session & Message Model](#session--message-model)
11. [UI Structure](#ui-structure)
12. [Configuration](#configuration)
13. [Keybindings](#keybindings)
14. [Database](#database)
15. [npm Distribution](#npm-distribution)

---

## Overview

HyperLite is a fully offline AI assistant that runs entirely on your local hardware. There are no cloud APIs, no telemetry, and no internet requirement after initial setup. It is written in Rust, renders in your terminal using [ratatui](https://ratatui.rs), and drives inference through any locally running llama.cpp-compatible server — or manages its own llamafile runtime automatically.

It ships as a single binary (`hl`) installable in one command via npm.

---

## Philosophy

- **Offline first.** Every feature works without an internet connection once models are downloaded.
- **Fast startup.** The TUI is visible within milliseconds. Hardware is detected, sessions load, and models enumerate before the user finishes reading the boot screen.
- **No bloat.** No Electron, no Python, no Docker. One Rust binary per platform.
- **Agentic by default.** The model can read files, write code, run shell commands, search the web, and chain multi-step operations — all within the terminal.
- **Any backend.** Works with llamafile (managed automatically), llama.cpp, Ollama, LM Studio, KoboldCpp, LocalAI, vLLM, Jan, text-generation-webui, and GPT4All.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│                  Terminal                   │
│  ┌───────────────────────────────────────┐  │
│  │           ratatui TUI (Rust)          │  │
│  │  message list │ sidebar │ input area  │  │
│  └───────────────┬───────────────────────┘  │
│                  │ crossterm events          │
│  ┌───────────────▼───────────────────────┐  │
│  │         App State (app.rs)            │  │
│  │  sessions │ messages │ models │ tools │  │
│  └──┬─────────────┬──────────────┬───────┘  │
│     │             │              │           │
│  SQLite       Provider        Tool System   │
│  (rusqlite)   Registry        (files, shell,│
│               │                web, search) │
│         ┌─────▼──────┐                      │
│         │HTTP client │                      │
│         │(reqwest)   │                      │
└─────────┴──────┬─────┴──────────────────────┘
                 │ SSE stream / JSON
         ┌───────▼────────┐
         │  Local server  │  llamafile · llama.cpp · Ollama · LM Studio · …
         │  localhost:*   │
         └───────┬────────┘
                 │
         ┌───────▼────────┐
         │   Model file   │  GGUF on disk
         └────────────────┘
```

### Technology Stack

| Concern | Crate | Notes |
|---|---|---|
| TUI rendering | `ratatui` + `crossterm` | Alternate screen, mouse support |
| Async runtime | `tokio` | Full feature set |
| HTTP / SSE streaming | `reqwest` (rustls) | No OpenSSL dependency |
| Database | `rusqlite` (bundled SQLite) | WAL mode, mmap |
| Config | `toml` + `serde` | `~/.config/hyperlite/settings.toml` |
| Fuzzy search | `nucleo-matcher` | Helix/fzf algorithm |
| Markdown rendering | `pulldown-cmark` + `syntect` | Full syntax highlighting |
| IDs | `ulid` | Sortable, collision-free |
| Text input | `tui-textarea` | Multi-line, history-aware |
| Clipboard | `arboard` | Cross-platform |
| Hardware detection | `sysinfo` + sysfs | CPU, RAM, GPU, arch |
| File scan | `walkdir` | Model directory scanning |
| Process detection | `which` | Finds llama-server, llamafile in PATH |
| UID / passwd | `libc` | Real home dir on Linux |

---

## How Inference Works

### Full pipeline from message to response

1. **User submits a message** — the input textarea content is collected, a `Message` record is created and written to SQLite, and the UI enters streaming mode.

2. **Provider selection** — the `ProviderRegistry` selects the backend serving the current model. With no other servers running, `DirectGgufProvider` manages a local llamafile or llama-server process automatically.

3. **Server health check** — HyperLite polls `http://127.0.0.1:<port>/health`. If a server is already running for this model, the existing process is reused.

4. **Server spawn (if not running)** — the runtime is launched as a subprocess with flags derived from detected hardware (thread count, GPU layers, context size). HyperLite waits up to 90 seconds for the `/health` endpoint before surfacing a timeout error.

5. **Streaming chat request** — a `POST /v1/chat/completions` request is sent with `stream: true`. The full conversation history is included so the model has context.

6. **SSE token streaming** — the response body is a Server-Sent Events stream. Each `data:` line contains a JSON chunk with a token delta. HyperLite appends each token to the active message in real time, re-rendering on every chunk.

7. **Tool call detection** — if the model emits `<tool_call>...</tool_call>` XML or a native OpenAI `tool_calls` JSON array, HyperLite intercepts the stream, executes the tool, and re-submits with the result appended — without user intervention (unless the tool requires permission).

8. **Message finalisation** — when the stream ends, the full message is written to SQLite with duration and token count. The footer updates with tokens/sec.

---

## Build Targets

HyperLite is compiled natively for four platforms. The GitHub Actions release workflow builds all targets on every version tag push.

| Target triple | Platform | npm package | Binary |
|---|---|---|---|
| `x86_64-pc-windows-msvc` | Windows 10/11 x64 | `@hyperlite-ai/win32-x64` | `hl.exe` |
| `x86_64-unknown-linux-gnu` | Linux x64 (glibc) | `@hyperlite-ai/linux-x64` | `hl` |
| `aarch64-unknown-linux-gnu` | Linux ARM64 (RPi5, SBCs) | `@hyperlite-ai/linux-arm64` | `hl` |
| `aarch64-apple-darwin` | macOS Apple Silicon | `@hyperlite-ai/darwin-arm64` | `hl` |

> **Note:** The ARM64 Linux target is cross-compiled on Ubuntu using the [`cross`](https://github.com/cross-rs/cross) tool with the `ghcr.io/cross-rs/aarch64-unknown-linux-gnu:edge` Docker image.

### Release workflow summary

1. Each platform builds its binary in parallel using GitHub Actions.
2. The binary is staged into the corresponding `npm/<pkg>/` directory.
3. The package version is set to the git tag (e.g. `v0.2.14` → `0.2.14`).
4. Each platform package is published to npm (`npm publish --access public`).
5. After all platform packages succeed, the main `hyperlite-ai` wrapper package is published with matching `optionalDependencies` versions.
6. A GitHub release is created with all four binaries attached as assets.

---

## First-Run Setup

On first launch (or when no models are found), HyperLite runs an animated setup wizard.

### Step 1 — Splash screen

Hardware is detected and displayed: CPU model, core count, RAM, and any GPUs. Press Enter to continue.

### Step 2 — Model selection

A filtered list of recommended GGUF models is shown based on available VRAM (GPU) or RAM (CPU-only). Models outside your hardware budget are hidden. Toggle selections with Space, confirm with Enter.

Models are downloaded directly from HuggingFace CDN to `~/.hyperlite/models/`. Downloads use a dedicated no-timeout HTTP client so large files (up to 40+ GB) complete without interruption.

### Step 3 — Runtime download

If no inference runtime is found (`llama-server`, `llamafile`, or the bundled binary), HyperLite downloads llamafile from the Mozilla-Ocho release page to `~/.hyperlite/llamafile` and marks it executable.

On ARM64 Linux (Raspberry Pi 5), HyperLite-PI compiles `llama-server` natively from source instead — see the [HyperLite-PI documentation](../Hyperlite-PI/DOCS.md).

### Step 4 — Launch

The setup wizard closes and the main chat interface opens. All future launches skip setup and go directly to chat.

---

## Provider System

HyperLite connects to any locally running inference server. All providers implement the `LocalProvider` trait and are probed concurrently at startup. Only reachable backends appear in the model picker.

| Provider | ID | Default Port | Formats supported |
|---|---|---|---|
| **Direct GGUF** | `direct` | 18080 (managed) | GGUF, GGML |
| llama.cpp server | `llamacpp` | 8080 | GGUF, GGML |
| LM Studio | `lmstudio` | 1234 | GGUF, EXL2 |
| KoboldCpp | `kobold` | 5001 | GGUF, GGML |
| text-generation-webui | `textgen` | 5000 | GGUF, GPTQ, AWQ, EXL2, SafeTensors, .bin, ONNX |
| LocalAI | `localai` | 8080 | GGUF, GGML, GPTQ, SafeTensors, ONNX |
| Jan.ai | `jan` | 1337 | GGUF |
| llamafile | `llamafile` | 8080 | llamafile, GGUF |
| vLLM | `vllm` | 8000 | SafeTensors, GPTQ, AWQ, EXL2 |
| GPT4All | `gpt4all` | 4891 | GGUF |

### Direct GGUF provider

The `DirectGgufProvider` scans multiple directories for model files and manages the inference server lifecycle itself. No external daemon is required.

**Scanned directories (in order):**
- `~/.hyperlite/models/` (always first — HyperLite's own download location)
- `~/.cache/huggingface/`
- `~/models/`, `~/Models/`
- `~/lm-studio/models/`
- `/opt/models/`

On ARM64 Linux, additional paths are scanned to handle models downloaded under a different user account:
- `/root/.hyperlite/models/`
- `/home/*/.hyperlite/models/`

**Supported file extensions:** `.gguf`, `.ggml`, `.bin`, `.safetensors`, `.llamafile`, `.onnx`, `.exl2`

---

## Tool System

HyperLite gives the model access to local tools it can call during a conversation. Two invocation modes are supported simultaneously:

- **Native function calling** — model emits an OpenAI-format `tool_calls` JSON array. Works with models explicitly fine-tuned for it (Llama 3.1, Qwen2.5, Mistral Nemo, etc.).
- **Tag-based parsing** — model emits `<tool_call><name>...</name><parameters>...</parameters></tool_call>` XML. Works with any model regardless of fine-tuning. HyperLite also handles malformed variants: code-fenced tool calls, bare `<name>` tags without a wrapper, and JSON-fenced tool calls.

### Available tools

| Tool | Permission required | Description |
|---|---|---|
| `make_plan` | No | Declare a multi-step plan before acting. Required for 3+ sequential tool calls. |
| `search` | No | DuckDuckGo web search. Returns titles, snippets, and URLs. No API key needed. |
| `read_file` | No | Read file contents. Capped at 300 lines by default; `start_line`/`end_line` to slice. |
| `batch_read` | No | Read up to 20 files at once, 80 lines each. For broad codebase scanning. |
| `write_file` | Yes | Write or create a file on disk. |
| `edit_file` | Yes | Search-and-replace within a file (exact match). |
| `append_file` | No | Append text to a file without overwriting. |
| `list_dir` | No | List directory contents with sizes. |
| `tree` | No | Recursive directory tree. Defaults to depth 3. |
| `glob` | No | Find files matching a glob pattern. |
| `grep` | No | Search file contents with regex. Returns file paths and line numbers. |
| `file_info` | No | File/directory metadata: type, size, modified date. |
| `create_dir` | No | Create a directory and any missing parents. |
| `delete_file` | Yes | Delete a file or directory tree. |
| `move_file` | No | Move or rename a file or directory. |
| `copy_file` | No | Copy a file or directory. |
| `shell` | Yes | Execute a shell command (30s timeout). Stdout + stderr returned. |
| `http_fetch` | No | Fetch a URL and return its text content (HTML stripped). |

### Permission gates

Tools marked `requires_permission: true` show an interactive dialog before execution. The user can:
- **Allow once** — execute this call only
- **Allow all** — approve all calls from this tool for the session
- **Deny** — reject this call; the model receives an error result

Permission rules can also be pre-configured in `settings.toml` to skip the dialog entirely:

```toml
[[permissions.rules]]
tool    = "shell"
pattern = "git *"
action  = "allow"
```

---

## Agent System

Agents are named configurations that set a custom system prompt, restrict which tools are available, and optionally fix the model. HyperLite ships three built-in agents.

### Built-in agents

| Agent | ID | Tools | Description |
|---|---|---|---|
| General | `general` | All | Conversational assistant with full tool access. |
| Build | `build` | All | Expert coding agent. Focused on writing, editing, and building code. |
| Plan | `plan` | Read-only subset | Analysis agent. Can read and search but cannot write files or run commands. |

The Plan agent has access to: `make_plan`, `read_file`, `batch_read`, `list_dir`, `tree`, `grep`, `glob`, `file_info`, `search`, `http_fetch`.

### Custom agents

Custom agents can be created inside the app with Ctrl+A. They are stored in the `agents` table in the SQLite database and can override any built-in agent by matching its ID.

```toml
# Example custom agent in settings.toml
[agents.devops]
name    = "DevOps"
system  = "You are a Linux systems expert. Always use shell commands to verify changes."
model   = "qwen2.5-coder-7b"
tools   = ["shell", "read_file", "list_dir", "grep"]
```

---

## Session & Message Model

### Sessions

A session is a conversation with a title, associated model, working directory, and timestamps. Sessions persist across restarts and are listed in the sidebar sorted by last activity. Sessions can be nested (parent/child) for branching conversations.

### Messages

Each message has a `role` (`user` or `assistant`) and a list of typed `Part` values:

| Part type | Contents |
|---|---|
| `Text` | Assistant response text (streamed in real time) |
| `Reasoning` | Thinking tokens from reasoning models (DeepSeek-R1, QwQ, etc.) |
| `Tool` | A tool call with name, input, state (`Pending → Running → Complete / Error / Denied`), and output |
| `File` | An attached file (binary, hex-encoded in DB) |

Parts are serialised as a JSON array in the `parts_json` column. New part types can be added without schema migrations.

---

## UI Structure

```
┌─────────────────────────────────┬────────────┐
│                                 │  Sessions  │
│         Message List            │  ──────── │
│                                 │  Models    │
│  [user]   hello                 │  ──────── │
│  [asst]   Hi! How can I help?   │  Agent     │
│  [tool]   $ ls -la              │  ──────── │
│           → file1.txt           │  Hardware  │
│  [asst]   Here are the files…   │            │
│                                 │            │
├─────────────────────────────────┴────────────┤
│ > type a message…                            │
├──────────────────────────────────────────────┤
│ model  ·  agent  ·  session  ·  tokens/sec   │
└──────────────────────────────────────────────┘
```

### Components

- **Message list** — full markdown rendering with syntax highlighting via `syntect`. Reasoning blocks from thinking models are shown inline. Tool calls display state icons and can be expanded to show full input/output.
- **Sidebar** — session list, current model, active agent, and hardware summary. Auto-hides on narrow terminals. Toggle with Ctrl+\.
- **Input area** — multi-line textarea (`tui-textarea`) with input history (↑/↓) and placeholder text.
- **Footer** — current model, agent, session title, and live tokens/sec after each response.
- **Dialogs** — model picker, session list, theme picker, and agent picker render as overlays with fuzzy search powered by `nucleo-matcher`.

---

## Configuration

Config file: `~/.config/hyperlite/settings.toml`

```toml
theme    = "dracula"       # theme name
model    = ""              # last used model ID
sidebar  = "auto"          # auto | always | never
thinking = false           # show reasoning tokens from thinking models

[[permissions.rules]]
tool    = "shell"
pattern = "git *"
action  = "allow"          # allow | deny | ask

[providers.llamacpp]
base_url = "http://localhost:8080"

[providers.lmstudio]
base_url = "http://localhost:1234"

[agents.devops]
name    = "DevOps"
system  = "You are a Linux systems expert..."
model   = "qwen2.5-coder-7b"
```

### Data directories

| Platform | Config | Data (DB + models) |
|---|---|---|
| Linux | `~/.config/hyperlite/` | `~/.hyperlite/` |
| macOS | `~/Library/Application Support/hyperlite/` | `~/.hyperlite/` |
| Windows | `%APPDATA%\hyperlite\` | `~\.hyperlite\` |

Models are stored in `~/.hyperlite/models/`. The inference runtime is at `~/.hyperlite/llamafile` (Linux/macOS/Windows) or `~/.hyperlite/llama-server` (RPi5 native build).

---

## Keybindings

| Key | Action |
|---|---|
| `Enter` | Send message |
| `Alt+Enter` | Insert newline |
| `Ctrl+K` | Command palette |
| `Ctrl+M` | Model picker |
| `Ctrl+S` | Session list |
| `Ctrl+N` | New session |
| `Ctrl+\` | Toggle sidebar |
| `Ctrl+A` | Agent picker |
| `Ctrl+Q` | Quit |
| `Ctrl+Z` | Undo last message |
| `Ctrl+C` | Interrupt generation |
| `Ctrl+L` | Copy last response |
| `?` | Help dialog |
| `↑` / `↓` | Scroll messages |
| `Ctrl+D` | Delete highlighted session (session list) |

---

## Database

SQLite at `~/.hyperlite/hyperlite.db`.

### Schema

```sql
sessions (
  id          TEXT PRIMARY KEY,   -- ULID
  title       TEXT NOT NULL,
  model_id    TEXT,
  provider_id TEXT,
  cwd         TEXT,
  parent_id   TEXT,               -- for nested sessions
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
)

messages (
  id           TEXT PRIMARY KEY,  -- ULID
  session_id   TEXT NOT NULL REFERENCES sessions(id),
  role         TEXT NOT NULL,     -- "user" | "assistant"
  parts_json   TEXT NOT NULL,     -- JSON array of Part values
  model        TEXT,
  duration_ms  INTEGER,
  created_at   TEXT NOT NULL
)

agents (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  description   TEXT,
  model         TEXT,
  provider      TEXT,
  system        TEXT,
  allowed_tools TEXT,             -- comma-separated tool names, NULL = all
  created_at    TEXT NOT NULL
)

drafts (
  id         TEXT PRIMARY KEY,
  label      TEXT,
  content    TEXT NOT NULL,
  created_at TEXT NOT NULL
)

kv_store (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
)
```

### Indexes

- `idx_messages_session` on `(session_id, created_at)` — fast message load per session
- `idx_sessions_updated` on `(updated_at DESC)` — fast recent session list

### PRAGMA tuning

```sql
PRAGMA journal_mode    = WAL;       -- concurrent reads during writes
PRAGMA foreign_keys    = ON;
PRAGMA synchronous     = NORMAL;    -- safe on SSD, avoids redundant fsyncs
PRAGMA cache_size      = -32768;    -- 32 MB in-memory page cache
PRAGMA mmap_size       = 268435456; -- 256 MB memory-mapped I/O
PRAGMA temp_store      = MEMORY;
```

---

## npm Distribution

HyperLite is distributed via npm using a platform-package pattern. The main package (`hyperlite-ai`) contains only a thin JavaScript launcher (`bin/run.js`). The actual compiled Rust binary is in a platform-specific optional dependency that npm installs automatically.

### Packages

| Package | Platform | Binary |
|---|---|---|
| `hyperlite-ai` | Wrapper (all platforms) | JavaScript launcher |
| `@hyperlite-ai/linux-x64` | Linux x64 | `hl` |
| `@hyperlite-ai/linux-arm64` | Linux ARM64 | `hl` |
| `@hyperlite-ai/win32-x64` | Windows x64 | `hl.exe` |
| `@hyperlite-ai/darwin-arm64` | macOS Apple Silicon | `hl` |

### Install

```bash
npm install -g hyperlite-ai
hl        # or: hyperlite
```

npm uses `os` and `cpu` fields in each platform package's `package.json` to install only the binary for the current platform. If the optional dependency is not auto-installed, `run.js` attempts to install it automatically before launching.

### Commands

After install, two commands are available:

```bash
hl          # short form
hyperlite   # long form
```

Both invoke `bin/run.js`, which resolves the platform binary path and exec's it with inherited stdio.

---

*HyperLite is open source. Source at [github.com/Sean504/HyperLite](https://github.com/Sean504/HyperLite)*
