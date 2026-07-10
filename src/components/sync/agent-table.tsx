// 文件作用: Sync Center 界面 Agent 表区 —— 工具条(同步全部/选择同步/查看差异/重试失败/
//           一键同步到所有 Agent) + Agent 表(复选框/名称/类型/安装位置/在线状态/Skill·MCP 数/
//           待同步/最后同步时间/操作); 纯展示 + 回调, 数据获取/mutation/进度事件由
//           pages/sync-center 统一持有, 与 components/installed/resource-list 的分层方式一致
// 创建日期: 2026-07-09
import { useState } from 'react';
import { Eye, Monitor, MoreVertical, RefreshCw, RotateCcw, SquareCheck, Zap } from 'lucide-react';

import type { AgentRow } from '@/api/agent';
import type { SyncSummary } from '@/api/sync';
import { DataTable, type DataTableColumn } from '@/components/common/data-table';
import { EmptyState } from '@/components/common/empty-state';
import { SkeletonTable } from '@/components/common/skeleton';
import { SyncStatusBadge } from '@/components/common/sync-status-badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { formatRelativeTime } from '@/lib/utils';
import { agentInstallKind, deriveAgentSyncStatus, isAgentSyncable } from './agent-display';

interface AgentTableProps {
	agents: AgentRow[];
	/** agent.id -> 待同步条目数(来自该 Agent 的 sync_diff 结果); undefined 表示尚未加载/加载失败 */
	pendingCountByAgentId: Map<number, number | undefined>;
	/** agent.id -> 期望关联(desired=1)给该 Agent 的 Skill/MCP 数, 由 pages/sync-center 从
	 * resourceAgentLinks + libraryList 聚合而来 */
	installedCountByAgentId: Map<number, { skill: number; mcp: number }>;
	/** agent.id -> 本次会话内最近一次单独同步该 Agent 的结果(见 agent-display.deriveAgentSyncStatus) */
	lastOutcomeByAgentId: Map<number, SyncSummary>;
	selectedId: number | null;
	/** Agent 列表首次加载中: 为真时表格区展示骨架屏而非空态, 避免探测完成前误判为"暂无 Agent" */
	isLoading?: boolean;
	onSelectAgent: (agent: AgentRow) => void;
	/** 触发对给定 Agent id 列表的同步(供工具条批量按钮与行内"立即同步"复用同一入口) */
	onSyncAgentIds: (agentIds: number[]) => void;
	/** 是否有同步正在进行(禁用会触发新同步的按钮, 避免重叠调用) */
	isSyncing?: boolean;
}

/** 表内 Agent 的可同步 id 列表(在线 + 本地类型, 见 agent-display.isAgentSyncable) */
function syncableIds(agents: AgentRow[]): number[] {
	return agents.filter(isAgentSyncable).map((agent) => agent.id);
}

/** Sync Center 的 Agent 表区: 工具条 + 表格, 内部持有勾选态(批量操作用), 选中态(diff 面板)
 * 由父级(pages/sync-center)通过 selectedId/onSelectAgent 控制 */
