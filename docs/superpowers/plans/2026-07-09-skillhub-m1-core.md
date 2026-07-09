# SkillHub M1 本地库 + 同步引擎 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 打通核心闭环：管理本地 Skill/MCP 资源，检测本机 AI 工具，把资源按声明式协调同步到各工具；并实现原型的 首页 / 已安装 / Agent 同步 三屏真实可用。

**Architecture:** Rust 后端分层 domain(类型+trait)/infra(仓储+适配器)/services(协调)/commands；前端 React 屏读命令、订阅同步进度事件。同步 = 期望态(resource_agent 关联) vs 实际态(适配器读工具配置) → DiffPlan → 应用(写前备份) → 记 sync_run/sync_item。

**Tech Stack:** Rust(rusqlite/serde/toml)、Tauri v2 command+event、React 18 + TS + TanStack Query + Zustand + Tailwind v4 + shadcn/ui + lucide + i18next。

## Global Constraints

- 版本：Tauri v2 / React 18 / TS strict。缩进 Tab（Rust rustfmt hard_tabs、前端 Prettier useTabs）；JSON/SQL/YAML 2 空格；LF；UTF-8。
- 注释中文；源码文件头注释块（文件作用 + 创建日期 2026-07-09）；禁「」弯角引号；Markdown 禁分割线。
- 数据库遵循阿里巴巴泰山版规约；表结构已在 `migrations/0001_init.sql`，M1 不改表结构（如需新增走 `0002_*.sql`）。
- 配色/交互遵循 `DESIGN.md`（克制青绿；颜色走 CSS 变量 `var(--sh-*)`；强调色仅用于主操作/当前项/状态；无紫、无渐变）。
- 前后端契约：Rust 命令返回体加 `#[serde(rename_all="camelCase")]`；前端类型与之逐字段对齐；所有后端调用经 `src/api/`。
- Git：作者/提交者恒为 Charles <w1400214654@outlook.com>；禁任何 AI 署名/水印；Conventional Commits；分支 `agents/feature/skillhub-mvp`。
- 测试：Rust 核心逻辑（仓储/适配器/协调）≥80%，纯逻辑表驱动；前端关键屏与公共组件 RTL 必测。适配器一律用 `tempfile` 临时目录 fixture，不碰真实工具配置。

## 计划详度说明

后端契约（类型/trait/命令签名/枚举值）在计划中给全，实现者按 TDD 补齐函数体并以签名为准；UI 任务给出：对应原型截图路径、`DESIGN.md`、精确数据类型与命令、组件清单，实现者**必须先 Read 原型截图**再据此还原，不逐像素预写 JSX。

## 文件结构（M1 结束时新增/改动）

```
src-tauri/src/
├── domain/
│   ├── mod.rs
│   ├── resource.rs          # Resource/ResourceType/SourceType 及转换
│   ├── agent.rs             # AgentKind/DetectedAgent/AgentScope/ActualState/McpServerDef/SkillRef
│   └── sync.rs              # DiffPlan/DiffItem/DiffAction/SyncOutcome + reconcile 纯算法
├── infra/
│   ├── store.rs             # (已存在)
│   ├── repo_resource.rs     # resource 表 CRUD
│   ├── repo_agent.rs        # agent 表 upsert/list
│   ├── repo_assoc.rs        # resource_agent 关联(期望态) + applied_hash
│   ├── repo_sync.rs         # sync_run/sync_item 写入与查询
│   ├── repo_activity.rs     # activity_log 写入/最近查询
│   └── adapter/
│       ├── mod.rs           # AgentAdapter trait + registry(all_adapters)
│       ├── json_mcp.rs      # 通用 JSON mcpServers 适配器(6 工具经配置表)
│       ├── vscode.rs        # VS Code(servers 键)
│       ├── codex.rs         # Codex(TOML mcp_servers)
│       └── skill_target.rs  # Skill 落地策略(skills 目录 / rules 文件)
├── services/
│   ├── mod.rs
│   ├── library.rs           # 资源增删改查/本地导入/计数
│   ├── sync.rs              # 检测/读实际态/协调/应用/历史 编排
│   └── dashboard.rs         # 首页汇总
└── commands/
    ├── mod.rs               # (已存在, 追加)
    ├── health.rs            # (已存在)
    ├── library.rs           # library_* / resource_* 命令
    ├── agent.rs             # agent_list / agent_detect
    ├── sync.rs              # sync_diff / sync_apply(事件) / assoc_set
    └── dashboard.rs         # dashboard_summary / activity_recent

src/
├── components/ui/           # shadcn 基元(button/badge/table/card/input/dialog/…)
├── components/common/       # type-badge / sync-status-badge / stat-card / detail-panel / data-table
├── api/                     # library.ts / agent.ts / sync.ts / dashboard.ts (类型化命令封装)
├── stores/                  # ui 状态(选中项/筛选) zustand
└── pages/                   # dashboard / installed / sync-center 三屏充实
```

