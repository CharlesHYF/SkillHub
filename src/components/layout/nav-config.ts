// 文件作用: 侧栏导航项配置(6 项, 对应 6 个顶级路由)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import {
	Home,
	Store,
	Package,
	RefreshCw,
	ArrowLeftRight,
	Settings,
	type LucideIcon,
} from 'lucide-react';

/** 导航项 */
export interface NavItem {
	key: string;
	path: string;
	icon: LucideIcon;
	i18nKey: string;
}

/** 6 个顶级导航项(资源详情/安装是资源中心的子路由, 不单列导航) */
export const NAV_ITEMS: NavItem[] = [
	{ key: 'dashboard', path: '/', icon: Home, i18nKey: 'nav.dashboard' },
	{ key: 'marketplace', path: '/marketplace', icon: Store, i18nKey: 'nav.marketplace' },
	{ key: 'installed', path: '/installed', icon: Package, i18nKey: 'nav.installed' },
	{ key: 'sync', path: '/sync', icon: RefreshCw, i18nKey: 'nav.sync' },
	{ key: 'portability', path: '/portability', icon: ArrowLeftRight, i18nKey: 'nav.portability' },
	{ key: 'settings', path: '/settings', icon: Settings, i18nKey: 'nav.settings' },
];
