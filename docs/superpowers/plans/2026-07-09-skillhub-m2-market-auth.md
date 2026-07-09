# SkillHub M2 市场聚合 + 应用内认证 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**Goal:** 在应用内浏览/搜索并下载安装 Skill/MCP（纯客户端聚合 GitHub + 官方 MCP Registry），需要认证时在应用内弹窗完成 OAuth（GitHub/Google/Microsoft）或输入访问令牌；还原原型第 2、3 屏。

**Architecture:** Rust 后端加 `market`(SourceProvider 聚合 + 归一化 + market_cache 缓存 + 下载安装到本地库)与 `auth`(AuthProvider + PKCE + 系统钥匙串 + 应用内 WebView 回调捕获)两个服务域；前端加 资源中心 与 资源详情/安装 两屏（含认证弹窗）。下载安装产出一条本地 `resource`，随后交给 M1 同步引擎分发（解耦）。

**Tech Stack:** reqwest(HTTP)、oauth2 或手写 PKCE、keyring(钥匙串)、Tauri v2 WebviewWindow + 本地 loopback 回调、wiremock(测试 mock HTTP)。前端沿用 M1 组件库。

## Global Constraints

（同 M0/M1）Tauri v2 / React 18 / TS strict；缩进 Tab（Rust rustfmt hard_tabs、前端 Prettier useTabs），JSON/SQL/YAML 2 空格，LF，UTF-8；中文注释 + 文件头(创建日期 2026-07-09)；禁「」弯角引号；Markdown 禁分割线；数据库遵阿里泰山版（M2 不改表，新表走 `0002_*.sql`；auth_account/market_cache 已在 0001）；配色遵 DESIGN.md（青绿 Restrained，无紫无渐变，颜色走 var(--sh-*)）；前后端 camelCase 契约；后端调用经 src/api/；Git 作者 Charles <w1400214654@outlook.com>、禁 AI 署名 trailer、Conventional Commits、分支 agents/feature/skillhub-mvp；测试 Rust 核心 ≥80%、前端关键路径必测；子代理**不要改 .superpowers/sdd/progress.md**。

## 真实环境边界（本里程碑无法自动端到端验证, 交 Charles）

- OAuth 需注册的真实应用 client_id（GitHub/Google/Microsoft）：代码从配置读取，计划里用占位常量 + `OAUTH_SETUP.md` 说明，Charles 填真实 id 后方可真机登录。单测用 mock HTTP 覆盖 PKCE/换 token/回调解析。
- 市场聚合命中真实 GitHub API / MCP Registry：单测一律 wiremock mock；真机跑会受网络与 GitHub 匿名限流影响（登录后提额）。
- 应用内 WebView 弹窗真实加载第三方登录页：结构可建可编译，真实交互需 Charles 在桌面端点。

## 计划详度

后端契约给全，实现者 TDD 补体；HTTP 一律 mock 测试；UI 任务先 Read 对应原型截图再实现。

## 文件结构（M2 新增）

```
src-tauri/src/
├── domain/market.rs        # MarketResource/SourceId/Query/AuthKind/InstallManifest
├── domain/auth.rs          # ProviderKind/AuthAccount/TokenSet/PkceChallenge
├── infra/
│   ├── repo_market.rs      # market_cache 读写(带 etag/fetch_time)
│   ├── repo_auth.rs        # auth_account 读写(密文不入库)
│   ├── http.rs             # reqwest 客户端封装(超时/代理/UA/条件请求)
│   ├── keychain.rs         # keyring 封装(存取/删 token)
│   └── source/
│       ├── mod.rs          # SourceProvider trait + all_sources()
│       ├── github_skills.rs
│       ├── mcp_registry.rs
│       └── github_mcp.rs
├── services/
│   ├── market.rs           # 聚合/归一化/缓存/搜索排序 + 下载安装
│   └── auth.rs             # OAuth(PKCE)编排 + PAT 校验 + 钥匙串
└── commands/
    ├── market.rs           # market_search/detail/refresh/install
    └── auth.rs             # auth_login/logout/accounts/enter_token

src/
├── api/market.ts  auth.ts
└── pages/marketplace.tsx  marketplace-detail.tsx (+ 认证弹窗组件)
```

