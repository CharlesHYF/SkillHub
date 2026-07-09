// 文件作用: Sync Center 界面 Agent 展示态的派生逻辑 —— 类型(本地/远程)归类、可同步判定、
//           在线状态徽标取值、diff 计划按 action 分组统计与过滤, 供 sync-center 页面与其子组件
//           (agent-table/sync-overview-card/diff-detail-panel)共用, 避免同一套映射写多份
// 创建日期: 2026-07-09
import type { AgentKind, AgentRow } from '@/api/agent';
import type { DiffAction, DiffItem, DiffPlan, SyncSummary } from '@/api/sync';
import type { SyncStatus } from '@/components/common/sync-status-badge';

/** 当前 8 种已知 AgentKind(见后端 domain::agent::AgentKind)均为本机安装的 IDE/CLI 工具,
 * 无一对应"远程"(网络其它机器)概念 —— 远程节点同步是产品明确的 M1 非目标(留待后续阶段)。
 * 用显式白名单而非"永远返回本地", 是为了在后端未来真的引入远程 kind 时, 未识别的新枚举值会
 * 自然落入"远程"分支被前端只读降级, 而不是被默默当作本地误放行真实同步 */
const LOCAL_AGENT_KINDS: readonly AgentKind[] = [
	'ClaudeCode',
	'ClaudeDesktop',
	'Cursor',
	'Windsurf',
	'Cline',
	'VsCode',
	'GeminiCli',
	'Codex',
];

/** Agent 安装类型展示值: 本地(本机工具) / 远程(M1 尚不支持, 只读展示) */
export type AgentInstallKind = '本地' | '远程';

/** 由 AgentKind 归类安装类型; 见上方 LOCAL_AGENT_KINDS 注释 */
export function agentInstallKind(kind: AgentKind): AgentInstallKind {
	return LOCAL_AGENT_KINDS.includes(kind) ? '本地' : '远程';
}

/** 某 Agent 当前是否可参与真实同步: 需在线且为本地类型(远程 M1 只读展示, 不真正同步) */
export function isAgentSyncable(agent: AgentRow): boolean {
	return agent.status && agentInstallKind(agent.agentKind) === '本地';
}

/** Agent 在线状态展示值, 供 SyncStatusBadge 渲染。离线优先于任何历史同步结果(哪怕本次会话
 * 刚同步失败, 一旦判定离线也应展示"离线"); 在线时若本次会话最近一次同步该 Agent 有失败项,
 * 覆盖展示为"部分同步"(有成功也有失败)或"同步失败"(全失败), 否则展示"在线"。
 * lastOutcome 为 undefined 表示本次会话尚未对该 Agent 执行过同步(见 sync-center 页面
 * 的会话级"上次结果"追踪, 后端未提供跨会话的同步历史查询命令) */
export function deriveAgentSyncStatus(agent: AgentRow, lastOutcome?: SyncSummary): SyncStatus {
	if (!agent.status) return '离线';
	if (lastOutcome && lastOutcome.failed > 0) {
		return lastOutcome.success > 0 ? '部分同步' : '同步失败';
	}
	return '在线';
}

/** 一个 DiffPlan 按 action 分组的统计, 供"同步概览"迷你卡片(新增/更新/移除/待同步总计) */
export interface DiffCounts {
	add: number;
	update: number;
	remove: number;
	total: number;
}

/** 统计某 DiffPlan 各 action 的条目数; plan 为 undefined(未选中 Agent 或该 Agent 的 diff
 * 尚未加载完成)时全部记为 0, 不抛错 */
export function countDiffByAction(plan: DiffPlan | undefined): DiffCounts {
	const items = plan?.items ?? [];
	const add = items.filter((item) => item.action === 'Add').length;
	const update = items.filter((item) => item.action === 'Update').length;
	const remove = items.filter((item) => item.action === 'Remove').length;
	return { add, update, remove, total: items.length };
}

/** 按 action 过滤 DiffItem 列表, 供差异详情面板 Tab 切换; 'All' 表示不过滤 */
export function filterDiffItems(items: DiffItem[], action: DiffAction | 'All'): DiffItem[] {
	return action === 'All' ? items : items.filter((item) => item.action === action);
}

/** 由一次 sync_apply 的结果汇总推导"结果"展示文案: 全部同步/部分同步/同步失败/暂无记录。
 * 供顶部"最近同步结果"统计卡与选中 Agent 概览面板的"上次结果"共用同一套措辞 */
export function lastResultLabel(outcome: SyncSummary | undefined): string {
	if (!outcome) return '暂无记录';
	if (outcome.failed === 0) return '全部同步';
	return outcome.success > 0 ? '部分同步' : '同步失败';
}
