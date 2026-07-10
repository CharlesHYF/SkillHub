// 文件作用: Agent 同步(Sync Center)界面(原型第 5 屏) —— 顶部统计卡 + 工具条 + Agent 表
//           (agent-table), 底部选中 Agent 的同步概览(sync-overview-card)与差异详情
//           (diff-detail-panel); 数据经 agent_list/agent_detect 聚合, 每个 Agent 的待同步/
//           差异明细经 sync_diff 逐个现算(见 services::dashboard::summary 文档注释:
//           首页只要一个量级参考, 精确差异走同步中心逐 Agent 现算), 同步经 sync_apply 触发,
//           订阅 sync://progress 事件驱动进度提示; 完成后失效相关 Query 触发刷新
// 创建日期: 2026-07-09
import { useEffect, useMemo, useState } from 'react';
import { CheckCircle2, ListTodo, RefreshCw, Users, Wifi } from 'lucide-react';
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import type { UnlistenFn } from '@tauri-apps/api/event';

import { agentDetect, agentList } from '@/api/agent';
import { libraryList } from '@/api/library';
import {
	onSyncProgress,
	resourceAgentLinks,
	syncApply,
	syncDiff,
	type DiffPlan,
	type SyncProgress,
	type SyncSummary,
} from '@/api/sync';
import { StatCard } from '@/components/common/stat-card';
import { AgentTable } from '@/components/sync/agent-table';
import { countDiffByAction, lastResultLabel } from '@/components/sync/agent-display';
import { DiffDetailPanel } from '@/components/sync/diff-detail-panel';
import { SyncOverviewCard } from '@/components/sync/sync-overview-card';
import { Button } from '@/components/ui/button';
import { useUiStore } from '@/stores/ui';

// 查询 key 与 pages/installed.tsx 保持字面一致(agent-list/resource-agent-links/library-list),
// 使两页面共享同一份 React Query 缓存与失效: 任一页面的写操作(同步/关联变更)都会让另一页面
// 下次访问时自然拿到最新数据, 避免跨页面缓存不一致(全应用只有一个 QueryClient, 见 App.tsx)
const AGENT_LIST_KEY = 'agent-list';
const RESOURCE_AGENT_LINKS_KEY = 'resource-agent-links';
const LIBRARY_LIST_KEY = 'library-list';
// 本页专属: 每个 Agent 的差异计划各自一条 Query, key 为 [SYNC_DIFF_KEY, agentId]
const SYNC_DIFF_KEY = 'sync-diff';

