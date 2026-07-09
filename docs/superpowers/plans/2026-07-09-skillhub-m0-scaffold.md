# SkillHub M0 脚手架 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭出可运行、可导航的 Tauri v2 + React/TS 应用外壳，含 SQLite 初始化与迁移、亮暗主题、中英 i18n、侧栏 + 7 个空路由页。

**Architecture:** 厚 Rust 后端（infra 层的 SQLite store + commands 层）+ React 前端（App Shell + React Router）。前端经 Tauri command 调用后端；M0 只打通一个 health 命令验证链路，其余业务留待 M1+。

**Tech Stack:** Tauri v2、Rust（rusqlite bundled）、React 18、TypeScript（strict）、Vite、React Router、Zustand、TanStack Query、Tailwind CSS、shadcn/ui、i18next、lucide-react、Vitest + @testing-library/react。

## Global Constraints

以下为项目级约束，每个 Task 都隐含遵守（取自 spec 与 charles-coding）：

- 版本底线：Tauri v2、React 18、TypeScript strict 模式。
- 缩进：默认 Tab（`indent_size=4`）；JSON/YAML/SQL 2 空格；Python/Kotlin 4 空格。Rust 用 rustfmt 且 `hard_tabs = true`；前端 Prettier `useTabs: true`。行尾统一 LF，编码 UTF-8。由 `.editorconfig` + `.gitattributes` 强制。
- 注释：中文。所有源码文件顶部含注释块（文件作用 + 创建日期 `YYYY-MM-DD`，今日 2026-07-09）。所有函数/方法注释功能。禁止「」弯角引号，统一用 `""` 或不加。
- Markdown：禁用 `---`/`***`/`___` 分割线。
- 数据库：遵循阿里巴巴 Java 开发手册（泰山版）设计规约，SQLite 按类型亲和性等价映射（详见 spec 第 4.2 节）。
- Git：作者与提交者恒为 `Charles <w1400214654@outlook.com>`；禁止任何 AI 联合署名或水印；所有产出在 `agents/feature/*` 分支，由 Charles 合并。提交信息用 Conventional Commits。
- 测试：核心逻辑覆盖率 ≥80%；关键路径与公共组件必测。
- 去掉原型底部 "Rust + React + Tauri" 一行（外壳不得包含技术栈页脚）。
- 分支：本里程碑在 `agents/feature/skillhub-mvp`（已创建）。

## 文件结构（M0 结束时）

```
SkillHub/
├── .editorconfig .gitattributes .gitignore        # 取自 charles-coding 模板
├── .github/pull_request_template.md               # 取自模板
├── .prettierrc.json  eslint.config.js             # 前端格式/lint
├── package.json  pnpm-lock.yaml  tsconfig.json  tsconfig.node.json
├── vite.config.ts  index.html  postcss.config.js  tailwind.config.ts
├── vitest.config.ts  vitest.setup.ts
├── src/
│   ├── main.tsx                                    # React 入口
│   ├── App.tsx                                     # Provider + Router 装配
│   ├── routes.tsx                                  # 路由表(7 条)
│   ├── index.css                                   # Tailwind 指令 + 设计令牌(CSS 变量)
│   ├── api/index.ts                                # Tauri command 类型化封装(appHealth)
│   ├── api/index.test.ts
│   ├── theme/theme-provider.tsx                    # 亮暗主题 Provider
│   ├── theme/theme-provider.test.tsx
│   ├── i18n/index.ts  i18n/zh.json  i18n/en.json   # i18next 初始化 + 中英文案
│   ├── components/layout/app-shell.tsx             # 侧栏 + 内容区
│   ├── components/layout/sidebar.tsx               # 导航
│   ├── components/layout/sidebar.test.tsx
│   ├── components/layout/nav-config.ts             # 7 个导航项配置
│   └── pages/{dashboard,marketplace,installed,sync-center,portability,settings}.tsx
└── src-tauri/
    ├── Cargo.toml  tauri.conf.json  build.rs  rustfmt.toml
    ├── capabilities/default.json  icons/
    ├── migrations/0001_init.sql                    # 10 张表
    └── src/
        ├── main.rs                                 # 桌面入口
        ├── lib.rs                                  # Tauri Builder 装配 + AppState
        ├── infra/mod.rs  infra/store.rs            # SQLite 连接 + 迁移执行器
        └── commands/mod.rs  commands/health.rs     # health 命令
```

设计单元边界：`infra/store.rs` 只管数据库连接与迁移；`commands/*` 是薄封装，把纯逻辑函数包成 `#[tauri::command]`；前端 `api/` 是唯一调用后端的层。

### Task 1: 脚手架与仓库配置

