# SkillHub M3 导入导出 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**Goal:** 一键导出全部 Skill/MCP(可选含配置/关联/版本锁, 格式 zip/tar/json)为可移植包；导入时预览、按冲突策略落地、可选导入后自动同步；还原原型第 6 屏。

**Architecture:** Rust 后端加 `portability` 服务域(导出打包 + 导入解析/校验/应用) + repo_impexp(历史); 前端 导入导出屏。导入落地复用 M1 的 repo_resource / 同步引擎; 导出读 data_dir 的 skills/mcp 内容 + resource/关联/设置。

**Tech Stack:** zip / tar + flate2 / serde_json / sha2(校验和); 前端沿用组件库 + 拖拽。

## Global Constraints

同 M0-M2：Tauri v2 / React 18 / TS strict；Tab 缩进(JSON/SQL/YAML 2 空格)；LF/UTF-8；中文注释 + 文件头(创建日期 2026-07-10)；禁「」；Markdown 禁分割线；数据库遵阿里泰山版(import_export_log 已在 0001, 不改表)；配色遵 DESIGN.md(青绿, 无紫无渐变, var(--sh-*))；前后端 camelCase；后端调用经 src/api/；Git 作者 Charles <w1400214654@outlook.com>、禁 AI 署名 trailer、Conventional Commits、分支 agents/feature/skillhub-mvp；测试 Rust 核心 ≥80%、前端关键路径必测；子代理不要改 .superpowers/sdd/progress.md。

## 安全红线

导入解压/落地必须**防 zip-slip 路径穿越**(拒绝绝对路径与含 `..` 逃逸出目标根的条目)；校验和不符/schema 版本不兼容要报错而非静默；大小/格式校验。

## 文件结构(M3 新增)

```
src-tauri/src/
├── domain/portability.rs   # Manifest/BundleFormat/Scope/ConflictStrategy/ExportOptions/ImportPreview
├── infra/repo_impexp.rs     # import_export_log 读写
├── services/portability.rs  # export_bundle / parse_bundle / import_bundle
└── commands/portability.rs  # export_bundle / import_preview / import_bundle / impexp_history
src/
├── api/portability.ts
└── pages/portability.tsx (+ 子组件)
```

### Task 1: portability 领域类型 + repo_impexp

**Files:** Create `domain/portability.rs`,`infra/repo_impexp.rs`。
**Interfaces:**
- `BundleFormat(Zip=1,Json=2,Tar=3)`、`Scope(All=0,ByType=1,ByTime=2)`、`ConflictStrategy(Overwrite=0,Skip=1,KeepBoth=2)`(与 0001 import_export_log 枚举一致)、`ExportOptions{include_skills:bool,include_mcp:bool,scope:Scope,format:BundleFormat,include_config:bool,include_version_lock:bool}`、`Manifest{schema_version:i64,exported_at:String,counts:Counts,checksums:BTreeMap<String,String>}`(Counts{skill,mcp,config,agent})、`ImportPreview{skill:i64,mcp:i64,config:i64,agent:i64,schema_ok:bool}`。全部 Serialize/Deserialize camelCase。
- `repo_impexp`:`add(&Connection,direction:i64,file_name,file_format:i64,summary,status:i64)->i64`、`recent(&Connection,limit)->Vec<ImpexpRow>`。
- [ ] TDD：类型往返 + repo 往返(内存库)。提交 `feat: 导入导出领域类型与历史仓储`。

### Task 2: 导出服务 + 命令

**Files:** Create `services/portability.rs`(export 部分)、`commands/portability.rs`(export_bundle)。Cargo.toml 加 `zip`、`tar`、`flate2`、`sha2`(若未加)。
**Interfaces:**
- `fn export_bundle(conn,data_dir:&Path,opts:&ExportOptions,out_path:&Path)->Result<Manifest>`：按 opts 收集 —— resource 元数据(repo_resource::list 过滤 skills/mcp/scope) + 各自 data_dir 内容(skills/<name>/、mcp/<name>.json) + 可选 agents.json(resource_agent 关联) + 可选 settings.json；算每文件 sha256 填 manifest.checksums；按 format 打包：Zip=zip 树、Tar=tar.gz 树、Json=单文件 JSON(内容 base64 内联)。写 import_export_log(direction=0 导出)。
- 命令 `export_bundle(state, options, out_path:String)->Manifest`(out_path 由前端经保存对话框选定, M3 前端可先用固定/输入路径, 真实文件对话框可用 tauri-plugin-dialog 或占位)。
- [ ] TDD：tempdir data_dir 造 1 skill+1 mcp，export 各格式→断言产物存在、manifest counts/checksums 正确、zip/tar 可解出预期条目、json 内联内容可还原。提交 `feat: 导出打包服务与命令`。

### Task 3: 导入解析 + 校验(防穿越) + 预览命令