### Task 1: market/auth 领域类型 + 迁移(如需) + 仓储

**Files:** Create `domain/market.rs`,`domain/auth.rs`,`infra/repo_market.rs`,`infra/repo_auth.rs`；Modify mod 声明。（market_cache/auth_account 已在 0001，不新增迁移。）

**Interfaces:**
- `domain::market`：`SourceId(GithubSkills=1,McpRegistry=2,GithubMcp=3)`、`MarketResource{source_type:SourceId,res_type:ResourceType,ext_id,name,display_name,description,author,version,stars:i64,category,tags:Vec<String>,auth_required:bool,install_manifest:InstallManifest,updated_at}`、`InstallManifest`(枚举: `Skill{repo,path,ref}` | `Mcp{server_def:McpServerDef}` | `McpTemplate{...需用户填 env}`)、`Query{keyword,res_type:Option,category:Option,sort:SortBy,page,page_size}`。全部 Serialize/Deserialize + camelCase。
- `domain::auth`：`ProviderKind(GitHub=1,Google=2,Microsoft=3,Token=4)`、`AuthAccount{id,provider,account,scope,status,connect_time}`、`TokenSet{access,refresh:Option,expires_at:Option}`(不进库, 进钥匙串)、`PkceChallenge{verifier,challenge,method="S256"}`。
- `repo_market`：`upsert_many(&Connection,&[MarketResource])`、`query(&Connection,&Query)->(Vec<MarketResource>,total)`、`get(&Connection,source_type,ext_id)`、`etag_for(&Connection,source_type)`。
- `repo_auth`：`upsert(&Connection,&AuthAccount)`、`list`、`get_by_provider`、`delete`。
- [ ] TDD：类型序列化往返；repo 往返(内存库)。RED→实现→GREEN。提交 `feat: 市场/认证领域类型与仓储`。

### Task 2: HTTP 客户端 + 钥匙串封装

**Files:** Create `infra/http.rs`,`infra/keychain.rs`；Cargo.toml 加 `reqwest`(rustls, json)、`keyring`、dev `wiremock`。

**Interfaces:**
- `infra::http`：`fn client()->reqwest::Client`(超时/UA "SkillHub/0.1"，可选 Authorization)；`async fn get_json<T>(url,headers)->Result<(T,Option<Etag>)>`(支持 If-None-Match，304 返回特定枚举)。
- `infra::keychain`：`set_token(service,account,token)`、`get_token`、`delete_token`（keyring；service 用 "skillhub"）。
- [ ] TDD：http 用 wiremock 起本地服务测 get_json/304/超时；keychain 用一个测试 service+account 存取删(或 feature-gate/mock，避免污染真实钥匙串——用随机 account 名并在测试末尾删除)。RED→GREEN。提交 `feat: HTTP 与系统钥匙串封装`。

### Task 3: github_skills 源

**Files:** Create `infra/source/mod.rs`(SourceProvider trait + all_sources)、`infra/source/github_skills.rs`。

**Interfaces:** `trait SourceProvider { fn id(&self)->SourceId; async fn search(&self,q:&Query,token:Option<&str>)->Result<Vec<MarketResource>>; async fn fetch_payload(&self,r:&MarketResource,token:Option<&str>)->Result<InstallPayload>; fn auth_kind(&self)->Option<AuthKind>; }`。github_skills：给定若干 GitHub 仓库(默认 Anthropic 官方 skills 仓库 + 用户可配)，用 GitHub API 列出含 SKILL.md 的目录，解析 frontmatter(name/description/version) 归一化为 MarketResource(res_type=Skill)；fetch_payload 拉取该 skill 子目录内容。
- [ ] TDD：wiremock mock GitHub contents/trees API 响应，断言 search 归一化正确、fetch_payload 组装正确。RED→GREEN。提交 `feat: github_skills 市场源`。

### Task 4: mcp_registry 源

