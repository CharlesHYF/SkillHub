<div align="center">

# 🧩 SkillHub

**简体中文** · [English](README.en.md) · [繁體中文](README.zh-TW.md)

Skill 与 MCP 统一管理器 —— 一处管理本机所有 AI 工具的 Skill 与 MCP，一键同步到全部 Agent。

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

- **统一管理** — 把散落在各 AI 工具里的 Skill 与 MCP 收拢到一处，集中查看、启用/停用、检索。
- **实时同步** — 声明式同步引擎把已启用的 Skill/MCP 一键下发到所有已安装的 Agent；只接管你托管的项，绝不改动你手写的其它配置。
- **市场下载** — 内置资源中心，聚合 GitHub 与官方 MCP Registry，应用内搜索、查看、下载安装（应用内 OAuth，无需外部后端）。
- **一键导入导出** — 把全部 Skill/MCP（可含配置、关联、版本锁）打包为 zip/tar/json 跨机迁移；导入带预览、冲突策略与防目录穿越校验。
- **导入已装** — 自动发现并导入你在各 Agent 里已经装好的 Skill 与 MCP，纳入统一管理。

## 🤖 支持的 Agent

自动探测本机以下 AI 工具，并读写其 Skill 与 MCP 配置：

- Claude Code · Claude Desktop
- Cursor · Windsurf · Cline
- VS Code · Gemini CLI
- Codex · Hermes

## 🖥️ 界面

首页（概览）· 资源中心（市场）· 已安装 · Agent 同步 · 导入导出 · 设置，共六屏；青绿(teal)克制配色，支持亮/暗主题。

## 🛠️ 技术栈

- **后端**：Rust + Tauri v2（厚核心：领域/服务/基础设施/命令分层，SQLite 内置，`user_version` 迁移）
- **前端**：React 18 + TypeScript(strict) + Vite + Tailwind + TanStack Query
- **无自建后端**：市场聚合与认证均在本机应用内完成

## 🚀 快速开始

前置依赖：Rust(stable)、Node.js ≥ 18、pnpm，以及 Tauri v2 的系统依赖（macOS 需 Xcode Command Line Tools；Linux 需 webkit2gtk 等，详见 Tauri 官方文档）。

```bash
pnpm install        # 安装前端依赖
pnpm tauri dev      # 启动桌面应用(开发模式)
```

常用命令：

```bash
pnpm dev            # 仅启动前端 Vite dev server
pnpm test           # 前端单测(Vitest)
pnpm typecheck      # 前端类型检查(tsc --noEmit)
pnpm build          # 构建前端
pnpm tauri build    # 打桌面安装包
cd src-tauri && cargo test    # 后端测试
cd src-tauri && cargo clippy --all-targets -- -D warnings    # 后端 Lint
```

## 📁 项目结构

```
src/          前端(React + TS)：pages / components / api / stores / theme
src-tauri/    后端(Rust + Tauri)：domain / services / infra / commands
docs/         设计文档与实现计划
prototype/    原型图
```

## 🧭 状态

当前 `0.1.0`，核心功能可用。已知边界：

- 社交登录（GitHub / Google / Microsoft OAuth）需先各自注册 OAuth 应用并填入凭据（见 `OAUTH_SETUP.md`）；在此之前可用"手动录入访问令牌(PAT)"路径。
- 部分设置（自定义存储目录、更新通道）当前仅持久化留存，尚未接入运行时行为，界面已如实标注。

## 📄 许可

[MIT](LICENSE) © 2026 Charles

选 MIT 的原因：它是最简洁、最宽松的主流开源协议 —— 任何人可自由使用、修改、分发乃至商用，仅需保留版权声明，法律负担与采纳门槛都最低；且与本项目所依赖生态（React、Tauri、绝大多数 npm/cargo 包）的主流协议一致，二次开发与集成无摩擦。若日后更看重专利授权保护，可再评估 Apache-2.0。
