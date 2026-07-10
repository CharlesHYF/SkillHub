// 文件作用: Sync Center 底部左侧"同步概览"面板 —— 选中 Agent 的基本信息 + 新增/更新/移除/
//           待同步总计迷你统计 + 最后同步时间/上次结果/上次详情; 纯展示组件, diffCounts/
//           lastOutcome 由 pages/sync-center 统一持有并计算
// 创建日期: 2026-07-09
import { ListChecks, Minus, Plus, RefreshCw } from 'lucide-react';

import type { AgentRow } from '@/api/agent';
import type { SyncSummary } from '@/api/sync';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { SyncStatusBadge } from '@/components/common/sync-status-badge';
import { StatCard } from '@/components/common/stat-card';
import { formatRelativeTime } from '@/lib/utils';
import { deriveAgentSyncStatus, lastResultLabel, type DiffCounts } from './agent-display';

interface SyncOverviewCardProps {
	/** 选中的 Agent; null 表示尚未选中任何 Agent */
	agent: AgentRow | null;
	diffCounts: DiffCounts;
	/** 本次会话内最近一次单独同步该 Agent 的结果; undefined 表示本次会话尚未同步过它
	 * (后端未提供跨会话的同步历史查询命令, 见 agent-display.deriveAgentSyncStatus 的说明) */
	lastOutcome?: SyncSummary;
}

/** Sync Center 底部左侧同步概览面板: 选中 Agent 信息 + 新增/更新/移除/待同步总计 + 上次同步结果 */
export function SyncOverviewCard({ agent, diffCounts, lastOutcome }: SyncOverviewCardProps) {
	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle>
					{agent ? (
						<span className="flex items-center gap-2">
							<span className="text-muted-foreground">选择的 Agent:</span>
							<span className="font-semibold">{agent.name}</span>
							<SyncStatusBadge status={deriveAgentSyncStatus(agent, lastOutcome)} />
						</span>
					) : (
						'同步概览'
					)}
				</CardTitle>
			</CardHeader>
			<CardContent className="flex min-h-0 flex-1 flex-col gap-4">
				{!agent ? (
					<p className="text-sm text-muted-foreground">
						请从左侧表格选择一个 Agent 查看同步概览
					</p>
				) : (
					<>
						<p
							className="truncate text-xs text-muted-foreground"
							title={agent.configPath}
						>
							{agent.configPath}
						</p>
						<div className="grid grid-cols-4 gap-3">
							<StatCard icon={Plus} label="新增" value={diffCounts.add} />
							<StatCard icon={RefreshCw} label="更新" value={diffCounts.update} />
							<StatCard icon={Minus} label="移除" value={diffCounts.remove} />
							<StatCard
								icon={ListChecks}
								label="待同步总计"
								value={diffCounts.total}
							/>
						</div>
						<div className="grid grid-cols-3 gap-3 text-sm">
							<div>
								<p className="text-xs text-muted-foreground">最后同步时间</p>
								<p className="text-foreground">
									{agent.lastSyncTime
										? formatRelativeTime(agent.lastSyncTime)
										: '—'}
								</p>
							</div>
							<div>
								<p className="text-xs text-muted-foreground">上次结果</p>
								<p className="text-foreground">{lastResultLabel(lastOutcome)}</p>
							</div>
							<div>
								<p className="text-xs text-muted-foreground">上次详情</p>
								<p className="text-foreground">
									{lastOutcome
										? `${lastOutcome.success} 成功 / ${lastOutcome.failed} 失败 / ${lastOutcome.skipped} 跳过`
										: '—'}
								</p>
							</div>
						</div>
					</>
				)}
			</CardContent>
		</Card>
	);
}