AppState 扩展：持有 `Mutex<rusqlite::Connection>`（rusqlite Connection 非 Sync，必须加锁）+ 存储根目录。M1 首个后端任务落地。

### Task 1: 后端状态与仓储基座（AppState 持有连接 + resource 仓储）

**Files:**
- Create: `src-tauri/src/domain/mod.rs`, `src-tauri/src/domain/resource.rs`, `src-tauri/src/infra/repo_resource.rs`
- Modify: `src-tauri/src/lib.rs`（AppState 改为持有 `Mutex<Connection>` + `data_dir`）, `src-tauri/src/infra/mod.rs`, `src-tauri/src/commands/health.rs`（适配新 AppState）

**Interfaces:**
- Produces:
  - `domain::resource::{ResourceType(Skill=1,Mcp=2), SourceType(LocalImport=0,Official=1,ThirdParty=2), Resource{id:i64,res_type:ResourceType,name:String,display_name:String,version:String,source_type:SourceType,local_path:String,enabled:bool,create_time:String,update_time:String}}`，`#[derive(Serialize,Deserialize,Clone,Debug,PartialEq)]` + `#[serde(rename_all="camelCase")]`；`ResourceType`/`SourceType` 以 `i64` 存取（`from_i64/into i64` 辅助）。
  - `infra::repo_resource`：`insert(&Connection,&NewResource)->Result<i64>`、`list(&Connection,filter:&ListFilter)->Result<Vec<Resource>>`、`get(&Connection,id)->Result<Option<Resource>>`、`update_meta`、`set_enabled(&Connection,id,bool)`、`delete(&Connection,id)`、`count_by_type(&Connection)->Result<(i64,i64)>`(skill,mcp)。`NewResource`/`ListFilter{res_type:Option<ResourceType>,keyword:Option<String>}`。
  - `AppState{db:Mutex<Connection>, data_dir:PathBuf}`；提供 `AppState::db()` 加锁访问帮助。
- Consumes: `infra::store::open_and_migrate`(M0)。

- [ ] Step 1: 写 `domain/resource.rs` 类型 + `i64` 互转的失败测试（`ResourceType::from_i64(2)==Mcp` 等），运行失败。
- [ ] Step 2: 实现类型与转换，测试通过。
- [ ] Step 3: 写 `repo_resource` 往返测试（用 `Connection::open_in_memory()` + `open_and_migrate` 建表；insert→get→list→set_enabled→count→delete 断言），运行失败。
- [ ] Step 4: 实现 `repo_resource` 各函数（显式列名、禁 SELECT *、参数化查询），测试通过。
- [ ] Step 5: 改 `lib.rs`：`AppState{db:Mutex<Connection>,data_dir}`；`setup` 里 `open_and_migrate` 后 `Mutex::new(conn)` 存入；`health.rs` 的 `app_health` 改为读 `state.db.lock()` 是否可用得到 `db_ok`（保持 `AppHealth` 契约）。`cargo build`+`cargo test`+`cargo fmt`+`clippy` 全绿。
- [ ] Step 6: 提交 `feat: 资源领域类型与 resource 仓储, AppState 持有连接`。

