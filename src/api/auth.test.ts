// 文件作用: auth api 层单测
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => []),
}));

import { authAccounts, authLogin, authEnterToken, authLogout, type AuthAccount } from './auth';

const sampleAccount: AuthAccount = {
	id: 1,
	provider: 'GitHub',
	account: 'demo@example.com',
	scope: 'repo,read:org',
	status: true,
	connectTime: '2026-07-01 00:00:00',
};

describe('auth api', () => {
	it('authAccounts 应以 command 名 auth_accounts 调用且不带参数', async () => {
		vi.mocked(invoke).mockResolvedValueOnce([sampleAccount]);
		const result = await authAccounts();
		expect(result).toEqual([sampleAccount]);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('auth_accounts');
	});

	it('authLogin 应以 command 名 auth_login 调用并传数值编码 provider', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(sampleAccount);
		const result = await authLogin(1);
		expect(result).toEqual(sampleAccount);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('auth_login', { provider: 1 });
	});

	it('authEnterToken 应以 command 名 auth_enter_token 调用并传 provider/token', async () => {
		const tokenAccount: AuthAccount = { ...sampleAccount, provider: 'Token' };
		vi.mocked(invoke).mockResolvedValueOnce(tokenAccount);
		const result = await authEnterToken(4, 'ghp_demo123');
		expect(result).toEqual(tokenAccount);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('auth_enter_token', {
			provider: 4,
			token: 'ghp_demo123',
		});
	});

	it('authLogout 应以 command 名 auth_logout 调用并传 provider', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(undefined);
		await authLogout(2);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('auth_logout', { provider: 2 });
	});
});
