# SkillHub M4 打磨收官 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**Goal:** 补齐最后一屏设置(原型 7)与其后端设置服务；接入原生文件对话框(导入导出真实路径、设置目录浏览)；清偿 M1-M3 遗留(Button forwardRef、批量操作);视觉一致性与亮暗主题打磨;全分支验证 → v1 可交付。

**Architecture:** 后端加 `setting` 服务域(键值表 → 类型化 Settings 读写) + 网络代理/超时接入共享 HTTP 客户端 + `app_version`;前端设置屏 + 原生对话框(@tauri-apps/plugin-dialog)。设置持久化复用既有 `infra::repo_setting`(upsert/list_all)。

**Tech Stack:** tauri-plugin-dialog(原生保存/打开对话框);沿用组件库、i18next、TanStack Query。

## Global Constraints

同 M0-M3：Tauri v2 / React 18 / TS strict；Tab 缩进(JSON/SQL/YAML 2 空格)；LF/UTF-8；中文注释 + 文件头(创建日期 2026-07-10)；禁「」；Markdown 禁分割线;数据库遵阿里泰山版(setting 表已在 0001, 不改表);配色遵 DESIGN.md(青绿, 无紫无渐变, var(--sh-*));前后端 camelCase;后端调用经 src/api/;Git 作者 Charles <w1400214654@outlook.com>、禁 AI 署名 trailer、Conventional Commits、分支 agents/feature/skillhub-mvp;测试 Rust 核心 ≥80%、前端关键路径必测;子代理不要改 .superpowers/sdd/progress.md。

## 设置契约(前后端逐字段对齐, 防 ImportOutcome 式契约不符)

后端 `domain::setting::Settings` ↔ 前端 `src/api/setting.ts` `Settings`(serde camelCase);缺键回落默认值;`setting` 表 cfg_key 命名见括注。

```
storageSkillDir: string          // (storage.skill_dir)   本地 Skill 目录; 默认 <data_dir>/skills 的字符串
storageMcpDir: string            // (storage.mcp_dir)     本地 MCP 目录; 默认 <data_dir>/mcp
syncAutoNewAgent: boolean        // (sync.auto_new_agent) 自动同步到新 Agent; 默认 true
syncCheckUpdateOnStart: boolean  // (sync.check_update_on_start) 启动时检查更新; 默认 true
syncConflictPrompt: boolean      // (sync.conflict_prompt) 冲突时提示; 默认 true
syncOnlyEnabled: boolean         // (sync.only_enabled)   仅同步已启用项; 默认 false
netProxyMode: 0 | 1 | 2          // (net.proxy_mode)      0 系统默认 / 1 不使用 / 2 手动; 默认 0
netHttpProxy: string             // (net.http_proxy)      默认 ''
netHttpsProxy: string            // (net.https_proxy)     默认 ''
netNoProxy: string               // (net.no_proxy)        默认 ''
netTimeoutSec: number            // (net.timeout_sec)     请求超时(秒); 默认 30
updateChannel: 0 | 1             // (update.channel)      0 Stable / 1 Beta; 默认 0
```

命令:`settings_get(state) -> Settings`(list_all → 解析, 缺键填默认);`settings_save(state, settings: Settings) -> Settings`(逐键 upsert, 回读返回);`app_version() -> String`(读 Cargo/tauri 版本, 供"关于"区)。bool 存 '0'/'1' 字符串, 数字/枚举存十进制字符串, 与阿里泰山版 setting 表 cfg_value TEXT 一致。

诚实边界(不过度声称):设置**一律持久化 + 展示 + 可编辑**;本轮**真实生效**的仅:网络代理/超时(接入市场/认证共享 HTTP 客户端)。存储目录(改后对既有数据的迁移/重定位)、更新通道与启动检查更新(尚无更新器)本轮仅持久化留用, 在代码注释与交付报告中标注。同步偏好在同步/导入流程中有清晰接入点者接入, 否则持久化留用并标注。

