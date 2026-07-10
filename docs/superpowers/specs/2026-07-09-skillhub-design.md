# SkillHub 设计方案

元信息
- 日期：2026-07-09
- 作者：Charles
- 状态：待评审
- 技术栈：Rust + Tauri v2 + React 18 + TypeScript

## 1. 背景与目标

SkillHub 是一个跨平台桌面应用，统一管理本机各 AI 工具的 Skill 与 MCP，并把它们同步到所有已安装的工具中。三个核心能力：

1. Skill 与 MCP 的统一管理，并同步到所有已安装的 Agent（本机 AI 工具）。
2. 在应用内下载 Skill / MCP；需要认证或登录时，在应用内弹窗完成。
3. 一键导出全部 Skill 与 MCP，并支持导入。

## 2. 三个地基决策

1. Agent = 本机安装的 AI 工具（Claude Code、Claude Desktop、Cursor、Windsurf、Cline、VS Code、Gemini CLI、Codex 等）。SkillHub 检测它们各自配置文件的位置与格式，把 MCP 写进配置、把 Skill 按各工具机制落地。
2. 资源来源 = 纯客户端聚合（GitHub 仓库 + 官方 MCP Registry），不自建后端。
3. 首期范围 = 全量做实，7 个界面都要真实可用。工具覆盖尽可能全、认证全部真实。

## 3. 系统架构

### 3.1 分层

```
React 前端 (纯展示)  Vite + TS · TanStack Query · Zustand · Tailwind + shadcn
        │  Tauri command (调用)        ▲  Tauri event (进度/状态推送)
        ▼                              │
Tauri 命令层 commands/   薄封装, 只做参数校验与转发
应用服务层 services/      library · sync · market · auth · portability · settings
领域层 domain/            Adapter / Provider trait + 协调算法
基础设施 infra/           store(SQLite) · fs · http · keychain
```

厚 Rust 核心掌管全部业务逻辑，React 只做展示，经 Tauri command 调用 + event 订阅进度。

### 3.2 技术选型

Rust 侧关键 crate：`tauri` v2、`tokio`、`serde`/`serde_json`、`rusqlite`(bundled SQLite)、`reqwest`、`oauth2`、`keyring`(系统钥匙串)、`zip` + `tar` + `flate2`(导入导出)、`notify`(可选，配置漂移监听)、`tempfile` + `wiremock`(测试)。

前端选型见第 9 节。

## 4. 数据模型与存储

### 4.1 三个核心概念

| 概念 | 含义 | 真值来源 |
|------|------|---------|
| Resource | SkillHub 托管的一个 Skill 或 MCP | Skill = 磁盘目录(含 SKILL.md)；MCP = 一条服务定义。元数据入 SQLite |
| Agent | 检测到的本机 AI 工具实例 | 工具自己的配置文件 |
| Association | 某 Resource 应出现在某 Agent 上，即期望态 | SQLite `resource_agent` 表 |

同步的本质：对每个 Agent，比较期望态（关联到它的 Resource 集合）与实际态（适配器从工具配置读出的现状），算出差异，再应用。原型里的差异详情、部分同步、本地修改，都是这套协调的自然产物。

### 4.2 数据库规约（阿里巴巴泰山版 → SQLite 映射）

本地库用 SQLite（嵌入式、零配置、单文件）。表结构遵循《阿里巴巴 Java 开发手册（泰山版）》数据库设计规约，类型级强制项按 SQLite 类型亲和性等价映射：

| 阿里规约（MySQL） | SQLite 落地 |
|------------------|------------|
| 表名/字段名 小写下划线、≤32、见名知义、禁数字开头/拼音 | 完全照做 |
| 索引命名 pk_ / uk_表名_字段 / ix_表名_字段 | 照做（具名索引） |
| 主键必备；自增 BIGINT UNSIGNED | id INTEGER PRIMARY KEY AUTOINCREMENT |
| 布尔 TINYINT(1) | INTEGER NOT NULL DEFAULT 0 CHECK (x IN (0,1)) |
| 禁 ENUM，用 TINYINT/VARCHAR | 用 INTEGER/TEXT，注释列明全部枚举值 |
| 禁 TEXT/BLOB | SQLite 字符串即 TEXT 亲和；短字段注释标业务长度；大文件走文件系统不入库 |
| 金额 DECIMAL | 本项目无金额字段 |
| 所有字段 NOT NULL + 有意义 DEFAULT | 照做 |
| 禁数据库层外键 | 不声明 FK，应用层保证一致性 |
| 字符集 UTF8MB4 | SQLite 原生 UTF-8，等价支持 emoji |
| 表/字段必须 COMMENT，状态列明枚举 | 用 DDL 内 -- 注释，随 schema 持久化于 sqlite_master |
| 三表 JOIN 上限、禁 SELECT *、显式列名 | 数据访问层强制 |
| 分库分表阈值(500 万行 / 50 列) | 本地桌面库天然不触及，表保持窄 |

