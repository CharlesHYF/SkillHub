// 文件作用: 同步状态徽标, 语义色只落在小圆点+文字上, 徽标底色/边框保持中性(见 DESIGN.md)
// 创建日期: 2026-07-09
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

/** 同步状态, 与后端 resource_agent.sync_status(0-4)语义一一对应, 由调用方按需映射数字码 */
export type SyncStatus = '已同步' | '待同步' | '失败' | '本地修改' | '已禁用';

/** 各状态对应的语义色: 已同步=ok, 待同步=warn, 失败=danger; 本地修改/已禁用为中性态, 不用状态色 */
const STATUS_COLOR: Record<SyncStatus, string> = {
	已同步: 'var(--sh-ok)',
	待同步: 'var(--sh-warn)',
	失败: 'var(--sh-danger)',
	本地修改: 'var(--sh-muted)',
	已禁用: 'var(--sh-muted)',
};

interface SyncStatusBadgeProps {
	status: SyncStatus;
	className?: string;
}

/** 同步状态徽标: 中性描边容器 + 语义色圆点 + 同色文字, 不给徽标底色/边框上色 */
export function SyncStatusBadge({ status, className }: SyncStatusBadgeProps) {
	const color = STATUS_COLOR[status];
	return (
		<Badge variant="outline" className={cn('gap-1.5', className)}>
			<span
				aria-hidden
				data-testid="sync-status-dot"
				className="size-1.5 shrink-0 rounded-full"
				style={{ background: color }}
			/>
			<span style={{ color }}>{status}</span>
		</Badge>
	);
}
