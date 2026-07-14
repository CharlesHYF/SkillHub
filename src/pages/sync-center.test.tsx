// 文件作用: Sync Center 页面集成测试(mock src/api) —— 统计卡聚合/Agent 表渲染/选中 Agent 打开
//           差异面板/一键同步触发 sync_apply 并随 onSyncProgress 事件更新进度/完成后失效刷新/
//           挂载时自动触发 agent_detect(M5 Task F1: 已移除手动"刷新"按钮)
// 创建日期: 2026-07-09
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { AgentRespVO } from '@/api/agent';
import type { DiffPlanRespVO, SyncProgress, SyncSummaryRespVO } from '@/api/sync';
import { useUiStore } from '@/stores/ui';
import SyncCenter from './sync-center';

vi.mock('@/api/agent', () => ({
	agentList: vi.fn(),
	agentDetect: vi.fn(),
}));
vi.mock('@/api/sync', () => ({
	syncDiff: vi.fn(),
	syncApply: vi.fn(),
	resourceAgentLinks: vi.fn().mockResolvedValue([]),
	onSyncProgress: vi.fn().mockResolvedValue(vi.fn()),
}));
vi.mock('@/api/library', () => ({
	libraryList: vi.fn().mockResolvedValue([]),
}));

import { agentDetect, agentList } from '@/api/agent';
import { onSyncProgress, resourceAgentLinks, syncApply, syncDiff } from '@/api/sync';
import { libraryList } from '@/api/library';

function makeAgent(overrides: Partial<AgentRespVO> = {}): AgentRespVO {
	return {
		id: 1,
		agentKind: 'ClaudeCode',
		name: 'Claude Code',
		configPath: '/home/demo/.claude.json',
		scope: 'Global',
		status: true,
		lastSyncTime: '',
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

const emptyPlan: DiffPlanRespVO = { items: [] };

function renderSyncCenter() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<SyncCenter />
		</QueryClientProvider>,
	);
}

