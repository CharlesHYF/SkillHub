// 文件作用: MarketDetailPanel 渲染与交互单测(字段展示/标签/兼容 Agent 占位/安装要求/认证说明/
//           下载并安装与关闭回调/安装错误提示/安装中态/"查看详情"跳转链接)
// 创建日期: 2026-07-10
import type { ReactElement } from 'react';
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import type { MarketResourceRespVO } from '@/api/market';
import { buildMarketDetailId } from '@/pages/marketplace-detail';
import { MarketDetailPanel } from './market-detail-panel';

// M5 起面板底部新增"查看详情"链接(react-router Link), Link 需要 Router 语境才能渲染(否则抛
// "useHref() may be used only in the context of a <Router>"), 故所有渲染都经这层 MemoryRouter
function renderPanel(ui: ReactElement) {
	return render(<MemoryRouter>{ui}</MemoryRouter>);
}

function makeMarketResource(overrides: Partial<MarketResourceRespVO> = {}): MarketResourceRespVO {
	return {
		sourceType: 'GithubSkills',
		resType: 'Skill',
		extId: 'acme/skills:demo',
		name: 'data-visualizer',
		displayName: 'data-visualizer',
		description: 'data-visualizer 能够将结构化数据快速转换为多种可视化图表。',
		author: 'SkillHub Official',
		version: '1.2.0',
		stars: 12300,
		category: 'productivity',
		tags: ['数据可视化', '图表', '导出', '报告'],
		authRequired: false,
		installManifest: { Skill: { repo: 'acme/skills', path: 'skills/demo', gitRef: 'main' } },
		updatedAt: '2025-05-20 00:00:00',
		...overrides,
	};
}

const baseProps = {
	onClose: vi.fn(),
	onDownload: vi.fn(),
};

describe('MarketDetailPanel', () => {
	it('应展示名称/类型徽标/作者/版本/更新时间/星标数', () => {
		renderPanel(<MarketDetailPanel {...baseProps} resource={makeMarketResource()} />);

		expect(screen.getByText('data-visualizer')).toBeInTheDocument();
		expect(screen.getByText('Skill')).toBeInTheDocument();
		expect(screen.getByText(/SkillHub Official/)).toBeInTheDocument();
		expect(screen.getByText(/1\.2\.0/)).toBeInTheDocument();
		expect(screen.getByText(/2025-05-20/)).toBeInTheDocument();
		expect(screen.getByText(/12\.3k/)).toBeInTheDocument();
	});

	it('应展示简介与标签', () => {
		renderPanel(<MarketDetailPanel {...baseProps} resource={makeMarketResource()} />);

		expect(
			screen.getByText('data-visualizer 能够将结构化数据快速转换为多种可视化图表。'),
		).toBeInTheDocument();
		expect(screen.getByText('数据可视化')).toBeInTheDocument();
		expect(screen.getByText('图表')).toBeInTheDocument();
	});

	it('version/updatedAt 为空字符串时应展示占位符 "—", 不展示裸 "v-" 或"更新于: "空白', () => {
		renderPanel(
			<MarketDetailPanel
				{...baseProps}
				resource={makeMarketResource({ version: '', updatedAt: '' })}
			/>,
		);
		expect(screen.queryByText(/v-/)).not.toBeInTheDocument();
		expect(screen.getByText('版本: — · 更新于: —')).toBeInTheDocument();
	});

	it('应展示兼容 Agent 区块的占位说明(当前领域模型未提供逐项兼容性数据)', () => {
		renderPanel(<MarketDetailPanel {...baseProps} resource={makeMarketResource()} />);
		expect(screen.getByText('兼容 Agent')).toBeInTheDocument();
		expect(screen.getByText(/暂无兼容性数据/)).toBeInTheDocument();
	});

	it('Skill 类资源应展示来源仓库/子目录/版本引用作为安装要求', () => {
		renderPanel(
			<MarketDetailPanel
				{...baseProps}
				resource={makeMarketResource({
					installManifest: {
						Skill: { repo: 'acme/skills', path: 'skills/demo', gitRef: 'v2.0.0' },
					},
				})}
			/>,
		);
		expect(screen.getByText('来源仓库: acme/skills')).toBeInTheDocument();
		expect(screen.getByText('子目录: skills/demo')).toBeInTheDocument();
		expect(screen.getByText('版本引用: v2.0.0')).toBeInTheDocument();
	});

	it('authRequired 为真时应展示需要登录或授权的说明', () => {
		renderPanel(
			<MarketDetailPanel
				{...baseProps}
				resource={makeMarketResource({ authRequired: true })}
			/>,
		);
		expect(screen.getByText(/若需要登录或授权/)).toBeInTheDocument();
	});

	it('authRequired 为假时应展示无需登录或授权的说明', () => {
		renderPanel(
			<MarketDetailPanel
				{...baseProps}
				resource={makeMarketResource({ authRequired: false })}
			/>,
		);
		expect(screen.getByText(/无需登录或授权/)).toBeInTheDocument();
	});

	it('点击"下载并安装"应触发 onDownload 并回传该资源', () => {
		const onDownload = vi.fn();
		const resource = makeMarketResource();
		renderPanel(
			<MarketDetailPanel {...baseProps} resource={resource} onDownload={onDownload} />,
		);

		screen.getByRole('button', { name: '下载并安装' }).click();
		expect(onDownload).toHaveBeenCalledWith(resource);
	});

	it('点击关闭按钮应触发 onClose', () => {
		const onClose = vi.fn();
		renderPanel(
			<MarketDetailPanel {...baseProps} resource={makeMarketResource()} onClose={onClose} />,
		);

		screen.getByRole('button', { name: '关闭' }).click();
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it('installError 非空时应展示该错误文案', () => {
		renderPanel(
			<MarketDetailPanel
				{...baseProps}
				resource={makeMarketResource()}
				installError="需在详情页登录"
			/>,
		);
		expect(screen.getByRole('alert')).toHaveTextContent('需在详情页登录');
	});

	it('isInstalling 为真时"下载并安装"按钮应禁用且文案变化', () => {
		renderPanel(
			<MarketDetailPanel {...baseProps} resource={makeMarketResource()} isInstalling />,
		);
		const button = screen.getByRole('button', { name: '安装中...' });
		expect(button).toBeDisabled();
	});

	// 还原原型: 主按钮"下载并安装"下方还有一个次级"查看详情", 跳到 /marketplace/:id 完整详情页
	// (该页信息更全, 也是 AUTH_REQUIRED 走认证弹窗重试安装的入口); 用已导出的 buildMarketDetailId
	// 反推期望的 href, 与 pages/marketplace-detail.tsx 的编码约定保持同一份真相
	it('应展示"查看详情"链接, 指向 /marketplace/:id 完整详情页', () => {
		const resource = makeMarketResource();
		renderPanel(<MarketDetailPanel {...baseProps} resource={resource} />);

		const link = screen.getByRole('link', { name: '查看详情' });
		expect(link).toHaveAttribute(
			'href',
			`/marketplace/${buildMarketDetailId(1, 'acme/skills:demo')}`,
		);
	});
});
