// 文件作用: AccountSection 组件单测(已连接/未连接展示、登录/退出/管理令牌回调、管理全部令牌展开)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { AuthAccount } from '@/api/auth';
import { AccountSection } from './account-section';

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

function renderSection(overrides: Partial<React.ComponentProps<typeof AccountSection>> = {}) {
	const onLogin = vi.fn();
	const onLogout = vi.fn();
	const onSaveToken = vi.fn();
	render(
		<AccountSection
			accounts={[]}
			pendingProvider={null}
			onLogin={onLogin}
			onLogout={onLogout}
			onSaveToken={onSaveToken}
			{...overrides}
		/>,
	);
	return { onLogin, onLogout, onSaveToken };
}

describe('AccountSection', () => {
	it('应渲染标题与三个服务行(GitHub/Google/Microsoft)', () => {
		renderSection();
		expect(screen.getByText('账号与认证 Account')).toBeInTheDocument();
		expect(screen.getByText('GitHub')).toBeInTheDocument();
		expect(screen.getByText('Google')).toBeInTheDocument();
		expect(screen.getByText('Microsoft')).toBeInTheDocument();
	});

	it('无匹配账号的服务应展示"未连接"与登录按钮', () => {
		renderSection();
		const githubRow = screen.getByText('GitHub').closest('div[class*="rounded-md"]');
		expect(githubRow).not.toBeNull();
		expect(within(githubRow as HTMLElement).getByText('未连接')).toBeInTheDocument();
		expect(
			within(githubRow as HTMLElement).getByRole('button', { name: /登录/ }),
		).toBeInTheDocument();
	});

	it('已连接的服务应展示邮箱与退出/管理令牌按钮', () => {
		renderSection({
			accounts: [makeAccount({ provider: 'GitHub', account: 'demo@example.com' })],
		});
		const githubRow = screen.getByText('GitHub').closest('div[class*="rounded-md"]');
		expect(
			within(githubRow as HTMLElement).getByText('已连接: demo@example.com'),
		).toBeInTheDocument();
		expect(
			within(githubRow as HTMLElement).getByRole('button', { name: /退出/ }),
		).toBeInTheDocument();
		expect(
			within(githubRow as HTMLElement).getByRole('button', { name: /管理令牌/ }),
		).toBeInTheDocument();
	});

	it('点击未连接服务的"登录"应以对应 provider 数值编码调用 onLogin', async () => {
		const user = userEvent.setup();
		const { onLogin } = renderSection();

		const googleRow = screen.getByText('Google').closest('div[class*="rounded-md"]');
		await user.click(within(googleRow as HTMLElement).getByRole('button', { name: /登录/ }));

		expect(onLogin).toHaveBeenCalledWith(2);
	});

	it('点击已连接服务的"退出"应以对应 provider 数值编码调用 onLogout', async () => {
		const user = userEvent.setup();
		const { onLogout } = renderSection({
			accounts: [makeAccount({ provider: 'Google', account: 'demo@example.com' })],
		});

		const googleRow = screen.getByText('Google').closest('div[class*="rounded-md"]');
		await user.click(within(googleRow as HTMLElement).getByRole('button', { name: /退出/ }));

		expect(onLogout).toHaveBeenCalledWith(2);
	});

	it('点击"管理令牌"应展开令牌输入, 填入令牌后点击保存应调用 onSaveToken(provider, token)', async () => {
		const user = userEvent.setup();
		const { onSaveToken } = renderSection({
			accounts: [makeAccount({ provider: 'GitHub', account: 'demo@example.com' })],
		});

		const githubRow = screen
			.getByText('GitHub')
			.closest('div[class*="rounded-md"]') as HTMLElement;
		await user.click(within(githubRow).getByRole('button', { name: /管理令牌/ }));

		const input = within(githubRow).getByPlaceholderText('访问令牌 / Personal Access Token');
		await user.type(input, 'ghp_demo123');
		await user.click(within(githubRow).getByRole('button', { name: '保存' }));

		expect(onSaveToken).toHaveBeenCalledWith(1, 'ghp_demo123');
	});

	it('点击底部"管理全部令牌"应同时展开三项的令牌输入', async () => {
		const user = userEvent.setup();
		renderSection({
			accounts: [
				makeAccount({ provider: 'GitHub', account: 'a@example.com' }),
				makeAccount({ id: 2, provider: 'Google', account: 'b@example.com' }),
			],
		});

		await user.click(screen.getByRole('button', { name: '管理全部令牌' }));

		expect(screen.getAllByPlaceholderText('访问令牌 / Personal Access Token')).toHaveLength(3);
	});

	it('pendingProvider 匹配某服务时, 该行按钮应禁用且文案提示进行中', () => {
		renderSection({
			pendingProvider: 3,
		});
		const microsoftRow = screen.getByText('Microsoft').closest('div[class*="rounded-md"]');
		expect(
			within(microsoftRow as HTMLElement).getByRole('button', { name: '登录中...' }),
		).toBeDisabled();
	});
});
