// 文件作用: 侧栏导航(品牌标 + 6 导航项), 高亮当前路由
// 创建日期: 2026-07-09
import { NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Box } from 'lucide-react';
import { NAV_ITEMS } from './nav-config';

/** 左侧导航栏 */
export function Sidebar() {
	const { t } = useTranslation();
	return (
		<aside className="flex h-full w-56 flex-col border-r">
			<div className="flex items-center gap-2 px-5 py-5 text-lg font-bold">
				<Box color="var(--sh-brand)" />
				SkillHub
			</div>
			<nav className="flex flex-col gap-1 px-3">
				{NAV_ITEMS.map((item) => (
					<NavLink
						key={item.key}
						to={item.path}
						end={item.path === '/'}
						className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
						style={({ isActive }) => ({
							background: isActive ? 'var(--sh-brand-tint)' : 'transparent',
							color: isActive ? 'var(--sh-brand)' : 'var(--sh-fg)',
						})}
					>
						<item.icon size={18} />
						{t(item.i18nKey)}
					</NavLink>
				))}
			</nav>
		</aside>
	);
}
