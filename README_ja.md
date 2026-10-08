# Agent Session Manager

<p align="center">
  <img src="./src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Agent Session Manager Logo" />
</p>

<p align="center">
  <strong>AI コーディングエージェント & ターミナルアシスタントの統合セッション管理ツール</strong>
</p>

<p align="center">
  <a href="#主な機能">主な機能</a> •
  <a href="#サポートされているエージェント">対応エージェント</a> •
  <a href="#インストール">インストール</a> •
  <a href="#開発とビルド">開発とビルド</a> •
  <a href="#macos-gatekeeper-の解除">macOS の注意事項</a>
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
  <img src="./docs/preview.png" alt="Agent Session Manager プレビュー" width="920" />
</p>

## 概要

**Agent Session Manager** は、**Tauri 2**、**Svelte 5**、および **Rust** で構築された軽量・高速なデスクトップアプリケーションです。多様な AI コーディングエージェント（Claude Code、OpenAI Codex、OpenCode、OpenClaw、Gemini など）のセッション履歴の検索、閲覧、ターミナルでの復元、およびストレージクリーンアップを一元管理できます。

エージェントツールの利用が増えるにつれ、ログやトランスクリプトが隠しフォルダに分散し、ディスク容量を圧迫したり過去のセッションの再開が困難になる問題が発生します。**Agent Session Manager** はこれらを統合し、直感的な UI で快適なエージェント管理を提供します。

---

## 主な機能

- **マルチエージェント統合**: 18 種類以上の主要な AI コーディングエージェントとツールを自動スキャン。
- **詳細なセッションインスペクター**: ユーザープロンプト、AI の回答、思考プロセス、ツール呼び出しを綺麗に可視化。
- **ワンクリックでターミナル復元**: 「復元」ボタンを押すだけで、コマンドをクリップボードにコピーし、お使いのターミナルを自動起動してセッションを即座に再開。
- **トークン & コスト分析**: プロンプト／完了トークン数、キャッシュヒット率、概算 API コスト、ディスク使用量をリアルタイムで集計。
- **安全なセッションクリーンアップ**: 古いまたは不要になったセッションをフィルタリングして安全に一括削除。
- **ネイティブデスクトップ体験**: Tauri 2 を採用し、高速起動、洗練されたダークテーマ、低メモリ消費（< 40MB）を実現。
- **多言語対応 (i18n)**: 英語、簡体字中国語、繁体字中国語、日本語、韓国語をフルサポート。
- **macOS 安全署名対応**: Hardened Runtime とディープコード署名、Gatekeeper 隔離解除スクリプトを同梱。

---

## インストール

[Releases ページ](https://github.com/jockiller/agent-session-manager/releases) からお使いの OS に適したインストーラーをダウンロードしてください：

- **macOS**: `Agent-Session-Manager_x.x.x_aarch64.dmg`（Apple Silicon）/ `_x64.dmg`（Intel）
- **Windows**: `Agent-Session-Manager_x.x.x_x64-setup.exe`
- **Linux**: `Agent-Session-Manager_x.x.x_amd64.AppImage` / `.deb`

### macOS Gatekeeper の解除

「開発元を検証できない」または「壊れているため開けません」と表示される場合：

1. **方法 1（簡単解除）**: 同梱されている `双击解除隔离.command` スクリプトをダブルクリックして実行します。
2. **方法 2（ターミナルコマンド）**:
   ```bash
   sudo xattr -dr com.apple.quarantine "/Applications/Agent Session Manager.app"
   ```
3. **方法 3（システム設定）**: 「システム設定」>「プライバシーとセキュリティ」で「このまま開く」をクリックします。

---

## ライセンス

[MIT License](LICENSE) の下で公開されています。
