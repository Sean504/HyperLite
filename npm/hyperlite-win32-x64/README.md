# @hyperlite-ai/win32-x64

Native Windows x64 binary for [HyperLite](https://hyperlite.org) — terminal-native local AI chat.

> **This is a platform binary package.** Install the main package instead:
> ```bash
> npm install -g hyperlite-ai
> ```

---

## What is HyperLite?

HyperLite is a Rust TUI that runs AI models entirely on your local hardware. No cloud, no API keys, no telemetry.

- Offline-first — works without internet after the initial model download
- Agentic — the model can read/write files, run shell commands, search the web
- Connects to llamafile (auto-managed), llama.cpp, LM Studio, KoboldCpp, vLLM, and more
- Persistent multi-session history in SQLite

## Contents

This package contains a single precompiled binary: `hl.exe` (PE32+, Windows x64).

Built from: `x86_64-pc-windows-msvc` with Rust stable.

## Requirements

- Windows 10 or 11 (x64)
- Node.js 16+ (for the npm launcher only)
- A terminal that supports VT sequences — **Windows Terminal** recommended

## Usage

```powershell
npm install -g hyperlite-ai   # installs this package automatically on Windows
hl                             # launch from PowerShell or Windows Terminal
hyperlite                      # same
```

## Note on terminals

HyperLite renders a full TUI using crossterm and Windows Console API. It works in:
- Windows Terminal ✓ (recommended)
- PowerShell 7 in Windows Terminal ✓
- Git Bash / WSL terminal ✓ (runs the Windows binary via interop)

Legacy `cmd.exe` may have rendering issues.

## Links

- [hyperlite.org](https://hyperlite.org)
- [Source](https://github.com/Sean504/HyperLite)
- [Full documentation](https://github.com/Sean504/HyperLite/blob/main/DOCS.md)

## License

MIT