### 4.3 表清单与 DDL

共 10 张表：resource、agent、resource_agent、sync_run、sync_item、market_cache、auth_account、import_export_log、setting、activity_log。

```sql
-- 资源表: SkillHub 托管的 Skill/MCP 元数据
CREATE TABLE resource (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  res_type      INTEGER  NOT NULL DEFAULT 1,                 -- 类型: 1-Skill, 2-MCP
  name          TEXT     NOT NULL DEFAULT '',                -- 资源名(唯一标识, 业务长度<=64)
  display_name  TEXT     NOT NULL DEFAULT '',                -- 展示名(<=128)
  version       TEXT     NOT NULL DEFAULT '',                -- 版本号(<=32)
  source_type   INTEGER  NOT NULL DEFAULT 0,                 -- 来源: 0-本地导入, 1-官方仓库, 2-第三方仓库
  local_path    TEXT     NOT NULL DEFAULT '',                -- 本地路径(<=512)
  enabled       INTEGER  NOT NULL DEFAULT 1,                 -- 是否启用: 0-禁用, 1-启用
  create_time   TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_resource_type_name ON resource (res_type, name);  -- 同类型下名称唯一
CREATE INDEX ix_resource_enabled ON resource (enabled);

-- Agent 表: 检测到的本机 AI 工具实例
CREATE TABLE agent (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  agent_kind     INTEGER  NOT NULL DEFAULT 0,                 -- 工具: 1-ClaudeCode,2-ClaudeDesktop,3-Cursor,4-Windsurf,5-Cline,6-VSCode,7-GeminiCli,8-Codex
  name           TEXT     NOT NULL DEFAULT '',                -- 显示名(<=64)
  config_path    TEXT     NOT NULL DEFAULT '',                -- 配置文件路径(<=512)
  scope          INTEGER  NOT NULL DEFAULT 0,                 -- 作用域: 0-全局, 1-项目
  status         INTEGER  NOT NULL DEFAULT 0,                 -- 状态: 0-离线/不可用, 1-在线/可用
  last_sync_time TEXT     NOT NULL DEFAULT '',                -- 最后同步时间(空串=从未)
  create_time    TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time    TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_agent_kind_path ON agent (agent_kind, config_path);  -- 同类型下配置路径唯一
CREATE INDEX ix_agent_status ON agent (status);

-- 资源-Agent 关联表: 期望态 + 上次应用指纹
CREATE TABLE resource_agent (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  resource_id   INTEGER  NOT NULL DEFAULT 0,                 -- 资源 id(应用层关联 resource.id)
  agent_id      INTEGER  NOT NULL DEFAULT 0,                 -- Agent id(应用层关联 agent.id)
  desired       INTEGER  NOT NULL DEFAULT 1,                 -- 期望态: 0-不应存在, 1-应存在
  applied_hash  TEXT     NOT NULL DEFAULT '',                -- 上次成功应用的内容指纹(漂移检测)
  sync_status   INTEGER  NOT NULL DEFAULT 0,                 -- 同步状态: 0-待同步,1-已同步,2-本地修改,3-同步失败,4-已禁用
  create_time   TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_resource_agent_rid_aid ON resource_agent (resource_id, agent_id);  -- 资源+Agent 唯一
CREATE INDEX ix_resource_agent_agent ON resource_agent (agent_id);

-- 同步运行表: 一次同步操作的汇总
CREATE TABLE sync_run (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  scope_type    INTEGER  NOT NULL DEFAULT 0,                 -- 范围: 0-全部Agent,1-单Agent,2-选择集
  agent_id      INTEGER  NOT NULL DEFAULT 0,                 -- 目标 Agent id(0=多个)
  total_cnt     INTEGER  NOT NULL DEFAULT 0,                 -- 总项数
  success_cnt   INTEGER  NOT NULL DEFAULT 0,                 -- 成功数
  failed_cnt    INTEGER  NOT NULL DEFAULT 0,                 -- 失败数
  skipped_cnt   INTEGER  NOT NULL DEFAULT 0,                 -- 跳过数
  status        INTEGER  NOT NULL DEFAULT 0,                 -- 状态: 0-进行中,1-成功,2-部分成功,3-失败
  run_time      TEXT     NOT NULL DEFAULT (datetime('now')), -- 运行时间
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_sync_run_agent ON sync_run (agent_id);

-- 同步明细表: 一次同步中每个资源项的结果
CREATE TABLE sync_item (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  run_id        INTEGER  NOT NULL DEFAULT 0,                 -- 所属 sync_run id
  resource_id   INTEGER  NOT NULL DEFAULT 0,                 -- 资源 id
  agent_id      INTEGER  NOT NULL DEFAULT 0,                 -- Agent id
  action        INTEGER  NOT NULL DEFAULT 0,                 -- 动作: 1-新增,2-更新,3-移除
  local_ver     TEXT     NOT NULL DEFAULT '',                -- 本地版本
  agent_ver     TEXT     NOT NULL DEFAULT '',                -- Agent 侧版本
  result        INTEGER  NOT NULL DEFAULT 0,                 -- 结果: 0-待处理,1-成功,2-失败,3-跳过
  err_msg       TEXT     NOT NULL DEFAULT '',                -- 失败信息(<=512)
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_sync_item_run ON sync_item (run_id);

-- 市场缓存表: 归一化后的市场资源缓存
CREATE TABLE market_cache (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  source_type   INTEGER  NOT NULL DEFAULT 0,                 -- 来源: 1-github_skills,2-mcp_registry,3-github_mcp
  res_type      INTEGER  NOT NULL DEFAULT 1,                 -- 类型: 1-Skill,2-MCP
  ext_id        TEXT     NOT NULL DEFAULT '',                -- 来源内唯一标识(<=128)
  name          TEXT     NOT NULL DEFAULT '',                -- 资源名(<=64)
  author        TEXT     NOT NULL DEFAULT '',                -- 作者(<=64)
  stars         INTEGER  NOT NULL DEFAULT 0,                 -- 星标数
  category      TEXT     NOT NULL DEFAULT '',                -- 分类(<=32)
  auth_required INTEGER  NOT NULL DEFAULT 0,                 -- 是否需认证: 0-否,1-是
  raw_json      TEXT     NOT NULL DEFAULT '',                -- 归一化载荷(缓存用)
  etag          TEXT     NOT NULL DEFAULT '',                -- HTTP ETag(增量刷新)
  fetch_time    TEXT     NOT NULL DEFAULT (datetime('now')), -- 拉取时间
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE UNIQUE INDEX uk_market_cache_src_ext ON market_cache (source_type, ext_id);  -- 来源+外部id唯一
CREATE INDEX ix_market_cache_type ON market_cache (res_type);

-- 认证账号表: 已连接的第三方账号(令牌存系统钥匙串, 此处不存密文)
CREATE TABLE auth_account (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  provider      INTEGER  NOT NULL DEFAULT 0,                 -- 提供方: 1-GitHub,2-Google,3-Microsoft,4-访问令牌
  account       TEXT     NOT NULL DEFAULT '',                -- 账号标识/邮箱(<=128)
  scope         TEXT     NOT NULL DEFAULT '',                -- 授权范围(<=256)
  keyring_ref   TEXT     NOT NULL DEFAULT '',                -- 钥匙串条目引用键(<=128)
  status        INTEGER  NOT NULL DEFAULT 1,                 -- 状态: 0-已断开,1-已连接
  connect_time  TEXT     NOT NULL DEFAULT (datetime('now')), -- 连接时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_auth_account_prov_acc ON auth_account (provider, account);  -- 提供方+账号唯一

-- 导入导出历史表
CREATE TABLE import_export_log (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  direction     INTEGER  NOT NULL DEFAULT 0,                 -- 方向: 0-导出,1-导入
  file_name     TEXT     NOT NULL DEFAULT '',                -- 文件名(<=256)
  file_format   INTEGER  NOT NULL DEFAULT 1,                 -- 格式: 1-zip,2-json,3-tar
  summary       TEXT     NOT NULL DEFAULT '',                -- 内容摘要(<=256)
  status        INTEGER  NOT NULL DEFAULT 0,                 -- 状态: 0-失败,1-成功,2-部分成功
  run_time      TEXT     NOT NULL DEFAULT (datetime('now')), -- 运行时间
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_import_export_log_dir ON import_export_log (direction);

-- 设置表: 键值对(存储目录/同步偏好/网络代理/更新通道)
CREATE TABLE setting (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  cfg_key       TEXT     NOT NULL DEFAULT '',                -- 配置键(<=64)
  cfg_value     TEXT     NOT NULL DEFAULT '',                -- 配置值(<=1024)
  create_time   TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_setting_key ON setting (cfg_key);  -- 配置键唯一

-- 活动流表: 首页"最近变更"来源
CREATE TABLE activity_log (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  act_type      INTEGER  NOT NULL DEFAULT 0,                 -- 类型: 1-新增,2-更新,3-下载,4-导入,5-导出,6-同步,7-卸载
  res_type      INTEGER  NOT NULL DEFAULT 0,                 -- 关联类型: 0-无,1-Skill,2-MCP,3-配置,4-Agent
  title         TEXT     NOT NULL DEFAULT '',                -- 标题(<=128)
  detail        TEXT     NOT NULL DEFAULT '',                -- 详情(<=256)
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_activity_log_time ON activity_log (create_time);
```

