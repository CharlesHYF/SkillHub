// 文件作用: 侧栏导航(品牌标 + 6 导航项), 高亮当前路由
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Box } from 'lucide-react';
import { cn } from '@/lib/utils';
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
						// 当前项: 品牌轻染底 + 品牌色文字(经内联 style, 呼应 DESIGN.md 当前项用显式
						// --sh-brand-tint 而非 alpha 叠色); 非当前项: 常态中性文字, hover 轻染中性底,
						// 过渡克制。focus-visible 焦点环供键盘导航
						className={({ isActive }) =>
							cn(
								'flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
								!isActive && 'text-foreground hover:bg-muted',
							)
						}
						style={({ isActive }) =>
							isActive
								? { background: 'var(--sh-brand-tint)', color: 'var(--sh-brand)' }
								: undefined
						}
					>
						<item.icon size={18} />
						{t(item.i18nKey)}
					</NavLink>
				))}
			</nav>
		</aside>
	);
}