## 文件结构(M4 新增/改动)

```
src-tauri/src/
├── domain/setting.rs        # Settings 类型 + 默认值 + <-> 键值表映射
├── services/setting.rs      # get_all(conn)->Settings / save(conn,&Settings)
├── commands/setting.rs      # settings_get / settings_save / app_version
├── infra/http.rs (或既有)   # 依 Settings 构造带代理/超时的 reqwest 客户端(Task 2)
└── lib.rs                    # 注册命令 + tauri_plugin_dialog::init()(Task 2)
src/
├── api/setting.ts           # Settings 类型 + settingsGet/settingsSave/appVersion
├── pages/settings.tsx (+子组件: account/storage/sync-prefs/network/update-channel/action-bar)
├── pages/portability.tsx    # 浏览/保存改走原生对话框(Task 4)
└── components/ui/button.tsx # forwardRef 修复(Task 4)
```

### Task 1: 设置领域类型 + 服务 + 命令 [backend, 主仓]

**Files:** Create `domain/setting.rs`、`services/setting.rs`、`commands/setting.rs`;改 `lib.rs`(注册 settings_get/settings_save/app_version)。
**Interfaces:** 见上「设置契约」。`domain::setting::Settings`(Serialize/Deserialize camelCase) + `impl Default` + `from_rows(&[SettingRow])->Settings` + `to_pairs(&self)->Vec<(String,String)>`。`services::setting::get_all(conn)->Result<Settings>`(repo_setting::list_all → from_rows)、`save(conn,&Settings)->Result<Settings>`(to_pairs 逐键 repo_setting::upsert, 回读 get_all)。命令三个。`app_version` 读 `env!("CARGO_PKG_VERSION")`。
- [ ] TDD:Settings 默认值 + 键值往返(from_rows/to_pairs 对称)+ save→get_all 幂等(内存库);bool/数字/枚举编解码边界(非法/缺失值回落默认);命令编译+注册。提交 `feat: 设置领域类型/服务/命令`。

### Task 2: tauri-plugin-dialog 注册 + 网络代理/超时接入 [backend, 主仓, 依赖 Task 1]

**Files:** `Cargo.toml`(加 tauri-plugin-dialog)、`lib.rs`(`.plugin(tauri_plugin_dialog::init())`)、`src-tauri/capabilities/*.json`(dialog 权限)、`infra/http.rs` 或现有市场/认证 HTTP 客户端构造处(依 Settings.netProxyMode/netHttpProxy/netHttpsProxy/netNoProxy/netTimeoutSec 构造 reqwest::Client)。
**Interfaces:** 共享 `fn build_http_client(settings:&Settings)->reqwest::Client`(mode=1 no_proxy()、mode=2 用手动代理、mode=0 走系统默认;timeout 生效)。市场刷新/安装、认证走该客户端。
- [ ] TDD:build_http_client 三种代理模式构造成功 + 超时设置生效(可断言 builder 不 panic / 客户端可用);dialog 插件注册后 `cargo build` + capabilities 校验通过。提交 `feat: 原生对话框插件与网络代理/超时接入`。

### Task 3: 设置界面(原型 7)[frontend, worktree A, 可与后端并行]