数据库迁移用带版本号的 SQL 脚本管理（`migrations/0001_init.sql` 起）。

### 4.4 文件系统布局

真实的 Skill 内容与 MCP 定义存文件系统（目录可在设置里改，对应第 7 屏）：

```
~/.skillhub/
├── skillhub.db          # SQLite 元数据/索引/历史
├── skills/<name>/       # Skill 目录(含 SKILL.md 与资源)
└── mcp/<name>.json      # MCP 服务定义
```

## 5. 同步引擎（核心）

### 5.1 AgentAdapter trait

一工具一适配器，统一接口，封装各工具配置位置与格式差异：

```rust
/// Agent 适配器: 每种 AI 工具实现一份
trait AgentAdapter {
    fn kind(&self) -> AgentKind;
    fn detect(&self) -> Vec<DetectedAgent>;                          // 检测本机安装
    fn supports(&self, ty: ResourceType) -> bool;                    // 能力矩阵(全部返回 true)
    fn read_state(&self, a: &DetectedAgent) -> ActualState;          // 读现状
    fn apply(&self, a: &DetectedAgent, plan: &DiffPlan) -> ApplyReport;  // 写入(先备份)
}
```

### 5.2 能力矩阵（所有工具都支持 Skill 与 MCP）

差异只在落地方式，全部封装在适配器内：

