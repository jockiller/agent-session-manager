# Agent Session Manager (會話管理器)

<p align="center">
  <img src="./src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Agent Session Manager Logo" />
</p>

<p align="center">
  <strong>一站式 AI 編程智能體與終端助手會話管理控制台</strong>
</p>

<p align="center">
  <a href="#核心特性">核心特性</a> •
  <a href="#已支援的-agent-生態">支援的 Agent</a> •
  <a href="#下載與安裝">下載安裝</a> •
  <a href="#本地開發與構建">開發構建</a> •
  <a href="#macos-安全隔離與簽名">macOS 放行說明</a>
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
  <img src="./docs/preview.png" alt="Agent Session Manager 預覽圖" width="920" />
</p>

## 項目簡介

**Agent Session Manager** 是一款基於 **Tauri 2**、**Svelte 5** 與 **Rust** 構建的輕量級、高效能桌面應用程式。旨在為各類現代 AI 編程智能體（如 Claude Code, OpenAI Codex, OpenCode, OpenClaw, Gemini 等）提供統一的歷史會話檢索、對話回溯、終端恢復以及磁碟清理管理平台。

隨著開發者日常使用的 AI 智能體工具日益多樣，會話記錄分散在各個隱藏資料夾中，不僅佔用數 GB 磁碟空間，且難以快速回顧和繼續歷史任務。**Agent Session Manager** 將它們彙總在一處，提供即時會話瀏覽、Token 消耗統計、工具執行記錄以及一鍵在終端恢復會話的能力。

---

## 核心特性

- **多 Agent 統一聚合**：自動掃描並支援 18+ 款主流 AI 編程助手與實驗工具。
- **深度會話審查**：清晰還原使用者 Prompt、模型回答、思考過程（Thinking Traces）與工具呼叫（Tool Calls）。
- **終端一鍵喚起恢復**：點擊「恢復」按鈕直接複製命令並喚起系統終端機（Terminal、iTerm2、Ghostty、Alacritty 等）繼續執行。
- **Token 與存儲成本看板**：即時彙總 Prompt/Completion Tokens、快取命中率、預估費用及歷史會話所佔磁碟容量。
- **安全批量清理**：按 Agent 平台、時間範圍或檔案大小安全清理陳舊或失效會話，釋放磁碟空間。
- **原生沉浸式桌面體驗**：借助 Tauri 2 實現極速啟動、毛玻璃效果（macOS Vibrancy）、極低記憶體佔用（< 40MB）。
- **完善多語言支援 (i18n)**：內建繁體中文、簡體中文、英語、日語、韓語，預設跟隨系統語言。
- **macOS 安全簽名**：包含 Hardened Runtime 與深簽名兜底，保障網路與本地資源穩定授權。

---

## 下載與安裝

請前往 [Releases 頁面](https://github.com/jockiller/agent-session-manager/releases) 下載適合您作業系統的安裝包：

- **macOS**: `Agent-Session-Manager_x.x.x_aarch64.dmg`（Apple Silicon）/ `_x64.dmg`（Intel）
- **Windows**: `Agent-Session-Manager_x.x.x_x64-setup.exe`
- **Linux**: `Agent-Session-Manager_x.x.x_amd64.AppImage` / `.deb`

### macOS 首次執行與放行說明

macOS Sequoia / Sonoma 對非 App Store 下載的應用實施了安全機制 (Gatekeeper)，如遇「應用已損壞」或「無法驗證開發者」提示：

1. **方式一（系統設定放行）**：打開「系統設定」->「隱私與安全性」，滑到底部點選「仍要打開」。
2. **方式二（終端機命令）**：
   ```bash
   sudo xattr -dr com.apple.quarantine "/Applications/Agent Session Manager.app"
   ```

---

## 開源許可證

本專案採用 [MIT 許可證](LICENSE)。
