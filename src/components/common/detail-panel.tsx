// 文件作用: 右侧详情面板容器(标题栏 + 关闭按钮 + 内容插槽), 定位/显隐由调用方决定
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import type { ReactNode } from 'react';
import { X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface DetailPanelProps {
	title: ReactNode;
	onClose: () => void;
	children?: ReactNode;
	className?: string;
}

/** 右侧详情面板容器: 标题栏(标题 + 关闭) + 可滚动内容区; 纯展示容器, 不含定位/动画 */
export function DetailPanel({ title, onClose, children, className }: DetailPanelProps) {
	return (
		<aside
			className={cn('flex h-full flex-col border-l bg-card text-card-foreground', className)}
		>
			<div className="flex items-center justify-between gap-2 border-b px-4 py-3">
				<h2 className="truncate text-sm font-semibold">{title}</h2>
				<Button variant="ghost" size="icon-sm" aria-label="关闭" onClick={onClose}>
					<X size={16} />
				</Button>
			</div>
			<div className="flex-1 overflow-auto p-4">{children}</div>
		</aside>
	);
}
