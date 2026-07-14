// 文件作用: 路由表(7 条: 6 导航路由 + marketplace 详情子路由)
// 创建日期: 2026-07-09
import type { RouteObject } from 'react-router-dom';
import { AppShell } from './components/layout/app-shell';
import Dashboard from './pages/dashboard';
import Marketplace from './pages/marketplace';
import MarketplaceDetail from './pages/marketplace-detail';
import Installed from './pages/installed';
import SyncCenter from './pages/sync-center';
import Portability from './pages/portability';
import SettingRespVO from './pages/settings';

/** 应用路由表 */
export const routes: RouteObject[] = [
	{
		path: '/',
		element: <AppShell />,
		children: [
			{ index: true, element: <Dashboard /> },
			{ path: 'marketplace', element: <Marketplace /> },
			{ path: 'marketplace/:id', element: <MarketplaceDetail /> },
			{ path: 'installed', element: <Installed /> },
			{ path: 'sync', element: <SyncCenter /> },
			{ path: 'portability', element: <Portability /> },
			{ path: 'settings', element: <SettingRespVO /> },
		],
	},
];
