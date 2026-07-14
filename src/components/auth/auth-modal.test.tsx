// 文件作用: AuthModal 渲染与交互单测(mock src/api/auth) —— 打开/关闭态、三个 OAuth 登录按钮
//           各自调用 authLogin、令牌录入展开与提交调用 authEnterToken、defaultProvider 为 Token
//           时自动展开令牌输入区、认证成功回调 onAuthenticated、失败态展示错误提示、取消关闭
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { AuthAccountRespVO } from '@/api/auth';
import { AuthModal } from './auth-modal';

vi.mock('@/api/auth', () => ({
	authLogin: vi.fn(),
	authEnterToken: vi.fn(),
}));

import { authLogin, authEnterToken } from '@/api/auth';

function makeAccount(overrides: Partial<AuthAccountRespVO> = {}): AuthAccountRespVO {
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

describe('AuthModal', () => {
	beforeEach(() => {
		vi.mocked(authLogin).mockReset();
		vi.mocked(authEnterToken).mockReset();
	});

	it('open 为 false 时不应展示弹窗内容', () => {
		render(<AuthModal open={false} onOpenChange={vi.fn()} onAuthenticated={vi.fn()} />);
		expect(screen.queryByText(/需要登录/)).not.toBeInTheDocument();
	});

	it('open 为 true 时应展示标题/说明/三个登录按钮/令牌入口/权限说明', () => {
		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={vi.fn()} />);

		expect(screen.getByText('需要登录 / Authentication Required')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /使用 GitHub 登录/ })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /使用 Google 登录/ })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /使用 Microsoft 登录/ })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: '输入访问令牌' })).toBeInTheDocument();
		expect(screen.getByText('授权后将允许 SkillHub:')).toBeInTheDocument();
		expect(screen.getByText('读取资源')).toBeInTheDocument();
	});

	it('点击"使用 GitHub 登录"应调用 authLogin(1), 成功后回调 onAuthenticated', async () => {
		const user = userEvent.setup();
		const account = makeAccount({ provider: 'GitHub' });
		vi.mocked(authLogin).mockResolvedValue(account);
		const onAuthenticated = vi.fn();

		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={onAuthenticated} />);
		await user.click(screen.getByRole('button', { name: /使用 GitHub 登录/ }));

		expect(authLogin).toHaveBeenCalledWith(1);
		await waitFor(() => expect(onAuthenticated).toHaveBeenCalledWith(account));
	});

	it('点击"使用 Google 登录"应调用 authLogin(2)', async () => {
		const user = userEvent.setup();
		vi.mocked(authLogin).mockResolvedValue(makeAccount({ provider: 'Google' }));
		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={vi.fn()} />);

		await user.click(screen.getByRole('button', { name: /使用 Google 登录/ }));
		expect(authLogin).toHaveBeenCalledWith(2);
	});

	it('点击"使用 Microsoft 登录"应调用 authLogin(3)', async () => {
		const user = userEvent.setup();
		vi.mocked(authLogin).mockResolvedValue(makeAccount({ provider: 'Microsoft' }));
		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={vi.fn()} />);

		await user.click(screen.getByRole('button', { name: /使用 Microsoft 登录/ }));
		expect(authLogin).toHaveBeenCalledWith(3);
	});

	it('点击"输入访问令牌"应展开输入框, 填入令牌后点击"继续"应调用 authEnterToken(4, token)', async () => {
		const user = userEvent.setup();
		const account = makeAccount({ provider: 'Token' });
		vi.mocked(authEnterToken).mockResolvedValue(account);
		const onAuthenticated = vi.fn();

		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={onAuthenticated} />);
		await user.click(screen.getByRole('button', { name: '输入访问令牌' }));

		const input = await screen.findByPlaceholderText('访问令牌 / Personal Access Token');
		await user.type(input, 'ghp_demo123');
		await user.click(screen.getByRole('button', { name: '继续' }));

		expect(authEnterToken).toHaveBeenCalledWith(4, 'ghp_demo123');
		await waitFor(() => expect(onAuthenticated).toHaveBeenCalledWith(account));
	});

	it('defaultProvider 为 4(Token)时应自动展开令牌输入区', () => {
		render(
			<AuthModal open defaultProvider={4} onOpenChange={vi.fn()} onAuthenticated={vi.fn()} />,
		);
		expect(screen.getByPlaceholderText('访问令牌 / Personal Access Token')).toBeInTheDocument();
	});

	it('未展开令牌输入区或令牌为空时"继续"按钮应禁用', () => {
		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={vi.fn()} />);
		expect(screen.getByRole('button', { name: '继续' })).toBeDisabled();
	});

	it('点击取消应触发 onOpenChange(false)', async () => {
		const user = userEvent.setup();
		const onOpenChange = vi.fn();
		render(<AuthModal open onOpenChange={onOpenChange} onAuthenticated={vi.fn()} />);

		await user.click(screen.getByRole('button', { name: '取消' }));
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it('authLogin 失败应展示错误提示且不回调 onAuthenticated', async () => {
		const user = userEvent.setup();
		vi.mocked(authLogin).mockRejectedValue(new Error('登录失败'));
		const onAuthenticated = vi.fn();

		render(<AuthModal open onOpenChange={vi.fn()} onAuthenticated={onAuthenticated} />);
		await user.click(screen.getByRole('button', { name: /使用 GitHub 登录/ }));

		expect(await screen.findByText('登录失败')).toBeInTheDocument();
		expect(onAuthenticated).not.toHaveBeenCalled();
	});
});
