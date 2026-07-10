// 文件作用: ResourceList 渲染与交互单测(行渲染/徽标/行选中/分段筛选/搜索/行内菜单/分页)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Resource } from '@/api/library';
import { ResourceList } from './resource-list';

/** displayName 默认跟随 name(与真实导入逻辑一致: 未显式改名时两者相同), 避免因固定默认值
 * 与 overrides.name 不一致而意外触发 deriveDescription 的"展示描述"分支 */
function makeResource(overrides: Partial<Resource> = {}): Resource {
	const name = overrides.name ?? 'demo-skill';
	return {
		id: 1,
		resType: 'Skill',
		name,
		displayName: name,
		version: '1.0.0',
		sourceType: 'LocalImport',
		localPath: '/tmp/demo-skill',
		enabled: true,
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

const baseProps = {
	linkCountByResource: new Map<number, number>(),
	selectedId: null,
	typeFilter: undefined,
	keyword: '',
	onTypeFilterChange: vi.fn(),
	onKeywordChange: vi.fn(),
	onSelectResource: vi.fn(),
	onToggleEnabled: vi.fn(),
	onRequestDelete: vi.fn(),
};

describe('ResourceList', () => {
	it('应渲染每行资源, 且类型/同步状态徽标可见', () => {
		const resources = [
			makeResource({ id: 1, name: 'demo-skill', resType: 'Skill', enabled: true }),
			makeResource({ id: 2, name: 'demo-mcp', resType: 'Mcp', enabled: false }),
		];
		render(<ResourceList {...baseProps} resources={resources} />);
		const table = within(screen.getByRole('table'));

		expect(table.getByText('demo-skill')).toBeInTheDocument();
		expect(table.getByText('demo-mcp')).toBeInTheDocument();
		expect(table.getByText('Skill')).toBeInTheDocument();
		expect(table.getByText('MCP')).toBeInTheDocument();
		expect(table.getByText('已同步')).toBeInTheDocument();
		expect(table.getByText('已禁用')).toBeInTheDocument();
	});

	it('应显示已关联 Agent 数', () => {
		const resources = [makeResource({ id: 1 })];
		render(
			<ResourceList
				{...baseProps}
				resources={resources}
				linkCountByResource={new Map([[1, 8]])}
			/>,
		);
		expect(screen.getByText('8')).toBeInTheDocument();
	});

	it('点击行(非操作区)应触发 onSelectResource 并回传该资源', () => {
		const onSelectResource = vi.fn();
		const resources = [makeResource({ id: 1, name: 'demo-skill' })];
		render(
			<ResourceList
				{...baseProps}
				resources={resources}
				onSelectResource={onSelectResource}
			/>,
		);

		fireEvent.click(screen.getByText('demo-skill'));
		expect(onSelectResource).toHaveBeenCalledWith(resources[0]);
	});

	it('点击分段 Skills 应触发 onTypeFilterChange("skill")', async () => {
		const user = userEvent.setup();
		const onTypeFilterChange = vi.fn();
		render(
			<ResourceList {...baseProps} resources={[]} onTypeFilterChange={onTypeFilterChange} />,
		);

		await user.click(screen.getByRole('tab', { name: 'Skills' }));
		expect(onTypeFilterChange).toHaveBeenCalledWith('skill');
	});

	it('搜索框输入应触发 onKeywordChange', () => {
		const onKeywordChange = vi.fn();
		render(<ResourceList {...baseProps} resources={[]} onKeywordChange={onKeywordChange} />);

		fireEvent.change(screen.getByPlaceholderText('搜索名称、描述或关键字'), {
			target: { value: 'demo' },
		});
		expect(onKeywordChange).toHaveBeenCalledWith('demo');
	});

	it('行内操作菜单点击"禁用"应触发 onToggleEnabled', async () => {
		const user = userEvent.setup();
		const onToggleEnabled = vi.fn();
		const resources = [makeResource({ id: 1, name: 'demo-skill', enabled: true })];
		render(
			<ResourceList {...baseProps} resources={resources} onToggleEnabled={onToggleEnabled} />,
		);

		await user.click(screen.getByRole('button', { name: 'demo-skill 操作' }));
		await user.click(await screen.findByText('禁用'));
		expect(onToggleEnabled).toHaveBeenCalledWith(resources[0]);
	});

	it('行内操作菜单点击"卸载"应触发 onRequestDelete', async () => {
		const user = userEvent.setup();
		const onRequestDelete = vi.fn();
		const resources = [makeResource({ id: 1, name: 'demo-skill' })];
		render(
			<ResourceList {...baseProps} resources={resources} onRequestDelete={onRequestDelete} />,
		);

		await user.click(screen.getByRole('button', { name: 'demo-skill 操作' }));
		await user.click(await screen.findByText('卸载'));
		expect(onRequestDelete).toHaveBeenCalledWith(resources[0]);
	});

	it('超过一页时应分页, 默认只显示第一页(10 条)且显示总数', () => {
		const resources = Array.from({ length: 12 }, (_, i) =>
			makeResource({ id: i + 1, name: `resource-${i + 1}` }),
		);
		render(<ResourceList {...baseProps} resources={resources} />);

		expect(screen.getByText('resource-1')).toBeInTheDocument();
		expect(screen.getByText('resource-10')).toBeInTheDocument();
		expect(screen.queryByText('resource-11')).not.toBeInTheDocument();
		expect(screen.getByText(/共 12 项/)).toBeInTheDocument();

		fireEvent.click(screen.getByRole('button', { name: '2' }));
		expect(screen.getByText('resource-11')).toBeInTheDocument();
		expect(screen.queryByText('resource-1')).not.toBeInTheDocument();
	});

	it('选中行应带有 data-state=selected', () => {
		const resources = [makeResource({ id: 1, name: 'demo-skill' })];
		render(<ResourceList {...baseProps} resources={resources} selectedId={1} />);
		const row = screen.getByText('demo-skill').closest('tr');
		expect(row).toHaveAttribute('data-state', 'selected');
	});

	it('筛选/排序/批量操作按钮应为禁用占位', () => {
		render(<ResourceList {...baseProps} resources={[]} />);
		expect(screen.getByRole('button', { name: /筛选/ })).toBeDisabled();
		expect(screen.getByRole('button', { name: /排序/ })).toBeDisabled();
		expect(screen.getByRole('button', { name: /批量操作/ })).toBeDisabled();
	});

	it('resources 为空时应展示空态文案, 不渲染表格', () => {
		render(<ResourceList {...baseProps} resources={[]} />);
		expect(screen.getByText('暂无匹配的资源')).toBeInTheDocument();
		expect(screen.queryByRole('table')).not.toBeInTheDocument();
	});
});
