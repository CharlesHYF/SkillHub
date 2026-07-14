// 文件作用: DiffDetailPanel 渲染与交互单测(空态/条目渲染/Tab 过滤计数)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { DiffItem, DiffPlanRespVO } from '@/api/sync';
import { DiffDetailPanel } from './diff-detail-panel';

function makeItem(overrides: Partial<DiffItem> = {}): DiffItem {
	return {
		resType: 'Skill',
		name: 'demo-skill',
		action: 'Add',
		localVer: '1.0.0',
		agentVer: '',
		payload: null,
		...overrides,
	};
}

describe('DiffDetailPanel', () => {
	it('未选中 Agent(diffPlan=undefined)应展示引导空态', () => {
		render(<DiffDetailPanel diffPlan={undefined} />);
		expect(screen.getByText(/选择.*Agent/)).toBeInTheDocument();
	});

	it('diffPlan.items 为空应展示"已一致"空态', () => {
		render(<DiffDetailPanel diffPlan={{ items: [] }} />);
		expect(screen.getByText(/已与本地库保持一致|无需同步/)).toBeInTheDocument();
	});

	it('应渲染每条差异(类型徽标/名称/本地版本/Agent 版本), 版本缺失兜底为"—"', () => {
		const plan: DiffPlanRespVO = {
			items: [
				makeItem({
					resType: 'Skill',
					name: 'data-visualizer',
					action: 'Add',
					localVer: '1.2.0',
					agentVer: '',
				}),
				makeItem({
					resType: 'Mcp',
					name: 'filesystem',
					action: 'Update',
					localVer: '1.0.0',
					agentVer: '0.9.1',
				}),
			],
		};
		render(<DiffDetailPanel diffPlan={plan} />);
		const table = within(screen.getByRole('table'));

		expect(table.getByText('data-visualizer')).toBeInTheDocument();
		expect(table.getByText('filesystem')).toBeInTheDocument();
		expect(table.getByText('Skill')).toBeInTheDocument();
		expect(table.getByText('MCP')).toBeInTheDocument();
		expect(table.getByText('1.2.0')).toBeInTheDocument();
		expect(table.getByText('0.9.1')).toBeInTheDocument();
		// data-visualizer 无 agentVer、filesystem 无 localVer 缺口, 均应兜底展示"—"
		expect(table.getAllByText('—').length).toBeGreaterThanOrEqual(1);
	});

	it('Tab 计数应正确, 切换 Tab 应按 action 过滤条目', async () => {
		const user = userEvent.setup();
		const plan: DiffPlanRespVO = {
			items: [
				makeItem({ name: 'add-1', action: 'Add' }),
				makeItem({ name: 'add-2', action: 'Add' }),
				makeItem({ name: 'update-1', action: 'Update' }),
				makeItem({ name: 'remove-1', action: 'Remove' }),
			],
		};
		render(<DiffDetailPanel diffPlan={plan} />);

		expect(screen.getByRole('tab', { name: '全部(4)' })).toBeInTheDocument();
		expect(screen.getByRole('tab', { name: '新增(2)' })).toBeInTheDocument();
		expect(screen.getByRole('tab', { name: '更新(1)' })).toBeInTheDocument();
		expect(screen.getByRole('tab', { name: '移除(1)' })).toBeInTheDocument();

		await user.click(screen.getByRole('tab', { name: '更新(1)' }));
		expect(screen.getByText('update-1')).toBeInTheDocument();
		expect(screen.queryByText('add-1')).not.toBeInTheDocument();
		expect(screen.queryByText('remove-1')).not.toBeInTheDocument();
	});
});
