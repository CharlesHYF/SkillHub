// 文件作用: 应用根组件(装配 Provider 与路由)
// 创建日期: 2026-07-09
import { RouterProvider, createHashRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from './theme/theme-provider';
import { routes } from './routes';
import './i18n';

const queryClient = new QueryClient();
// Tauri 自定义协议下用 HashRouter 更稳(刷新不 404)
const router = createHashRouter(routes);

export default function App() {
	return (
		<ThemeProvider>
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>
		</ThemeProvider>
	);
}
