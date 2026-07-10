// 文件作用: 六屏共享的页面页眉 —— 统一页面主标题(text-2xl 粗体)+ 可选一句副标题(次要灰) +
//           可选右侧操作区, 使六屏标题字号字重/留白节奏一致(见 DESIGN.md 排版层级)。此前六屏
//           各自内联 <header><h1> 结构, 抽为组件后仅一处维护, 且天然承载副标题这一层级
// 创建日期: 2026-07-10
import type { ReactNode } from 'react';

interface PageHeaderProps {
	/** 页面主标题(双语, 如 "首页 / Dashboard") */
	title: string;
	/** 副标题: 一句话说明本屏用途, 可选 */
	description?: string;
	/** 右侧操作区(按钮等), 可选 */
	actions?: ReactNode;
}

/** 页面页眉: 主标题 + 可选副标题 + 可选右侧操作区 */
export function PageHeader({ title, description, actions }: PageHeaderProps) {
	return (
		<header className="flex items-start justify-between gap-4">
			<div className="flex flex-col gap-0.5">
				<h1 className="text-2xl font-bold tracking-tight text-foreground">{title}</h1>
				{description ? (
					<p className="text-sm text-muted-foreground">{description}</p>
				) : null}
			</div>
			{actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
		</header>
	);
}
