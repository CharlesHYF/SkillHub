// 文件作用: MarketCard 渲染与交互单测(字段展示/类型徽标/选中态/查看详情与下载回调/安装错误提示)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MarketResource } from '@/api/market';
import { MarketCard } from './market-card';

function makeMarketResource(overrides: Partial<MarketResource> = {}): MarketResource {
	return {
		sourceType: 'GithubSkills',
		resType: 'Skill',
		extId: 'acme/skills:demo',
		name: 'data-visualizer',
		displayName: 'data-visualizer',
		description: '将结构化数据转换为多种可视化图表, 支持导出与自定义主题。',
		author: 'SkillHub Official',
		version: '1.2.0',
		stars: 12300,
		category: 'productivity',
		tags: ['数据可视化', '图表'],
		authRequired: false,
		installManifest: { Skill: { repo: 'acme/skills', path: 'skills/demo', gitRef: 'main' } },
		updatedAt: '2025-05-20 00:00:00',
		...overrides,
	};
}

const baseProps = {
	selected: false,
	onSelect: vi.fn(),
	onDownload: vi.fn(),
};

describe('MarketCard', () => {
	it('应展示名称/类型徽标/描述/作者/版本/星标数', () => {
		const resource = makeMarketResource();
		render(<MarketCard {...baseProps} resource={resource} />);

		expect(screen.getByText('data-visualizer')).toBeInTheDocument();
		expect(screen.getByText('Skill')).toBeInTheDocument();
		expect(
			screen.getByText('将结构化数据转换为多种可视化图表, 支持导出与自定义主题。'),
		).toBeInTheDocument();
		expect(screen.getByText(/SkillHub Official/)).toBeInTheDocument();
		expect(screen.getByText(/1\.2\.0/)).toBeInTheDocument();
		expect(screen.getByText(/12\.3k/)).toBeInTheDocument();
	});

	it('MCP 类型应展示 MCP 徽标', () => {
		render(<MarketCard {...baseProps} resource={makeMarketResource({ resType: 'Mcp' })} />);
		expect(screen.getByText('MCP')).toBeInTheDocument();
	});

	it('点击卡片主体应触发 onSelect 并回传该资源', async () => {
		const user = userEvent.setup();
		const onSelect = vi.fn();
		const resource = makeMarketResource();
		render(<MarketCard {...baseProps} resource={resource} onSelect={onSelect} />);

		await user.click(screen.getByText('data-visualizer'));
		expect(onSelect).toHaveBeenCalledWith(resource);
	});

	it('点击"查看详情"应触发 onSelect 并回传该资源', async () => {
		const user = userEvent.setup();
		const onSelect = vi.fn();
		const resource = makeMarketResource();
		render(<MarketCard {...baseProps} resource={resource} onSelect={onSelect} />);

		await user.click(screen.getByRole('button', { name: '查看详情' }));
		expect(onSelect).toHaveBeenCalledWith(resource);
	});

	it('点击"下载"应触发 onDownload 并回传该资源, 且不触发 onSelect', async () => {
		const user = userEvent.setup();
		const onSelect = vi.fn();
		const onDownload = vi.fn();
		const resource = makeMarketResource();
		render(
			<MarketCard
				{...baseProps}
				resource={resource}
				onSelect={onSelect}
				onDownload={onDownload}
			/>,
		);

		await user.click(screen.getByRole('button', { name: '下载' }));
		expect(onDownload).toHaveBeenCalledWith(resource);
		expect(onSelect).not.toHaveBeenCalled();
	});

	it('selected 为真时卡片应带 data-state=selected', () => {
		const resource = makeMarketResource();
		render(<MarketCard {...baseProps} resource={resource} selected />);
		expect(screen.getByText('data-visualizer').closest('[data-state]')).toHaveAttribute(
			'data-state',
			'selected',
		);
	});

	it('installError 非空时应展示该错误文案', () => {
		render(
			<MarketCard
				{...baseProps}
				resource={makeMarketResource()}
				installError="需在详情页登录"
			/>,
		);
		expect(screen.getByRole('alert')).toHaveTextContent('需在详情页登录');
	});
});
