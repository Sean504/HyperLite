# @hyperlite-ai/linux-arm64

Native Linux ARM64 binary for [HyperLite](https://hyperlite.org) — terminal-native local AI chat. Optimised for Raspberry Pi 5 and other aarch64 SBCs.

> **This is a platform binary package.** Install the main package instead:
> ```bash
> npm install -g hyperlite-ai
> ```
> For the full Raspberry Pi 5 experience (native llama-server build, Pi-specific optimisations), use [HyperLite-PI](https://www.npmjs.com/package/@hyperlite-ai/hyperlite-pi) instead.

---

## What is HyperLite?

HyperLite is a Rust TUI that runs AI models entirely on your local hardware. No cloud, no API keys, no telemetry.

- Offline-first — works without internet after the initial model download
- Agentic — the model can read/write files, run shell commands, search the web
- Connects to llamafile (auto-managed), llama.cpp, LM Studio, KoboldCpp, vLLM, and more
- Persistent multi-session history in SQLite

## Contents

This package contains a single precompiled binary: `hl` (ELF, Linux ARM64, glibc).

Built from: `aarch64-unknown-linux-gnu` using `cross` with Rust stable.

## Requirements

- Linux ARM64 with glibc 2.17+ (Raspberry Pi 5, Pi 4, Orange Pi 5, Rock 5, etc.)
- Node.js 16+ (for the npm launcher only)

## Usage

```bash
npm install -g hyperlite-ai   # installs this package automatically on ARM64 Linux
hl                             # launch
hyperlite                      # same
```

## Raspberry Pi 5 note

This binary runs on Pi 5 but uses the same llamafile runtime as other platforms. For best performance on Pi 5, use **HyperLite-PI** which compiles `llama-server` natively from source with `GGML_NATIVE=ON`, enabling NEON SIMD and Cortex-A76 dot product instructions for 5–10× higher tokens/sec.

## Links

- [hyperlite.org](https://hyperlite.org)
- [Source](https://github.com/Sean504/HyperLite)
- [Full documentation](https://github.com/Sean504/HyperLite/blob/main/DOCS.md)

## License

MIT