**Files:**
- Create: `package.json`, `vite.config.ts`, `index.html`, `tsconfig.json`, `tsconfig.node.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `src-tauri/icons/*`（由脚手架生成）
- Create: `.editorconfig`, `.gitattributes`, `.gitignore`, `.github/pull_request_template.md`（取自模板）
- Create: `src-tauri/rustfmt.toml`, `.prettierrc.json`

**Interfaces:**
- Produces: 一个能 `pnpm tauri dev` 启动空白窗口的工程；后续 Task 在此之上加文件。

- [ ] **Step 1: 用官方脚手架在临时目录生成 Tauri v2 + react-ts 模板**

Run:
```bash
cd /tmp && rm -rf skillhub-scaffold-tmp
pnpm create tauri-app@latest skillhub-scaffold-tmp --template react-ts --manager pnpm
```
说明：若该 CLI 版本的非交互参数不同，先跑 `pnpm create tauri-app@latest --help` 确认 `--template`/`--manager` 写法。期望生成含 `src/`、`src-tauri/`、`package.json`、`vite.config.ts` 的目录。

- [ ] **Step 2: 将脚手架产物拷入仓库根目录（保留已有 README/prototype/docs/.git）**

Run:
```bash
cd /tmp/skillhub-scaffold-tmp
cp -R src-tauri /Library/CodeProject/SkillHub/
cp -R src /Library/CodeProject/SkillHub/
cp package.json vite.config.ts index.html tsconfig.json tsconfig.node.json /Library/CodeProject/SkillHub/
```
说明：脚手架自带的 `src/` 会在 Task 4/5 被覆盖为我们的外壳，这里先整体拷入以获得可运行基线。

- [ ] **Step 3: 覆盖仓库配置文件（取自 charles-coding 模板）**

Run:
```bash
TPL=/Users/hola/.claude/skills/charles-coding/reference/project-template
cp "$TPL/.editorconfig" "$TPL/.gitattributes" "$TPL/.gitignore" /Library/CodeProject/SkillHub/
mkdir -p /Library/CodeProject/SkillHub/.github
cp "$TPL/.github/pull_request_template.md" /Library/CodeProject/SkillHub/.github/
```
然后在 `.gitignore` 末尾追加 Tauri 相关忽略项（`src-tauri/target/` 已被 `target/` 覆盖，无需重复；补充 `src-tauri/gen/`）：
```
# --- Tauri ---
src-tauri/gen/
```

- [ ] **Step 4: 新增 `src-tauri/rustfmt.toml`（用 Tab 落地缩进约定）**

Create `src-tauri/rustfmt.toml`:
```toml
# 文件作用: rustfmt 配置, 用硬 Tab 落地 charles-coding 的 Tab 缩进约定
# 创建日期: 2026-07-09
hard_tabs = true
edition = "2021"
```

- [ ] **Step 5: 新增 `.prettierrc.json`（前端用 Tab）**

Create `.prettierrc.json`:
```json
{
  "useTabs": true,
  "semi": true,
  "singleQuote": true,
  "trailingComma": "all",
  "printWidth": 100
}
```

- [ ] **Step 6: 设 TypeScript 严格模式**

修改 `tsconfig.json`，确保 `compilerOptions` 含：
```json
{
  "compilerOptions": {
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  }
}
```

- [ ] **Step 7: 安装依赖并验证后端可编译**

Run:
```bash
cd /Library/CodeProject/SkillHub && pnpm install
cd src-tauri && cargo build
```
Expected: `pnpm install` 成功；`cargo build` 以 `Finished` 结束，无 error。

- [ ] **Step 8: 验证应用可启动（可导航前的空白基线）**

Run: `cd /Library/CodeProject/SkillHub && pnpm tauri dev`
Expected: 弹出一个 Tauri 窗口显示脚手架默认页；确认后 Ctrl-C 关闭。

- [ ] **Step 9: 提交**

```bash
cd /Library/CodeProject/SkillHub
git add -A
git commit -m "chore: 初始化 Tauri v2 + React/TS 脚手架与仓库配置"
```

### Task 2: SQLite 连接与迁移执行器（TDD）

**Files:**
- Create: `src-tauri/migrations/0001_init.sql`
- Create: `src-tauri/src/infra/mod.rs`, `src-tauri/src/infra/store.rs`
- Modify: `src-tauri/Cargo.toml`（加 `rusqlite`）, `src-tauri/src/lib.rs`（挂载 infra 模块）

**Interfaces:**
- Produces: `infra::store::open_and_migrate(path: &std::path::Path) -> rusqlite::Result<rusqlite::Connection>`；内部 `migrate(&Connection) -> rusqlite::Result<()>`。

- [ ] **Step 1: 加依赖**

修改 `src-tauri/Cargo.toml`，在 `[dependencies]` 增加：
```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

- [ ] **Step 2: 写迁移 SQL（10 张表，取自 spec 第 4.3 节）**

Create `src-tauri/migrations/0001_init.sql`，内容为 spec `docs/superpowers/specs/2026-07-09-skillhub-design.md` 第 4.3 节的完整 10 表 DDL（resource、agent、resource_agent、sync_run、sync_item、market_cache、auth_account、import_export_log、setting、activity_log），逐字复制。文件顶部加注释：
```sql
-- 文件作用: SkillHub 初始数据库结构(10 张表), 遵循阿里巴巴泰山版规约
-- 创建日期: 2026-07-09
```

- [ ] **Step 3: 写失败测试**

Create `src-tauri/src/infra/store.rs`（先只放测试与空实现签名）:
```rust
// 文件作用: SQLite 连接管理与数据库迁移执行
// 创建日期: 2026-07-09

use rusqlite::Connection;
use std::path::Path;

/// 迁移脚本表: (版本号, SQL 内容), 按版本升序执行
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../../migrations/0001_init.sql"))];

/// 打开数据库并执行迁移, 返回可用连接
pub fn open_and_migrate(path: &Path) -> rusqlite::Result<Connection> {
	let conn = Connection::open(path)?;
	migrate(&conn)?;
	Ok(conn)
}

/// 按 PRAGMA user_version 增量执行未应用的迁移
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
	unimplemented!()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 迁移后 10 张业务表应全部存在
	#[test]
	fn migrate_creates_all_tables() {
		let conn = Connection::open_in_memory().unwrap();
		migrate(&conn).unwrap();
		let n: i64 = conn
			.query_row(
				"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
				 ('resource','agent','resource_agent','sync_run','sync_item','market_cache',\
				  'auth_account','import_export_log','setting','activity_log')",
				[],
				|r| r.get(0),
			)
			.unwrap();
		assert_eq!(n, 10);
	}

	/// 迁移应幂等: 重复执行不报错, 版本停在 1
	#[test]
	fn migrate_is_idempotent() {
		let conn = Connection::open_in_memory().unwrap();
		migrate(&conn).unwrap();
		migrate(&conn).unwrap();
		let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
		assert_eq!(v, 1);
	}
}
```
Create `src-tauri/src/infra/mod.rs`:
```rust
// 文件作用: 基础设施层模块聚合
// 创建日期: 2026-07-09
pub mod store;
```
在 `src-tauri/src/lib.rs` 顶部加 `mod infra;`（若 lib.rs 尚无模块声明区，加在 use 之后）。

- [ ] **Step 3b: 运行测试确认失败**

Run: `cd src-tauri && cargo test infra::store`
Expected: 编译通过但测试 panic（`unimplemented!`）或未通过。

- [ ] **Step 4: 实现 migrate**

把 `store.rs` 中的 `migrate` 替换为：
```rust
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
	let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
	for (ver, sql) in MIGRATIONS {
		if *ver > current {
			conn.execute_batch(sql)?;
			// user_version 不支持参数绑定, 用格式化写入(ver 为常量整数, 无注入风险)
			conn.pragma_update(None, "user_version", *ver)?;
		}
	}
	Ok(())
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test infra::store`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/migrations src-tauri/src/infra src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: SQLite 迁移执行器与初始 10 表结构"
```

### Task 3: health 命令与前端调用层（TDD）

**Files:**
- Create: `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/health.rs`
- Modify: `src-tauri/src/lib.rs`（注册命令 + 初始化数据库到 AppState）
- Create: `src/api/index.ts`, `src/api/index.test.ts`
- Create: `vitest.config.ts`, `vitest.setup.ts`

**Interfaces:**
- Consumes: `infra::store::open_and_migrate`（Task 2）。
- Produces: Rust `commands::health::app_health() -> AppHealth`（`AppHealth { version: String, db_ok: bool }`，`#[derive(serde::Serialize)]`）；纯函数 `build_health(version: &str, db_ok: bool) -> AppHealth`。前端 `appHealth(): Promise<AppHealth>`，TS 类型 `AppHealth { version: string; dbOk: boolean }`。

- [ ] **Step 1: 写 Rust 失败测试**

Create `src-tauri/src/commands/health.rs`:
```rust
// 文件作用: 应用健康检查命令(M0 用于打通前后端调用链路)
// 创建日期: 2026-07-09

use serde::Serialize;

/// 健康信息: 应用版本 + 数据库是否就绪
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppHealth {
	pub version: String,
	pub db_ok: bool,
}

/// 纯逻辑: 组装健康信息(便于单测, 与 Tauri 运行时解耦)
pub fn build_health(version: &str, db_ok: bool) -> AppHealth {
	AppHealth { version: version.to_string(), db_ok }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn build_health_maps_fields() {
		let h = build_health("0.1.0", true);
		assert_eq!(h.version, "0.1.0");
		assert!(h.db_ok);
	}
}
```
Create `src-tauri/src/commands/mod.rs`:
```rust
// 文件作用: Tauri 命令层模块聚合
// 创建日期: 2026-07-09
pub mod health;
```
并在 `src-tauri/src/lib.rs` 增加一行 `mod commands;` 声明该模块（此步先不改动脚手架默认的 `run()`，仅让下一步的测试能编译到 `commands::health`）。

- [ ] **Step 2: 运行测试确认通过（纯函数先行）**

Run: `cd src-tauri && cargo test commands::health`
Expected: `1 passed`。

- [ ] **Step 3: 加 #[tauri::command] 包装并注册**

在 `health.rs` 末尾（tests 之前）加命令包装：
```rust
/// Tauri 命令: 返回应用健康信息
#[tauri::command]
pub fn app_health(state: tauri::State<'_, crate::AppState>) -> AppHealth {
	build_health(env!("CARGO_PKG_VERSION"), state.db_ok)
}
```
修改 `src-tauri/src/lib.rs`：声明 `mod commands;`，定义 `AppState`，在 `setup` 中初始化数据库，注册命令：
```rust
mod commands;
mod infra;

/// 应用共享状态
pub struct AppState {
	pub db_ok: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.setup(|app| {
			// 初始化数据库(路径: 应用数据目录/skillhub.db)
			let dir = app.path().app_data_dir().expect("无法获取应用数据目录");
			std::fs::create_dir_all(&dir).ok();
			let db_ok = infra::store::open_and_migrate(&dir.join("skillhub.db")).is_ok();
			app.manage(AppState { db_ok });
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![commands::health::app_health])
		.run(tauri::generate_context!())
		.expect("运行 Tauri 应用失败");
}
```
说明：`use tauri::Manager;` 需在文件顶部引入以使用 `path()`/`manage()`。

- [ ] **Step 4: 确认后端编译**

Run: `cd src-tauri && cargo build`
Expected: `Finished`，无 error。

- [ ] **Step 5: 写前端调用层与失败测试**

Create `src/api/index.ts`:
```typescript
// 文件作用: Tauri command 的类型化封装层(前端唯一调用后端的入口)
// 创建日期: 2026-07-09
import { invoke } from '@tauri-apps/api/core';

/** 应用健康信息 */
export interface AppHealth {
	version: string;
	dbOk: boolean;
}

