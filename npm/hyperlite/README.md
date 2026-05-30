# hyperlite-ai

Terminal-native local AI chat client — fast, offline, agentic.

```bash
npm install -g hyperlite-ai
hl
```

## What it is

HyperLite is a Rust TUI that runs AI models entirely on your local hardware. No cloud, no API keys, no telemetry. One binary per platform, installed automatically.

- **Offline first** — works without internet after the initial model download
- **Agentic** — the model can read/write files, run shell commands, search the web, chain multi-step tasks
- **Any backend** — auto-manages its own llamafile runtime, or connects to llama.cpp, LM Studio, KoboldCpp, vLLM, Ollama, LocalAI, Jan, GPT4All, and text-generation-webui
- **Full markdown** — syntax-highlighted responses, collapsible reasoning blocks from thinking models
- **Multi-session** — persistent conversation history in SQLite with fuzzy session search

## Install

```bash
npm install -g hyperlite-ai
```

npm installs the correct native binary for your platform automatically via an optional dependency. If the optional install is skipped, the launcher detects and installs it on first run.

## Commands

```bash
hl          # short form
hyperlite   # long form
```

On first launch, an animated setup wizard lets you download a model and the inference runtime. Everything is stored in `~/.hyperlite/`.

## Supported platforms

| Platform | Binary package |
|---|---|
| Linux x64 | `@hyperlite-ai/linux-x64` |
| Linux ARM64 (Raspberry Pi 5, SBCs) | `@hyperlite-ai/linux-arm64` |
| macOS Apple Silicon | `@hyperlite-ai/darwin-arm64` |
| Windows x64 | `@hyperlite-ai/win32-x64` |

## Key bindings

| Key | Action |
|---|---|
| `Enter` | Send message |
| `Alt+Enter` | Insert newline |
| `Ctrl+M` | Model picker |
| `Ctrl+N` | New session |
| `Ctrl+S` | Session list |
| `Ctrl+A` | Agent picker |
| `Ctrl+\` | Toggle sidebar |
| `Ctrl+K` | Command palette |
| `Ctrl+Q` | Quit |
| `?` | Help |

## Agents

| Agent | Description |
|---|---|
| `general` | Full tool access — chat, analysis, general questions |
| `build` | Expert coding agent — writes, edits, and builds code |
| `plan` | Read-only — explores and analyzes without making changes |

Custom agents can be created inside the app with `Ctrl+A`.

## Tools the model can use

`read_file` · `batch_read` · `write_file` · `edit_file` · `append_file` · `list_dir` · `tree` · `glob` · `grep` · `file_info` · `create_dir` · `delete_file` · `move_file` · `copy_file` · `shell` · `search` · `http_fetch` · `make_plan`

## Links

- [Source on GitHub](https://github.com/Sean504/HyperLite)
- [Releases](https://github.com/Sean504/HyperLite/releases)
- [Full documentation](https://github.com/Sean504/HyperLite/blob/main/DOCS.md)
- [hyperlite.org](https://hyperlite.org)

## License

MIT
