<div align="center">

# 🧩 SkillHub

[简体中文](README.md) · [English](README.en.md) · **繁體中文**

Skill 與 MCP 統一管理器 —— 一處管理本機所有 AI 工具的 Skill 與 MCP，一鍵同步到全部 Agent。

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-0.1.0-14b8a6?style=flat-square">
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-v2-24C8DB?style=flat-square&logo=tauri&logoColor=white">
  <img alt="React" src="https://img.shields.io/badge/React-18-61DAFB?style=flat-square&logo=react&logoColor=black">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-stable-000000?style=flat-square&logo=rust&logoColor=white">
  <img alt="TypeScript" src="https://img.shields.io/badge/TypeScript-strict-3178C6?style=flat-square&logo=typescript&logoColor=white">
  <img alt="SQLite" src="https://img.shields.io/badge/SQLite-bundled-003B57?style=flat-square&logo=sqlite&logoColor=white">
  <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-555555?style=flat-square">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-14b8a6?style=flat-square">
</p>

</div>

## ✨ 特性

- **統一管理** — 把散落在各 AI 工具裡的 Skill 與 MCP 收攏到一處，集中檢視、啟用/停用、搜尋。
- **即時同步** — 宣告式同步引擎把已啟用的 Skill/MCP 一鍵下發到所有已安裝的 Agent；只接管你託管的項目，絕不更動你手寫的其它設定。
- **市集下載** — 內建資源中心，聚合 GitHub 與官方 MCP Registry，於應用程式內搜尋、檢視、下載安裝（應用程式內 OAuth，無需外部後端）。
- **一鍵匯入匯出** — 把全部 Skill/MCP（可含設定、關聯、版本鎖）打包成 zip/tar/json 跨機遷移；匯入附預覽、衝突策略與防目錄穿越檢查。
- **匯入已安裝項** — 自動探索並匯入你在各 Agent 裡已經裝好的 Skill 與 MCP，納入統一管理。

## 🤖 支援的 Agent

自動偵測本機以下 AI 工具，並讀寫其 Skill 與 MCP 設定：

- Claude Code · Claude Desktop
- Cursor · Windsurf · Cline
- VS Code · Gemini CLI
- Codex · Hermes

## 🖥️ 介面

首頁（總覽）· 資源中心（市集）· 已安裝 · Agent 同步 · 匯入匯出 · 設定，共六個畫面；青綠(teal)克制配色，支援亮/暗主題。

## 🛠️ 技術棧

- **後端**：Rust + Tauri v2（厚核心：領域/服務/基礎設施/命令分層，內建 SQLite，`user_version` 遷移）
- **前端**：React 18 + TypeScript(strict) + Vite + Tailwind + TanStack Query
- **無自建後端**：市集聚合與認證皆在本機應用程式內完成

## 🚀 快速開始

前置需求：Rust(stable)、Node.js ≥ 18、pnpm，以及 Tauri v2 的系統相依套件（macOS 需 Xcode Command Line Tools；Linux 需 webkit2gtk 等，詳見 Tauri 官方文件）。

```bash
pnpm install        # 安裝前端相依套件
pnpm tauri dev      # 啟動桌面應用程式(開發模式)
```

常用指令：

```bash
pnpm dev            # 僅啟動前端 Vite dev server
pnpm test           # 前端單元測試(Vitest)
pnpm typecheck      # 前端型別檢查(tsc --noEmit)
pnpm build          # 建置前端
pnpm tauri build    # 產生桌面安裝檔
cd src-tauri && cargo test    # 後端測試
cd src-tauri && cargo clippy --all-targets -- -D warnings    # 後端 Lint
```

## 📁 專案結構

```
src/          前端(React + TS)：pages / components / api / stores / theme
src-tauri/    後端(Rust + Tauri)：domain / services / infra / commands
docs/         設計文件與實作計畫
prototype/    原型圖
```

## 🧭 狀態

目前 `0.1.0`，核心功能可用。已知邊界：

- 社群登入（GitHub / Google / Microsoft OAuth）需先各自註冊 OAuth 應用程式並填入憑證（見 `OAUTH_SETUP.md`）；在此之前可用"手動輸入存取權杖(PAT)"路徑。
- 部分設定（自訂儲存目錄、更新通道）目前僅持久化保存，尚未接入執行期行為，介面已如實標註。

## 📄 授權

[MIT](LICENSE) © 2026 Charles
