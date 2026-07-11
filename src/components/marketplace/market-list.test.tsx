// 文件作用: MarketList 渲染与交互单测(搜索/分段/筛选 chips/分类/排序/卡片网格/分页)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MarketResource } from '@/api/market';
import { MarketList } from './market-list';

function makeMarketResource(overrides: Partial<MarketResource> = {}): MarketResource {
	const name = overrides.name ?? 'demo-skill';
	return {
		sourceType: 'GithubSkills',
		resType: 'Skill',
		extId: `acme/skills:${name}`,
		name,
		displayName: name,
		description: '示例描述',
		author: 'acme',
		version: '1.0.0',
		stars: 100,
		category: 'productivity',
		tags: [],
		authRequired: false,
		installManifest: { Skill: { repo: 'acme/skills', path: `skills/${name}`, gitRef: 'main' } },
		updatedAt: '2026-07-01 00:00:00',
		...overrides,
	};
}

const baseProps = {
	total: 2,
	categories: ['productivity', 'dev-tools'],
	resTypeFilter: 'skill' as const,
	keyword: '',
	chip: 'all' as const,
	category: undefined,
	sort: 0,
	page: 1,
	pageSize: 10,
	selectedKey: null,
	installErrors: {},
	onResTypeFilterChange: vi.fn(),
	onKeywordChange: vi.fn(),
	onChipChange: vi.fn(),
	onCategoryChange: vi.fn(),
	onSortChange: vi.fn(),
	onPageChange: vi.fn(),
	onPageSizeChange: vi.fn(),
	onSelectItem: vi.fn(),
	onDownload: vi.fn(),
};

