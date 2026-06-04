```
  ___ ___                             .____    .__  __
 /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____
/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \
\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/
 \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >
       \/  \/     |__|        \/              \/             \/
```

# @hyperlite-ai/darwin-x64

Native macOS Intel binary for **HyperLite** — terminal-native local AI, fully offline.

> **This is a platform binary package.** Install the main package:
> ```bash
> npm install -g hyperlite-ai
> hl
> ```

→ **[hyperlite.org](https://hyperlite.org)**

---

## About HyperLite

HyperLite is a local AI assistant built in Rust that runs entirely on your hardware. No cloud, no API keys, no telemetry. Runs fully offline after the initial model download. Connects to llama.cpp, Ollama, LM Studio, and other local inference backends.

## This package

Contains a single precompiled binary: `hl`

- Format: Mach-O x86\_64, macOS
- Target: `x86_64-apple-darwin`
- Compiler: Rust stable

## Requirements

- macOS 11+ on Intel (x86\_64)
- Node.js 16+ (launcher only)
- Homebrew recommended (for automatic runtime install)

## Usage

```bash
npm install -g hyperlite-ai
hl
```

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
