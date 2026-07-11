// 文件作用: 六屏共享的骨架屏组件 —— 列表/表格/卡片网格加载时以脉冲占位块替代白屏或裸转圈,
//           提升感知性能(见 DESIGN.md 动效/感知性能)。Skeleton 为单个脉冲块;
//           SkeletonList/SkeletonTable/SkeletonCards 为三种数据区形状(列表/表格/卡片网格)的
//           组合, 分别贴合 Dashboard 列表、Installed/Sync/Portability 表格、Marketplace 卡片网格,
//           保证六屏加载态风格一致。脉冲动画尊重 prefers-reduced-motion(reduce 时不脉冲, 仍是
//           占位块)。组合骨架以 role=status + aria-label 标注, 供无障碍读屏与测试识别加载态
// 创建日期: 2026-07-10
import { cn } from '@/lib/utils';

/** 单个骨架块: 中性轻染 + 脉冲; 尺寸由调用方经 className 决定 */
export function Skeleton({ className }: { className?: string }) {
	return (
		<div
			aria-hidden
			className={cn(
				'animate-pulse rounded-md bg-muted motion-reduce:animate-none',
				className,
			)}
		/>
	);
}

interface SkeletonListProps {
	/** 占位行数 */
	rows?: number;
	className?: string;
}

/** 列表骨架: rows 行, 每行 图标圆 + 两行文字 + 行尾短块, 贴合 Dashboard"最近变更"等列表区 */
export function SkeletonList({ rows = 4, className }: SkeletonListProps) {
	return (
		<div role="status" aria-label="加载中" className={cn('flex flex-col', className)}>
			{Array.from({ length: rows }).map((_, i) => (
				<div key={i} className="flex items-center gap-3 py-3">
					<Skeleton className="size-9 shrink-0 rounded-lg" />
					<div className="flex min-w-0 flex-1 flex-col gap-1.5">
						<Skeleton className="h-3.5 w-1/3" />
						<Skeleton className="h-3 w-1/2" />
					</div>
					<Skeleton className="h-3 w-14 shrink-0" />
				</div>
			))}
		</div>
	);
}

interface SkeletonTableProps {
	/** 占位行数 */
	rows?: number;
	/** 每行占位单元格数 */
	columns?: number;
	className?: string;
}

/** 表格骨架: rows x columns 单元格, 首列略窄呼应"名称"列, 贴合各表格区加载态 */
export function SkeletonTable({ rows = 5, columns = 4, className }: SkeletonTableProps) {
	return (
		<div role="status" aria-label="加载中" className={cn('flex flex-col gap-3 p-3', className)}>
			{Array.from({ length: rows }).map((_, r) => (
				<div key={r} className="flex items-center gap-4">
					{Array.from({ length: columns }).map((_, c) => (
						<Skeleton key={c} className={cn('h-4', c === 0 ? 'w-1/5' : 'flex-1')} />
					))}
				</div>
			))}
		</div>
	);
}

interface SkeletonCardsProps {
	/** 占位卡片数 */
	count?: number;
	className?: string;
}

/** 卡片网格骨架: count 张卡片占位(外形近似 MarketCard), 贴合 Marketplace 卡片网格区; 列数规则
 * 与 marketplace/market-list.tsx 的实际卡片网格保持一致(稳定两列, 仅容器极窄时降为单列),
 * 避免加载态与加载完成后的实际内容列数对不上而"跳一下"。本组件不自带 @container: 调用方
 * (market-list.tsx)已在外层声明容器, @max-[500px]: 直接查询该祖先容器即可, 断点取值同一口径;
 * 若未来脱离该外层容器单独复用, 会安全回退为固定两列(容器查询无祖先容器时不生效, 不会报错) */
export function SkeletonCards({ count = 4, className }: SkeletonCardsProps) {
	return (
		<div
			role="status"
			aria-label="加载中"
			className={cn(
				'grid auto-rows-min grid-cols-2 gap-4 @max-[500px]:grid-cols-1',
				className,
			)}
		>
			{Array.from({ length: count }).map((_, i) => (
				<div key={i} className="flex flex-col gap-3 rounded-lg border p-4">
					<div className="flex items-start gap-3">
						<Skeleton className="size-10 shrink-0 rounded-lg" />
						<div className="flex min-w-0 flex-1 flex-col gap-2">
							<Skeleton className="h-4 w-1/2" />
							<Skeleton className="h-3 w-full" />
							<Skeleton className="h-3 w-2/3" />
						</div>
					</div>
					<div className="flex items-center justify-between">
						<Skeleton className="h-3 w-16" />
						<Skeleton className="h-8 w-28" />
					</div>
				</div>
			))}
		</div>
	);
}
