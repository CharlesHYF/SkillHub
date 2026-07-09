// 文件作用: 首页(Dashboard)界面(还原原型第 1 屏) —— 4 张统计卡 + 最近变更 + 快速操作 +
//           同步状态概览; 数据经 dashboard_summary/activity_recent/agent_list 获取。
//           M0 阶段临时挂的 version/db health 探针 UI 随本次替换整段移除(app_health 命令本身
//           仍保留在后端, 只是首页不再读它)
// 创建日期: 2026-07-09
import type { LucideIcon } from 'lucide-react';
import {
	Download,
	FileDown,
	FileUp,
	ListTodo,
	Monitor,
	Plug,
	PlusCircle,
	RefreshCw,
	Sparkles,
	Trash2,
	Users,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';

import { agentList } from '@/api/agent';
import type { AgentRow } from '@/api/agent';
import { activityRecent, dashboardSummary, type DashboardSummary } from '@/api/dashboard';
import { DataTable, type DataTableColumn } from '@/components/common/data-table';
import { StatCard } from '@/components/common/stat-card';
import { SyncStatusBadge } from '@/components/common/sync-status-badge';
import { Button } from '@/components/ui/button';
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { formatRelativeTime } from '@/lib/utils';

const DASHBOARD_SUMMARY_KEY = 'dashboard-summary';
const ACTIVITY_RECENT_KEY = 'activity-recent';
// 与 pages/installed.tsx、pages/sync-center.tsx 保持字面一致, 共享同一份 React Query 缓存
const AGENT_LIST_KEY = 'agent-list';

/** 首页"最近变更"展示条数, 与原型一致 */
const RECENT_ACTIVITY_LIMIT = 6;

/** 数据尚未加载完成时统计卡的兜底值, 避免 undefined 访问报错(不是"假数据", 只是加载中的占位) */
const EMPTY_SUMMARY: DashboardSummary = {
	skillCount: 0,
	mcpCount: 0,
	agentCount: 0,
	onlineCount: 0,
	pendingCount: 0,
};

/** activity_log.act_type(1-新增,2-更新,3-下载,4-导入,5-导出,6-同步,7-卸载, 见
 * infra::repo_activity::add 文档注释)对应的展示图标; 未知编码兜底为 RefreshCw, 不因后端
 * 未来新增枚举值而崩溃 */
const ACT_TYPE_ICON: Record<number, LucideIcon> = {
	1: PlusCircle,
	2: RefreshCw,
	3: Download,
	4: FileDown,
	5: FileUp,
	6: RefreshCw,
	7: Trash2,
};

/** 快速操作项: 图标 + 文案 + 目标路由。下载资源指向 Marketplace(M2 内容占位), 导出全部/导入配置
 * 均指向 Import/Export(M3 内容占位, 与侧栏"导入导出"同一路由), 本页只负责导航, 不在此实现
 * 具体导出/导入逻辑 */
interface QuickAction {
	label: string;
	icon: LucideIcon;
	to: string;
}

const QUICK_ACTIONS: QuickAction[] = [
	{ label: '下载资源', icon: Download, to: '/marketplace' },
	{ label: '一键同步', icon: RefreshCw, to: '/sync' },
	{ label: '导出全部', icon: FileUp, to: '/portability' },
	{ label: '导入配置', icon: FileDown, to: '/portability' },
];

/** 同步状态概览表列配置: 名称 + 在线状态 + 最后同步时间, 均直接取自 agent_list, 不额外现算
 * 逐 Agent 差异(精确差异见 Sync Center 的 sync_diff, 见 services::dashboard::summary 文档注释) */
const AGENT_STATUS_COLUMNS: DataTableColumn<AgentRow>[] = [
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
		key: 'status',
		header: '连接状态',
		render: (row) => <SyncStatusBadge status={row.status ? '在线' : '离线'} />,
	},
	{
		key: 'lastSyncTime',
		header: '最后同步时间',
		render: (row) => (row.lastSyncTime ? formatRelativeTime(row.lastSyncTime) : '从未同步'),
	},
];