**Files:** `services/portability.rs`(parse/validate 部分)、`commands/portability.rs`(import_preview)。
**Interfaces:**
- `fn parse_bundle(path:&Path)->Result<ParsedBundle>`：按扩展名/魔数识别 zip/tar/json，解析 manifest + 条目清单(**不落地**, 只读进内存/临时校验)。
- 校验：schema_version 兼容(不兼容→Err)；逐文件 sha256 与 manifest.checksums 比对(不符→Err)；**zip-slip**：任何条目路径规范化后必须仍在目标根内(绝对路径/`..` 逃逸→Err)；大小上限。
- `fn preview(parsed:&ParsedBundle)->ImportPreview`(counts + schema_ok)。
- 命令 `import_preview(path:String)->ImportPreview`。
- [ ] TDD(**含安全用例**)：正常包→预览 counts 正确；篡改校验和→Err；构造含 `../evil` / 绝对路径条目的恶意包→被拒(Err, 不写任何文件)；schema 版本过高→Err。提交 `feat: 导入解析校验(防 zip-slip)与预览`。

### Task 4: 导入应用(冲突策略 + 自动同步) + 命令

**Files:** `services/portability.rs`(import_apply)、`commands/portability.rs`(import_bundle)。
**Interfaces:**
- `fn import_bundle(conn,data_dir,parsed:ParsedBundle,strategy:ConflictStrategy,auto_sync:bool)->Result<ImportOutcome>`：对每个资源按 strategy —— Overwrite(覆盖同名 resource + 内容)、Skip(存在则跳过)、KeepBoth(重命名 `<name>-imported`/`-2` 落地)；写入 data_dir 内容 + repo_resource upsert/insert；可选恢复关联(agents.json)/设置；写 import_export_log(direction=1 导入, status 成功/部分)。auto_sync=true 则对受影响 agent 触发 services::sync(或返回标记让命令层调)。
- 命令 `import_bundle(state, path:String, strategy:i64, auto_sync:bool)->ImportOutcome`(注意 !Send: 若含 async 同步, 按 market 惯例分段)。
- [ ] TDD：三种冲突策略各测(覆盖/跳过/保留两者的落地与库行为)；auto_sync 触发同步(mock 或对内存库+tempdir agent 验证)。提交 `feat: 导入应用(冲突策略/自动同步)与命令`。

### Task 5: 历史命令 + 前端 api

**Files:** `commands/portability.rs`(impexp_history)、`src/api/portability.ts`。
- 命令 `impexp_history(state, limit:i64)->Vec<ImpexpRow>`。前端 `src/api/portability.ts`：类型 + `exportBundle(options,outPath)`、`importPreview(path)`、`importBundle(path,strategy,autoSync)`、`impexpHistory(limit)`。
- [ ] TDD：命令编译+注册；前端 api mock 测(命令名/参数)。提交 `feat: 导入导出历史命令与前端调用封装`。

### Task 6: 导入导出界面(原型 6)[前端, 可与后端并行]

**UI 保真：先 Read `prototype/ChatGPT Image 2026年7月9日 11_23_39 (6).png`。**
**Files:** `src/pages/portability.tsx` + 子组件(导出面板/导入面板/历史表)。
**内容(对齐原型 6)：** 左"导出"：导出内容(全部 Skill/全部 MCP 勾选)+ 范围(全部/按类型/按时间)+ 目标格式(zip/json/tar 单选)+ 是否含配置 + 是否含版本锁 + `一键导出全部`。右"导入"：拖拽区/选择文件(zip/json/tar)+ 将导入内容预览(Skill/MCP/配置/Agent 计数, 调 importPreview)+ 冲突处理策略(覆盖=推荐/跳过/保留两者)+ `导入后自动同步 Agent` 勾选 + `开始导入`。底部导入导出历史表(操作/文件名/类型/内容摘要/状态/时间, 调 impexpHistory)。文件选择/保存可用 tauri-plugin-dialog；无法接则输入路径占位并在报告说明。
- [ ] TDD：mock api → 导出表单交互 + 触发 exportBundle；导入选文件→预览渲染→选策略→触发 importBundle；历史表渲染。先 Read 截图→RED→GREEN。提交 `feat: 导入导出(Import/Export)界面`。

### Task 7: M3 集成收口

- [ ] 前端 `pnpm test/typecheck/build/format:check` + 后端 `cargo test/fmt/clippy -D warnings` 全绿。总控 GUI 冒烟(如预览工具恢复)。导出→导入往返端到端测试(cargo 层: export 一个包再 import 回另一个 data_dir, 断言资源一致)。提交 `chore: M3 集成收口`。

## Self-Review(对照 spec §8 与原型 6)

- 导出(内容/范围/格式/含配置/版本锁 + 三格式 + manifest/checksum)Task 2；导入(解析/校验/防穿越/预览/冲突策略/自动同步)Task 3-4；历史 Task 1/5；界面 Task 6；往返验证 Task 7。✓
- 安全: zip-slip + 校验和 + schema 版本(Task 3 含安全用例)。✓
- 解耦: 导入落地复用 repo_resource + 同步引擎。✓