| 工具 | MCP 落地 | Skill 落地方式 |
|------|---------|---------------|
| Claude Code | ~/.claude.json / 项目 .mcp.json | 原生 ~/.claude/skills/<name>/SKILL.md |
| Claude Desktop | claude_desktop_config.json | 原生能力/skills 目录 |
| Cursor | ~/.cursor/mcp.json | .cursor/rules/*.mdc |
| Windsurf | ~/.codeium/windsurf/mcp_config.json | .windsurf/rules/ 或 AGENTS.md |
| Cline | cline_mcp_settings.json | .clinerules/ |
| VS Code | .vscode/mcp.json(servers 键) | .github/copilot-instructions.md 或 *.instructions.md |
| Gemini CLI | ~/.gemini/settings.json | GEMINI.md 上下文文件 |
| Codex | ~/.codex/config.toml(TOML) | AGENTS.md |

适配器把统一的资源模型序列化成各工具格式（JSON/TOML、mcpServers/servers 差异都在内部消化）。具体路径在实现阶段逐一核对（易随版本变动），跨平台路径差异由 detect() 内部处理，主测 macOS，Win/Linux 预置。

### 5.3 声明式协调流程

```
检测 Agent → 读实际态 → 与期望态 diff → 生成 DiffPlan(新增/更新/移除)
→ 预览(差异详情) → 应用(写入前时间戳备份, 失败可回滚)
→ 写 sync_run / sync_item 历史 → event 推送进度
```

漂移检测：适配器读出的实际态与 `resource_agent.applied_hash` 不一致时，标为本地修改。原型的部分同步 = 只对选中项生成并应用 DiffPlan。

## 6. 市场聚合（纯客户端，无后端）

### 6.1 SourceProvider trait

```rust
/// 资源来源: 每个来源实现一份
trait SourceProvider {
    fn id(&self) -> SourceId;
    async fn search(&self, q: &Query) -> Vec<MarketResource>;  // 检索/列举
    async fn fetch(&self, id: &ResId) -> ResourcePayload;      // 拉取可安装内容
    fn auth_kind(&self) -> Option<AuthKind>;                   // 是否需要认证
}
```

三个来源：`github_skills`（Anthropic 官方 + 用户自配仓库，遍历树找 SKILL.md 解析 frontmatter）、`mcp_registry`（官方 MCP Registry API）、`github_mcp`（GitHub 上的 MCP 仓库）。

### 6.2 归一化与缓存

三者归一化成统一 `MarketResource`（type/name/author/version/stars/tags/auth_required/compatible_agents/install_manifest）。原型的筛选（全部/推荐/已认证/免费/最近更新/分类）与排序都作用在归一化字段上。结果缓存进 `market_cache`（带 ETag + 拉取时间，增量刷新，缓解 GitHub 限流）。

### 6.3 下载安装流程

1. 点"下载并安装"。
2. 若 auth_required 且未登录，弹出应用内认证窗口（见第 7 节）。
3. 拉取内容：Skill 拉子树到 skills/<name>/；MCP 生成 mcp/<name>.json，需要密钥的 MCP 用小表单收集。
4. 登记为一条 resource（source_type=官方/第三方）。
5. 按需自动关联并同步到 Agent。

关键边界：市场只负责产出一个本地 resource，之后交给同步引擎分发，两者解耦。

## 7. 认证（全部真实，应用内弹窗）

核心要求：登录在 SkillHub 内部弹窗完成，不跳系统浏览器。实现为 Tauri 二级 WebView 窗口承载 OAuth 页面，回调用 loopback（http://127.0.0.1:<port>）或自定义 scheme（skillhub://callback）被 Tauri 拦截。

```rust
/// 认证提供方: 在应用内 WebView 窗口完成 OAuth
trait AuthProvider {
    fn kind(&self) -> ProviderKind;                     // GitHub/Google/Microsoft
    async fn login(&self, win: AppWindow) -> Account;   // Auth Code + PKCE
    async fn refresh(&self, acc: &Account) -> Token;
    async fn logout(&self, acc: &Account);
}
```

| 方式 | 流程 |
|------|------|
| GitHub | OAuth Auth Code + PKCE，应用内窗口，回调拦截；设备码流程兜底 |
| Google | OAuth 2.0 Auth Code + PKCE（installed-app / loopback） |
| Microsoft | OAuth 2.0 Auth Code + PKCE（loopback） |
| 访问令牌 | 直接粘贴 PAT，调对应 API 校验 |

令牌只存系统钥匙串（keyring）；auth_account 只留非敏感引用。全程 PKCE + state 防 CSRF，令牌绝不落日志。Settings 屏管理已连接账号与令牌。

## 8. 导入导出

导出：勾选内容（Skill/MCP）+ 范围（全部/按类型/按时间）+ 格式（zip/json/tar）+ 是否含配置 + 是否含版本锁，产出可移植包：

```
skillhub_backup_xxx.zip
├── manifest.json     # schema 版本, 导出时间, 计数, 校验和
├── skills/<name>/    # Skill 目录原样
├── mcp/<name>.json   # MCP 定义
├── agents.json       # 关联关系(可选)
└── settings.json     # 设置(可选)
```

导入：拖拽/选择文件（zip/json/tar）→ 解析 manifest 预览计数 → 选冲突策略（覆盖=推荐 / 跳过 / 保留两者-重命名）→ 可勾"导入后自动同步 Agent" → 落地为本地 resource（含关联）→ 按需触发同步。安全：校验和验证 + 防 zip-slip 路径穿越 + 大小/格式校验。全程写 import_export_log。

## 9. 前端架构

| 项 | 选型 |
|----|------|
| 框架 | React 18 + TypeScript + Vite |
| 服务端状态 | TanStack Query（缓存/失效 Tauri command，订阅同步进度） |
| UI 状态 | Zustand |
| 路由 | React Router |
| 组件 | Tailwind CSS + shadcn/ui（Radix） |
| 图标 | lucide-react |
| i18n | i18next（zh-CN + en） |
| 图表 | v1 不引入（YAGNI） |

前端按功能切分目录（dashboard/marketplace/installed/sync/portability/settings + 共享 ui），src/api/ 为 Tauri command 的类型化封装层。

## 10. 设计语言（配色 / 主题）

| 角色 | 颜色 |
|------|------|
| 主品牌色（靛蓝） | #6366F1 / 深 #4F46E5 |
| Skill 强调色（紫罗兰） | #8B5CF6 |
| MCP 强调色（青） | #14B8A6 |
| 成功/已同步/在线 | #10B981 |
| 部分/待同步 | #F59E0B |
| 失败/离线 | #EF4444 |
| 信息 | #3B82F6 |

亮/暗双主题（默认跟随系统）；字体 Inter + 系统 CJK（PingFang/雅黑）；圆角卡片 + 细边框还原原型；用 CSS 变量 / Tailwind token 保证 Skill=紫、MCP=青 全局一致。去掉原型底部"Rust + React + Tauri"一行。像素级视觉在实现阶段用实时预览打磨。

## 11. 界面清单（7 屏）

| 屏 | 路由 | 主要后端调用 |
|----|------|-------------|
| 首页 Dashboard | / | library.counts · agents.summary · sync.pending · activity.recent |
| 资源中心 Marketplace | /marketplace | market.search/filter/sort · market.detail |
| 资源详情/安装 | /marketplace/:id | market.detail · auth.login · market.install |
| 已安装 Installed | /installed | library.list · resource.detail · sync.apply · portability.exportOne |
| Agent 同步 Sync Center | /sync | sync.agents · sync.diff · sync.apply(事件推进度) |
| 导入导出 Import/Export | /portability | portability.export/import · portability.history |
| 设置 Settings | /settings | settings.get/set · auth.accounts |

## 12. 测试策略（核心逻辑 ≥80%）

- Rust（TDD 优先）：适配器（temp 目录 fixture 配置，测 detect/read/apply）、同步协调/diff（纯函数、表驱动）、providers（wiremock mock HTTP）、导入导出往返（export→import 一致）、zip-slip 安全 fixture。
- 前端：Vitest + RTL 测组件/hooks；Playwright 跑关键 E2E（市场安装流、同步流，mock Tauri）。
- 实机验证：实现阶段用 /verify 驱动真实应用观察闭环。

## 13. 分阶段实现计划

每阶段独立可验证，均在 agents/feature/* 分支开发，由 Charles 审查合并。

| 里程碑 | 交付 |
|--------|------|
| M0 脚手架 | Tauri v2 + React/TS 骨架、editorconfig/gitattributes/gitignore、应用外壳（侧栏 + 7 空路由 + 主题 + i18n）、SQLite 初始化与迁移、去底栏。可运行可导航 |
| M1 本地库 + 同步引擎 | resource CRUD、全工具适配器 detect/read、MCP+Skill 协调 diff/apply、Sync Center + Installed + Dashboard。核心闭环打通 |
| M2 市场 + 认证 | 三个 Provider + 归一化 + 缓存、Marketplace + 详情、应用内 OAuth（GitHub→Google→Microsoft）+ PAT、下载安装到同步 |
| M3 导入导出 | 导出（zip/json/tar + 选项）、导入（预览/冲突策略/防穿越/自动同步）、历史 |
| M4 打磨 | Settings 完善、视觉打磨（亮暗主题）、错误态/边界、覆盖率补齐。v1 可发布 |

## 14. 非目标 / YAGNI（v1 不做）

- 远程节点同步（原型里的 10.0.0.12 一类）留待后续阶段。
- 自建后端 registry。
- 应用内图表库。
- 移动端。

## 15. 风险与未决项

- 各工具配置路径/格式随版本漂移：用适配器隔离 + 实现阶段逐一核对 + 集成测试兜底。
- OAuth 应用注册：GitHub/Google/Microsoft 需分别注册 OAuth App 拿 client_id（PKCE 公共客户端，无需 client_secret）。
- GitHub API 限流：登录后额度提升 + 本地缓存缓解。
- Skill 在非 Claude 工具上的语义损耗：SKILL.md 映射成 rules/instructions 时可能丢失部分结构，落地策略需逐工具打磨。
