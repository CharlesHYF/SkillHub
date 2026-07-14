// 文件作用: ResourceDetailPanel 渲染与交互单测(字段展示/关联 Agent 列表/底部动作回调)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { ResourceRespVO } from '@/api/library';
import { ResourceDetailPanel } from './resource-detail-panel';

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

const baseProps = {
	linkedAgentNames: [] as string[],
	onClose: vi.fn(),
	onSyncToAllAgents: vi.fn(),
	onRequestDelete: vi.fn(),
};

describe('ResourceDetailPanel', () => {
	it('应显示资源名称/类型徽标/来源/版本/本地路径', () => {
		const resource = makeResource();
		render(<ResourceDetailPanel {...baseProps} resource={resource} />);

		expect(screen.getByText('data-visualizer')).toBeInTheDocument();
		expect(screen.getByText('Skill')).toBeInTheDocument();
		expect(screen.getByText('本地导入')).toBeInTheDocument();
		expect(screen.getByText(/1\.2\.0/)).toBeInTheDocument();
		expect(screen.getByText('/tmp/data-visualizer')).toBeInTheDocument();
	});

	it('displayName 与 name 不同时应展示描述, 相同时展示"暂无描述"', () => {
		const { rerender } = render(
			<ResourceDetailPanel
				{...baseProps}
				resource={makeResource({ displayName: '数据可视化工具集合' })}
			/>,
		);
		expect(screen.getByText('数据可视化工具集合')).toBeInTheDocument();

		rerender(<ResourceDetailPanel {...baseProps} resource={makeResource()} />);
		expect(screen.getByText('暂无描述')).toBeInTheDocument();
	});

	it('已关联 Agent 列表应展示各 Agent 名, 为空时展示占位文案', () => {
		const { rerender } = render(
			<ResourceDetailPanel
				{...baseProps}
				resource={makeResource()}
				linkedAgentNames={['Agent Alpha', 'Agent Beta']}
			/>,
		);
		expect(screen.getByText('Agent Alpha')).toBeInTheDocument();
		expect(screen.getByText('Agent Beta')).toBeInTheDocument();

		rerender(
			<ResourceDetailPanel {...baseProps} resource={makeResource()} linkedAgentNames={[]} />,
		);
		expect(screen.getByText('暂无关联')).toBeInTheDocument();
	});

	it('点击关闭按钮应触发 onClose', () => {
		const onClose = vi.fn();
		render(<ResourceDetailPanel {...baseProps} resource={makeResource()} onClose={onClose} />);
		fireEvent.click(screen.getByRole('button', { name: '关闭' }));
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it('点击"同步到全部 Agent"应触发 onSyncToAllAgents', () => {
		const onSyncToAllAgents = vi.fn();
		render(
			<ResourceDetailPanel
				{...baseProps}
				resource={makeResource()}
				onSyncToAllAgents={onSyncToAllAgents}
			/>,
		);
		fireEvent.click(screen.getByRole('button', { name: '同步到全部 Agent' }));
		expect(onSyncToAllAgents).toHaveBeenCalledTimes(1);
	});

	it('点击"卸载"应触发 onRequestDelete 并回传该资源', () => {
		const onRequestDelete = vi.fn();
		const resource = makeResource();
		render(
			<ResourceDetailPanel
				{...baseProps}
				resource={resource}
				onRequestDelete={onRequestDelete}
			/>,
		);
		fireEvent.click(screen.getByRole('button', { name: '卸载' }));
		expect(onRequestDelete).toHaveBeenCalledWith(resource);
	});

	it('"仅导出此项"与"查看详情"应为禁用占位(M1 不强求)', () => {
		render(<ResourceDetailPanel {...baseProps} resource={makeResource()} />);
		expect(screen.getByRole('button', { name: '仅导出此项' })).toBeDisabled();
		expect(screen.getByRole('button', { name: '查看详情' })).toBeDisabled();
	});

	it('isSyncing 为真时"同步到全部 Agent"按钮应禁用且文案变化', () => {
		render(<ResourceDetailPanel {...baseProps} resource={makeResource()} isSyncing />);
		const button = screen.getByRole('button', { name: '同步中...' });
		expect(button).toBeDisabled();
	});
});
