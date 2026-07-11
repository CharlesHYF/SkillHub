// 文件作用: SyncOverviewCard 渲染单测(未选中空态/选中后概览计数与上次同步结果展示/配置路径可复制)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { AgentRow } from '@/api/agent';
import { SyncOverviewCard } from './sync-overview-card';

function makeAgent(overrides: Partial<AgentRow> = {}): AgentRow {
	return {
		id: 1,
		agentKind: 'ClaudeCode',
		name: 'Agent Beta',
		configPath: '/home/demo/.claude.json',
		scope: 'Global',
		status: true,
		lastSyncTime: '',
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

describe('SyncOverviewCard', () => {
	it('未选中 Agent 时应展示引导空态', () => {
		render(
			<SyncOverviewCard
				agent={null}
				diffCounts={{ add: 0, update: 0, remove: 0, total: 0 }}
			/>,
		);
		expect(screen.getByText(/选择.*Agent/)).toBeInTheDocument();
	});

	it('选中 Agent 后应展示其名称/在线状态与新增/更新/移除/待同步总计', () => {
		render(
			<SyncOverviewCard
				agent={makeAgent({ name: 'Agent Beta', status: true })}
				diffCounts={{ add: 2, update: 3, remove: 0, total: 5 }}
			/>,
		);
		expect(screen.getByText('Agent Beta')).toBeInTheDocument();
		expect(screen.getByText('在线')).toBeInTheDocument();
		expect(screen.getByText('2')).toBeInTheDocument();
		expect(screen.getByText('3')).toBeInTheDocument();
		expect(screen.getByText('5')).toBeInTheDocument();
	});

	it('无本次会话同步记录时, 上次结果/上次详情应展示"暂无记录"兜底', () => {
		render(
			<SyncOverviewCard
				agent={makeAgent()}
				diffCounts={{ add: 0, update: 0, remove: 0, total: 0 }}
			/>,
		);
		expect(screen.getByText('暂无记录')).toBeInTheDocument();
	});

	it('有本次会话同步记录时应展示对应结果文案与成功/失败/跳过详情', () => {
		render(
			<SyncOverviewCard
				agent={makeAgent()}
				diffCounts={{ add: 0, update: 0, remove: 0, total: 0 }}
				lastOutcome={{ success: 3, failed: 2, skipped: 0 }}
			/>,
		);
		// "部分同步"会同时出现在标题的在线状态徽标与"上次结果"两处, 二者独立计算但恰好同值,
		// 属预期的展示重叠(非 bug), 断言至少各出现一次即可
		expect(screen.getAllByText('部分同步').length).toBeGreaterThanOrEqual(2);
		expect(screen.getByText('3 成功 / 2 失败 / 0 跳过')).toBeInTheDocument();
	});

	it('应展示 Agent 配置路径, 点击"复制配置路径"应把该路径写入剪贴板', () => {
		const writeText = vi.fn().mockResolvedValue(undefined);
		vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });

		render(
			<SyncOverviewCard
				agent={makeAgent({ configPath: '/home/demo/.claude.json' })}
				diffCounts={{ add: 0, update: 0, remove: 0, total: 0 }}
			/>,
		);

		expect(screen.getByText('/home/demo/.claude.json')).toBeInTheDocument();
		fireEvent.click(screen.getByRole('button', { name: '复制配置路径' }));
		expect(writeText).toHaveBeenCalledWith('/home/demo/.claude.json');

		vi.unstubAllGlobals();
	});
});
