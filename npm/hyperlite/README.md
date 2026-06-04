```
  ___ ___                             .____    .__  __
 /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____
/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \
\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/
 \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >
       \/  \/     |__|        \/              \/             \/
```

# hyperlite-ai

**Terminal-native local AI — offline, agentic, GPU-accelerated.**

```bash
npm install -g hyperlite-ai
hl
```

→ **[hyperlite.org](https://hyperlite.org)**

---

## What is HyperLite?

HyperLite is a full-featured AI assistant that runs entirely on your local hardware. No cloud. No API keys. No subscription. No telemetry. One command installs it, one command launches it, and it works offline from that point forward.

It is built in Rust as a terminal UI (TUI) — fast enough to feel native, capable enough to replace cloud tools for most development and productivity workflows.

```
npm install -g hyperlite-ai   # install
hl                             # launch
```

---

## Features

### Local inference — any model, any backend
HyperLite auto-detects your GPU on first launch and installs the right inference runtime for your hardware — Ollama with CUDA on Linux, Metal-accelerated llama.cpp on macOS, or a pre-built GPU binary on Windows. It connects to any OpenAI-compatible local server: Ollama, llama.cpp, LM Studio, Jan, GPT4All, KoboldCpp, vLLM, LocalAI, and text-generation-webui.

### Agentic tool use — works with any model
The AI can read and write files, run shell commands, search the web, query APIs, analyze CSVs, read PDFs, monitor system resources, run git operations, and more — on any local model, not just models with official function-calling support. Tool calls are parsed from model output in real time and executed with explicit user approval for file writes.

### Visual diff approval
Every file write shows a syntax-highlighted diff before it hits disk — green for additions, red for deletions. You read the change, then approve or discard. Nothing writes without your confirmation.

### Multi-session history
Conversations persist in a local SQLite database. Sessions are searchable, resumable, and branch-capable. Context is compacted automatically when switching models to preserve coherence.

### Semantic search over your codebase
Index any folder for RAG — local ONNX embeddings, stored in SQLite, no API key required. The AI searches your codebase by meaning, not just keywords.

### Persistent memory
Save facts across sessions. The AI recalls preferences, project context, and instructions automatically on every conversation.

---

## Install

```bash
npm install -g hyperlite-ai
```

The correct native binary for your platform installs automatically as an optional dependency (`@hyperlite-ai/linux-x64`, `@hyperlite-ai/darwin-arm64`, `@hyperlite-ai/win32-x64`). Everything is stored in `~/.hyperlite/`.

---

## First launch

On first run, an animated setup wizard:
1. Detects your GPU and hardware
2. Installs an inference runtime (Ollama on Linux, llama-server on macOS/Windows)
3. Downloads a model from HuggingFace — recommended models are filtered to what fits your hardware

After setup, `hl` launches directly to the chat interface.

---

## Key bindings

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Alt+Enter` / `Ctrl+J` | Insert newline |
| `Alt+P` | Model picker |
| `Ctrl+A` | Agent picker |
| `Ctrl+K` | Command palette |
| `Ctrl+N` | New session |
| `Ctrl+S` | Session list |
| `Alt+\` | Toggle sidebar |
| `Ctrl+H` | Help (shortcuts, agents, tools, indexing) |
| `Ctrl+T` | Toggle reasoning display |
| `Alt+T` | Cycle theme |
| `Ctrl+X` | Quit |

---

## Agents

| Agent | Description |
|-------|-------------|
| `general` | Conversational assistant with full tool access |
| `build` | Expert coding agent — reads, writes, builds, runs shell commands |
| `plan` | Read-only analysis — explores code, never writes or executes |

Create custom agents with a name, description, system prompt, and optional tool restrictions via `Ctrl+A → New Agent`.

---

## Tools

The AI has access to 30+ tools across all categories:

**Files** — `read_file` · `batch_read` · `write_file` · `edit_file` · `append_file` · `delete_file` · `move_file` · `copy_file` · `create_dir` · `list_dir` · `tree` · `glob` · `grep` · `file_info`

**Documents** — `read_pdf` · `analyze_csv` · `scrape_page` · `read_notes` · `write_note`

**System** — `shell` · `system_status` · `check_ports`

**Git** — `git_status` · `git_log` · `git_diff` · `git_blame` · `git_commit` · `git_push` · `git_pull` · `git_branch` · `git_stash`

**Web** — `search` · `http_fetch` · `scrape_page`

**Knowledge** — `index_dir` · `search_index` · `make_plan`

---

## Themes

16 built-in themes: `cyberpunk` · `dracula` · `tokyonight` · `catppuccin` · `nord` · `gruvbox` · `monokai` · `one-dark` · `synthwave84` · `matrix` · `rosepine` · `everforest` · `solarized` · `kanagawa` · `vesper` · `aura` and more.

Cycle with `Alt+T` or pick via `Ctrl+K → Display → Pick Theme`.

---

## Supported platforms

| Platform | Package |
|----------|---------|
| Linux x64 (glibc) | `@hyperlite-ai/linux-x64` |
| macOS Apple Silicon (M1/M2/M3/M4) | `@hyperlite-ai/darwin-arm64` |
| macOS Intel | `@hyperlite-ai/darwin-x64` |
| Windows 10/11 x64 | `@hyperlite-ai/win32-x64` |

---

→ **[hyperlite.org](https://hyperlite.org)**

## License

MIT