**Files:** `infra/source/mcp_registry.rs`。
**Interfaces:** 命中官方 MCP Registry REST API，列表/详情归一化为 MarketResource(res_type=Mcp, install_manifest=Mcp/McpTemplate)。auth_kind=None(公开)。
- [ ] TDD：wiremock mock registry 响应。RED→GREEN。提交 `feat: mcp_registry 市场源`。

### Task 5: github_mcp 源 + all_sources 注册

**Files:** `infra/source/github_mcp.rs`；Modify `source/mod.rs`(all_sources 注册 3 源)。
**Interfaces:** 从 GitHub 上的 MCP 服务器合集仓库(如 modelcontextprotocol/servers 或用户可配)解析出 MCP 服务定义模板 → MarketResource。
- [ ] TDD：wiremock。RED→GREEN。提交 `feat: github_mcp 市场源与源注册`。

### Task 6: 市场服务(聚合/缓存/搜索) + 命令

**Files:** Create `services/market.rs`,`commands/market.rs`；Modify mod/lib 注册。
**Interfaces:**
- `services::market`：`async fn refresh(conn,home,token_provider)`（并发调 all_sources.search → upsert_many 进 market_cache，带 etag）；`fn search(conn,&Query)->(Vec<MarketResource>,total)`（读缓存 + 过滤/排序/分页，对应原型筛选 全部/推荐/已认证/免费/最近更新/分类 与排序）；`fn detail(conn,source_type,ext_id)->Option<MarketResource>`。
- 命令：`market_search(query)->{items,total}`、`market_detail(sourceType,extId)`、`market_refresh()->{count}`（拉取；用已连接 GitHub token 提额，无则匿名）。
- [ ] TDD：service 用内存库 + mock 源数据测搜索/过滤/排序/分页/缓存命中；命令编译+注册。RED→GREEN。提交 `feat: 市场聚合服务与命令`。

### Task 7: 认证 —— PKCE + PAT + 钥匙串 + auth 服务/命令(非弹窗部分)

**Files:** Create `services/auth.rs`,`commands/auth.rs`。
**Interfaces:**
- `services::auth`：`fn build_pkce()->PkceChallenge`；`fn authorize_url(provider,challenge,redirect,state)->String`（GitHub/Google/Microsoft 各自 authorize 端点 + client_id[占位常量] + scope + code_challenge）；`async fn exchange_code(provider,code,verifier,redirect)->Result<TokenSet>`（换 token，mock 测）；`async fn validate_pat(provider,token)->Result<AuthAccount>`（调 provider API 校验 PAT 并取账号）；`fn store(conn,account,tokenset)`（account 入库 + token 入钥匙串）；`logout(conn,provider)`。
- 命令：`auth_accounts()->Vec<AuthAccount>`、`auth_enter_token(provider,token)->AuthAccount`、`auth_logout(provider)`。
- client_id 占位 + 写 `OAUTH_SETUP.md`（Charles 注册应用后填）。
- [ ] TDD：build_pkce(verifier/challenge S256 正确)、authorize_url 拼接、exchange_code(wiremock)、validate_pat(wiremock)、store/logout(内存库 + 测试钥匙串条目)。RED→GREEN。提交 `feat: 认证 PKCE/PAT/钥匙串 服务与命令`。

### Task 8: 应用内 OAuth 弹窗(WebView + loopback 回调)

**Files:** Modify `commands/auth.rs`(加 `auth_login`)、`lib.rs`(如需窗口配置)。
**Interfaces:** `auth_login(app, provider)->AuthAccount`：起本地 loopback 监听(127.0.0.1:随机端口)作 redirect_uri；开一个 Tauri WebviewWindow 加载 `authorize_url`；用户在其中登录授权后 provider 重定向到 loopback，本地监听捕获 `?code&state`，校验 state → `exchange_code` → `store` → 关窗口 → 返回 AuthAccount。超时/取消要能优雅结束。
- [ ] TDD：loopback 回调解析(给定重定向 URL 提取 code/state、state 不符报错)可单测；窗口/真实登录不可自动化，报告里说明留 Charles 真机。编译 + 注册。提交 `feat: 应用内 OAuth 弹窗与 loopback 回调`。

### Task 9: 下载安装(market → 本地 resource)

