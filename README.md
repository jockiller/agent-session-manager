# Agent Session Manager

<p align="center">
  <img src="./src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Agent Session Manager Logo" />
</p>

<p align="center">
  <strong>Unified Control Center for AI Agent CLI & Assistant Sessions</strong>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#supported-agents">Supported Agents</a> •
  <a href="#installation">Installation</a> •
  <a href="#development">Development</a> •
  <a href="#code-signing--gatekeeper">macOS Gatekeeper</a>
</p>

<p align="center">
  <a href="README.md">English</a> |
  <a href="README_zh-CN.md">简体中文</a> |
  <a href="README_zh-TW.md">繁體中文</a> |
  <a href="README_ja.md">日本語</a> |
  <a href="README_ko.md">한국어</a>
</p>

---

<p align="center">
  <img src="./docs/preview.png" alt="Agent Session Manager Preview" width="920" />
</p>

## Overview

**Agent Session Manager** is a sleek, high-performance desktop application built with **Tauri 2**, **Svelte 5**, and **Rust**. It provides a unified workspace to discover, inspect, resume, and clean up historical sessions from across modern AI coding agents and CLI tools.

As developers adopt diverse AI agents (Claude Code, OpenAI Codex, OpenCode, OpenClaw, Gemini, Cursor, etc.), session logs and transcripts become scattered in hidden folders, eating up gigabytes of disk space and making past discussions difficult to resume. **Agent Session Manager** brings all agent sessions under one roof with real-time inspection, token analytics, and instant terminal resumption.

---

## Features

- **Multi-Agent Unified Workspace**: Automatically scans and detects sessions across 18+ agent ecosystems.
- **Deep Session Inspection**: View conversation turns, user prompts, assistant answers, thinking processes, and tool call invocations in a clean markdown viewer.
- **Instant Resume in Terminal**: Click the **Resume** button to launch your terminal emulator (Terminal, iTerm2, Ghostty, Alacritty, WezTerm, Kitty) and resume the session with the exact command.
- **Storage & Token Analytics**: Visualize prompt/completion token usage, cache hits, estimated API costs, and disk space occupied by transcripts.
- **Safe Session Cleanup**: Filter and safely purge old, stale, or abandoned agent sessions to reclaim disk space with one click.
- **Native Look & Feel**: Built with Tauri 2 with macOS vibrancy, native title bar overlay, smooth fluid animations, and lightweight memory footprint (< 40MB RAM).
- **Internationalization (i18n)**: Full multilingual support for English, Simplified Chinese, Traditional Chinese, Japanese, and Korean.
- **macOS Hardened Runtime & Deep Code Signing**: Follows strict macOS codesign guidelines with ad-hoc deep signing fallback.

---

## Supported Agents

| Agent / Tool | Type | Key Features |
| :--- | :--- | :--- |
| **Claude Code** | CLI | Full transcript parsing, cost calculation, resume command |
| **OpenAI Codex** | CLI | Session discovery, multi-turn history |
| **OpenCode** | CLI / TUI | Session tree inspection, token tracking |
| **OpenClaw** | CLI / Agent | Conversation log decoding, tool inspection |
| **DeepSeek Helper (DSH)** | CLI / Plugin | Plugin session history & usage metrics |
| **Google Gemini CLI** | CLI | Multi-turn reasoning traces |
| **Antigravity** | Agent | Advanced subagent and orchestrator sessions |
| **WorkBuddy / CodeBuddy** | IDE / CLI | Workspaces, tool executions, task logs |
| **Qwen / Kimi** | CLI | Domestic LLM agent logs & checkpoints |
| **Cursor & Trae** | Editor / Agent | Workspace session state & prompt archives |
| **Grok, Hermes, Pi, ZCode** | Experiments | Custom agent log formats |

---

## Installation

Download the latest prebuilt binaries from [Releases](https://github.com/jockiller/agent-session-manager/releases):

- **macOS**: `Agent-Session-Manager_x.x.x_aarch64.dmg` / `_x64.dmg`
- **Windows**: `Agent-Session-Manager_x.x.x_x64-setup.exe`
- **Linux**: `Agent-Session-Manager_x.x.x_amd64.AppImage` / `.deb`

### macOS Gatekeeper & First Launch

If macOS displays *"Agent Session Manager is damaged and cannot be opened"* or blocks untrusted developers:

1. **Option 1 (System Settings)**: Open `System Settings` > `Privacy & Security` and click `Open Anyway`.
2. **Option 2 (Terminal Command)**:
   ```bash
   sudo xattr -dr com.apple.quarantine "/Applications/Agent Session Manager.app"
   ```

---

## Development

### Prerequisites

- Node.js >= 20
- pnpm >= 9
- Rust >= 1.80
- Platform-specific build tools (Xcode CLI tools on macOS; Build Tools on Windows)

### Quick Start

```bash
# Clone the repository
git clone git@github.com:jockiller/agent-session-manager.git
cd agent-session-manager

# Install dependencies
pnpm install

# Start local dev server with hot reload
pnpm tauri dev
```

### Building & Code Signing

```bash
# Build frontend only
pnpm build

# macOS full build, ad-hoc deep code signing and release packaging
pnpm build:mac

# Manually re-sign an existing .app bundle
pnpm sign:mac
```

---

## License

Released under the [MIT License](LICENSE).
