```
  ___ ___                             .____    .__  __
 /   |   \ ___.__.______   ___________|    |   |__|/  |_  ____
/    ~    <   |  |\____ \_/ __ \_  __ \    |   |  \   __\/ __ \
\    Y    /\___  ||  |_> >  ___/|  | \/    |___|  ||  | \  ___/
 \___|_  / / ____||   __/ \___  >__|  |_______ \__||__|  \___  >
       \/  \/     |__|        \/              \/             \/
```

# @hyperlite-ai/win32-x64

Native Windows x64 binary for **HyperLite** — terminal-native local AI, offline and GPU-accelerated.

> **This is a platform binary package.** Install the main package:
> ```bash
> npm install -g hyperlite-ai
> hl
> ```

→ **[hyperlite.org](https://hyperlite.org)**

---

## About HyperLite

HyperLite is a local AI assistant built in Rust that runs entirely on your hardware. No cloud, no API keys, no telemetry. On Windows with an NVIDIA GPU, it downloads a CUDA-accelerated llama-server automatically. Runs fully offline after setup.

## This package

Contains a single precompiled binary: `hl.exe`

- Format: PE32+, Windows x64
- Target: `x86_64-pc-windows-msvc`
- Compiler: Rust stable, MSVC toolchain

## Requirements

- Windows 10 or 11 (x64)
- Node.js 16+ (launcher only)
- **Windows Terminal** recommended for best rendering

## Usage

```powershell
npm install -g hyperlite-ai
hl
```

## Terminal compatibility

| Terminal | Status |
|----------|--------|
| Windows Terminal | ✓ Recommended |
| PowerShell 7 in Windows Terminal | ✓ Full support |
| WSL2 terminal | ✓ Full support |
| Git Bash | ✓ Works |
| Legacy cmd.exe | ⚠ Rendering may degrade |

---

→ **[hyperlite.org](https://hyperlite.org)**

## License

MIT
