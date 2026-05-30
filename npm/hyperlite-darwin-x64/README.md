# @hyperlite-ai/darwin-x64

Native macOS Intel binary for [HyperLite](https://hyperlite.org) — terminal-native local AI chat.

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

This package contains a single precompiled binary: `hl` (Mach-O, x86_64, macOS).

Built from: `x86_64-apple-darwin` with Rust stable.

## Requirements

- macOS 11+ on Intel (x86_64)
- Node.js 16+ (for the npm launcher only)

## Usage

```bash
npm install -g hyperlite-ai   # installs this package automatically on Intel Macs
hl                             # launch from Terminal or iTerm2
hyperlite                      # same
```

## Note

On first launch, macOS Gatekeeper may block the binary if it is not notarised. If you see a security warning, run:

```bash
xattr -d com.apple.quarantine $(which hl)
```

Or go to **System Settings → Privacy & Security** and click "Open Anyway".

## Links

- [hyperlite.org](https://hyperlite.org)
- [Source](https://github.com/Sean504/HyperLite)
- [Full documentation](https://github.com/Sean504/HyperLite/blob/main/DOCS.md)

## License

MIT