describe('MarketList', () => {
	it('应渲染 items 对应的卡片, 且展示结果总数', () => {
		const items = [
			makeMarketResource({ name: 'data-visualizer' }),
			makeMarketResource({ name: 'web-scraper' }),
		];
		render(<MarketList {...baseProps} items={items} />);

		expect(screen.getByText('data-visualizer')).toBeInTheDocument();
		expect(screen.getByText('web-scraper')).toBeInTheDocument();
		expect(screen.getByText(/共 2 项结果/)).toBeInTheDocument();
	});

	it('chip=certified 时只展示 authRequired 为真的卡片', () => {
		const items = [
			makeMarketResource({ name: 'needs-auth', authRequired: true }),
			makeMarketResource({ name: 'no-auth', authRequired: false }),
		];
		render(<MarketList {...baseProps} items={items} chip="certified" />);

		expect(screen.getByText('needs-auth')).toBeInTheDocument();
		expect(screen.queryByText('no-auth')).not.toBeInTheDocument();
	});

	it('chip=free 时只展示 authRequired 为假的卡片', () => {
		const items = [
			makeMarketResource({ name: 'needs-auth', authRequired: true }),
			makeMarketResource({ name: 'no-auth', authRequired: false }),
		];
		render(<MarketList {...baseProps} items={items} chip="free" />);

		expect(screen.queryByText('needs-auth')).not.toBeInTheDocument();
		expect(screen.getByText('no-auth')).toBeInTheDocument();
	});

	it('点击 MCP 分段应触发 onResTypeFilterChange("mcp")', async () => {
		const user = userEvent.setup();
		const onResTypeFilterChange = vi.fn();
		render(
			<MarketList {...baseProps} items={[]} onResTypeFilterChange={onResTypeFilterChange} />,
		);

		await user.click(screen.getByRole('tab', { name: 'MCP' }));
		expect(onResTypeFilterChange).toHaveBeenCalledWith('mcp');
	});

	it('搜索框输入应触发 onKeywordChange', () => {
		const onKeywordChange = vi.fn();
		render(<MarketList {...baseProps} items={[]} onKeywordChange={onKeywordChange} />);

		fireEvent.change(screen.getByPlaceholderText('搜索 Skills 和 MCP...'), {
			target: { value: 'demo' },
		});
		expect(onKeywordChange).toHaveBeenCalledWith('demo');
	});

	it('点击筛选 chip 应触发 onChipChange', async () => {
		const user = userEvent.setup();
		const onChipChange = vi.fn();
		render(<MarketList {...baseProps} items={[]} onChipChange={onChipChange} />);

		await user.click(screen.getByRole('button', { name: '已认证' }));
		expect(onChipChange).toHaveBeenCalledWith('certified');

		await user.click(screen.getByRole('button', { name: '免费' }));
		expect(onChipChange).toHaveBeenCalledWith('free');

		await user.click(screen.getByRole('button', { name: '最近更新' }));
		expect(onChipChange).toHaveBeenCalledWith('updated');
	});

	it('点击分类下拉项应触发 onCategoryChange', async () => {
		const user = userEvent.setup();
		const onCategoryChange = vi.fn();
		render(<MarketList {...baseProps} items={[]} onCategoryChange={onCategoryChange} />);

		await user.click(screen.getByRole('button', { name: /分类/ }));
		await user.click(await screen.findByText('dev-tools'));
		expect(onCategoryChange).toHaveBeenCalledWith('dev-tools');
	});

	it('点击排序下拉项应触发 onSortChange', async () => {
		const user = userEvent.setup();
		const onSortChange = vi.fn();
		render(<MarketList {...baseProps} items={[]} onSortChange={onSortChange} />);

		await user.click(screen.getByRole('button', { name: /排序/ }));
		await user.click(await screen.findByText('星标数'));
		expect(onSortChange).toHaveBeenCalledWith(1);
	});

	it('点击卡片"查看详情"应触发 onSelectItem, 点击"下载"应触发 onDownload', async () => {
		const user = userEvent.setup();
		const onSelectItem = vi.fn();
		const onDownload = vi.fn();
		const item = makeMarketResource({ name: 'data-visualizer' });
		render(
			<MarketList
				{...baseProps}
				items={[item]}
				onSelectItem={onSelectItem}
				onDownload={onDownload}
			/>,
		);

		await user.click(screen.getAllByRole('button', { name: '下载' })[0]);
		expect(onDownload).toHaveBeenCalledWith(item);
		expect(onSelectItem).not.toHaveBeenCalled();

		await user.click(screen.getAllByRole('button', { name: '查看详情' })[0]);
		expect(onSelectItem).toHaveBeenCalledWith(item);
	});

	it('installErrors 中对应 key 的卡片应展示错误提示', () => {
		const item = makeMarketResource({ name: 'data-visualizer', sourceType: 'GithubSkills' });
		render(
			<MarketList
				{...baseProps}
				items={[item]}
				installErrors={{ 'GithubSkills:acme/skills:data-visualizer': '需在详情页登录' }}
			/>,
		);
		expect(screen.getByRole('alert')).toHaveTextContent('需在详情页登录');
	});

	it('分页: 应展示页码按钮, 点击触发 onPageChange, 首页禁用上一页', async () => {
		const user = userEvent.setup();
		const onPageChange = vi.fn();
		render(
			<MarketList
				{...baseProps}
				items={[]}
				total={45}
				pageSize={10}
				page={1}
				onPageChange={onPageChange}
			/>,
		);

		expect(screen.getByRole('button', { name: '上一页' })).toBeDisabled();
		await user.click(screen.getByRole('button', { name: '2' }));
		expect(onPageChange).toHaveBeenCalledWith(2);
	});

	it('分页: 末页应禁用下一页', () => {
		render(<MarketList {...baseProps} items={[]} total={20} pageSize={10} page={2} />);
		expect(screen.getByRole('button', { name: '下一页' })).toBeDisabled();
	});

	it('无匹配项时应展示占位文案', () => {
		render(<MarketList {...baseProps} items={[]} total={0} />);
		expect(screen.getByText('暂无匹配的资源')).toBeInTheDocument();
	});

	it('isLoading 为真时应展示骨架屏加载态, 不展示"暂无匹配的资源"空态', () => {
		render(<MarketList {...baseProps} items={[]} total={0} isLoading />);
		expect(screen.getByRole('status', { name: '加载中' })).toBeInTheDocument();
		expect(screen.queryByText('暂无匹配的资源')).not.toBeInTheDocument();
	});

	// 还原原型第 2 屏"稳定两列", 不再用 auto-fill+minmax 按可用宽度自动铺出很多窄列(实机宽屏
	// 反馈的具体症状); 仅在详情面板展开挤占大半宽度、逼近应用最小宽 1024 时才用容器查询优雅降级
	// 单列(见 market-list.tsx 顶部对应注释), jsdom 不跑真实布局/容器查询, 这里只锁定类名不回归
	it('卡片网格默认应为稳定两列(grid-cols-2), 并声明窄容器降级单列的类名(容器查询)', () => {
		const items = [
			makeMarketResource({ name: 'data-visualizer' }),
			makeMarketResource({ name: 'web-scraper' }),
		];
		render(<MarketList {...baseProps} items={items} />);

		const grid = screen.getByText('data-visualizer').closest('.grid-cols-2');
		expect(grid).not.toBeNull();
		expect(grid?.className).toMatch(/@max-\[500px\]:grid-cols-1/);
	});
});