export function AgentTable({
	agents,
	pendingCountByAgentId,
	installedCountByAgentId,
	lastOutcomeByAgentId,
	selectedId,
	isLoading = false,
	onSelectAgent,
	onSyncAgentIds,
	isSyncing = false,
}: AgentTableProps) {
	const [checkedIds, setCheckedIds] = useState<Set<number>>(new Set());

	const allSyncableIds = syncableIds(agents);
	const allChecked = agents.length > 0 && agents.every((agent) => checkedIds.has(agent.id));
	const checkedSyncableIds = agents
		.filter((agent) => checkedIds.has(agent.id) && isAgentSyncable(agent))
		.map((agent) => agent.id);
	const failedAgentIds = agents
		.filter((agent) => (lastOutcomeByAgentId.get(agent.id)?.failed ?? 0) > 0)
		.filter(isAgentSyncable)
		.map((agent) => agent.id);

	function toggleChecked(id: number, checked: boolean) {
		setCheckedIds((prev) => {
			const next = new Set(prev);
			if (checked) next.add(id);
			else next.delete(id);
			return next;
		});
	}

	function toggleAll(checked: boolean) {
		setCheckedIds(checked ? new Set(agents.map((agent) => agent.id)) : new Set());
	}

	const columns: DataTableColumn<AgentRow>[] = [
		{
			key: 'checkbox',
			header: (
				<Checkbox
					checked={allChecked}
					onCheckedChange={(checked) => toggleAll(checked === true)}
					aria-label="全选"
				/>
			),
			render: (row) => (
				<span onClick={(e) => e.stopPropagation()}>
					<Checkbox
						checked={checkedIds.has(row.id)}
						onCheckedChange={(checked) => toggleChecked(row.id, checked === true)}
						aria-label={`选中 ${row.name}`}
					/>
				</span>
			),
		},
		{
			key: 'name',
			header: 'Agent 名称',
			render: (row) => (
				<div className="flex items-center gap-2">
					<Monitor size={16} className="shrink-0 text-muted-foreground" />
					<span className="font-medium text-foreground">{row.name}</span>
				</div>
			),
		},
		{
			key: 'kind',
			header: '类型',
			render: (row) => agentInstallKind(row.agentKind),
		},
		{
			key: 'configPath',
			header: '安装位置',
			render: (row) => (
				<span
					className="block max-w-56 truncate text-muted-foreground"
					title={row.configPath}
				>
					{row.configPath}
				</span>
			),
		},
		{
			key: 'status',
			header: '在线状态',
			render: (row) => (
				<SyncStatusBadge
					status={deriveAgentSyncStatus(row, lastOutcomeByAgentId.get(row.id))}
				/>
			),
		},
		{
			key: 'skillCount',
			header: '已装 Skill',
			render: (row) => installedCountByAgentId.get(row.id)?.skill ?? 0,
		},
		{
			key: 'mcpCount',
			header: '已装 MCP',
			render: (row) => installedCountByAgentId.get(row.id)?.mcp ?? 0,
		},
		{
			key: 'pending',
			header: '待同步',
			render: (row) => pendingCountByAgentId.get(row.id) ?? '—',
		},
		{
			key: 'lastSyncTime',
			header: '最后同步时间',
			render: (row) => (row.lastSyncTime ? formatRelativeTime(row.lastSyncTime) : '—'),
		},
		{
			key: 'actions',
			header: '',
			render: (row) => {
				const syncable = isAgentSyncable(row);
				return (
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button
								variant="ghost"
								size="icon-sm"
								aria-label={`${row.name} 操作`}
								onClick={(e) => e.stopPropagation()}
							>
								<MoreVertical size={16} />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
							<DropdownMenuItem onSelect={() => onSelectAgent(row)}>
								查看差异
							</DropdownMenuItem>
							<DropdownMenuItem
								disabled={!syncable}
								title={
									syncable
										? undefined
										: '远程/离线 Agent 暂不支持同步(M1 只读展示)'
								}
								onSelect={() => syncable && onSyncAgentIds([row.id])}
							>
								立即同步
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				);
			},
		},
	];

	return (
		<div className="flex flex-col gap-3">
			<div className="flex flex-wrap items-center gap-2">
				<Button
					variant="outline"
					size="sm"
					disabled={isSyncing || allSyncableIds.length === 0}
					onClick={() => onSyncAgentIds(allSyncableIds)}
				>
					<RefreshCw size={14} />
					同步全部
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={isSyncing || checkedSyncableIds.length === 0}
					onClick={() => onSyncAgentIds(checkedSyncableIds)}
				>
					<SquareCheck size={14} />
					选择同步
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={checkedIds.size !== 1}
					onClick={() => {
						const only = agents.find((agent) => checkedIds.has(agent.id));
						if (only) onSelectAgent(only);
					}}
				>
					<Eye size={14} />
					查看差异
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={isSyncing || failedAgentIds.length === 0}
					onClick={() => onSyncAgentIds(failedAgentIds)}
				>
					<RotateCcw size={14} />
					重试失败
				</Button>
				<Button
					className="ml-auto"
					disabled={isSyncing || allSyncableIds.length === 0}
					onClick={() => onSyncAgentIds(allSyncableIds)}
				>
					<Zap size={14} />
					一键同步到所有 Agent
				</Button>
			</div>

			<div className="min-h-0 overflow-auto rounded-lg border">
				{isLoading ? (
					<SkeletonTable rows={4} columns={6} />
				) : agents.length === 0 ? (
					<EmptyState
						icon={Monitor}
						title="暂无已连接 Agent"
						description="启动时会自动探测本机 Claude Code / Cursor 等 Agent, 探测到后将在此列出"
						autoRefresh
						size="sm"
					/>
				) : (
					<DataTable
						columns={columns}
						rows={agents}
						rowKey={(row) => row.id}
						onRowClick={onSelectAgent}
						selectedRowKey={selectedId ?? undefined}
					/>
				)}
			</div>
		</div>
	);
}
