// 文件作用: Marketplace 页面集成测试(mock src/api/market) —— 卡片渲染/分段与筛选/排序/分类/
//           分页交互触发 market_search 参数变化/点击卡片经 market_detail 填充详情面板/下载按钮
//           触发 market_install(鉴权失败时的占位提示)/挂载时市场缓存为空则自动触发 market_refresh
//           (M5 Task F1: 已移除手动"刷新"按钮)/挂载后默认选中首个结果且详情面板常驻、无结果时
//           详情面板展示空态(M5 Task F2)。MarketDetailPanel 内新增的"查看详情"链接依赖 Router
//           语境, 故 renderMarketplace 经 MemoryRouter 包裹
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import type { MarketResource } from '@/api/market';
import { useUiStore } from '@/stores/ui';
import Marketplace from './marketplace';

vi.mock('@/api/market', () => ({
	marketSearch: vi.fn(),
	marketDetail: vi.fn(),
	marketRefresh: vi.fn().mockResolvedValue({ count: 0 }),
	marketInstall: vi.fn(),
}));

import { marketSearch, marketDetail, marketRefresh, marketInstall } from '@/api/market';

function makeMarketResource(overrides: Partial<MarketResource> = {}): MarketResource {
	const name = overrides.name ?? 'data-visualizer';
	return {
		sourceType: 'GithubSkills',
		resType: 'Skill',
		extId: `acme/skills:${name}`,
		name,
		displayName: name,
		description: '示例描述',
		author: 'SkillHub Official',
		version: '1.2.0',
		stars: 12300,
		category: 'productivity',
		tags: ['数据可视化'],
		authRequired: false,
		installManifest: { Skill: { repo: 'acme/skills', path: `skills/${name}`, gitRef: 'main' } },
		updatedAt: '2025-05-20 00:00:00',
		...overrides,
	};
}

function renderMarketplace() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<MemoryRouter>
				<Marketplace />
			</MemoryRouter>
		</QueryClientProvider>,
	);
}

