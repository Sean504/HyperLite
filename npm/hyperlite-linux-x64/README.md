# @hyperlite-ai/linux-x64

Native Linux x64 binary for [HyperLite](https://hyperlite.org) — terminal-native local AI chat.

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

This package contains a single precompiled binary: `hl` (ELF, Linux x64, glibc).

Built from: `x86_64-unknown-linux-gnu` with Rust stable.

## Requirements

- Linux x64 with glibc 2.17+
- Node.js 16+ (for the npm launcher only)

## Usage

```bash
npm install -g hyperlite-ai   # installs this package automatically
hl                             # launch
hyperlite                      # same
```

## Links

- [hyperlite.org](https://hyperlite.org)
- [Source](https://github.com/Sean504/HyperLite)
- [Full documentation](https://github.com/Sean504/HyperLite/blob/main/DOCS.md)

## License

MIT
