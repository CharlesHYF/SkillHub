// 文件作用: 首页/同步中心用的统计卡片(图标 + 标签 + 数值 + 可选说明)
// 创建日期: 2026-07-09
import type { LucideIcon } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';

interface StatCardProps {
	icon: LucideIcon;
	label: string;
	value: string | number;
	hint?: string;
}

/** 统计卡片: 品牌轻染图标底 + 标签 + 数值 + 可选说明, 强调色只用在图标上 */
export function StatCard({ icon: Icon, label, value, hint }: StatCardProps) {
	return (
		<Card>
			<CardContent className="flex items-center gap-4">
				<span
					className="flex size-10 shrink-0 items-center justify-center rounded-lg"
					style={{ background: 'var(--sh-brand-tint)' }}
				>
					<Icon size={20} color="var(--sh-brand)" />
				</span>
				<div className="min-w-0">
					<p className="text-sm text-muted-foreground">{label}</p>
					<p className="text-2xl font-semibold text-foreground">{value}</p>
					{hint ? <p className="text-xs text-muted-foreground">{hint}</p> : null}
				</div>
			</CardContent>
		</Card>
	);
}