### Task 2: Agent 领域类型 + AgentAdapter trait + 注册表 + agent 仓储

**Files:** Create `src-tauri/src/domain/agent.rs`, `src-tauri/src/infra/adapter/mod.rs`, `src-tauri/src/infra/repo_agent.rs`；Modify `domain/mod.rs`,`infra/mod.rs`.

**Interfaces:**
- Produces:
  - `domain::agent`：`AgentKind`(ClaudeCode=1,ClaudeDesktop=2,Cursor=3,Windsurf=4,Cline=5,VsCode=6,GeminiCli=7,Codex=8; `code()->i64`,`label()->&str`)、`AgentScope(Global=0,Project=1)`、`DetectedAgent{kind:AgentKind,name:String,config_path:String,scope:AgentScope,online:bool}`、`McpServerDef{name:String,command:Option<String>,args:Vec<String>,env:BTreeMap<String,String>,url:Option<String>}`、`SkillRef{name:String,version:String}`、`ActualState{mcp:Vec<McpServerDef>,skills:Vec<SkillRef>}`。全部 Serialize/Deserialize + camelCase。
  - `infra::adapter::AgentAdapter` trait：`fn kind(&self)->AgentKind; fn supports(&self,ty:ResourceType)->bool; fn detect(&self)->Vec<DetectedAgent>; fn read_state(&self,a:&DetectedAgent)->anyhow::Result<ActualState>; fn apply(&self,a:&DetectedAgent,plan:&DiffPlan)->anyhow::Result<Vec<ItemOutcome>>;`（`apply` 的 DiffPlan/ItemOutcome 见 Task 7，可先用占位签名并在 Task 7 补全 apply）。
  - `infra::adapter::all_adapters(home:&Path)->Vec<Box<dyn AgentAdapter>>`（注册 8 适配器；`home` 便于测试注入临时家目录）。
  - `infra::repo_agent`：`upsert(&Connection,&DetectedAgent)->Result<i64>`（按 `uk_agent_kind_path` 冲突更新 name/scope/status/update_time）、`list(&Connection)->Result<Vec<AgentRow>>`、`get(&Connection,id)`。
- Consumes: `domain::resource::ResourceType`。
- 依赖：`anyhow`、`toml`、`dirs`（家目录/APPDATA）加入 `Cargo.toml`。

- [ ] Step 1: 加依赖（anyhow, toml, dirs）。
- [ ] Step 2: 写 `agent.rs` 类型 + `AgentKind::code/label` 测试（含 8 值往返），失败→实现→通过。
- [ ] Step 3: 写 `repo_agent` upsert 幂等测试（同 kind+path upsert 两次仅一行、字段更新），失败→实现→通过。
- [ ] Step 4: 定义 `AgentAdapter` trait 与 `all_adapters` 骨架（各适配器 `todo!()` 于 read_state/apply，detect 先返回空），`cargo build` 通过（trait 对象安全）。
- [ ] Step 5: 全绿后提交 `feat: Agent 领域类型/适配器 trait/注册表/agent 仓储`。

### Task 3: JSON mcpServers 适配器（6 工具）detect + read_state（MCP）

**Files:** Create `src-tauri/src/infra/adapter/json_mcp.rs`；Modify `adapter/mod.rs`。

