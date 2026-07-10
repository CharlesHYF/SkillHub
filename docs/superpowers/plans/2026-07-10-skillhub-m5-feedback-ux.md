# SkillHub M5 反馈修复 + UX 重构 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**来源:** Charles 实机(pnpm tauri dev)反馈 4 项:(1) 资源中心空;(2) Agent 同步没读到 Hermes 这类 agent;(3) 去掉刷新按钮, 改实时刷新;(4) UI 重新设计, 交互不好。指令"不要询问我, 直接做, 明天审查"。

**根因(已定位):**
- 前端从不"进入即自动检测/拉取": Marketplace 只读 market_cache、Sync Center 只读 agent 表, `market_refresh`/`agent_detect` 仅由手动"刷新"按钮触发 → 首次进入必然空。(对应 1、3)
- Agent 适配器仅 8 款(ClaudeCode/ClaudeDesktop/Cursor/Windsurf/Cline/GeminiCli/VSCode/Codex), 无 Hermes。(对应 2)
- 手动刷新按钮遍布四屏(Dashboard/Marketplace/Sync/ImportExport)。(对应 3)

## Global Constraints

同既有: Tauri v2 / React 18 / TS strict; Tab 缩进(JSON/YAML/SQL 2 空格); LF/UTF-8; 中文注释 + 文件头(创建日期 2026-07-10); 禁「」; Markdown 禁分割线; 配色遵 DESIGN.md(青绿, 无紫无渐变, var(--sh-*)); 前后端 camelCase; 后端调用经 src/api/; Git 作者 Charles <w1400214654@outlook.com>、禁 AI 署名 trailer、Conventional Commits; 分支 agents/feature/skillhub-ux; 测试 Rust 核心 ≥80%、前端关键路径必测; 子代理不改 .superpowers/sdd/progress.md。

## Hermes 适配器契约(据官方文档核实)

- 安装/数据目录: `~/.hermes/`。检测依据: `~/.hermes/config.yaml` 存在即视为已安装。
- MCP 配置: `~/.hermes/config.yaml`(**YAML**, 非 JSON)顶层 `mcp_servers:` 映射; 每项 stdio(`command`/`args`/`env`)或 http(`url`/`headers`), 另有 `enabled`/`timeout` 等可选键。读写只动 `mcp_servers` 子树, 保留文件其余内容与顺序尽量不破坏(至少不丢失其它键)。
- Skills: `~/.hermes/skills/<name>/SKILL.md`(与 Claude Code 的 SKILL.md 同形), 复用 SkillTarget::ClaudeSkillsDir 落地形态。
- 需新增依赖 serde_yaml(或 serde_yaml_ng, 择 tauri v2 兼容且维护活跃者); 新增 AgentKind::Hermes; 注册进 all_adapters。

## 实时刷新策略(去手动刷新)

前端为主, 不引入重后端事件管道(sync://progress 既有事件保留):
- 进入即拉: 关键页 useQuery 用 `refetchOnMount: 'always'`; Marketplace 挂载时若缓存为空/过期自动触发 `market_refresh`; Sync Center 挂载时自动触发 `agent_detect`(二者用 useEffect + mutation, 幂等)。
- 保持新鲜: 易变数据(Agent 在线态/待同步数、Dashboard 概览)加 `refetchInterval`(如 5s)+ `refetchOnWindowFocus: true`; 市场缓存较重, 挂载拉一次 + 手动搜索即可, 不高频轮询。
- 写后即失效: 安装/同步/导入成功后 invalidate 相关 query(既有逻辑保留强化)。
- 移除四屏 header 的"刷新"Button 与相关 import; 以"自动更新 + 细微'刚刚更新'提示"替代。

## UI 重设计方向(保持连贯, 不推倒重来)

- 保留: 青绿 teal 体系、6 屏 IA、中英双语标签、无紫无渐变。
- 提升(视觉): 更清晰的层级与留白节奏; 卡片质感(统一 border/subtle shadow/hover 抬升); **有意义的空态**(图标 + 一句说明 + 行动指引/自动加载提示, 取代裸"暂无 X"); 骨架屏加载态; 一致的图标尺寸与语气; 焦点态/禁用态齐全。
- 提升(交互): 全局实时刷新(无手动按钮); 动作即时反馈(轻量 toast 或内联状态); 加载/空/错误三态齐全且跨屏一致; 过渡/微交互克制得体(hover/展开/进度)。
- 不做: 推翻 IA、改配色主色、引入紫色/渐变、加重后端。

## 任务

### B1: Hermes 适配器 [backend, 主仓]
**Files:** `Cargo.toml`(+serde_yaml); `domain/agent.rs`(+AgentKind::Hermes、label/scope); 新增 `infra/adapter/hermes.rs`(YAML mcp_servers 读写 + skills 目录复用 ClaudeSkillsDir); `infra/adapter/mod.rs`(注册)。
- 读: 解析 `~/.hermes/config.yaml` 的 `mcp_servers`(缺文件/空即空 MCP), + `~/.hermes/skills/*/SKILL.md`。
- 写(apply): 按 DiffPlan 增删改 `mcp_servers` 子树后写回 config.yaml(保留其余键), skills 写入 `~/.hermes/skills/<name>/SKILL.md`。
- [ ] TDD: tempdir 造 config.yaml + skills, detect/read_state/apply(增删改)往返; 缺文件兜底; YAML 只动 mcp_servers 子树不误伤其它键。提交 `feat: Hermes Agent 适配器(YAML mcp_servers + skills 目录)`。

### F1: 实时数据 + 去刷新按钮 [frontend, worktree, 可与 B1 并行]
**Files:** `pages/{marketplace,sync-center,dashboard,portability}.tsx` 及相关 hooks; 可抽 `hooks/useAutoRefresh` 或就地实现。
- 进入即自动 agent_detect / market_refresh(见"实时刷新策略"); 移除四屏"刷新"按钮; 配置 refetchOnMount/Interval/OnWindowFocus; 写后失效强化。
- [ ] TDD: mock api → 挂载自动触发 detect/refresh; 无刷新按钮; interval/focus 重取(可测 refetch 被调度/mock 计时)。提交 `feat: 六屏实时刷新, 移除手动刷新按钮`。

### F2: UI 重设计 + 交互打磨 [frontend, worktree, 依赖 F1]
**REQUIRED SUB-SKILL: 先 invoke frontend-design(或 impeccable)。** 按"UI 重设计方向"逐屏提升视觉与交互: 空/加载/错误三态、卡片质感、层级留白、微交互、一致性; 契约/逻辑不变(F1 的数据层不动)。
- [ ] TDD 保持既有测试绿 + 视觉走查(对照 DESIGN.md); 必要时补空态/加载态测试。提交 `feat: 六屏 UI 重设计与交互打磨`。

### 收官
- [ ] 合并 worktree → skillhub-ux; 全量验证(cargo test/clippy/fmt + pnpm test/typecheck/build/format); `pnpm tauri dev` 实机冒烟(市场自动填充/Agent 自动检测含 Hermes(如本机装了)/无刷新按钮/视觉); 更新账本; 交 Charles 审查(不并入 main)。

## Self-Review
- 1 资源中心空 → F1 自动拉取; 2 Hermes → B1 适配器; 3 去刷新+实时 → F1; 4 UI 重设计 → F2。✓
- 并行: B1 主仓 ∥ F1→F2 worktree(前端两任务同文件, 串行避免冲突)。✓
- Hermes 契约据官方文档核实(config.yaml 路径/mcp_servers YAML 形状/skills SKILL.md)。✓
