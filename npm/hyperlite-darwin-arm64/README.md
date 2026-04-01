# HyperLite

A terminal-native local LLM chat client. Fast, offline, and agentic — runs entirely on your machine using [Ollama](https://ollama.com).

## Install

```bash
npm install -g hyperlite-ai
```

## Run

```bash
hyperlite
```

## Requirements

- [Ollama](https://ollama.com) installed and running
- A downloaded model (e.g. `ollama pull qwen2.5-coder:14b`)
- Node.js 16+

## Features

- Chat with any local Ollama model
- Agentic coding tools — read, write, edit, search files directly from chat
- Multi-session history with persistent storage
- Tabbed command palette (Ctrl+P)
- Visual folder browser (Ctrl+O) — open any repo as working directory
- Download models from inside the app
- Syntax-highlighted responses with markdown rendering
- Hardware detection — recommends models for your GPU/RAM

## Supported Platforms

| Platform | Architecture |
|----------|-------------|
| Windows  | x64 |
| Linux    | x64 |
| macOS    | Apple Silicon (arm64) |
| macOS    | Intel (x64) |

## Source

[github.com/Sean504/HyperLite](https://github.com/Sean504/HyperLite)