/** 调用后端 app_health 命令 */
export async function appHealth(): Promise<AppHealth> {
	return invoke<AppHealth>('app_health');
}
```
Create `src/api/index.test.ts`:
```typescript
// 文件作用: api 层单测
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => ({ version: '0.1.0', dbOk: true })),
}));

import { appHealth } from './index';

describe('appHealth', () => {
	it('返回后端健康信息', async () => {
		const h = await appHealth();
		expect(h.version).toBe('0.1.0');
		expect(h.dbOk).toBe(true);
	});
});
```

- [ ] **Step 6: 安装并配置 Vitest, 运行 api 测试**

Run:
```bash
cd /Library/CodeProject/SkillHub
pnpm add -D vitest @testing-library/react @testing-library/jest-dom jsdom
```
Create `vitest.config.ts`:
```typescript
// 文件作用: Vitest 配置(jsdom 环境 + setup)
// 创建日期: 2026-07-09
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
	plugins: [react()],
	test: { environment: 'jsdom', setupFiles: ['./vitest.setup.ts'], globals: true },
});
```
Create `vitest.setup.ts`:
```typescript
// 文件作用: 测试全局 setup(jest-dom 断言 + matchMedia 兜底)
// 创建日期: 2026-07-09
import '@testing-library/jest-dom';

