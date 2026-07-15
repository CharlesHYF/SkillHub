// 文件作用: Installed 页面集成测试(mock src/api) —— 表格渲染/行选中开面板/卸载确认流程/
//           同步到全部 Agent 只对在线 Agent 生效
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ResourceRespVO } from '@/api/library';
import type { AgentRespVO } from '@/api/agent';
import type { ResourceAgentLinkRespVO } from '@/api/sync';
import { useUiStore } from '@/stores/ui';
import Installed from './installed';

vi.mock('@/api/library', () => ({
	libraryList: vi.fn(),
	resourceSetEnabled: vi.fn().mockResolvedValue(undefined),
	resourceDelete: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('@/api/sync', () => ({
	resourceAgentLinks: vi.fn().mockResolvedValue([]),
	syncApply: vi.fn().mockResolvedValue({ success: 0, failed: 0, skipped: 0 }),
}));
vi.mock('@/api/agent', () => ({
	agentList: vi.fn().mockResolvedValue([]),
}));

import { libraryList, resourceDelete } from '@/api/library';
import { resourceAgentLinks, syncApply } from '@/api/sync';
import { agentList } from '@/api/agent';

function makeResource(overrides: Partial<ResourceRespVO> = {}): ResourceRespVO {
	const name = overrides.name ?? 'data-visualizer';
	return {
		id: 1,
		resType: 'Skill',
		name,
		displayName: name,
		version: '1.2.0',
		sourceType: 'LocalImport',
		localPath: '/tmp/data-visualizer',
		enabled: true,
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

function renderInstalled() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<Installed />
		</QueryClientProvider>,
	);
}

describe('Installed 页面', () => {
	beforeEach(() => {
		useUiStore.getState().reset();
		vi.mocked(libraryList).mockReset();
		vi.mocked(resourceAgentLinks).mockReset().mockResolvedValue([]);
		vi.mocked(agentList).mockReset().mockResolvedValue([]);
		vi.mocked(resourceDelete).mockReset().mockResolvedValue(undefined);
		vi.mocked(syncApply).mockReset().mockResolvedValue({ success: 0, failed: 0, skipped: 0 });
	});

	it('应渲染 library_list 返回的资源行, 类型/同步状态徽标可见, 点击行打开详情面板显示该资源名', async () => {
		const resources = [
			makeResource({ id: 1, name: 'data-visualizer', resType: 'Skill', enabled: true }),
			makeResource({ id: 2, name: 'filesystem', resType: 'Mcp', enabled: false }),
		];
		vi.mocked(libraryList).mockResolvedValue(resources);

		renderInstalled();

		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();
		expect(screen.getByText('filesystem')).toBeInTheDocument();
		expect(screen.getByText('Skill')).toBeInTheDocument();
		expect(screen.getByText('已禁用')).toBeInTheDocument();

		fireEvent.click(screen.getByText('data-visualizer'));

		// DetailPanel 标题栏也会渲染一次资源名, 断言至少出现两处(列表行 + 面板标题)即视为已打开
		expect(await screen.findAllByText('data-visualizer')).toHaveLength(2);
	});

	it('卸载: 行内菜单点击卸载 -> 弹确认框 -> 确认后调用 resource_delete', async () => {
		const user = userEvent.setup();
		const resource = makeResource({ id: 1, name: 'data-visualizer' });
		vi.mocked(libraryList).mockResolvedValue([resource]);

		renderInstalled();
		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: 'data-visualizer 操作' }));
		await user.click(await screen.findByText('卸载'));

		expect(await screen.findByText(/确认卸载/)).toBeInTheDocument();
		expect(resourceDelete).not.toHaveBeenCalled();

		await user.click(screen.getByRole('button', { name: '卸载' }));

		expect(resourceDelete).toHaveBeenCalledWith(1);
	});

	it('取消卸载不应调用 resource_delete', async () => {
		const user = userEvent.setup();
		const resource = makeResource({ id: 1, name: 'data-visualizer' });
		vi.mocked(libraryList).mockResolvedValue([resource]);

		renderInstalled();
		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: 'data-visualizer 操作' }));
		await user.click(await screen.findByText('卸载'));
		expect(await screen.findByText(/确认卸载/)).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: '取消' }));

		expect(resourceDelete).not.toHaveBeenCalled();
	});

	it('同步到全部 Agent 只对在线 Agent 调用 sync_apply', async () => {
		const resource = makeResource({ id: 1, name: 'data-visualizer' });
		vi.mocked(libraryList).mockResolvedValue([resource]);
		const agents: AgentRespVO[] = [
			{
				id: 10,
				agentKind: 'ClaudeCode',
				name: 'Claude Code',
				configPath: '/home/demo/.claude.json',
				scope: 'Global',
				status: true,
				lastSyncTime: '',
				createTime: '2026-07-09 00:00:00',
				updateTime: '2026-07-09 00:00:00',
			},
			{
				id: 11,
				agentKind: 'Cursor',
				name: 'Cursor',
				configPath: '/home/demo/.cursor.json',
				scope: 'Global',
				status: false,
				lastSyncTime: '',
				createTime: '2026-07-09 00:00:00',
				updateTime: '2026-07-09 00:00:00',
			},
		];
		vi.mocked(agentList).mockResolvedValue(agents);
		const links: ResourceAgentLinkRespVO[] = [
			{ resourceId: 1, agentId: 10, agentName: 'Claude Code' },
		];
		vi.mocked(resourceAgentLinks).mockResolvedValue(links);

		renderInstalled();
		fireEvent.click(await screen.findByText('data-visualizer'));
		// 确保 agentList 已解析落到 agentsQuery.data, 否则 mutationFn 快照到的在线 Agent 会是空
		await waitFor(() => expect(agentList).toHaveBeenCalled());

		const syncButton = await screen.findByRole('button', { name: '同步到全部 Agent' });
		fireEvent.click(syncButton);

		await waitFor(() => expect(syncApply).toHaveBeenCalledWith([10]));
	});

	it('不应再渲染手动"刷新"按钮(F1 遗留已移除, 数据改由实时保鲜策略自动刷新)', async () => {
		vi.mocked(libraryList).mockResolvedValue([
			makeResource({ id: 1, name: 'data-visualizer' }),
		]);
		renderInstalled();
		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /^刷新$/ })).not.toBeInTheDocument();
	});
});
