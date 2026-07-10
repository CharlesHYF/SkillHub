// 文件作用: 六屏共享的空态组件 —— lucide 图标 + 标题 + 可选说明 + 可选行动区/自动保鲜提示,
//           统一各数据区"暂无X"的呈现, 替换此前散落各屏的裸居中文案(实机主要痛点; 见 DESIGN.md
//           "状态齐全"原则)。size=default 用于整屏数据区, sm 用于面板内较小空态, 图标/间距/字号
//           随之收敛, 保证六屏空态风格一致
// 创建日期: 2026-07-10
import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import { Loader2 } from 'lucide-react';

import { cn } from '@/lib/utils';

interface EmptyStateProps {
	/** 语义图标(lucide), 统一以品牌轻染圆底承载, 尺寸随 size 收敛 */
	icon: LucideIcon;
	/** 标题: 一句话概括"这里为什么是空的" */
	title: string;
	/** 说明: 补充一句下一步指引或原因, 可选 */
	description?: string;
	/** 行动区: 通常是一个按钮或链接, 可选 */
	action?: ReactNode;
	/** 为真时在下方追加"正在自动保持最新"提示: 用于走实时保鲜策略的数据区(见 lib/query.ts),
	 * 告诉用户数据会自动刷新、无需手动操作(呼应 F1 去掉手动刷新按钮后的交互预期) */
	autoRefresh?: boolean;
	/** 尺寸: default 用于整屏数据区, sm 用于详情/概览面板内的小空态 */
	size?: 'default' | 'sm';
	className?: string;
}

/** 自动保鲜提示文案: 六屏统一措辞, 避免各处各写一句 */
const AUTO_REFRESH_TEXT = '数据会自动保持最新, 无需手动刷新';

/** 空态: 品牌轻染圆底图标 + 标题 + 可选说明 + 可选自动保鲜提示 + 可选行动区, 居中排布 */
export function EmptyState({
	icon: Icon,
	title,
	description,
	action,
	autoRefresh = false,
	size = 'default',
	className,
}: EmptyStateProps) {
	const compact = size === 'sm';
	return (
		<div
			className={cn(
				'flex flex-col items-center justify-center text-center',
				compact ? 'gap-2 px-4 py-8' : 'gap-3 px-6 py-12',
				className,
			)}
		>
			<span
				className={cn(
					'flex shrink-0 items-center justify-center rounded-full',
					compact ? 'size-9' : 'size-12',
				)}
				style={{ background: 'var(--sh-brand-tint)' }}
			>
				<Icon size={compact ? 18 : 22} color="var(--sh-brand)" />
			</span>
			<div className="flex flex-col gap-1">
				<p className={cn('font-medium text-foreground', compact ? 'text-sm' : 'text-base')}>
					{title}
				</p>
				{description ? (
					<p
						className={cn(
							'mx-auto max-w-xs text-muted-foreground',
							compact ? 'text-xs' : 'text-sm',
						)}
					>
						{description}
					</p>
				) : null}
			</div>
			{autoRefresh ? (
				<p className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
					<Loader2 size={13} className="animate-spin motion-reduce:animate-none" />
					{AUTO_REFRESH_TEXT}
				</p>
			) : null}
			{action ? <div className={compact ? 'mt-0.5' : 'mt-1'}>{action}</div> : null}
		</div>
	);
}