**Files:** Modify `services/market.rs`(install)、`commands/market.rs`(market_install)。
**Interfaces:** `async fn install(conn,home,data_dir,source_type,ext_id,token,env_overrides)->Result<Resource>`：detail → provider.fetch_payload → 落地(Skill 拉子树到 data_dir/skills/<name>/；MCP 生成 data_dir/mcp/<name>.json；McpTemplate 用 env_overrides 填充)→ repo_resource::insert(source_type=官方/第三方)→ activity(act_type=3 下载)。命令 `market_install(sourceType,extId,envOverrides?)->Resource`；`auth_required` 且对应 provider 未连接 → 返回特定错误让前端弹认证。
- [ ] TDD：mock provider payload，install 后 data_dir 有内容 + resource 入库 + activity。RED→GREEN。提交 `feat: 市场下载安装到本地库`。

### Task 10: 前端 api + 资源中心 Marketplace 屏(原型 2)

**UI 保真：先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_36 (2).png`。**
**Files:** `src/api/market.ts`,`src/api/auth.ts`；`src/pages/marketplace.tsx` + 子组件(资源卡/右侧详情面板)。
**Interfaces/内容：** 搜索框 + Skills/MCP 分段 + 筛选 chips(全部/推荐/已认证/免费/最近更新/分类) + 排序；卡片网格(图标/名称/类型徽标/作者+认证勾/版本/星标/下载量/查看详情/下载)；右侧详情面板(简介/标签/兼容 Agent/安装要求/认证与授权说明/下载并安装)。数据 market_search/detail；刷新 market_refresh。
- [ ] TDD：mock market_search → 卡片渲染、筛选/排序交互、点击卡片填充详情面板、下载按钮触发(auth_required 时弹认证)。先 Read 截图→RED→GREEN。提交 `feat: 资源中心(Marketplace)界面`。

### Task 11: 资源详情/安装屏(原型 3) + 认证弹窗

**UI 保真：先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_37 (3).png`。**
**Files:** `src/pages/marketplace-detail.tsx`(路由 /marketplace/:id) + `AuthModal` 组件。
**内容：** 返回 Marketplace；资源大图/名称/版本/发布者/类别/大小/下载量/更新时间/兼容性；下载并安装 + 收藏；版本历史；权限说明；安装步骤；**认证弹窗**(需要登录时)：使用 GitHub/Google/Microsoft 登录(调 auth_login) + 输入访问令牌(auth_enter_token)，授权说明，取消/继续。安装流程：未认证且需认证 → 弹窗 → 认证成功 → 继续 market_install。
- [ ] TDD：mock 数据渲染详情；点“下载并安装”在 auth_required 时打开 AuthModal；选 GitHub 登录调 auth_login(mock)、输入令牌调 auth_enter_token(mock)；认证后调 market_install(mock)。先 Read 截图→RED→GREEN。提交 `feat: 资源详情/安装界面与应用内认证弹窗`。

### Task 12: M2 集成收口

**Files:** `OAUTH_SETUP.md`(补全)；小修/接线。
- [ ] 前端 `pnpm test/typecheck/build/format:check`；后端 `cargo test/fmt/clippy -D warnings` 全绿。总控 GUI 冒烟 Marketplace/详情两屏(mock 数据注入或空态)对齐原型。写 `OAUTH_SETUP.md` 列出三家 OAuth 应用注册步骤与需填 client_id 位置。提交 `chore: M2 集成收口与 OAuth 配置说明`。

## Self-Review（对照 spec §6/§7/§11 与原型 2、3）

- 三源聚合 + 归一化 + 缓存 + 搜索排序(Task 1-6)；OAuth PKCE + PAT + 钥匙串 + 应用内弹窗(Task 7-8)；下载安装到本地库并交同步引擎(Task 9)；资源中心 + 详情/安装 + 认证弹窗对齐原型(Task 10-11)。✓
- 真实环境项(OAuth client_id / 真机登录 / 活体 API)全程 mock 测试 + OAUTH_SETUP.md，交 Charles。✓
- 解耦：market 只产出本地 resource，分发复用 M1 引擎。✓
