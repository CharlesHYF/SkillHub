// 文件作用: Sync Center 底部右侧"差异详情"面板 —— 选中 Agent 的本地版本 vs Agent 版本,
//           Tab(全部/新增/更新/移除)按 DiffItem.action 分组过滤; 纯展示组件, diffPlan 的获取
//           (sync_diff)由 pages/sync-center 统一持有
// 创建日期: 2026-07-09
import { useState } from 'react';
import { CheckCircle2, MousePointerClick } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { DataTable, type DataTableColumn } from '@/components/common/data-table';
import { EmptyState } from '@/components/common/empty-state';
import { SkeletonTable } from '@/components/common/skeleton';
import { SyncStatusBadge } from '@/components/common/sync-status-badge';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { toResourceKind } from '@/components/installed/resource-display';
import { TypeBadge } from '@/components/common/type-badge';
import type { DiffAction, DiffItem, DiffPlanRespVO } from '@/api/sync';
import { countDiffByAction, filterDiffItems } from './agent-display';

interface DiffDetailPanelProps {
	/** 选中 Agent 的差异计划; undefined 表示尚未选中任何 Agent(或该 Agent 的 diff 还没加载完成,
	 * 由 isLoading 区分) */
	diffPlan: DiffPlanRespVO | undefined;
	isLoading?: boolean;
}

const TAB_LABEL: Record<DiffAction | 'All', string> = {
	All: '全部',
	Add: '新增',
	Update: '更新',
	Remove: '移除',
};

const columns: DataTableColumn<DiffItem>[] = [
	{
		key: 'resType',
		header: '类型',
		render: (row) => <TypeBadge type={toResourceKind(row.resType)} />,
	},
	{ key: 'name', header: '名称' },
	{ key: 'localVer', header: '本地版本', render: (row) => row.localVer || '—' },
	{ key: 'agentVer', header: 'Agent 版本', render: (row) => row.agentVer || '—' },
	{
		key: 'state',
		header: '状态',
		// diff 计划里的每一项本身就代表"尚待落地的差异", 状态恒为待同步(应用后该项就不会再出现
		// 在下一次 diff 结果里了), 不需要额外字段承载
		render: () => <SyncStatusBadge status="待同步" />,
	},
];

/** Sync Center 底部右侧差异详情面板: Tab(全部/新增/更新/移除) + 差异条目表。Tab 过滤态为本组件
 * 内部状态(纯展示层关注点, 无需上提); 若调用方需要在切换选中 Agent 时把 Tab 重置回"全部",
 * 按 React 惯例以 `key={selectedAgentId}` 挂载即可(见 pages/sync-center 的用法) */
export function DiffDetailPanel({ diffPlan, isLoading = false }: DiffDetailPanelProps) {
	const [filter, setFilter] = useState<DiffAction | 'All'>('All');
	const counts = countDiffByAction(diffPlan);
	const countByTab: Record<DiffAction | 'All', number> = {
		All: counts.total,
		Add: counts.add,
		Update: counts.update,
		Remove: counts.remove,
	};
	const visibleItems = filterDiffItems(diffPlan?.items ?? [], filter);

	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle>差异详情(本地版本 vs Agent 版本)</CardTitle>
			</CardHeader>
			<CardContent className="flex min-h-0 flex-1 flex-col gap-3">
				{isLoading ? (
					<SkeletonTable rows={4} columns={4} />
				) : diffPlan === undefined ? (
					<EmptyState
						icon={MousePointerClick}
						title="未选中 Agent"
						description="请从左侧表格选择一个 Agent 查看差异详情"
						size="sm"
					/>
				) : diffPlan.items.length === 0 ? (
					<EmptyState
						icon={CheckCircle2}
						title="已与本地库保持一致"
						description="该 Agent 的资源与本地库一致, 目前没有需要同步的差异"
						size="sm"
					/>
				) : (
					<>
						<Tabs
							value={filter}
							onValueChange={(value) => setFilter(value as DiffAction | 'All')}
						>
							<TabsList variant="line">
								{(['All', 'Add', 'Update', 'Remove'] as const).map((action) => (
									<TabsTrigger key={action} value={action}>
										{TAB_LABEL[action]}({countByTab[action]})
									</TabsTrigger>
								))}
							</TabsList>
						</Tabs>
						<div className="min-h-0 flex-1 overflow-auto">
							<DataTable
								columns={columns}
								rows={visibleItems}
								rowKey={(row) => `${row.resType}:${row.name}`}
							/>
						</div>
					</>
				)}
			</CardContent>
		</Card>
	);
}
