```
  ___ ___                             .____    .__  __
 /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____
/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \
\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/
 \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >
       \/  \/     |__|        \/              \/             \/
```

# @hyperlite-ai/linux-x64

Native Linux x64 binary for **HyperLite** — terminal-native local AI, offline and GPU-accelerated.

> **This is a platform binary package.** Install the main package:
> ```bash
> npm install -g hyperlite-ai
> hl
> ```

→ **[hyperlite.org](https://hyperlite.org)**

---

## About HyperLite

HyperLite is a local AI assistant built in Rust that runs entirely on your hardware. No cloud, no API keys, no telemetry. It auto-installs Ollama with CUDA support on Linux for GPU-accelerated inference, downloads models directly from HuggingFace, and gives the AI full tool access — files, shell, git, web search, PDF reading, CSV analysis, and more.

## This package

Contains a single precompiled binary: `hl`

- Format: ELF 64-bit, Linux x86_64, dynamically linked (glibc)
- Target: `x86_64-unknown-linux-gnu`
- Compiler: Rust stable

## Requirements

- Linux x64 with glibc 2.17+
- Node.js 16+ (launcher only — not required to run `hl` directly)
- GPU acceleration: NVIDIA GPU + CUDA drivers, or AMD GPU + ROCm (CPU fallback available)

## Usage

```bash
npm install -g hyperlite-ai
hl
```

On first launch, HyperLite detects your GPU and installs Ollama automatically via `apt`, `pacman`, `dnf`, or `brew` — whichever is available. Models are downloaded from HuggingFace, sized to your hardware.

---

→ **[hyperlite.org](https://hyperlite.org)**

## License

MIT
