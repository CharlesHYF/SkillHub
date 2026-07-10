// 文件作用: MarketplaceDetail 页面集成测试(mock src/api/market 与 src/api/auth, 保留
//           market.ts 内真实的 parseAuthRequiredProvider 实现以验证端到端解析约定) —— 详情渲染
//           (含无真实字段的合理占位)、下载并安装成功态、market_install 因 AUTH_REQUIRED 失败时
//           打开 AuthModal 并在认证成功后自动重试安装(GitHub 与令牌两条路径)、返回 Marketplace
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import type { MarketResource } from '@/api/market';
import type { AuthAccount } from '@/api/auth';
import type { Resource } from '@/api/library';
import MarketplaceDetail, { buildMarketDetailId, parseMarketDetailId } from './marketplace-detail';

vi.mock('@/api/market', async (importOriginal) => {
	const actual = await importOriginal<typeof import('@/api/market')>();
	return { ...actual, marketDetail: vi.fn(), marketInstall: vi.fn() };
});
vi.mock('@/api/auth', () => ({
	authLogin: vi.fn(),
	authEnterToken: vi.fn(),
}));

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async (importOriginal) => {
	const actual = await importOriginal<typeof import('react-router-dom')>();
	return { ...actual, useNavigate: () => mockNavigate };
});

import { marketDetail, marketInstall } from '@/api/market';
import { authLogin, authEnterToken } from '@/api/auth';

function makeResource(overrides: Partial<MarketResource> = {}): MarketResource {
	const name = overrides.name ?? 'github-sync-mcp';
	return {
		sourceType: 'GithubMcp',
		resType: 'Mcp',
		extId: `acme/mcp:${name}`,
		name,
		displayName: name,
		description: '将 GitHub 仓库、Issue 和 Pull Request 数据同步到你的 Agent',
		author: 'SkillHub Team',
		version: '1.2.3',
		stars: 1200,
		category: 'dev-tools',
		tags: [],
		authRequired: true,
		installManifest: {
			Mcp: { serverDef: { name, command: 'npx', args: ['demo'], env: {}, url: null } },
		},
		updatedAt: '2024-05-20 00:00:00',
		...overrides,
	};
}

function makeAccount(overrides: Partial<AuthAccount> = {}): AuthAccount {
	return {
		id: 1,
		provider: 'GitHub',
		account: 'demo@example.com',
		scope: 'repo',
		status: true,
		connectTime: '2026-07-10 00:00:00',
		...overrides,
	};
}

function makeInstalledResource(overrides: Partial<Resource> = {}): Resource {
	return {
		id: 1,
		resType: 'Mcp',
		name: 'github-sync-mcp',
		displayName: 'github-sync-mcp',
		version: '1.2.3',
		sourceType: 'Official',
		localPath: '/tmp/github-sync-mcp',
		enabled: true,
		createTime: '2026-07-10 00:00:00',
		updateTime: '2026-07-10 00:00:00',
		...overrides,
	};
}

function renderDetail(id: string) {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<MemoryRouter initialEntries={[`/marketplace/${id}`]}>
				<Routes>
					<Route path="/marketplace/:id" element={<MarketplaceDetail />} />
					<Route path="/marketplace" element={<div>MarketplaceListStub</div>} />
					<Route path="/installed" element={<div>InstalledStub</div>} />
				</Routes>
			</MemoryRouter>
		</QueryClientProvider>,
	);
}

describe('parseMarketDetailId / buildMarketDetailId', () => {
	it('应互为逆运算, 支持含 / 与 : 的 extId', () => {
		const id = buildMarketDetailId(3, 'owner/repo:path/to/skill');
		expect(parseMarketDetailId(id)).toEqual({
			sourceType: 3,
			extId: 'owner/repo:path/to/skill',
		});
	});

	it('缺失或不合法的 id 应返回 null', () => {
		expect(parseMarketDetailId(undefined)).toBeNull();
		expect(parseMarketDetailId('not-a-valid-id')).toBeNull();
	});
});