**UI 保真:先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_39 (7).png`。**
**Files:** `src/api/setting.ts`、`src/pages/settings.tsx` + 子组件、对应 `.test`。
**内容(对齐原型 7, 双列 + 底部通栏 + 右下操作条):**
- 账号与认证:已连接服务列表(复用 auth_accounts;每项 已连接→退出+管理令牌 / 未连接→登录, 复用 auth_login/auth_logout/auth_enter_token)+ 管理全部令牌。
- 存储目录:本地 Skill 目录 + 本地 MCP 目录(input + 浏览按钮, 浏览走原生目录对话框;Task 4 提供 dialog 封装, 本任务可先留 TODO 占位调用点)。
- 同步偏好:4 个开关(自动同步到新 Agent / 启动时检查更新 / 冲突时提示 / 仅同步已启用项)。
- 网络与代理:代理模式 select(系统默认/不使用/手动)+ HTTP/HTTPS 代理 + 不使用代理地址 + 请求超时(数字)。
- 更新通道:Stable / Beta 单选(各含说明)。
- 操作条:恢复默认(重置为 Settings 默认值, 本地态)+ 保存更改(settingsSave)。脏态提示未保存更改。
- 关于:版本(appVersion)——侧边栏已有"关于 SkillHub", 本屏可选。
- [ ] TDD:mock api → 渲染五区;开关/输入/单选交互改本地态;保存触发 settingsSave 且传全量 Settings;恢复默认重置本地态。先 Read 截图→RED→GREEN。提交 `feat: 设置(Settings)界面`。

### Task 4: 原生对话框接入导入导出 + Button forwardRef + 批量操作 [frontend, worktree B]

**Files:** `package.json`(加 @tauri-apps/plugin-dialog)、导入导出 `src/pages/portability.tsx`(浏览/保存改原生对话框)、`src/components/ui/button.tsx`(forwardRef)、`src/pages/installed.tsx`(批量启用/停用/同步)、对应 `.test`(mock @tauri-apps/plugin-dialog)。
**Interfaces:** 薄封装 `src/lib/dialog.ts`:`pickSaveFile(opts)`/`pickOpenFile(opts)`/`pickDirectory()`。导入导出的输入路径占位替换为对话框结果。Button 用 React.forwardRef 消除告警。已安装页多选 + 批量动作条。
- [ ] TDD:mock dialog → 导出点浏览拿保存路径、导入点选择拿打开路径;Button forwardRef 转发(ref 落到 button);批量选择 → 批量启用/停用调用。提交 `feat: 原生文件对话框与批量操作, 修复 Button forwardRef`。

### Task 5: 视觉一致性与亮暗主题打磨 [both, 主仓, 依赖 Task 1-4 合并]

**REQUIRED SUB-SKILL:先 invoke `impeccable`(或 frontend-design)做跨屏一致性审查。**
- 六屏(首页/资源中心/已安装/同步中心/导入导出/设置)一致性:间距/字号/圆角/图标尺寸/空态/错误态/加载态;亮暗主题双向(data-theme)核对无紫无渐变;焦点态/禁用态;i18n 无硬编码漏项。
- [ ] 逐屏走查修正(仅样式/展示层, 不动契约);`pnpm test/typecheck/build` 绿。提交 `style: 六屏视觉一致性与亮暗主题打磨`。

### Task 6: M4 集成收口 + 全分支验证 [both, 主仓]

- [ ] 后端 `cargo test/fmt/clippy -D warnings`、前端 `pnpm test/typecheck/build/format:check` 全绿;设置往返端到端(settings_save→settings_get 一致);总控 GUI 冒烟(如预览工具恢复:切到设置屏, 改开关→保存→重载仍在)。核对无 AI 署名 trailer、无技术栈页脚。提交 `chore: M4 集成收口`。

## 收官(M4 后, 交给 Charles, AI 不并入 main)

- [ ] 全分支代码审查(superpowers code-review skill:契约/安全红线/阿里泰山版/无水印)。
- [ ] finishing-a-development-branch:整理提交、交付报告(已完成/遗留/真实生效 vs 持久化留用/OAuth 真实环境待办), 交 Charles 决定合并。

## Self-Review(对照 spec §13 与原型 7)

- 设置五区(账号/存储/同步偏好/网络代理/更新通道 + 操作条)Task 1+3;原生对话框 Task 2+4;遗留清偿(forwardRef/批量)Task 4;视觉打磨 Task 5;验证 Task 6。✓
- 契约前后端逐字段对齐(吸取 ImportOutcome 教训)。✓
- 诚实边界:真实生效 vs 持久化留用已标注, 不过度声称。✓
- 并行:BE(主仓 Task 1→2)∥ FE(worktree A Task 3, worktree B Task 4), 文件不相交。✓