if (!window.matchMedia) {
	window.matchMedia = ((q: string) => ({
		matches: false,
		media: q,
		onchange: null,
		addEventListener: () => {},
		removeEventListener: () => {},
		addListener: () => {},
		removeListener: () => {},
		dispatchEvent: () => false,
	})) as unknown as typeof window.matchMedia;
}
```
Run: `pnpm vitest run src/api`
Expected: `1 passed`。（Task 4/5 的前端测试复用这份配置。）

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/commands src-tauri/src/lib.rs src/api
git commit -m "feat: health 命令打通前后端调用链路"
```

### Task 4: 设计令牌与亮暗主题 Provider（TDD）

**Files:**
- Create: `src/index.css`（Tailwind 指令 + CSS 变量令牌）
- Create: `tailwind.config.ts`, `postcss.config.js`
- Create: `src/theme/theme-provider.tsx`, `src/theme/theme-provider.test.tsx`
- Modify: `package.json`（加 tailwind/postcss/autoprefixer）

**Interfaces:**
- Produces: `ThemeProvider`（React 组件，包裹应用，读取系统偏好并在 `document.documentElement` 上打 `data-theme="light|dark"`）；`useTheme(): { theme: 'light'|'dark'; toggle: () => void }`。

- [ ] **Step 1: 安装 Tailwind 与 PostCSS**

Run:
```bash
cd /Library/CodeProject/SkillHub
pnpm add -D tailwindcss postcss autoprefixer
```

- [ ] **Step 2: 配置 Tailwind（映射设计令牌）**

Create `tailwind.config.ts`:
```typescript
// 文件作用: Tailwind 配置, 将 SkillHub 设计令牌接入工具类
// 创建日期: 2026-07-09
import type { Config } from 'tailwindcss';

export default {
	content: ['./index.html', './src/**/*.{ts,tsx}'],
	theme: {
		extend: {
			colors: {
				brand: { DEFAULT: 'var(--sh-brand)', deep: 'var(--sh-brand-deep)' },
				skill: 'var(--sh-skill)',
				mcp: 'var(--sh-mcp)',
				ok: 'var(--sh-ok)',
				warn: 'var(--sh-warn)',
				danger: 'var(--sh-danger)',
				info: 'var(--sh-info)',
			},
		},
	},
	plugins: [],
} satisfies Config;
```
Create `postcss.config.js`:
```javascript
// 文件作用: PostCSS 配置
// 创建日期: 2026-07-09
export default {
	plugins: { tailwindcss: {}, autoprefixer: {} },
};
```

