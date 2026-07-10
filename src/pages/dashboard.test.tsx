// 文件作用: Dashboard 页面集成测试(mock src/api) —— 统计卡渲染 dashboard_summary、最近变更渲染
//           activity_recent、快速操作按钮点击各自导航到目标路由、同步状态概览渲染 agent_list、
//           不再渲染手动"刷新"按钮(M5 Task F1: 三处数据源改由 refetchInterval 等策略自动保鲜)
// 创建日期: 2026-07-09
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { AgentRow } from '@/api/agent';
import type { ActivityRow, DashboardSummary } from '@/api/dashboard';
import Dashboard from './dashboard';

vi.mock('@/api/dashboard', () => ({
	dashboardSummary: vi.fn(),
	activityRecent: vi.fn(),
}));
vi.mock('@/api/agent', () => ({
	agentList: vi.fn(),
}));

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async (importOriginal) => {
	const actual = await importOriginal<typeof import('react-router-dom')>();
	return { ...actual, useNavigate: () => mockNavigate };
});

import { activityRecent, dashboardSummary } from '@/api/dashboard';
import { agentList } from '@/api/agent';

function makeSummary(overrides: Partial<DashboardSummary> = {}): DashboardSummary {
	return {
		skillCount: 128,
		mcpCount: 45,
		agentCount: 8,
		onlineCount: 6,
		pendingCount: 23,
		...overrides,
	};
}

function makeAgent(overrides: Partial<AgentRow> = {}): AgentRow {
	return {
		id: 1,
		agentKind: 'ClaudeCode',
		name: 'Agent Alpha',
		configPath: '/home/demo/.claude.json',
		scope: 'Global',
		status: true,
		lastSyncTime: '2026-07-09 10:00:00',
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

function renderDashboard() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<Dashboard />
		</QueryClientProvider>,
	);
}

describe('Dashboard 页面', () => {
	beforeEach(() => {
		mockNavigate.mockReset();
		vi.mocked(dashboardSummary).mockReset().mockResolvedValue(makeSummary());
		vi.mocked(activityRecent).mockReset().mockResolvedValue([]);
		vi.mocked(agentList).mockReset().mockResolvedValue([]);
	});

	it('应渲染 dashboard_summary 的 4 项统计卡数值', async () => {
		renderDashboard();

		// 等待实际数值(而非标签)出现: 标签在 summary 查询解析前就已用 EMPTY_SUMMARY 兜底渲染,
		// 等标签反而会在数据落地前就断言, 导致后续数值断言假性失败
		expect(await screen.findByText('128')).toBeInTheDocument();
		expect(screen.getByText('Skill 数量')).toBeInTheDocument();
		expect(screen.getByText('MCP 数量')).toBeInTheDocument();
		expect(screen.getByText('45')).toBeInTheDocument();
		expect(screen.getByText('已连接 Agent')).toBeInTheDocument();
		expect(screen.getByText('8')).toBeInTheDocument();
		expect(screen.getByText('待同步项')).toBeInTheDocument();
		expect(screen.getByText('23')).toBeInTheDocument();
		// 已连接 Agent 的 hint 应据 online/offline 拆分展示(offline = agentCount - onlineCount)
		expect(screen.getByText('在线 6 · 离线 2')).toBeInTheDocument();
	});

	it('应渲染 activity_recent 返回的最近变更列表(标题/详情/相对时间)', async () => {
		const fiveMinAgo = new Date(Date.now() - 5 * 60 * 1000)
			.toISOString()
			.slice(0, 19)
			.replace('T', ' ');
		const rows: ActivityRow[] = [
			{
				id: 1,
				actType: 1,
				resType: 1,
				title: '新增 Skill: data-visualizer',
				detail: '版本 1.2.0',
				createTime: fiveMinAgo,
			},
		];
		vi.mocked(activityRecent).mockResolvedValue(rows);

		renderDashboard();

		expect(await screen.findByText('新增 Skill: data-visualizer')).toBeInTheDocument();
		expect(screen.getByText('版本 1.2.0')).toBeInTheDocument();
		expect(screen.getByText(/分钟前|刚刚/)).toBeInTheDocument();
	});

	it('无活动记录时最近变更应显示空态文案', async () => {
		renderDashboard();
		expect(await screen.findByText('暂无活动记录')).toBeInTheDocument();
	});

	it('快速操作 4 个按钮应存在且点击后各自导航到目标路由', async () => {
		const user = userEvent.setup();
		renderDashboard();

		await user.click(await screen.findByRole('button', { name: '下载资源' }));
		expect(mockNavigate).toHaveBeenCalledWith('/marketplace');

		await user.click(screen.getByRole('button', { name: '一键同步' }));
		expect(mockNavigate).toHaveBeenCalledWith('/sync');

		await user.click(screen.getByRole('button', { name: '导出全部' }));
		expect(mockNavigate).toHaveBeenCalledWith('/portability');

		await user.click(screen.getByRole('button', { name: '导入配置' }));
		expect(mockNavigate).toHaveBeenCalledWith('/portability');
	});

	it('应渲染 agent_list 聚合而来的同步状态概览(名称+在线状态+最后同步时间)', async () => {
		vi.mocked(agentList).mockResolvedValue([
			makeAgent({ id: 1, name: 'Agent Alpha', status: true }),
			makeAgent({ id: 2, name: 'Agent Gamma', status: false, lastSyncTime: '' }),
		]);

		renderDashboard();

		expect(await screen.findByText('Agent Alpha')).toBeInTheDocument();
		expect(screen.getByText('Agent Gamma')).toBeInTheDocument();
		expect(screen.getByText('在线')).toBeInTheDocument();
		expect(screen.getByText('离线')).toBeInTheDocument();
		expect(screen.getByText('从未同步')).toBeInTheDocument();
	});

	it('点击"查看全部 Agent"应导航到 /sync', async () => {
		const user = userEvent.setup();
		renderDashboard();

		await user.click(await screen.findByRole('button', { name: '查看全部 Agent' }));
		expect(mockNavigate).toHaveBeenCalledWith('/sync');
	});

	it('不应再渲染手动"刷新"按钮', async () => {
		renderDashboard();
		await screen.findByText('128');
		expect(screen.queryByRole('button', { name: /^刷新$/ })).not.toBeInTheDocument();
	});
});