describe('MarketplaceDetail 页面', () => {
	beforeEach(() => {
		mockNavigate.mockReset();
		vi.mocked(marketDetail).mockReset();
		vi.mocked(marketInstall).mockReset();
		vi.mocked(authLogin).mockReset();
		vi.mocked(authEnterToken).mockReset();
	});

	it('应据 marketDetail 渲染资源详情, 无真实字段(大小/下载量/兼容性)按合理占位展示', async () => {
		const resource = makeResource();
		vi.mocked(marketDetail).mockResolvedValue(resource);

		renderDetail(buildMarketDetailId(3, resource.extId));

		expect(await screen.findByText('github-sync-mcp')).toBeInTheDocument();
		expect(screen.getByText('v1.2.3')).toBeInTheDocument();
		expect(screen.getByText('SkillHub Team')).toBeInTheDocument();
		expect(screen.getByText('dev-tools')).toBeInTheDocument();
		expect(screen.getAllByText('2024-05-20 00:00').length).toBeGreaterThanOrEqual(1);
		expect(screen.getAllByText(/暂无数据/).length).toBeGreaterThan(0);
		expect(screen.getByText('权限说明')).toBeInTheDocument();
		expect(screen.getByText('读取资源')).toBeInTheDocument();
		expect(screen.getByText('安装步骤')).toBeInTheDocument();
		expect(screen.getByText('版本历史')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /收藏/ })).toBeInTheDocument();
	});

	it('marketDetail 返回 null 时应展示资源不存在提示', async () => {
		vi.mocked(marketDetail).mockResolvedValue(null);
		renderDetail(buildMarketDetailId(1, 'acme/skills:missing'));
		expect(await screen.findByText(/资源不存在/)).toBeInTheDocument();
	});

	it('点击返回 Marketplace 应导航到 /marketplace', async () => {
		const user = userEvent.setup();
		vi.mocked(marketDetail).mockResolvedValue(makeResource());
		renderDetail(buildMarketDetailId(3, 'acme/mcp:github-sync-mcp'));

		await user.click(await screen.findByRole('button', { name: /返回 Marketplace/ }));
		expect(mockNavigate).toHaveBeenCalledWith('/marketplace');
	});

	it('点击下载并安装成功后应展示已安装成功提示', async () => {
		const user = userEvent.setup();
		const resource = makeResource({ authRequired: false });
		vi.mocked(marketDetail).mockResolvedValue(resource);
		vi.mocked(marketInstall).mockResolvedValue(makeInstalledResource());

		renderDetail(buildMarketDetailId(3, resource.extId));
		await user.click(await screen.findByRole('button', { name: '下载并安装' }));

		expect(await screen.findByText('已安装成功')).toBeInTheDocument();
		expect(marketInstall).toHaveBeenCalledWith(3, resource.extId);
	});

	it('market_install 因 AUTH_REQUIRED 失败应打开 AuthModal, GitHub 登录成功后应自动重试安装', async () => {
		const user = userEvent.setup();
		const resource = makeResource({ authRequired: true });
		vi.mocked(marketDetail).mockResolvedValue(resource);
		vi.mocked(marketInstall)
			.mockRejectedValueOnce(new Error('AUTH_REQUIRED:1'))
			.mockResolvedValueOnce(makeInstalledResource());
		vi.mocked(authLogin).mockResolvedValue(makeAccount({ provider: 'GitHub' }));

		renderDetail(buildMarketDetailId(3, resource.extId));
		await user.click(await screen.findByRole('button', { name: '下载并安装' }));

		expect(await screen.findByText('需要登录 / Authentication Required')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: /使用 GitHub 登录/ }));

		expect(authLogin).toHaveBeenCalledWith(1);
		await waitFor(() => expect(marketInstall).toHaveBeenCalledTimes(2));
		expect(await screen.findByText('已安装成功')).toBeInTheDocument();
		expect(screen.queryByText('需要登录 / Authentication Required')).not.toBeInTheDocument();
	});

	it('market_install 因 AUTH_REQUIRED(Token) 失败, 输入令牌认证成功后应自动重试安装', async () => {
		const user = userEvent.setup();
		const resource = makeResource({ authRequired: true });
		vi.mocked(marketDetail).mockResolvedValue(resource);
		vi.mocked(marketInstall)
			.mockRejectedValueOnce(new Error('AUTH_REQUIRED:4'))
			.mockResolvedValueOnce(makeInstalledResource());
		vi.mocked(authEnterToken).mockResolvedValue(makeAccount({ provider: 'Token' }));

		renderDetail(buildMarketDetailId(3, resource.extId));
		await user.click(await screen.findByRole('button', { name: '下载并安装' }));
		await screen.findByText('需要登录 / Authentication Required');

		// AUTH_REQUIRED:4 对应 Token, 弹窗应已自动展开令牌输入区(见 AuthModal defaultProvider)
		const input = await screen.findByPlaceholderText('访问令牌 / Personal Access Token');
		await user.type(input, 'ghp_demo');
		await user.click(screen.getByRole('button', { name: '继续' }));

		expect(authEnterToken).toHaveBeenCalledWith(4, 'ghp_demo');
		await waitFor(() => expect(marketInstall).toHaveBeenCalledTimes(2));
		expect(await screen.findByText('已安装成功')).toBeInTheDocument();
	});
});