- [ ] **Step 3: 写设计令牌（spec 第 10 节配色，亮暗双主题）**

Create `src/index.css`:
```css
/* 文件作用: 全局样式与 SkillHub 设计令牌(亮暗双主题) */
/* 创建日期: 2026-07-09 */
@tailwind base;
@tailwind components;
@tailwind utilities;

:root,
:root[data-theme='light'] {
	--sh-brand: #6366f1;
	--sh-brand-deep: #4f46e5;
	--sh-skill: #8b5cf6;
	--sh-mcp: #14b8a6;
	--sh-ok: #10b981;
	--sh-warn: #f59e0b;
	--sh-danger: #ef4444;
	--sh-info: #3b82f6;
	--sh-bg: #ffffff;
	--sh-fg: #0f172a;
	--sh-muted: #64748b;
	--sh-border: #e2e8f0;
}

:root[data-theme='dark'] {
	--sh-brand: #818cf8;
	--sh-brand-deep: #6366f1;
	--sh-skill: #a78bfa;
	--sh-mcp: #2dd4bf;
	--sh-ok: #34d399;
	--sh-warn: #fbbf24;
	--sh-danger: #f87171;
	--sh-info: #60a5fa;
	--sh-bg: #0f172a;
	--sh-fg: #e2e8f0;
	--sh-muted: #94a3b8;
	--sh-border: #1e293b;
}

body {
	background: var(--sh-bg);
	color: var(--sh-fg);
}
```

- [ ] **Step 4: 写主题 Provider 失败测试**

Create `src/theme/theme-provider.test.tsx`:
```tsx
// 文件作用: 主题 Provider 单测
// 创建日期: 2026-07-09
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThemeProvider, useTheme } from './theme-provider';

function Probe() {
	const { theme, toggle } = useTheme();
	return (
		<button onClick={toggle} data-testid="btn">
			{theme}
		</button>
	);
}

describe('ThemeProvider', () => {
	beforeEach(() => document.documentElement.removeAttribute('data-theme'));

	it('默认在根元素写入 data-theme', () => {
		render(
			<ThemeProvider>
				<Probe />
			</ThemeProvider>,
		);
		expect(document.documentElement.getAttribute('data-theme')).toMatch(/light|dark/);
	});

	it('toggle 在亮暗之间切换', () => {
		render(
			<ThemeProvider>
				<Probe />
			</ThemeProvider>,
		);
		const before = screen.getByTestId('btn').textContent;
		fireEvent.click(screen.getByTestId('btn'));
		const after = screen.getByTestId('btn').textContent;
		expect(after).not.toBe(before);
		expect(document.documentElement.getAttribute('data-theme')).toBe(after);
	});
});
```

- [ ] **Step 5: 运行确认失败**

Run: `pnpm vitest run src/theme`
Expected: FAIL（模块 `./theme-provider` 不存在）。

- [ ] **Step 6: 实现主题 Provider**

Create `src/theme/theme-provider.tsx`:
```tsx
// 文件作用: 亮暗主题 Provider, 读系统偏好并在根元素打 data-theme
// 创建日期: 2026-07-09
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';

type Theme = 'light' | 'dark';
interface ThemeCtx {
	theme: Theme;
	toggle: () => void;
}

const Ctx = createContext<ThemeCtx | null>(null);

/** 读取系统偏好作为初始主题 */
function systemTheme(): Theme {
	return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/** 主题 Provider: 管理亮暗状态并同步到根元素 */
export function ThemeProvider({ children }: { children: ReactNode }) {
	const [theme, setTheme] = useState<Theme>(systemTheme);

	useEffect(() => {
		document.documentElement.setAttribute('data-theme', theme);
	}, [theme]);

	const toggle = () => setTheme((t) => (t === 'dark' ? 'light' : 'dark'));
	return <Ctx.Provider value={{ theme, toggle }}>{children}</Ctx.Provider>;
}

/** 主题 Hook */
export function useTheme(): ThemeCtx {
	const c = useContext(Ctx);
	if (!c) throw new Error('useTheme 必须在 ThemeProvider 内使用');
	return c;
}
```

- [ ] **Step 7: 运行确认通过**

Run: `pnpm vitest run src/theme`
Expected: `2 passed`。

- [ ] **Step 8: 提交**

```bash
git add src/index.css src/theme tailwind.config.ts postcss.config.js package.json pnpm-lock.yaml
git commit -m "feat: 设计令牌与亮暗主题 Provider"
```

### Task 5: 应用外壳、侧栏、7 路由与 i18n（TDD）