/** Sync Center 界面: 还原原型第 5 屏 —— 统计卡 + 工具条 + Agent 表 + 选中 Agent 的同步概览/差异详情 */
export default function SyncCenter() {
	const queryClient = useQueryClient();
	const { selectedAgentId, setSelectedAgentId } = useUiStore();

	// 本次会话内的同步进度与结果追踪: 后端未提供跨会话的同步历史查询命令(仅
	// assoc_set/resource_agent_links/sync_diff/sync_apply 四个同步相关命令), "最近同步结果"
	// 统计卡与"上次结果/上次详情"只能反映本次会话内发生过的 sync_apply 调用, 而非持久化历史
	const [progress, setProgress] = useState<SyncProgress | null>(null);
	const [lastSummary, setLastSummary] = useState<SyncSummary | undefined>(undefined);
	const [lastOutcomeByAgentId, setLastOutcomeByAgentId] = useState<Map<number, SyncSummary>>(
		new Map(),
	);

	const agentsQuery = useQuery({ queryKey: [AGENT_LIST_KEY], queryFn: agentList });
	const agents = useMemo(() => agentsQuery.data ?? [], [agentsQuery.data]);

	const resourcesQuery = useQuery({ queryKey: [LIBRARY_LIST_KEY], queryFn: () => libraryList() });
	const linksQuery = useQuery({
		queryKey: [RESOURCE_AGENT_LINKS_KEY],
		queryFn: resourceAgentLinks,
	});

	// 逐 Agent 现算差异(sync_diff), 供 Agent 表"待同步"列与顶部"待同步项"统计卡使用同一份数据,
	// 保证二者数字自洽(不会出现汇总卡与逐行相加不一致的情况)
	const diffResults = useQueries({
		queries: agents.map((agent) => ({
			queryKey: [SYNC_DIFF_KEY, agent.id],
			queryFn: () => syncDiff(agent.id),
			retry: false,
		})),
	});

	const diffPlanByAgentId = useMemo(() => {
		const map = new Map<number, DiffPlan | undefined>();
		agents.forEach((agent, idx) => map.set(agent.id, diffResults[idx]?.data));
		return map;
	}, [agents, diffResults]);

	const diffLoadingByAgentId = useMemo(() => {
		const map = new Map<number, boolean>();
		agents.forEach((agent, idx) => map.set(agent.id, diffResults[idx]?.isLoading ?? false));
		return map;
	}, [agents, diffResults]);

	const pendingCountByAgentId = useMemo(() => {
		const map = new Map<number, number | undefined>();
		diffPlanByAgentId.forEach((plan, id) => map.set(id, plan?.items.length));
		return map;
	}, [diffPlanByAgentId]);

	// 期望关联(desired=1)给每个 Agent 的 Skill/MCP 数, 作为"已装 Skill/MCP 数"列的展示口径
	// (与 Installed 界面"已关联 Agent 数"同一份数据来源, 见 resourceAgentLinks 文档注释)
	const installedCountByAgentId = useMemo(() => {
		const resTypeById = new Map(
			(resourcesQuery.data ?? []).map((res) => [res.id, res.resType]),
		);
		const counts = new Map<number, { skill: number; mcp: number }>();
		for (const link of linksQuery.data ?? []) {
			const resType = resTypeById.get(link.resourceId);
			if (!resType) continue;
			const entry = counts.get(link.agentId) ?? { skill: 0, mcp: 0 };
			if (resType === 'Skill') entry.skill += 1;
			else entry.mcp += 1;
			counts.set(link.agentId, entry);
		}
		return counts;
	}, [resourcesQuery.data, linksQuery.data]);

	// 订阅同步进度事件, 组件卸载时取消订阅; cancelled 兜底: 若组件在 Promise resolve 前已卸载,
	// 拿到 unlisten 后立即调用而不是留下悬空订阅
	useEffect(() => {
		let unlisten: UnlistenFn | undefined;
		let cancelled = false;
		onSyncProgress((next) => setProgress(next)).then((fn) => {
			if (cancelled) fn();
			else unlisten = fn;
		});
		return () => {
			cancelled = true;
			unlisten?.();
		};
	}, []);

	function invalidateAfterSync() {
		queryClient.invalidateQueries({ queryKey: [AGENT_LIST_KEY] });
		queryClient.invalidateQueries({ queryKey: [SYNC_DIFF_KEY] });
		queryClient.invalidateQueries({ queryKey: [RESOURCE_AGENT_LINKS_KEY] });
	}

	const detectMutation = useMutation({
		mutationFn: agentDetect,
		// agent_detect 本身就返回探测落库后的全量列表(与 agent_list 同形), 直接写入缓存即可,
		// 不必再多打一次 agent_list 请求
		onSuccess: (rows) => queryClient.setQueryData([AGENT_LIST_KEY], rows),
	});

	const applyMutation = useMutation({
		mutationFn: (agentIds: number[]) => syncApply(agentIds),
		onSuccess: (summary, agentIds) => {
			setLastSummary(summary);
			// sync_apply 的返回值是多 Agent 结果相加后的总计, 只有单 Agent 调用时才能把这份汇总
			// 精确归因到该 Agent(见 agent-display.deriveAgentSyncStatus 的说明); 批量调用只更新
			// 顶部的会话级汇总, 不往下归因到具体某一行, 避免展示误导性的"某 Agent 同步失败"
			if (agentIds.length === 1) {
				setLastOutcomeByAgentId((prev) => new Map(prev).set(agentIds[0], summary));
			}
			setProgress(null);
			invalidateAfterSync();
		},
	});

	function handleSyncAgentIds(agentIds: number[]) {
		if (agentIds.length === 0) return;
		applyMutation.mutate(agentIds);
	}

	const onlineCount = agents.filter((agent) => agent.status).length;
	const offlineCount = agents.length - onlineCount;
	const totalPending = Array.from(pendingCountByAgentId.values())
		.map((count) => count ?? 0)
		.reduce((sum, count) => sum + count, 0);
	const agentsNeedingSyncCount = Array.from(pendingCountByAgentId.values()).filter(
		(count) => (count ?? 0) > 0,
	).length;

	const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null;
	const selectedDiffPlan =
		selectedAgentId != null ? diffPlanByAgentId.get(selectedAgentId) : undefined;
	const selectedDiffLoading =
		selectedAgentId != null ? (diffLoadingByAgentId.get(selectedAgentId) ?? false) : false;
	const selectedDiffCounts = countDiffByAction(selectedDiffPlan);
	const selectedLastOutcome =
		selectedAgentId != null ? lastOutcomeByAgentId.get(selectedAgentId) : undefined;

	const isSyncing = applyMutation.isPending;

	return (
		<div className="flex h-full flex-col gap-4">
			<header className="flex items-center justify-between">
				<h1 className="text-2xl font-bold">Agent 同步 / Sync Center</h1>
				<Button variant="outline" onClick={() => detectMutation.mutate()}>
					<RefreshCw
						size={14}
						className={detectMutation.isPending ? 'animate-spin' : undefined}
					/>
					刷新
				</Button>
			</header>

			<div className="grid grid-cols-4 gap-4">
				<StatCard
					icon={Users}
					label="已连接 Agent"
					value={agents.length}
					hint={`在线 ${onlineCount}`}
				/>
				<StatCard
					icon={Wifi}
					label="在线"
					value={onlineCount}
					hint={`离线 ${offlineCount}`}
				/>
				<StatCard
					icon={ListTodo}
					label="待同步项"
					value={totalPending}
					hint={`需同步 ${agentsNeedingSyncCount} 个 Agent`}
				/>
				<StatCard
					icon={CheckCircle2}
					label="最近同步结果"
					value={lastResultLabel(lastSummary)}
					hint={
						lastSummary
							? `${lastSummary.success} 成功 / ${lastSummary.failed} 失败 / ${lastSummary.skipped} 跳过`
							: undefined
					}
				/>
			</div>

			{isSyncing ? (
				<div className="flex flex-col gap-1.5 rounded-lg border px-3 py-2">
					<p className="text-sm text-muted-foreground">
						{progress
							? `正在同步 ${progress.currentName}(${progress.done}/${progress.total})`
							: '同步中...'}
					</p>
					<div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
						<div
							className="h-full rounded-full transition-all"
							style={{
								background: 'var(--sh-brand)',
								width:
									progress && progress.total > 0
										? `${Math.round((progress.done / progress.total) * 100)}%`
										: '15%',
							}}
						/>
					</div>
				</div>
			) : null}

			<AgentTable
				agents={agents}
				pendingCountByAgentId={pendingCountByAgentId}
				installedCountByAgentId={installedCountByAgentId}
				lastOutcomeByAgentId={lastOutcomeByAgentId}
				selectedId={selectedAgentId}
				onSelectAgent={(agent) => setSelectedAgentId(agent.id)}
				onSyncAgentIds={handleSyncAgentIds}
				isSyncing={isSyncing}
			/>

			<div className="grid min-h-0 flex-1 grid-cols-2 gap-4">
				<SyncOverviewCard
					agent={selectedAgent}
					diffCounts={selectedDiffCounts}
					lastOutcome={selectedLastOutcome}
				/>
				{/* key=selectedAgentId: 切换选中 Agent 时让面板重新挂载, 内部 Tab 过滤态回到"全部" */}
				<DiffDetailPanel
					key={selectedAgentId ?? 'none'}
					diffPlan={selectedDiffPlan}
					isLoading={selectedDiffLoading}
				/>
			</div>
		</div>
	);
}
