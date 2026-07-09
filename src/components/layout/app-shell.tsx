// 文件作用: 应用外壳(侧栏 + 内容区), 不含技术栈页脚
// 创建日期: 2026-07-09
import { Outlet } from 'react-router-dom';
import { Sidebar } from './sidebar';

/** 应用整体布局外壳 */
export function AppShell() {
	return (
		<div className="flex h-screen w-screen overflow-hidden">
			<Sidebar />
			<main className="flex-1 overflow-auto p-8">
				<Outlet />
			</main>
		</div>
	);
}
