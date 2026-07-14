// 文件作用: 应用根组件(装配 Provider 与路由 + 启动时自动初始化 Agent 探测/市场缓存/从已装
//           Agent 反向导入本地库)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { useEffect, useRef } from 'react';
import { RouterProvider, createHashRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { agentDetect } from '@/api/agent';
import { libraryImportFromAgents } from '@/api/library';
import { marketRefresh } from '@/api/market';
import { useUiStore } from '@/stores/ui';
import { ThemeProvider } from './theme/theme-provider';
import { routes } from './routes';
import './i18n';

const queryClient = new QueryClient();
// Tauri 自定义协议下用 HashRouter 更稳(刷新不 404)
const router = createHashRouter(routes);

// 与 pages/dashboard.tsx、pages/sync-center.tsx、pages/marketplace.tsx 内的同名字面量保持一致,
// 靠字符串值(而非引用)匹配同一份 React Query 缓存条目
const AGENT_LIST_KEY = 'agent-list';
const DASHBOARD_SUMMARY_KEY = 'dashboard-summary';
const MARKET_SEARCH_KEY = 'market-search';
// 与 pages/installed.tsx、pages/sync-center.tsx 内的同名字面量保持一致
const LIBRARY_LIST_KEY = 'library-list';

export default function App() {
	// M5 Task F1: 应用不再"进入即空"——启动时各自动触发一次 agent_detect/market_refresh(fire-
	// and-forget), 修复此前 Marketplace/Sync Center/首页首次进入必须先手动点"刷新"才有数据的问题。
	// initializedRef 只保证本次挂载只触发一次: StrictMode 开发环境下会对首次挂载的组件多模拟一轮
	// "卸载+重新挂载"以侵测未清理的副作用, 但 React 会在这轮模拟中还原 useRef 挂载前的值(而非
	// 重新初始化), 故足以防止重复触发; 不能用模块级变量替代守卫, 否则会跨测试文件残留状态
	const initializedRef = useRef(false);

	useEffect(() => {
		if (initializedRef.current) return;
		initializedRef.current = true;

		// 启动自动探测本机 Agent: 成功后失效 Agent 列表与首页概览, 二者的统计数值都依赖 Agent
		// 表。失败不阻塞界面渲染(静默忽略), 用户之后进入 Sync Center 页面时该页也会自动探测一次,
		// 相当于自然重试(见 pages/sync-center.tsx)
		agentDetect()
			.then(() => {
				queryClient.invalidateQueries({ queryKey: [AGENT_LIST_KEY] });
				queryClient.invalidateQueries({ queryKey: [DASHBOARD_SUMMARY_KEY] });
			})
			.catch(() => {
				/* 静默忽略: 刚进应用就弹错误提示打断用户没有必要 */
			});

		// 启动自动刷新市场缓存: market_refresh 是较重的网络操作, 后台跑即可, 不等待其结果;
		// 期间 Marketplace 页面按 searchQuery 的加载态展示"加载中"(见 pages/marketplace.tsx)。
		// 成功后标记本会话已刷新, 供 Marketplace 页面挂载时的"缓存为空则自动刷新"判断复用,
		// 避免同一会话内重复刷新
		marketRefresh()
			.then(() => {
				useUiStore.getState().setMarketRefreshed();
				queryClient.invalidateQueries({ queryKey: [MARKET_SEARCH_KEY] });
			})
			.catch(() => {
				/* 静默忽略: Marketplace 页面挂载时若发现缓存仍为空会再自动尝试一次 */
			});

		// 启动自动反向导入: 把各已探测 Agent 配置里"已装但本地库尚未收录"的 Skill/MCP 导入为
		// 本地库资源, 修复用户手动往 Agent 装过资源却在 SkillHub 里看不到的问题。成功后失效本地库
		// 列表与首页概览(二者数值都可能因新导入的资源而变化); 失败静默忽略, 不阻塞界面渲染
		// (与上面两个启动自动初始化调用同一策略)
		libraryImportFromAgents()
			.then(() => {
				queryClient.invalidateQueries({ queryKey: [LIBRARY_LIST_KEY] });
				queryClient.invalidateQueries({ queryKey: [DASHBOARD_SUMMARY_KEY] });
			})
			.catch(() => {
				/* 静默忽略: 与 agentDetect/marketRefresh 同一策略, 刚进应用就弹错误提示没有必要 */
			});
	}, []);

	return (
		<ThemeProvider>
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>
		</ThemeProvider>
	);
}