describe('Marketplace 页面', () => {
	beforeEach(() => {
		useUiStore.getState().reset();
		// 默认 total=1(非空), 避免与本文件无关的用例意外触发"缓存为空自动刷新"逻辑(该逻辑本身
		// 由下方专门的用例覆盖, 见"挂载时市场缓存为空..."与"挂载时市场缓存非空...")
		vi.mocked(marketSearch).mockReset().mockResolvedValue({ items: [], total: 1 });
		// 默认兜底 resolve null(非 undefined): M5 起挂载后默认选中首个结果, 会让更多用例真的
		// 触发 detailQuery; TanStack Query 的 queryFn 不允许 resolve 出 undefined(会被内部当
		// 作错误处理), 未显式 mockResolvedValue 的用例若保持"裸 reset 无返回值"会在控制台报错。
		// 各用例仍可自行 mockResolvedValue(resource) 覆盖这个默认值
		vi.mocked(marketDetail).mockReset().mockResolvedValue(null);
		vi.mocked(marketRefresh).mockReset().mockResolvedValue({ count: 0 });
		vi.mocked(marketInstall).mockReset();
	});

	it('应渲染 market_search 返回的卡片, 默认按 Skills(resType=1)查询', async () => {
		vi.mocked(marketSearch).mockResolvedValue({
			items: [makeMarketResource({ name: 'data-visualizer' })],
			total: 1,
		});

		renderMarketplace();

		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();
		expect(marketSearch).toHaveBeenLastCalledWith(expect.objectContaining({ resType: 1 }));
	});

	it('点击 MCP 分段应以 resType=2 重新查询', async () => {
		const user = userEvent.setup();
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });
		renderMarketplace();
		await waitFor(() => expect(marketSearch).toHaveBeenCalled());

		await user.click(screen.getByRole('tab', { name: 'MCP' }));

		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(expect.objectContaining({ resType: 2 })),
		);
	});

	it('搜索框输入应触发带 keyword 的查询', async () => {
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });
		renderMarketplace();
		await waitFor(() => expect(marketSearch).toHaveBeenCalled());

		fireEvent.change(screen.getByPlaceholderText('搜索 Skills 和 MCP...'), {
			target: { value: 'demo' },
		});

		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(
				expect.objectContaining({ keyword: 'demo' }),
			),
		);
	});

	it('已认证/免费筛选 chip 应在当页数据上客户端过滤', async () => {
		const user = userEvent.setup();
		vi.mocked(marketSearch).mockResolvedValue({
			items: [
				makeMarketResource({ name: 'needs-auth', authRequired: true }),
				makeMarketResource({ name: 'no-auth', authRequired: false }),
			],
			total: 2,
		});
		renderMarketplace();
		expect(await screen.findByText('needs-auth')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: '已认证' }));
		expect(screen.queryByText('no-auth')).not.toBeInTheDocument();
		expect(screen.getByText('needs-auth')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: '免费' }));
		expect(screen.queryByText('needs-auth')).not.toBeInTheDocument();
		expect(screen.getByText('no-auth')).toBeInTheDocument();
	});

	it('推荐/最近更新 chip 应分别以 sort=0/sort=2 重新查询', async () => {
		const user = userEvent.setup();
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });
		renderMarketplace();
		await waitFor(() => expect(marketSearch).toHaveBeenCalled());

		await user.click(screen.getByRole('button', { name: '最近更新' }));
		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(expect.objectContaining({ sort: 2 })),
		);

		await user.click(screen.getByRole('button', { name: '推荐' }));
		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(expect.objectContaining({ sort: 0 })),
		);
	});

	it('排序下拉选择"星标数"应以 sort=1 重新查询', async () => {
		const user = userEvent.setup();
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });
		renderMarketplace();
		await waitFor(() => expect(marketSearch).toHaveBeenCalled());

		await user.click(screen.getByRole('button', { name: /排序/ }));
		await user.click(await screen.findByText('星标数'));

		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(expect.objectContaining({ sort: 1 })),
		);
	});

	it('分类下拉选择应以对应 category 重新查询', async () => {
		const user = userEvent.setup();
		vi.mocked(marketSearch).mockResolvedValue({
			items: [makeMarketResource({ category: 'dev-tools' })],
			total: 1,
		});
		renderMarketplace();
		expect(await screen.findByText('data-visualizer')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: /分类/ }));
		await user.click(await screen.findByText('dev-tools'));

		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(
				expect.objectContaining({ category: 'dev-tools' }),
			),
		);
	});

	it('分页点击第 2 页应以 page=2 重新查询', async () => {
		const user = userEvent.setup();
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 45 });
		renderMarketplace();
		// 等页码按钮渲染出来(依赖 total 落到 searchQuery.data 之后才会有 5 页), 而非仅仅等
		// marketSearch 被调用(调用发生在 promise resolve 之前, 过早的话此时分页还按 total=0 计算)
		await user.click(await screen.findByRole('button', { name: '2' }));

		await waitFor(() =>
			expect(marketSearch).toHaveBeenLastCalledWith(expect.objectContaining({ page: 2 })),
		);
	});

	it('点击卡片应经 market_detail 查询并在详情面板中展示该资源', async () => {
		const user = userEvent.setup();
		const resource = makeMarketResource({ name: 'data-visualizer' });
		vi.mocked(marketSearch).mockResolvedValue({ items: [resource], total: 1 });
		vi.mocked(marketDetail).mockResolvedValue(resource);

		renderMarketplace();
		await user.click(await screen.findByText('data-visualizer'));

		await waitFor(() =>
			expect(marketDetail).toHaveBeenCalledWith(1, 'acme/skills:data-visualizer'),
		);
		expect(await screen.findByText('简介')).toBeInTheDocument();
	});

	it('点击下载应调用 market_install(sourceType 数值编码, extId)', async () => {
		const user = userEvent.setup();
		const resource = makeMarketResource({ name: 'data-visualizer' });
		vi.mocked(marketSearch).mockResolvedValue({ items: [resource], total: 1 });
		vi.mocked(marketInstall).mockResolvedValue({
			id: 1,
			resType: 'Skill',
			name: 'data-visualizer',
			displayName: 'data-visualizer',
			version: '1.2.0',
			sourceType: 'Official',
			localPath: '/tmp/data-visualizer',
			enabled: true,
			createTime: '2026-07-10 00:00:00',
			updateTime: '2026-07-10 00:00:00',
		});

		renderMarketplace();
		await screen.findByText('data-visualizer');
		await user.click(screen.getByRole('button', { name: '下载' }));

		await waitFor(() =>
			expect(marketInstall).toHaveBeenCalledWith(1, 'acme/skills:data-visualizer', undefined),
		);
	});

	it('market_install 失败(如需鉴权)应在详情面板展示占位提示"需在详情页登录"', async () => {
		const user = userEvent.setup();
		const resource = makeMarketResource({ name: 'data-visualizer', authRequired: true });
		vi.mocked(marketSearch).mockResolvedValue({ items: [resource], total: 1 });
		vi.mocked(marketDetail).mockResolvedValue(resource);
		vi.mocked(marketInstall).mockRejectedValue(new Error('需要鉴权'));

		renderMarketplace();
		await user.click(await screen.findByText('data-visualizer'));
		await screen.findByText('简介');

		await user.click(screen.getByRole('button', { name: '下载并安装' }));

		expect(await screen.findByText('需在详情页登录')).toBeInTheDocument();
	});

	it('不应再渲染手动"刷新"按钮', async () => {
		renderMarketplace();
		await waitFor(() => expect(marketSearch).toHaveBeenCalled());
		expect(screen.queryByRole('button', { name: /刷新/ })).not.toBeInTheDocument();
	});

	it('挂载时市场缓存为空(首次搜索 total=0)应自动调用 market_refresh 并失效重新搜索', async () => {
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });
		renderMarketplace();

		await waitFor(() => expect(marketRefresh).toHaveBeenCalledTimes(1));
		// 刷新成功后应失效 market-search 缓存, 触发再次搜索(至少 2 次: 挂载首次 + 刷新后重取)
		await waitFor(() => expect(marketSearch).toHaveBeenCalledTimes(2));
	});

	it('挂载时市场缓存非空(首次搜索 total>0)不应自动调用 market_refresh', async () => {
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 5 });
		renderMarketplace();

		await waitFor(() => expect(marketSearch).toHaveBeenCalled());
		expect(marketRefresh).not.toHaveBeenCalled();
	});

	it('本会话已刷新过市场缓存时, 即使首次搜索 total=0 也不应再次自动调用 market_refresh', async () => {
		useUiStore.getState().setMarketRefreshed();
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });
		renderMarketplace();

		await waitFor(() => expect(marketSearch).toHaveBeenCalled());
		expect(marketRefresh).not.toHaveBeenCalled();
	});

	// 还原原型第 2 屏: 默认选中 data-visualizer(即当页第一条结果), 面板"常驻"且无需用户点击就有
	// 内容, 与"点击卡片应经 market_detail 查询..."用例的区别是这里全程不做任何点击操作
	it('挂载后应默认选中当页第一条结果, 详情面板无需点击即展示该资源', async () => {
		const resource = makeMarketResource({ name: 'data-visualizer' });
		vi.mocked(marketSearch).mockResolvedValue({ items: [resource], total: 1 });
		vi.mocked(marketDetail).mockResolvedValue(resource);

		renderMarketplace();

		expect(await screen.findByText('简介')).toBeInTheDocument();
		expect(marketDetail).toHaveBeenCalledWith(1, 'acme/skills:data-visualizer');
	});

	// 详情面板改为常驻展示后, 无结果(如搜索/筛选后当页为空)不应让面板从布局里整体消失, 而是在
	// 同一位置展示空态, 且不应凭空调用 market_detail(没有可选中的第一条)
	it('搜索无结果时详情面板应展示空态, 且不调用 market_detail', async () => {
		vi.mocked(marketSearch).mockResolvedValue({ items: [], total: 0 });

		renderMarketplace();

		expect(await screen.findByText('暂无可查看的详情')).toBeInTheDocument();
		expect(marketDetail).not.toHaveBeenCalled();
	});
});
