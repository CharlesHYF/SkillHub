# SkillHub

Skill 与 MCP 统一管理器 —— 跨平台桌面应用，统一管理本机各 AI 工具的 Skill 与 MCP，并一键同步到所有已安装的 Agent。

技术栈：Rust + Tauri v2 + React 18 + TypeScript。

设计文档见 `docs/superpowers/specs/`，实现计划见 `docs/superpowers/plans/`。

## 开发

- 安装依赖：`pnpm install`
- 启动桌面应用（开发模式）：`pnpm tauri dev`
- 仅启动前端 dev server：`pnpm dev`
- 前端单测：`pnpm vitest run`
- 前端类型检查：`pnpm tsc --noEmit`
- 前端格式化 / 校验：`pnpm format` / `pnpm format:check`
- 后端测试：`cd src-tauri && cargo test`
- 后端格式化校验 / Lint：`cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- 构建：`pnpm build`（前端）/ `pnpm tauri build`（桌面安装包）
