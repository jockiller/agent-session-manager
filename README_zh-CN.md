# Agent Session Manager (会话管理器)

<p align="center">
  <img src="./src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Agent Session Manager Logo" />
</p>

<p align="center">
  <strong>一站式 AI 编程智能体与终端助手会话管理控制台</strong>
</p>

<p align="center">
  <a href="#核心特性">核心特性</a> •
  <a href="#已支持的-agent-生态">支持的 Agent</a> •
  <a href="#下载与安装">下载安装</a> •
  <a href="#本地开发与构建">开发构建</a> •
  <a href="#macos-安全隔离与签名">macOS 放行说明</a>
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
  <img src="./docs/preview.png" alt="Agent Session Manager 预览图" width="920" />
</p>

## 项目介绍

**Agent Session Manager** 是一款基于 **Tauri 2**、**Svelte 5** 与 **Rust** 构建的轻量级、高性能桌面客户端。旨在为各类现代 AI 编程智能体（如 Claude Code, OpenAI Codex, OpenCode, OpenClaw, Gemini 等）提供统一的历史会话检索、对话回溯、终端恢复以及磁盘清理管理平台。

随着开发者日常使用的 AI 智能体工具日益多样，会话记录分散在各个隐藏文件夹中，不仅占用数 GB 磁盘空间，且难以快速回顾和继续历史任务。**Agent Session Manager** 将它们汇总在一处，提供实时会话浏览、Token 消耗统计、工具执行记录以及一键在终端恢复会话的能力。

---

## 核心特性

- **多 Agent 统一聚合**：自动扫描并支持 18+ 款主流 AI 编程助手与实验工具。
- **深度会话审查**：清晰还原用户 Prompt、模型回答、思考过程（Thinking Traces）与工具调用（Tool Calls）。
- **终端一键唤起恢复**：点击“恢复”按钮直接复制命令并唤起系统终端（Terminal、iTerm2、Ghostty、Alacritty 等）继续执行。
- **Token 与存储成本看板**：实时汇总 Prompt/Completion Tokens、缓存命中率、预估费用及历史会话所占磁盘容量。
- **安全批量清理**：按 Agent 平台、时间范围或文件大小安全清理陈旧或失效会话，释放磁盘空间。
- **原生沉浸式桌面体验**：借助 Tauri 2 实现极速启动、毛玻璃效果（macOS Vibrancy）、极低内存占用（< 40MB）。
- **完善多语言支持 (i18n)**：内置简体中文、繁体中文、英语、日语、韩语，默认跟随系统语言。
- **macOS 安全签名与隔离解除**：包含 Hardened Runtime 与深签名兜底，并提供双击解除 Gatekeeper 隔离脚本。

---

## 已支持的 Agent 生态

| 平台 / 工具 | 类型 | 核心能力 |
| :--- | :--- | :--- |
| **Claude Code** | 官方 CLI | 全量 JSONL 日志解析、费用与 Token 统计、一键终端恢复 |
| **OpenAI Codex** | CLI / SDK | 会话自动扫描、多轮对话结构化查看 |
| **OpenCode** | 终端 TUI | 会话树解析、消耗统计与命令恢复 |
| **OpenClaw** | 智能体框架 | 对话记录解码、工具调用参数透视 |
| **DeepSeek Helper (DSH)** | CLI 插件 | 插件会话历史与调用指标展示 |
| **Google Gemini CLI** | CLI | 推理轨迹解析与多步骤记录 |
| **Antigravity** | Agent 平台 | 子智能体（Subagent）与编排日志解析 |
| **WorkBuddy / CodeBuddy** | 研发助手 | 工作区会话、工具调用与步骤追踪 |
| **通义千问 (Qwen) / Kimi** | CLI / 模型 | 国内主流模型智能体会话适配 |
| **Cursor & Trae** | 编辑器智能体 | 工作区对话归档与历史提示词提取 |
| **Grok, Hermes, Pi, ZCode** | 实验平台 | 专有日志格式解析与展示 |

---

## 下载与安装

请前往 [Releases 页面](https://github.com/jockiller/agent-session-manager/releases) 下载适合您操作系统的安装包：

- **macOS**: `Agent-Session-Manager_x.x.x_aarch64.dmg`（Apple Silicon）/ `_x64.dmg`（Intel）
- **Windows**: `Agent-Session-Manager_x.x.x_x64-setup.exe`
- **Linux**: `Agent-Session-Manager_x.x.x_amd64.AppImage` / `.deb`

### macOS 首次运行与放行说明

macOS Sequoia / Sonoma 对非 App Store 下载的应用实施了严格的安全隔离 (Gatekeeper)，如遇“应用已损坏”或“无法验证开发者”提示：

1. **方式一（一键解除）**：双击运行安装包配套提供的 `双击解除隔离.command` 脚本。
2. **方式二（终端命令）**：
   ```bash
   sudo xattr -dr com.apple.quarantine "/Applications/Agent Session Manager.app"
   ```
3. **方式三（系统设置）**：打开「系统设置」->「隐私与安全性」，滑到底部点击「仍要打开」。

---

## 本地开发与构建

### 环境要求

- Node.js >= 20
- pnpm >= 9
- Rust >= 1.80

### 快速开始

```bash
# 克隆仓库
git clone git@github.com:jockiller/agent-session-manager.git
cd agent-session-manager

# 安装依赖
pnpm install

# 启动本地开发模式（热重载）
pnpm tauri dev
```

### 构建与打包

```bash
# 仅构建前端
pnpm build

# macOS 完整打包、签名与发布包整理
pnpm build:mac

# 独立运行 macOS 代码签名脚本
pnpm sign:mac
```

---

## 开源协议

本项目采用 [MIT 许可证](LICENSE)。
