<div align="center">

# 🧩 SkillHub

[简体中文](README.md) · **English** · [繁體中文](README.zh-TW.md)

A unified manager for Skills and MCP servers — organize the Skills and MCP across every local AI tool in one place, and sync them to all your agents with a single click.

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

## ✨ Features

- **Unified management** — gather the Skills and MCP scattered across your AI tools into one place to view, enable/disable, and search.
- **Real-time sync** — a declarative sync engine pushes enabled Skills/MCP to every installed agent with one click; it only touches the items you manage and never rewrites your other hand-written config.
- **Marketplace** — a built-in catalog aggregating GitHub and the official MCP Registry; search, inspect, and install right inside the app (in-app OAuth, no external backend).
- **One-click import/export** — package all Skills/MCP (optionally with config, associations, and version locks) into zip/tar/json for machine-to-machine migration; imports include a preview, conflict strategies, and path-traversal protection.
- **Import what's already installed** — automatically discover and import the Skills and MCP you already have in each agent, bringing them under unified management.

## 🤖 Supported Agents

SkillHub auto-detects the following local AI tools and reads/writes their Skill and MCP config:

- Claude Code · Claude Desktop
- Cursor · Windsurf · Cline
- VS Code · Gemini CLI · Codex
- Hermes · CodeBuddy · WorkBuddy

## 🖥️ Screens

Dashboard · Marketplace · Installed · Sync Center · Import/Export · Settings — six screens in all, with a restrained teal palette and light/dark themes.

## 🛠️ Tech Stack

- **Backend**: Rust + Tauri v2 (thick core: domain / service / infra / command layering, bundled SQLite, `user_version` migrations)
- **Frontend**: React 18 + TypeScript (strict) + Vite + Tailwind + TanStack Query
- **No self-hosted backend**: marketplace aggregation and auth all run locally in the app

## 🚀 Getting Started

Prerequisites: Rust (stable), Node.js ≥ 18, pnpm, and the Tauri v2 system dependencies (Xcode Command Line Tools on macOS; webkit2gtk etc. on Linux — see the Tauri docs).

```bash
pnpm install        # install frontend deps
pnpm tauri dev      # launch the desktop app (dev mode)
```

Common commands:

```bash
pnpm dev            # frontend Vite dev server only
pnpm test           # frontend unit tests (Vitest)
pnpm typecheck      # frontend type check (tsc --noEmit)
pnpm build          # build the frontend
pnpm tauri build    # produce desktop installers
cd src-tauri && cargo test    # backend tests
cd src-tauri && cargo clippy --all-targets -- -D warnings    # backend lint
```

## 📁 Project Structure

```
src/          frontend (React + TS): pages / components / api / stores / theme
src-tauri/    backend (Rust + Tauri): domain / services / infra / commands
docs/         design specs and implementation plans
prototype/    prototype mockups
```

## 🧭 Status

Currently `0.1.0`, core features working. Known boundaries:

- Social login (GitHub / Google / Microsoft OAuth) requires registering each OAuth app and filling each `client_id` into the constants at the top of `src-tauri/src/services/auth.rs`; until then, the "enter a personal access token (PAT)" path works.
- Some settings (custom storage directories, update channel) are persisted but not yet wired to runtime behavior — the UI labels this honestly.

## 📄 License

[MIT](LICENSE) © 2026 Charles