/** 首页(Dashboard): 还原原型第 1 屏 —— 统计卡 + 最近变更 + 快速操作 + 同步状态概览 */
export default function Dashboard() {
	const navigate = useNavigate();
	const queryClient = useQueryClient();

	const summaryQuery = useQuery({
		queryKey: [DASHBOARD_SUMMARY_KEY],
		queryFn: dashboardSummary,
	});
	const activityQuery = useQuery({
		queryKey: [ACTIVITY_RECENT_KEY, RECENT_ACTIVITY_LIMIT],
		queryFn: () => activityRecent(RECENT_ACTIVITY_LIMIT),
	});
	const agentsQuery = useQuery({ queryKey: [AGENT_LIST_KEY], queryFn: agentList });

	const summary = summaryQuery.data ?? EMPTY_SUMMARY;
	const offlineCount = summary.agentCount - summary.onlineCount;
	const activities = activityQuery.data ?? [];
	const agents = agentsQuery.data ?? [];

	const isFetching =
		summaryQuery.isFetching || activityQuery.isFetching || agentsQuery.isFetching;

	/** 顶部"刷新"按钮: 三处数据源统一失效重取, 呼应 Installed/Sync Center 页面的刷新语义 */
	function refreshAll() {
		queryClient.invalidateQueries({ queryKey: [DASHBOARD_SUMMARY_KEY] });
		queryClient.invalidateQueries({ queryKey: [ACTIVITY_RECENT_KEY] });
		queryClient.invalidateQueries({ queryKey: [AGENT_LIST_KEY] });
	}

	return (
		<div className="flex flex-col gap-4">
			<header className="flex items-center justify-between">
				<h1 className="text-2xl font-bold">首页 / Dashboard</h1>
				<Button variant="outline" onClick={refreshAll}>
					<RefreshCw size={14} className={isFetching ? 'animate-spin' : undefined} />
					刷新
				</Button>
			</header>

			<div className="grid grid-cols-4 gap-4">
				<StatCard icon={Sparkles} label="Skill 数量" value={summary.skillCount} />
				<StatCard icon={Plug} label="MCP 数量" value={summary.mcpCount} />
				<StatCard
					icon={Users}
					label="已连接 Agent"
					value={summary.agentCount}
					hint={`在线 ${summary.onlineCount} · 离线 ${offlineCount}`}
				/>
				<StatCard icon={ListTodo} label="待同步项" value={summary.pendingCount} />
			</div>

			<div className="grid grid-cols-3 gap-4">
				<Card className="col-span-2">
					<CardHeader>
						<CardTitle>最近变更 Recent Changes</CardTitle>
					</CardHeader>
					<CardContent>
						{activities.length === 0 ? (
							<p className="py-6 text-center text-sm text-muted-foreground">
								暂无活动记录
							</p>
						) : (
							<ul className="divide-y" style={{ borderColor: 'var(--sh-border)' }}>
								{activities.map((activity) => {
									const Icon = ACT_TYPE_ICON[activity.actType] ?? RefreshCw;
									return (
										<li
											key={activity.id}
											className="flex items-center gap-3 py-3"
										>
											<span
												className="flex size-9 shrink-0 items-center justify-center rounded-lg"
												style={{ background: 'var(--sh-brand-tint)' }}
											>
												<Icon size={16} color="var(--sh-brand)" />
											</span>
											<div className="min-w-0 flex-1">
												<p className="truncate text-sm font-medium text-foreground">
													{activity.title}
												</p>
												{activity.detail ? (
													<p className="truncate text-xs text-muted-foreground">
														{activity.detail}
													</p>
												) : null}
											</div>
											<span className="shrink-0 text-xs text-muted-foreground">
												{formatRelativeTime(activity.createTime)}
											</span>
										</li>
									);
								})}
							</ul>
						)}
					</CardContent>
				</Card>

				<Card>
					<CardHeader>
						<CardTitle>快速操作 Quick Actions</CardTitle>
					</CardHeader>
					<CardContent className="flex flex-col gap-2">
						{QUICK_ACTIONS.map((action) => (
							<Button
								key={action.label}
								variant="outline"
								className="w-full justify-start"
								onClick={() => navigate(action.to)}
							>
								<action.icon size={14} />
								{action.label}
							</Button>
						))}
					</CardContent>
				</Card>
			</div>

			<Card>
				<CardHeader>
					<CardTitle>同步状态概览 Sync Status</CardTitle>
					<CardAction>
						<Button variant="ghost" size="sm" onClick={() => navigate('/sync')}>
							查看全部 Agent
						</Button>
					</CardAction>
				</CardHeader>
				<CardContent>
					{agents.length === 0 ? (
						<p className="py-6 text-center text-sm text-muted-foreground">
							暂无已连接 Agent
						</p>
					) : (
						<DataTable
							columns={AGENT_STATUS_COLUMNS}
							rows={agents}
							rowKey={(row) => row.id}
						/>
					)}
				</CardContent>
			</Card>
		</div>
	);
}