**Files:**
- Create: `src/i18n/index.ts`, `src/i18n/zh.json`, `src/i18n/en.json`
- Create: `src/components/layout/nav-config.ts`, `src/components/layout/sidebar.tsx`, `src/components/layout/app-shell.tsx`
- Create: `src/components/layout/sidebar.test.tsx`
- Create: `src/pages/{dashboard,marketplace,installed,sync-center,portability,settings}.tsx`
- Create: `src/routes.tsx`
- Modify: `src/App.tsx`, `src/main.tsx`
- Create: `vitest.config.ts`, `vitest.setup.ts`（若 Task 3 未建）

**Interfaces:**
- Consumes: `ThemeProvider`（Task 4）。
- Produces: `NAV_ITEMS: NavItem[]`（`NavItem { key: string; path: string; icon: LucideIcon; i18nKey: string }`，7 项）；`AppShell`；`routes`（React Router `RouteObject[]`）。

- [ ] **Step 1: 安装前端运行时依赖**

Run:
```bash
cd /Library/CodeProject/SkillHub
pnpm add react-router-dom i18next react-i18next lucide-react @tanstack/react-query zustand
```
（Vitest / RTL / jsdom 及 `vitest.config.ts`、`vitest.setup.ts` 已在 Task 3 建好；下方若文件已存在可跳过创建。）
Create `vitest.config.ts`:
```typescript
// 文件作用: Vitest 配置(jsdom 环境 + setup)
// 创建日期: 2026-07-09
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
	plugins: [react()],
	test: { environment: 'jsdom', setupFiles: ['./vitest.setup.ts'], globals: true },
});
```
Create `vitest.setup.ts`:
```typescript
// 文件作用: 测试全局 setup(注入 jest-dom 断言 + matchMedia 兜底)
// 创建日期: 2026-07-09
import '@testing-library/jest-dom';

if (!window.matchMedia) {
	window.matchMedia = ((q: string) => ({
		matches: false,
		media: q,
		onchange: null,
		addEventListener: () => {},
		removeEventListener: () => {},
		addListener: () => {},
		removeListener: () => {},
		dispatchEvent: () => false,
	})) as unknown as typeof window.matchMedia;
}
```

- [ ] **Step 2: 写中英文案**

Create `src/i18n/zh.json`:
```json
{
  "nav": {
    "dashboard": "首页",
    "marketplace": "资源中心",
    "installed": "已安装",
    "sync": "Agent 同步",
    "portability": "导入导出",
    "settings": "设置"
  }
}
```
Create `src/i18n/en.json`:
```json
{
  "nav": {
    "dashboard": "Dashboard",
    "marketplace": "Marketplace",
    "installed": "Installed",
    "sync": "Sync Center",
    "portability": "Import/Export",
    "settings": "Settings"
  }
}
```
Create `src/i18n/index.ts`:
```typescript
// 文件作用: i18next 初始化(中英双语, 默认中文)
// 创建日期: 2026-07-09
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import zh from './zh.json';
import en from './en.json';

void i18n.use(initReactI18next).init({
	resources: { zh: { translation: zh }, en: { translation: en } },
	lng: 'zh',
	fallbackLng: 'en',
	interpolation: { escapeValue: false },
});

export default i18n;
```

- [ ] **Step 3: 写导航配置（7 项）**

Create `src/components/layout/nav-config.ts`:
```typescript
// 文件作用: 侧栏导航项配置(7 项, 对应 7 个路由)
// 创建日期: 2026-07-09
import {
	Home,
	Store,
	Package,
	RefreshCw,
	ArrowLeftRight,
	Settings,
	type LucideIcon,
} from 'lucide-react';

/** 导航项 */
export interface NavItem {
	key: string;
	path: string;
	icon: LucideIcon;
	i18nKey: string;
}

/** 7 个顶级导航项 */
export const NAV_ITEMS: NavItem[] = [
	{ key: 'dashboard', path: '/', icon: Home, i18nKey: 'nav.dashboard' },
	{ key: 'marketplace', path: '/marketplace', icon: Store, i18nKey: 'nav.marketplace' },
	{ key: 'installed', path: '/installed', icon: Package, i18nKey: 'nav.installed' },
	{ key: 'sync', path: '/sync', icon: RefreshCw, i18nKey: 'nav.sync' },
	{ key: 'portability', path: '/portability', icon: ArrowLeftRight, i18nKey: 'nav.portability' },
	{ key: 'settings', path: '/settings', icon: Settings, i18nKey: 'nav.settings' },
];
```
说明：原型侧栏有 6 个功能项（首页/资源中心/已安装/Agent 同步/导入导出/设置）；"资源详情/安装" 是资源中心的子路由，不单列导航。故导航 6 项、路由 7 条（含 `/marketplace/:id`）。

- [ ] **Step 4: 写侧栏失败测试**

