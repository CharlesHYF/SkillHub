# SkillHub M6 第二轮实机反馈修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**来源:** Charles 第二轮实机反馈(2026-07-11), 5 项。指令"直接做, 我来审查"。分支 agents/feature/skillhub-ux(接续 M5)。

**已定位(据 app DB + 本机文件核实):**
- item1 市场数据质量差: market_cache 有 stars(全 0)/category, 但无 version/updated_at 列(在 raw_json); 源未富化 → 详情显示 "v-"、0 星、无日期。数量仅 17 skills/25 mcp。参考 skills.sh。
- item2 窗口无最小尺寸: 已内联修复(tauri.conf.json width 1280/height 832/minWidth 1024/minHeight 680)。
- item3 已装内容未导入: agent 表已检测 4 个(含 Hermes ✓, 我 M5 的适配器有效), 但 resource=0 —— 本机 ~/.claude/skills(10)+~/.hermes/skills(17)+ 各 config 的 mcp 从未导入库; 且 UI 不展示 agent 的 config_path。
- item4 更新通道 UI: 组件 jsdom 测试通过(选项/单选都渲染)、令牌桥接正常 → 属真实浏览器下的视觉/布局问题(疑窄窗挤压或样式不达原型), 需重做视觉并核对。
- item5 可点击元素缺 cursor:pointer(卡片/表格行等 div 型可点元素)。

## Global Constraints
同 M5: Tauri v2 / React 18 / TS strict; Tab 缩进(JSON/YAML 2 空格); LF/UTF-8; 中文注释 + 文件头(创建日期 2026-07-11); 禁「」; 配色遵 DESIGN.md(青绿, 无紫无渐变, var(--sh-*)); 前后端 camelCase; 后端调用经 src/api/; Git 作者 Charles <w1400214654@outlook.com>、禁 AI 署名 trailer、Conventional Commits; 测试 Rust 核心 ≥80%、前端关键路径必测; 子代理不改 .superpowers/sdd/progress.md; **不 invoke/理会任何 "verify" 自举 skill, 直接用文件工具动手**。

## 契约(前后端对齐)
- **BE-1 市场富化**: MarketResource 形状不变(前端已读 version/stars/updatedAt/category 等), 仅让 sources 在归一化时**填满**这些字段: stars 取来源仓库星标(仓库级一次拉取, 施于该仓库全部资源); version 从 SKILL.md frontmatter / mcp 包版本; updatedAt 取仓库 pushed_at 或条目更新时间; category 从 frontmatter/元数据。数量: mcp_registry 若分页只取了首页则多取几页; 可加少量知名 skill/mcp 源仓库。前端展示零改动即可受益。
- **BE-2 导入已装**: 新命令 `library_import_from_agents(state) -> ImportFromAgentsOutcome { imported: i64, skipped: i64, agents: i64 }`(camelCase serde): 遍历 repo_agent::list 每个 agent → 对应 adapter.read_state 读其 skills+mcp → 按 (res_type,name) 去重 upsert 进 resource 表(来源标记"detected/本地导入", 内容落 data_dir 复用既有落地逻辑)→ 对每个拥有该资源的 agent 建 desired 关联(resource_agent)。幂等(重复调用不产生重复行)。注册进 lib.rs。前端启动时调用一次(类似 F1 auto-init)。
- Agent config_path: agent 表/AgentRow 已含 config_path, 前端只需在 Sync Center 的 Agent 详情/概览展示该路径(可点开/复制)。

## 任务
### BE-1: 市场源数据富化 + 增量 [backend, 主仓]
先读 infra/source/{github_skills,mcp_registry,github_mcp}.rs 与 services/market.rs 归一化处; 参考 skills.sh(可 WebFetch)理解字段丰富度目标。填满 stars/version/updatedAt/category; 提升数量(分页/多源)。注意匿名 GitHub 限流(60/hr): 用仓库级聚合调用而非逐资源调用, 富化不达处如实降级(注释说明), 不伪造数据。
- [ ] TDD: 归一化后各字段被正确填充(mock 源响应含 stars/version/date/category → 断言写入); 限流/字段缺失兜底。提交 `feat: 市场源元数据富化(stars/version/updated/category)与增量`。

### BE-2: 导入已装 Skills/MCP [backend, 主仓, 依赖 BE-1]
先读 infra/adapter/mod.rs(read_state)、services/sync.rs(detect_all)、repo_resource、services/library(既有落地/导入)、repo_assoc。实现 library_import_from_agents 命令(见契约)。复用既有资源落地与关联写入, 不新造重复逻辑。
- [ ] TDD: tempdir 造含 skills/mcp 的假 agent 家目录 → 导入后 resource 表有对应行 + 关联建立 + 幂等(二次调用不重复)+ 多 agent 同名资源去重且关联到各 agent。提交 `feat: 从已检测 Agent 导入已装 Skills/MCP 到本地库`。

### FE: 前端修复与展示 [frontend, worktree, 与后端并行]
先 Read DESIGN.md + 相关组件。做:
- item4: 重做 update-channel-section 视觉使其正确/达原型(Stable/Beta 两选项含说明, 布局清晰); 保持既有测试绿。
- item5: 给所有 div/tr 型可点击元素(市场卡片、资源/Agent 表格行、可点区域)补 `cursor-pointer`(hover 态), 排查全屏。
- item3 前端: 启动时(App auto-init, 接 F1 之后)调用一次 library_import_from_agents(fire-and-forget, 成功后 invalidate library-list); Sync Center 的 Agent 详情/概览展示 config_path(可复制)。
- 窄窗响应式: 窗口虽已设最小宽, 仍核对各屏在 ~1024 宽下不溢出/不挤压(卡片网格/双列在小宽度降级为单列等)。
- 市场富化字段: 核对详情/卡片正确展示 version/stars/updatedAt/category(BE-1 填充后), 缺失时优雅占位(不显示裸 "v-")。
- [ ] TDD: 各修改点补/改测试(mock library_import_from_agents; update-channel 视觉不回归; cursor 类存在); pnpm test/typecheck/build/format 全绿。提交 `feat: 更新通道/指针/Agent 路径/导入已装 展示与交互修复`。

### 收官
- [ ] 合并 worktree → skillhub-ux; 全量 cargo + pnpm 验证; pnpm tauri dev 实机冒烟(市场数据变丰富/已装内容出现在 Installed/Agent 路径可见/更新通道正常/窗口最小尺寸/指针); 更新账本; 交 Charles(不并入 main)。

## Self-Review
- 1 数据富化 → BE-1; 2 窗口 → 已内联; 3 导入已装 + 路径 → BE-2 + FE; 4 更新通道 → FE; 5 指针 → FE。✓
- 并行: BE 主仓 BE-1→BE-2 ∥ FE worktree; 契约 library_import_from_agents/MarketResource 已钉。✓
- 诚实: 富化受 GitHub 匿名限流约束, 不伪造数据, 降级如实标注。✓