**Interfaces:**
- `JsonMcpAdapter{kind:AgentKind, rel_paths:Vec<PathBuf>, servers_key:&'static str, home:PathBuf, skill_target:SkillTarget}`；构造器按工具给定配置文件相对家目录的候选路径与 `mcpServers` 键。覆盖 ClaudeCode(`~/.claude.json`)、ClaudeDesktop(平台配置路径)、Cursor(`~/.cursor/mcp.json`)、Windsurf(`~/.codeium/windsurf/mcp_config.json`)、Cline(VS Code globalStorage 路径)、GeminiCli(`~/.gemini/settings.json`)。
- `detect`：候选路径存在即视为已安装，`online=true`；`read_state`：解析 JSON 的 `servers_key` 对象为 `Vec<McpServerDef>`（command/args/env 或 url）。
- 平台路径：用 `dirs::home_dir()`/`config_dir()`；实现阶段核对，Win/macOS/Linux 分支预置，主测 macOS。路径不确定处以「候选列表 + 存在即用」容错。

- [ ] Step 1: 写 fixture 测试：在 tempdir 造各工具的 `mcp*.json`（含 `mcpServers:{name:{command,args,env}}` 与一条 `url` 型），断言 `read_state().mcp` 正确解析、`detect()` 命中；失败。
- [ ] Step 2: 实现 `JsonMcpAdapter` 的 detect + read_state（serde_json 宽松解析，缺字段给默认），测试通过。
- [ ] Step 3: 在 `all_adapters` 用配置表实例化上述 6 个；`apply` 仍 `todo!()`（Task 7 补）。
- [ ] Step 4: 提交 `feat: JSON mcpServers 适配器(6 工具)检测与读取`。

### Task 4: VS Code(servers) + Codex(TOML) 适配器 detect + read_state

**Files:** Create `adapter/vscode.rs`, `adapter/codex.rs`；Modify `adapter/mod.rs`。

**Interfaces:** `VsCodeAdapter`（`.vscode/mcp.json` 或用户 settings 的 `servers` 键；结构 `{servers:{name:{command,args,env}|{url}}}`）；`CodexAdapter`（`~/.codex/config.toml` 的 `[mcp_servers.<name>]` 表，`toml` 解析为 `McpServerDef`）。均 detect + read_state。

- [ ] Step 1: fixture 测试：tempdir 造 `.vscode/mcp.json`(servers 键) 与 `config.toml`([mcp_servers.foo] command/args)，断言解析正确；失败。
- [ ] Step 2: 实现两适配器 read_state（VS Code 走 serde_json、Codex 走 toml crate），detect 按路径存在；通过。
- [ ] Step 3: 注册进 `all_adapters`（凑齐 8）；提交 `feat: VS Code 与 Codex 适配器检测与读取`。

### Task 5: Skill 落地策略（read_state.skills + 后续 apply 复用）

**Files:** Create `adapter/skill_target.rs`；Modify各适配器接线 `SkillTarget`。

**Interfaces:** `enum SkillTarget{ ClaudeSkillsDir(PathBuf), RulesFile{dir:PathBuf, ext:&'static str}, InstructionsFile(PathBuf) }`；`fn read_skills(&self)->Vec<SkillRef>`（Claude 家族读 `skills/<name>/SKILL.md` 的 frontmatter name/version；rules/instructions 家族读约定目录/文件名推导已装 skill 名）。各 `AgentAdapter::read_state` 组合 mcp + skills。

- [ ] Step 1: fixture 测试：tempdir 造 `~/.claude/skills/foo/SKILL.md`(带 frontmatter) 与 `.cursor/rules/bar.mdc`，断言 `read_skills` 得到 foo/bar；失败。
- [ ] Step 2: 实现 SkillTarget::read_skills + 接入各适配器 read_state.skills；通过。
- [ ] Step 3: 提交 `feat: Skill 落地策略与技能读取`。

### Task 6: 关联仓储 + sync/activity 仓储

**Files:** Create `infra/repo_assoc.rs`, `infra/repo_sync.rs`, `infra/repo_activity.rs`。

