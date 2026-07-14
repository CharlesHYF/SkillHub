// 文件作用: 首页/同步中心用的统计卡片(图标 + 标签 + 数值 + 可选说明); 可选 loading 态下
//           数值/说明以骨架块占位, 避免首次加载时先闪一个 0 再跳到真实值
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import type { LucideIcon } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/common/skeleton';

interface StatCardProps {
	icon: LucideIcon;
	label: string;
	value: string | number;
	hint?: string;
	/** 首次加载中: 为真时以骨架块占位数值与说明, 提升感知性能 */
	loading?: boolean;
}

/** 统计卡片: 品牌轻染图标底 + 标签 + 数值 + 可选说明, 强调色只用在图标上 */
export function StatCard({ icon: Icon, label, value, hint, loading = false }: StatCardProps) {
	return (
		<Card>
			<CardContent className="flex items-center gap-4">
				<span
					className="flex size-10 shrink-0 items-center justify-center rounded-lg"
					style={{ background: 'var(--sh-brand-tint)' }}
				>
					<Icon size={20} color="var(--sh-brand)" />
				</span>
				<div className="min-w-0 flex-1">
					<p className="text-sm text-muted-foreground">{label}</p>
					{loading ? (
						<Skeleton className="my-1 h-7 w-14" />
					) : (
						<p className="text-2xl font-semibold text-foreground">{value}</p>
					)}
					{hint && !loading ? (
						<p className="text-xs text-muted-foreground">{hint}</p>
					) : null}
				</div>
			</CardContent>
		</Card>
	);
}