Create `src/components/layout/sidebar.test.tsx`:
```tsx
// 文件作用: 侧栏渲染与导航单测
// 创建日期: 2026-07-09
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '../../i18n';
import { Sidebar } from './sidebar';
import { NAV_ITEMS } from './nav-config';

describe('Sidebar', () => {
	it('渲染全部导航项(中文文案)', () => {
		render(
			<MemoryRouter>
				<Sidebar />
			</MemoryRouter>,
		);
		expect(screen.getByText('首页')).toBeInTheDocument();
		expect(screen.getByText('资源中心')).toBeInTheDocument();
		expect(screen.getAllByRole('link')).toHaveLength(NAV_ITEMS.length);
	});
});
```

- [ ] **Step 5: 运行确认失败**

Run: `pnpm vitest run src/components/layout`
Expected: FAIL（`./sidebar` 不存在）。

- [ ] **Step 6: 实现侧栏、外壳、页面、路由、App**

Create `src/components/layout/sidebar.tsx`:
```tsx
// 文件作用: 侧栏导航(品牌标 + 6 导航项), 高亮当前路由
// 创建日期: 2026-07-09
import { NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Box } from 'lucide-react';
import { NAV_ITEMS } from './nav-config';

/** 左侧导航栏 */
export function Sidebar() {
	const { t } = useTranslation();
	return (
		<aside className="flex h-full w-56 flex-col border-r" style={{ borderColor: 'var(--sh-border)' }}>
			<div className="flex items-center gap-2 px-5 py-5 text-lg font-bold">
				<Box color="var(--sh-brand)" />
				SkillHub
			</div>
			<nav className="flex flex-col gap-1 px-3">
				{NAV_ITEMS.map((item) => (
					<NavLink
						key={item.key}
						to={item.path}
						end={item.path === '/'}
						className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm"
						style={({ isActive }) => ({
							background: isActive ? 'color-mix(in srgb, var(--sh-brand) 12%, transparent)' : 'transparent',
							color: isActive ? 'var(--sh-brand)' : 'var(--sh-fg)',
						})}
					>
						<item.icon size={18} />
						{t(item.i18nKey)}
					</NavLink>
				))}
			</nav>
		</aside>
	);
}
```
Create `src/components/layout/app-shell.tsx`:
```tsx
// 文件作用: 应用外壳(侧栏 + 内容区), 不含技术栈页脚
// 创建日期: 2026-07-09
import { Outlet } from 'react-router-dom';
import { Sidebar } from './sidebar';

/** 应用整体布局外壳 */
export function AppShell() {
	return (
		<div className="flex h-screen w-screen overflow-hidden">
			<Sidebar />
			<main className="flex-1 overflow-auto p-8">
				<Outlet />
			</main>
		</div>
	);
}
```
Create 六个页面（以 dashboard 为例，其余同构，仅改标题）：
`src/pages/dashboard.tsx`:
```tsx
// 文件作用: 首页(M0 占位, 内容留待 M1)
// 创建日期: 2026-07-09
export default function Dashboard() {
	return <h1 className="text-2xl font-bold">首页 / Dashboard</h1>;
}
```
按同一模式创建 `marketplace.tsx`（资源中心 / Marketplace）、`installed.tsx`（已安装 / Installed）、`sync-center.tsx`（Agent 同步 / Sync Center）、`portability.tsx`（导入导出 / Import/Export）、`settings.tsx`（设置 / Settings），各自 default export 一个同名组件、渲染对应中英标题。

Create `src/routes.tsx`:
```tsx
// 文件作用: 路由表(7 条: 6 导航路由 + marketplace 详情子路由)
// 创建日期: 2026-07-09
import type { RouteObject } from 'react-router-dom';
import { AppShell } from './components/layout/app-shell';
import Dashboard from './pages/dashboard';
import Marketplace from './pages/marketplace';
import Installed from './pages/installed';
import SyncCenter from './pages/sync-center';
import Portability from './pages/portability';
import Settings from './pages/settings';

/** 应用路由表 */
export const routes: RouteObject[] = [
	{
		path: '/',
		element: <AppShell />,
		children: [
			{ index: true, element: <Dashboard /> },
			{ path: 'marketplace', element: <Marketplace /> },
			{ path: 'marketplace/:id', element: <Marketplace /> },
			{ path: 'installed', element: <Installed /> },
			{ path: 'sync', element: <SyncCenter /> },
			{ path: 'portability', element: <Portability /> },
			{ path: 'settings', element: <Settings /> },
		],
	},
];
```
Modify `src/App.tsx`:
```tsx
// 文件作用: 应用根组件(装配 Provider 与路由)
// 创建日期: 2026-07-09
import { RouterProvider, createHashRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from './theme/theme-provider';
import { routes } from './routes';
import './i18n';

const queryClient = new QueryClient();
// Tauri 自定义协议下用 HashRouter 更稳(刷新不 404)
const router = createHashRouter(routes);

export default function App() {
	return (
		<ThemeProvider>
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>
		</ThemeProvider>
	);
}
```
Modify `src/main.tsx`（确保引入全局样式与挂载 App）:
```tsx
// 文件作用: React 入口
// 创建日期: 2026-07-09
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
	<React.StrictMode>
		<App />
	</React.StrictMode>,
);
```

