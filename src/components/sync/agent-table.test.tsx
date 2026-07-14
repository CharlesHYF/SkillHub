// 文件作用: AgentTable 渲染与交互单测(行渲染/工具条批量同步/行内菜单/选中态/同步中禁用)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { AgentRespVO } from '@/api/agent';
import type { SyncSummaryRespVO } from '@/api/sync';
import { AgentTable } from './agent-table';

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

const baseProps = {
	pendingCountByAgentId: new Map<number, number | undefined>(),
	installedCountByAgentId: new Map<number, { skill: number; mcp: number }>(),
	lastOutcomeByAgentId: new Map<number, SyncSummaryRespVO>(),
	selectedId: null,
	onSelectAgent: vi.fn(),
	onSyncAgentIds: vi.fn(),
};

describe('AgentTable', () => {
	it('应渲染每行 Agent 的名称/类型/安装位置/在线状态/Skill 与 MCP 数, 空值兜底为"—"', () => {
		const agents = [
			makeAgent({
				id: 1,
				name: 'Claude Code',
				status: true,
				configPath: '/home/demo/.claude.json',
			}),
			makeAgent({
				id: 2,
				name: 'Cursor',
				status: false,
				configPath: '/home/demo/.cursor/mcp.json',
			}),
		];
		render(
			<AgentTable
				{...baseProps}
				agents={agents}
				installedCountByAgentId={new Map([[1, { skill: 3, mcp: 2 }]])}
				pendingCountByAgentId={new Map([[1, 5]])}
			/>,
		);
		const table = within(screen.getByRole('table'));

		expect(table.getByText('Claude Code')).toBeInTheDocument();
		expect(table.getByText('Cursor')).toBeInTheDocument();
		expect(table.getAllByText('本地')).toHaveLength(2);
		expect(table.getByText('/home/demo/.claude.json')).toBeInTheDocument();
		expect(table.getByText('在线')).toBeInTheDocument();
		expect(table.getByText('离线')).toBeInTheDocument();
		expect(table.getByText('3')).toBeInTheDocument();
		expect(table.getByText('2')).toBeInTheDocument();
		expect(table.getByText('5')).toBeInTheDocument();
		// agent 2 未提供 installedCount/pendingCount/lastSyncTime, 均应兜底展示"—"
		const dashCells = table.getAllByText('—');
		expect(dashCells.length).toBeGreaterThan(0);
	});

	it('点击行(非勾选框)应触发 onSelectAgent 并回传该 Agent', () => {
		const onSelectAgent = vi.fn();
		const agents = [makeAgent({ id: 1, name: 'Claude Code' })];
		render(<AgentTable {...baseProps} agents={agents} onSelectAgent={onSelectAgent} />);

		fireEvent.click(screen.getByText('Claude Code'));
		expect(onSelectAgent).toHaveBeenCalledWith(agents[0]);
	});

	it('选中行应带有 data-state=selected', () => {
		const agents = [makeAgent({ id: 1, name: 'Claude Code' })];
		render(<AgentTable {...baseProps} agents={agents} selectedId={1} />);
		expect(screen.getByText('Claude Code').closest('tr')).toHaveAttribute(
			'data-state',
			'selected',
		);
	});

	it('勾选框: 勾选一行后点击"选择同步"应以该 id 调用 onSyncAgentIds, 且不触发 onSelectAgent', () => {
		const onSyncAgentIds = vi.fn();
		const onSelectAgent = vi.fn();
		const agents = [
			makeAgent({ id: 1, name: 'Claude Code' }),
			makeAgent({ id: 2, name: 'Cursor' }),
		];
		render(
			<AgentTable
				{...baseProps}
				agents={agents}
				onSyncAgentIds={onSyncAgentIds}
				onSelectAgent={onSelectAgent}
			/>,
		);

		fireEvent.click(screen.getByRole('checkbox', { name: '选中 Claude Code' }));
		expect(onSelectAgent).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole('button', { name: '选择同步' }));
		expect(onSyncAgentIds).toHaveBeenCalledWith([1]);
	});

	it('"同步全部"应以全部在线本地 Agent id 调用 onSyncAgentIds(离线 Agent 排除在外)', () => {
		const onSyncAgentIds = vi.fn();
		const agents = [
			makeAgent({ id: 1, name: 'Claude Code', status: true }),
			makeAgent({ id: 2, name: 'Cursor', status: false }),
		];
		render(<AgentTable {...baseProps} agents={agents} onSyncAgentIds={onSyncAgentIds} />);

		fireEvent.click(screen.getByRole('button', { name: '同步全部' }));
		expect(onSyncAgentIds).toHaveBeenCalledWith([1]);
	});

	it('"一键同步到所有 Agent"应与"同步全部"效果相同', () => {
		const onSyncAgentIds = vi.fn();
		const agents = [makeAgent({ id: 1, name: 'Claude Code', status: true })];
		render(<AgentTable {...baseProps} agents={agents} onSyncAgentIds={onSyncAgentIds} />);

		fireEvent.click(screen.getByRole('button', { name: '一键同步到所有 Agent' }));
		expect(onSyncAgentIds).toHaveBeenCalledWith([1]);
	});

	it('"查看差异"默认禁用, 恰好勾选一行后启用并触发 onSelectAgent', () => {
		const onSelectAgent = vi.fn();
		const agents = [
			makeAgent({ id: 1, name: 'Claude Code' }),
			makeAgent({ id: 2, name: 'Cursor' }),
		];
		render(<AgentTable {...baseProps} agents={agents} onSelectAgent={onSelectAgent} />);

		expect(screen.getByRole('button', { name: '查看差异' })).toBeDisabled();

		fireEvent.click(screen.getByRole('checkbox', { name: '选中 Claude Code' }));
		expect(screen.getByRole('button', { name: '查看差异' })).toBeEnabled();

		fireEvent.click(screen.getByRole('button', { name: '查看差异' }));
		expect(onSelectAgent).toHaveBeenCalledWith(agents[0]);
	});

	it('"重试失败"在无失败记录时禁用, 有失败记录时启用并只对失败的 Agent 调用 onSyncAgentIds', () => {
		const onSyncAgentIds = vi.fn();
		const agents = [
			makeAgent({ id: 1, name: 'Claude Code', status: true }),
			makeAgent({ id: 2, name: 'Cursor', status: true }),
		];
		const { rerender } = render(<AgentTable {...baseProps} agents={agents} />);
		expect(screen.getByRole('button', { name: '重试失败' })).toBeDisabled();

		rerender(
			<AgentTable
				{...baseProps}
				agents={agents}
				onSyncAgentIds={onSyncAgentIds}
				lastOutcomeByAgentId={new Map([[2, { success: 0, failed: 1, skipped: 0 }]])}
			/>,
		);
		expect(screen.getByRole('button', { name: '重试失败' })).toBeEnabled();
		fireEvent.click(screen.getByRole('button', { name: '重试失败' }));
		expect(onSyncAgentIds).toHaveBeenCalledWith([2]);
	});

	it('行内菜单: 离线 Agent 的"立即同步"应为禁用态, 在线 Agent 的应可点击并调用 onSyncAgentIds', async () => {
		const user = userEvent.setup();
		const onSyncAgentIds = vi.fn();
		const agents = [
			makeAgent({ id: 1, name: 'Claude Code', status: true }),
			makeAgent({ id: 2, name: 'Cursor', status: false }),
		];
		render(<AgentTable {...baseProps} agents={agents} onSyncAgentIds={onSyncAgentIds} />);

		await user.click(screen.getByRole('button', { name: 'Cursor 操作' }));
		expect(await screen.findByText('立即同步')).toHaveAttribute('data-disabled');
		await user.keyboard('{Escape}');

		await user.click(screen.getByRole('button', { name: 'Claude Code 操作' }));
		await user.click(await screen.findByText('立即同步'));
		expect(onSyncAgentIds).toHaveBeenCalledWith([1]);
	});

	it('isSyncing=true 时批量同步按钮均应禁用', () => {
		const agents = [makeAgent({ id: 1, name: 'Claude Code', status: true })];
		render(<AgentTable {...baseProps} agents={agents} isSyncing />);

		expect(screen.getByRole('button', { name: '同步全部' })).toBeDisabled();
		expect(screen.getByRole('button', { name: '一键同步到所有 Agent' })).toBeDisabled();
	});

	it('无可同步 Agent(空列表或全离线)时"同步全部"/"一键同步到所有 Agent"应禁用', () => {
		render(<AgentTable {...baseProps} agents={[]} />);
		expect(screen.getByRole('button', { name: '同步全部' })).toBeDisabled();
		expect(screen.getByRole('button', { name: '一键同步到所有 Agent' })).toBeDisabled();
	});

	it('agents 为空时应展示空态文案, 不渲染表格', () => {
		render(<AgentTable {...baseProps} agents={[]} />);
		expect(screen.getByText('暂无已连接 Agent')).toBeInTheDocument();
		expect(screen.queryByRole('table')).not.toBeInTheDocument();
	});
});
