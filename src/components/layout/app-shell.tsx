// 文件作用: 应用外壳(侧栏 + 内容区), 不含技术栈页脚
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { Outlet } from 'react-router-dom';
import { TooltipProvider } from '@/components/ui/tooltip';
import { Sidebar } from './sidebar';

/** 应用整体布局外壳; 用 TooltipProvider 包裹以支持全局 Tooltip 基元 */
export function AppShell() {
	return (
		<TooltipProvider>
			<div className="flex h-screen w-screen overflow-hidden">
				<Sidebar />
				{/* min-w-0 不可省略: main 是 Sidebar+main 这个 flex 行里的弹性子项, flex 子项默认
				    min-width:auto(即"不缩小到低于内容固有最小宽度"), 窗口收窄到最小宽 1024 时,
				    内部子级(表格/卡片网格等)只要有一处固有最小宽度较大, 就会把 main 撑宽进而挤压
				    甚至溢出整个外壳; 显式 min-w-0 交回"允许缩小到 0, 溢出由 main 自身的
				    overflow-auto 或各子级自己的截断/横向滚动处理"的正确语义 */}
				<main className="min-w-0 flex-1 overflow-auto p-8">
					<Outlet />
				</main>
			</div>
		</TooltipProvider>
	);
}