**Interfaces:**
- `repo_assoc`：`set(&Connection,resource_id,agent_id,desired:bool)`(upsert)、`desired_for_agent(&Connection,agent_id)->Vec<i64>`(resource ids)、`agents_for_resource`、`set_applied_hash`、`set_sync_status`、`counts_for_resource`(已关联 agent 数)。
- `repo_sync`：`start_run(&Connection,scope_type,agent_id,total)->run_id`、`finish_run(run_id,success,failed,skipped,status)`、`add_item(&Connection,run_id,resource_id,agent_id,action,local_ver,agent_ver,result,err)`、`recent_runs`。
- `repo_activity`：`add(&Connection,act_type,res_type,title,detail)`、`recent(&Connection,limit)->Vec<ActivityRow>`。
- 枚举值严格用 `0001` 注释里的语义。

- [ ] Step 1: 往返测试（关联 set/desired_for_agent/applied_hash；start/add/finish run 读回；activity add/recent），失败→实现→通过（各仓储独立测试）。
- [ ] Step 2: 提交 `feat: 关联/同步历史/活动流 仓储`。

### Task 7: 声明式协调引擎 + apply

**Files:** Create `domain/sync.rs`, `services/sync.rs`；Modify 各 `adapter::apply`。

**Interfaces:**
- `domain::sync`：`enum DiffAction{Add=1,Update=2,Remove=3}`、`struct DiffItem{res_type:ResourceType,name:String,action:DiffAction,local_ver:String,agent_ver:String}`、`struct DiffPlan{items:Vec<DiffItem>}`、`struct ItemOutcome{name:String,action:DiffAction,ok:bool,err:String}`。
- 纯函数 `reconcile(desired:&[ResourceRef], actual:&ActualState)->DiffPlan`：desired 有 actual 无=Add；两边有且版本不同=Update；actual 有但 desired 无(且属托管)=Remove。`ResourceRef{res_type,name,version}`。
- `AgentAdapter::apply(a,plan)`：按 items 写工具配置（JSON/TOML 合并写回、Skill 目录/rules 复制或删除），**写前对目标文件时间戳备份**，返回 `Vec<ItemOutcome>`。
- `services::sync`：`detect_all`、`diff_for_agent(state,agent_id)`、`apply_for_agent(state,agent_id,plan)`(调 adapter.apply + 写 sync_run/item + 更新 applied_hash/sync_status + activity)。

- [ ] Step 1: `reconcile` 表驱动测试（add/update/remove/no-op 组合），失败→实现→通过。
- [ ] Step 2: `apply` fixture 测试：tempdir 工具配置，apply 一个 Add(MCP) 后 read_state 能读到；Update 改版本；Remove 删除；并确认生成 `.bak` 备份；失败→实现（先 JSON_mcp，再 vscode/codex/skill）→通过。
- [ ] Step 3: `services::sync` 编排测试（内存库 + tempdir 适配器注入：diff→apply→sync_run/item 落库、applied_hash 更新）；失败→实现→通过。
- [ ] Step 4: 提交 `feat: 声明式协调引擎与写入应用`。

### Task 8: 命令层 + 进度事件（library/agent/sync/dashboard）

**Files:** Create `commands/library.rs`,`commands/agent.rs`,`commands/sync.rs`,`commands/dashboard.rs`；Modify `commands/mod.rs`,`lib.rs`(注册)。`services/library.rs`,`services/dashboard.rs`。

**Interfaces（camelCase 返回；均取 `State<AppState>` 加锁）：**
- library：`library_list(filter)->Vec<Resource>`、`library_get(id)->Option<Resource>`、`library_counts()->{skill,mcp}`、`resource_import_local(path)->Resource`（把本地目录/文件登记为 resource + activity）、`resource_set_enabled(id,enabled)`、`resource_delete(id)`。
- agent：`agent_detect()->Vec<AgentRow>`(检测并 upsert)、`agent_list()->Vec<AgentRow>`。
- sync：`assoc_set(resourceId,agentId,desired)`、`sync_diff(agentId)->DiffPlan`、`sync_apply(agentIds:Vec<i64>)`：逐 agent 应用，`app.emit("sync://progress", {agentId,done,total,current})`，返回汇总 `{success,failed,skipped}`。
- dashboard：`dashboard_summary()->{skillCount,mcpCount,agentCount,onlineCount,pendingCount}`、`activity_recent(limit)->Vec<ActivityRow>`。
- 前端 `src/api/{library,agent,sync,dashboard}.ts` 对应类型化封装 + 事件订阅 helper `onSyncProgress(cb)`。