- [ ] **Step 7: 运行确认侧栏测试通过**

Run: `pnpm vitest run src/components/layout`
Expected: `1 passed`。

- [ ] **Step 8: 全量类型检查与构建**

Run: `cd /Library/CodeProject/SkillHub && pnpm tsc --noEmit && pnpm build`
Expected: 无类型错误；`vite build` 成功产出 `dist/`。

- [ ] **Step 9: 提交**

```bash
git add src vitest.config.ts vitest.setup.ts package.json pnpm-lock.yaml
git commit -m "feat: 应用外壳/侧栏/7 路由/中英 i18n"
```

### Task 6: 集成冒烟与 M0 验收

**Files:**
- Modify: `README.md`（加开发说明）
- Modify: `src/pages/dashboard.tsx`（临时挂 health 探针，验收后移除或保留为调试）

**Interfaces:**
- Consumes: `appHealth`（Task 3）、`AppShell`/routes（Task 5）。

- [ ] **Step 1: 在首页临时调用 health 验证前后端链路**

修改 `src/pages/dashboard.tsx`，用 TanStack Query 调 `appHealth`，把结果渲染出来：
```tsx
// 文件作用: 首页(M0 临时挂 health 探针验证链路; M1 换真实内容)
// 创建日期: 2026-07-09
import { useQuery } from '@tanstack/react-query';
import { appHealth } from '../api';

export default function Dashboard() {
	const { data } = useQuery({ queryKey: ['health'], queryFn: appHealth });
	return (
		<div>
			<h1 className="text-2xl font-bold">首页 / Dashboard</h1>
			<p className="mt-2 text-sm" style={{ color: 'var(--sh-muted)' }}>
				version {data?.version ?? '...'} · db {data?.dbOk ? 'ok' : '...'}
			</p>
		</div>
	);
}
```

- [ ] **Step 2: 实机启动并逐屏验证（/verify）**

Run: `cd /Library/CodeProject/SkillHub && pnpm tauri dev`
逐项确认（观察，非自动化）：
1. 窗口打开显示侧栏 + 首页。
2. 首页显示 `version 0.1.0 · db ok`（证明 SQLite 迁移成功、命令链路通）。
3. 点击 6 个导航项都能切换到对应页面，标题正确。
4. 侧栏当前项高亮为品牌靛蓝色。
5. 系统切到暗色时配色随之变化（或临时加按钮调 `toggle` 验证）。
6. 底部无 "Rust + React + Tauri" 页脚。
Expected: 6 项全部满足。

- [ ] **Step 3: 补 README 开发说明**

在 `README.md` 追加：
```markdown
## 开发

- 安装依赖：`pnpm install`
- 启动桌面应用：`pnpm tauri dev`
- 前端单测：`pnpm vitest run`
- 后端测试：`cd src-tauri && cargo test`
- 构建：`pnpm build`（前端）/ `pnpm tauri build`（安装包）
```

- [ ] **Step 4: 全量测试与 lint 收口**

Run:
```bash
cd /Library/CodeProject/SkillHub && pnpm vitest run && pnpm tsc --noEmit
cd src-tauri && cargo test && cargo fmt --check && cargo clippy -- -D warnings
```
Expected: 前端测试全绿、无类型错误；后端测试全绿、fmt 无差异、clippy 无告警。

- [ ] **Step 5: 提交并标注 M0 完成**

```bash
cd /Library/CodeProject/SkillHub
git add README.md src/pages/dashboard.tsx
git commit -m "chore: M0 集成冒烟与开发文档, 应用外壳可运行可导航"
```

## Self-Review（对照 spec 的落地检查）

- **spec §3.1 分层**：Task 2(infra)、Task 3(commands + api)确立后端分层与前端调用层。✓
- **spec §4.3 十表**：Task 2 迁移 SQL 建全 10 表，含幂等测试。✓
- **spec §9 前端选型**：Task 4/5 落地 Vite + React Router + Zustand + TanStack Query + Tailwind + i18next + lucide（shadcn/ui 组件在 M1 按需引入，M0 不强加）。✓
- **spec §10 设计语言**：Task 4 CSS 变量落地全部配色 + 亮暗；Task 5 去页脚。✓
- **spec §11 界面清单**：Task 5 建 6 导航 + 7 路由（含 marketplace 详情子路由）。✓
- **spec §12 测试**：迁移器/health/主题/侧栏均 TDD；Task 6 收口 cargo test + vitest + clippy。✓
- **Global Constraints**：文件头注释块、中文注释、Tab（rustfmt hard_tabs / prettier useTabs）、无 AI 署名提交、feature 分支 —— 各 Task 已内建。✓
- **缺口**：Playwright E2E 未纳入 M0（仅 Vitest + 实机 /verify），留待 M1 引入端到端流；shadcn/ui 初始化留待 M1 首个真实组件时做。已在计划中显式说明，非遗漏。
```