describe('SyncCenter 页面', () => {
	beforeEach(() => {
		useUiStore.getState().reset();
		vi.mocked(agentList).mockReset();
		vi.mocked(agentDetect).mockReset();
		vi.mocked(syncDiff).mockReset().mockResolvedValue(emptyPlan);
		vi.mocked(syncApply).mockReset().mockResolvedValue({ success: 0, failed: 0, skipped: 0 });
		vi.mocked(resourceAgentLinks).mockReset().mockResolvedValue([]);
		vi.mocked(libraryList).mockReset().mockResolvedValue([]);
		vi.mocked(onSyncProgress).mockReset().mockResolvedValue(vi.fn());
	});

	it('应渲染 agent_list 聚合而来的统计卡数值与 Agent 表行', async () => {
		const agents: AgentRespVO[] = [
			makeAgent({ id: 1, name: 'Claude Code', status: true }),
			makeAgent({ id: 2, name: 'Cursor', status: false }),
		];
		vi.mocked(agentList).mockResolvedValue(agents);
		vi.mocked(syncDiff).mockImplementation((agentId: number) =>
			Promise.resolve(
				agentId === 1
					? {
							items: [
								{
									resType: 'Skill',
									name: 'data-visualizer',
									action: 'Add',
									localVer: '1.2.0',
									agentVer: '',
									payload: null,
								},
								{
									resType: 'Mcp',
									name: 'filesystem',
									action: 'Update',
									localVer: '1.0.0',
									agentVer: '0.9.1',
									payload: null,
								},
							],
						}
					: emptyPlan,
			),
		);

		renderSyncCenter();

		expect(await screen.findByText('Claude Code')).toBeInTheDocument();
		expect(screen.getByText('Cursor')).toBeInTheDocument();

		// 已连接 Agent = 2, 在线 = 1(卡片值与"名称"标签同处一个卡片容器内, 用 closest 限定范围
		// 避免与表格里其它同为"2"的单元格误撞)
		const connectedCard = screen.getByText('已连接 Agent').closest('div');
		expect(connectedCard).not.toBeNull();
		expect(within(connectedCard as HTMLElement).getByText('2')).toBeInTheDocument();
		expect(screen.getByText('在线 1')).toBeInTheDocument();
		expect(screen.getByText('离线 1')).toBeInTheDocument();

		// 待同步项应为两条 diff 汇总: agent 1 的 2 条 + agent 2 的 0 条 = 2
		await waitFor(() => expect(screen.getByText('需同步 1 个 Agent')).toBeInTheDocument());

		// 最近同步结果初始应为"暂无记录"(本次会话尚未执行过 sync_apply)
		expect(screen.getByText('暂无记录')).toBeInTheDocument();
	});

	it('点击某 Agent 行应据其 sync_diff 结果打开差异面板并显示条目', async () => {
		const user = userEvent.setup();
		const agents: AgentRespVO[] = [makeAgent({ id: 1, name: 'Claude Code' })];
		vi.mocked(agentList).mockResolvedValue(agents);
		vi.mocked(syncDiff).mockResolvedValue({
			items: [
				{
					resType: 'Skill',
					name: 'data-visualizer',
					action: 'Add',
					localVer: '1.2.0',
					agentVer: '',
					payload: null,
				},
			],
		});

		renderSyncCenter();
		await user.click(await screen.findByText('Claude Code'));

		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();
		expect(screen.getByRole('tab', { name: '全部(1)' })).toBeInTheDocument();
	});

	it('点击"一键同步到所有 Agent"应调用 sync_apply(在线本地 Agent id), 并随 onSyncProgress 更新进度提示', async () => {
		const user = userEvent.setup();
		const agents: AgentRespVO[] = [
			makeAgent({ id: 1, name: 'Claude Code', status: true }),
			makeAgent({ id: 2, name: 'Cursor', status: false }),
		];
		vi.mocked(agentList).mockResolvedValue(agents);
		let resolveApply!: (value: SyncSummaryRespVO) => void;
		vi.mocked(syncApply).mockReturnValue(
			new Promise<SyncSummaryRespVO>((resolve) => {
				resolveApply = resolve;
			}),
		);

		renderSyncCenter();
		await screen.findByText('Claude Code');

		await user.click(screen.getByRole('button', { name: '一键同步到所有 Agent' }));
		expect(syncApply).toHaveBeenCalledWith([1]);

		// 模拟后端推送一次同步进度事件, 应驱动进度提示文案更新
		const progressHandler = vi.mocked(onSyncProgress).mock.calls[0][0] as (
			p: SyncProgress,
		) => void;
		progressHandler({ agentId: 1, done: 0, total: 1, currentName: 'Claude Code' });
		expect(await screen.findByText(/Claude Code/)).toBeInTheDocument();

		resolveApply({ success: 1, failed: 0, skipped: 0 });
		await waitFor(() => expect(screen.getByText('全部同步')).toBeInTheDocument());
		// 完成后应失效刷新, agent_list 应被再次调用
		await waitFor(() => expect(agentList).toHaveBeenCalledTimes(2));
	});

	it('挂载时应自动调用 agent_detect(不再需要手动点刷新), 其结果落地为 Agent 表数据', async () => {
		vi.mocked(agentList).mockResolvedValue([]);
		vi.mocked(agentDetect).mockResolvedValue([makeAgent({ id: 1, name: 'Claude Code' })]);

		renderSyncCenter();

		await waitFor(() => expect(agentDetect).toHaveBeenCalledTimes(1));
		expect(await screen.findByText('Claude Code')).toBeInTheDocument();
	});

	it('不应再渲染手动"刷新"按钮', async () => {
		vi.mocked(agentList).mockResolvedValue([]);
		renderSyncCenter();
		await waitFor(() => expect(agentDetect).toHaveBeenCalled());
		expect(screen.queryByRole('button', { name: /刷新/ })).not.toBeInTheDocument();
	});
});