- [ ] Step 1: 后端命令纯逻辑（service 层）单测（library_counts、dashboard_summary 组装），失败→实现→通过。
- [ ] Step 2: 接 `#[tauri::command]` + 注册 `generate_handler!`；`cargo build`+test+clippy 绿。
- [ ] Step 3: 前端 `src/api/*.ts` 封装 + 类型；`pnpm typecheck` 绿；api mock 单测（命令名断言，比照 M0 风格）。
- [ ] Step 4: 提交 `feat: 本地库/Agent/同步/首页 命令层与前端调用封装`。

### Task 9: 前端公共组件（shadcn 基元 + 业务组件）

**Files:** `src/components/ui/*`（shadcn），`src/components/common/{type-badge,sync-status-badge,stat-card,data-table,detail-panel}.tsx`，`src/stores/ui.ts`。

**说明：** 先 `pnpm dlx shadcn@latest init`（配置为使用我们的 CSS 变量、Tailwind v4、Tab、无 CSS-in-JS 冲突），按需 `add button badge table card input dialog dropdown-menu tabs checkbox select tooltip`。业务组件：`TypeBadge(type: 'skill'|'mcp')`（Skill/MCP 文字徽标，中性描边 + 图标，禁两种高饱和色）、`SyncStatusBadge(status)`（已同步=绿点/待同步=琥珀/失败=红/本地修改=中性/已禁用=灰，语义色仅点+文字）、`StatCard`、`DataTable`(泛型列)、`DetailPanel`。全部用 `var(--sh-*)`，遵循 DESIGN.md。

- [ ] Step 1: 初始化 shadcn + 添加基元；确认 `pnpm build` 绿、基元用我们的令牌（抽查 button 前景/底色走 var）。
- [ ] Step 2: 写业务组件 + 各自 RTL 测试（TypeBadge 渲染 Skill/MCP 文案；SyncStatusBadge 各状态可辨；StatCard 显示 label/value）。失败→实现→通过。
- [ ] Step 3: 提交 `feat: shadcn 基元与 SkillHub 业务组件`。

### Task 10: 已安装 Installed 屏（原型第 4 屏）

**UI 保真：先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_37 (4).png`** 再实现，对齐布局与信息层级；配色用 DESIGN.md。

**Files:** `src/pages/installed.tsx` + 拆分子组件（列表 + 右侧详情面板）。

**内容：** 顶部 全部/Skills/MCP 分段 + 搜索 + 筛选 + 排序 + 批量操作；表格列（名称+描述、类型 TypeBadge、当前版本、来源、最后更新、同步状态 SyncStatusBadge、已关联 Agent 数、操作菜单）；右侧详情面板（描述、本地路径、Changelog、已关联 Agent、同步到全部 Agent、仅导出此项-占位到 M3、查看详情/卸载）。数据经 `library_list/library_get`、`sync_apply`（同步到全部）、`resource_set_enabled/delete`。TanStack Query 拉取 + 失效刷新；选中行状态入 `stores/ui`。

- [ ] Step 1: Read 原型截图；搭页面结构 + 接 `library_list`（真实数据）；RTL 测试：给定 mock query 数据渲染出行、点击行打开详情面板、类型/状态徽标出现。失败→实现→通过。
- [ ] Step 2: 接操作（启用/禁用、卸载确认 dialog、同步到全部 Agent 触发 sync_apply）；`pnpm typecheck`+`vitest`+`build` 绿。
- [ ] Step 3: 提交 `feat: 已安装(Installed)界面`。

### Task 11: Agent 同步 Sync Center 屏（原型第 5 屏）

**UI 保真：先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_38 (5).png`**。

