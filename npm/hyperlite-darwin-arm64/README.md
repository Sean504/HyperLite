```
  ___ ___                             .____    .__  __
 /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____
/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \
\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/
 \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >
       \/  \/     |__|        \/              \/             \/
```

# @hyperlite-ai/darwin-arm64

Native macOS Apple Silicon binary for **HyperLite** — terminal-native local AI, offline and Metal-accelerated.

> **This is a platform binary package.** Install the main package:
> ```bash
> npm install -g hyperlite-ai
> hl
> ```

→ **[hyperlite.org](https://hyperlite.org)**

---

## About HyperLite

HyperLite is a local AI assistant built in Rust that runs entirely on your hardware. No cloud, no API keys, no telemetry. On Apple Silicon, inference runs through a Metal-accelerated llama-server installed via Homebrew — using the full unified memory bandwidth of M-series chips for fast local LLM inference.

## This package

Contains a single precompiled binary: `hl`

- Format: Mach-O ARM64, macOS
- Target: `aarch64-apple-darwin`
- Compiler: Rust stable, built on macOS 14 (Sonoma)

## Requirements

- macOS 11+ on Apple Silicon (M1 / M2 / M3 / M4)
- Node.js 16+ (launcher only)
- Homebrew recommended (for automatic runtime install)

## Usage

```bash
npm install -g hyperlite-ai
hl
```

On first launch, HyperLite installs `llama.cpp` via Homebrew for Metal-accelerated inference and downloads a model sized to your RAM.

## Gatekeeper note

macOS may block unsigned binaries on first run. If you see a security warning:

```bash
xattr -d com.apple.quarantine $(which hl)
```

Or go to **System Settings → Privacy & Security → Open Anyway**.

---

→ **[hyperlite.org](https://hyperlite.org)**

## License

MIT