**Files:** `src/pages/sync-center.tsx` + 子组件（顶部统计卡、Agent 表、差异详情面板）。

**内容：** 顶部 4 统计卡（已连接/在线/待同步项/最近同步结果）；工具条（同步全部/选择同步/查看差异/重试失败/一键同步到所有 Agent）；Agent 表（名称、类型 本地/远程、安装位置、在线状态、已装 Skill/MCP 数、待同步、最后同步时间、操作）；底部差异详情面板（选中 Agent 的 本地版本 vs Agent 版本，Tab 全部/新增/更新/移除）。数据经 `agent_detect/agent_list`、`sync_diff`、`sync_apply`；**订阅 `sync://progress` 事件**驱动进度与按钮 loading 态；同步完成后失效刷新。远程类型 M1 只读展示（同步真机留后续），本地类型可真正 diff/apply。

- [ ] Step 1: Read 截图；搭结构 + 统计卡 + Agent 表接 `agent_list`（真实检测数据）；RTL：渲染 agent 行、点击行显示 diff 面板（mock sync_diff）。失败→实现→通过。
- [ ] Step 2: 接 `sync_apply` + 进度事件（进度条/按钮态）、查看差异、重试失败；`typecheck`+`vitest`+`build` 绿。
- [ ] Step 3: 提交 `feat: Agent 同步(Sync Center)界面`。

### Task 12: 首页 Dashboard 屏（原型第 1 屏）+ M1 集成验收

**UI 保真：先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_36 (2).png` 对应的首页**（注：首页为第 1 张；如文件名不符，Read `prototype/` 下首页那张——统计卡 + 最近变更 + 快速操作 + 同步状态）。

**Files:** `src/pages/dashboard.tsx`（替换 M0 health 探针）。

**内容：** 统计卡（Skill 数/MCP 数/已连接 Agent/待同步）经 `dashboard_summary`；最近变更列表经 `activity_recent`；快速操作（下载资源→跳 Marketplace 占位到 M2、一键同步→跳 Sync Center、导出全部→占位 M3、导入配置→占位 M3）；同步状态概览（复用 agent 数据）。移除 M0 的 version/db 探针 UI。

- [ ] Step 1: Read 首页截图；实现 Dashboard 接 `dashboard_summary`+`activity_recent`；RTL：mock 数据渲染统计卡与最近变更。失败→实现→通过。
- [ ] Step 2: M1 集成收口：全量 `pnpm test`+`typecheck`+`format:check`+`build`；`cargo test`+`fmt`+`clippy -D warnings` 全绿。总控经 Vite dev server + 预览对 首页/已安装/Agent 同步 三屏做 GUI 冒烟（截图对照原型），确认无 console 错误、配色符合 DESIGN.md。
- [ ] Step 3: 提交 `chore: 首页界面与 M1 集成验收`。

## Self-Review（对照 spec §4/§5/§11 与原型）

- 数据模型：resource/agent/resource_agent/sync_run/sync_item/activity 均有仓储（Task 1/2/6）。✓
- 同步引擎：trait+8 适配器(Task 2-5)、reconcile+apply+备份+历史(Task 7)、命令+事件(Task 8)。✓
- 三屏对齐原型：Installed/Sync Center/Dashboard(Task 10-12)读真实命令。✓
- 阿里规约：不改表；仓储显式列名、禁 SELECT *、参数化。✓
- DESIGN.md：公共组件与三屏走 var(--sh-*)，语义色仅状态(Task 9-12)。✓
- 真实环境验证项（非本里程碑门禁）：对真实已安装工具的 apply 真机联调 → 交 Charles；M1 适配器全部 tempfile fixture 测试。
- 缺口：远程 Agent 的真正网络同步不在 M1（只读展示）；Marketplace/导入导出入口在三屏中为占位跳转，M2/M3 兑现。
