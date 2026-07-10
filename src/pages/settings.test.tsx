// 文件作用: Settings 页面集成测试(mock src/api/setting 与 src/api/auth) —— 五个分区渲染、
//           切换开关/修改超时改变本地态、保存更改携带完整 Settings、恢复默认回到硬编码默认值、
//           账号区登录/退出对应 auth api、脏态下保存按钮可用性
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Settings } from '@/api/setting';
import type { AuthAccount } from '@/api/auth';
import Settings_ from './settings';

vi.mock('@/api/setting', () => ({
	settingsGet: vi.fn(),
	settingsSave: vi.fn(),
	appVersion: vi.fn(),
}));
vi.mock('@/api/auth', () => ({
	authAccounts: vi.fn(),
	authLogin: vi.fn(),
	authLogout: vi.fn(),
	authEnterToken: vi.fn(),
}));

import { settingsGet, settingsSave } from '@/api/setting';
import { authAccounts, authLogin, authLogout, authEnterToken } from '@/api/auth';

/** 与页面内硬编码的默认设置口径一致(见 pages/settings.tsx DEFAULT_SETTINGS), 独立重复定义,
 * 使本测试文件作为一份不依赖实现内部常量的独立规格(与 portability.test.tsx 的
 * defaultExportOptions 同一惯例) */
const defaultSettings: Settings = {
	storageSkillDir: '',
	storageMcpDir: '',
	syncAutoNewAgent: true,
	syncCheckUpdateOnStart: true,
	syncConflictPrompt: true,
	syncOnlyEnabled: false,
	netProxyMode: 0,
	netHttpProxy: '',
	netHttpsProxy: '',
	netNoProxy: '',
	netTimeoutSec: 30,
	updateChannel: 0,
};

/** 一份与默认值处处不同的"已保存设置", 用于验证"恢复默认"确实回到硬编码默认值, 而不是回到
 * 本次 settingsGet 加载到的值 */
const loadedSettings: Settings = {
	storageSkillDir: '/Users/demo/.skillhub/skills',
	storageMcpDir: '/Users/demo/.skillhub/mcp',
	syncAutoNewAgent: true,
	syncCheckUpdateOnStart: true,
	syncConflictPrompt: true,
	syncOnlyEnabled: true,
	netProxyMode: 2,
	netHttpProxy: 'http://127.0.0.1:1087',
	netHttpsProxy: 'http://127.0.0.1:1087',
	netNoProxy: 'localhost',
	netTimeoutSec: 90,
	updateChannel: 1,
};

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

function renderSettings() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<Settings_ />
		</QueryClientProvider>,
	);
}

describe('Settings 页面', () => {
	beforeEach(() => {
		vi.mocked(settingsGet).mockReset().mockResolvedValue(loadedSettings);
		vi.mocked(settingsSave)
			.mockReset()
			.mockImplementation(async (next: Settings) => next);
		vi.mocked(authAccounts)
			.mockReset()
			.mockResolvedValue([makeAccount({ provider: 'GitHub', account: 'demo@example.com' })]);
		vi.mocked(authLogin)
			.mockReset()
			.mockResolvedValue(makeAccount({ provider: 'Google' }));
		vi.mocked(authLogout).mockReset().mockResolvedValue(undefined);
		vi.mocked(authEnterToken)
			.mockReset()
			.mockResolvedValue(makeAccount({ provider: 'GitHub' }));
	});

	it('应渲染标题与五个分区标题', async () => {
		renderSettings();

		expect(screen.getByText('设置 / Settings')).toBeInTheDocument();
		expect(screen.getByText('账号与认证 Account')).toBeInTheDocument();
		expect(screen.getByText('存储目录 Storage')).toBeInTheDocument();
		expect(screen.getByText('同步偏好 Sync Preferences')).toBeInTheDocument();
		expect(screen.getByText('网络与代理 Network')).toBeInTheDocument();
		expect(screen.getByText('更新通道 Update Channel')).toBeInTheDocument();
		await waitFor(() => expect(settingsGet).toHaveBeenCalled());
	});

	it('账号区应展示已连接账号邮箱与未连接服务', async () => {
		renderSettings();

		expect(await screen.findByText('已连接: demo@example.com')).toBeInTheDocument();
		expect(screen.getAllByText('未连接')).toHaveLength(2);
	});

	it('点击已连接账号的"退出"应以对应 provider 数值编码调用 authLogout', async () => {
		const user = userEvent.setup();
		renderSettings();
		await screen.findByText('已连接: demo@example.com');

		await user.click(screen.getByRole('button', { name: /退出/ }));

		expect(authLogout).toHaveBeenCalledWith(1);
	});

	it('点击未连接服务的"登录"应调用 authLogin', async () => {
		const user = userEvent.setup();
		renderSettings();
		await screen.findByText('已连接: demo@example.com');

		const loginButtons = screen.getAllByRole('button', { name: /登录/ });
		await user.click(loginButtons[0]);

		expect(authLogin).toHaveBeenCalled();
	});

	it('加载完成前保存按钮应禁用, 加载完成且未改动时仍应禁用', async () => {
		renderSettings();

		await waitFor(() =>
			expect(screen.getByLabelText('请求超时(秒)')).toHaveValue(loadedSettings.netTimeoutSec),
		);

		expect(screen.getByRole('button', { name: '保存更改' })).toBeDisabled();
	});

	it('切换"仅同步已启用项"开关应改变本地态并使保存按钮可用', async () => {
		const user = userEvent.setup();
		renderSettings();
		await waitFor(() =>
			expect(screen.getByRole('switch', { name: '仅同步已启用项' })).toBeChecked(),
		);

		await user.click(screen.getByRole('switch', { name: '仅同步已启用项' }));

		expect(screen.getByRole('switch', { name: '仅同步已启用项' })).not.toBeChecked();
		expect(screen.getByRole('button', { name: '保存更改' })).toBeEnabled();
	});

	it('修改请求超时输入后点击"保存更改"应以完整 Settings(含被改字段)调用 settingsSave', async () => {
		const user = userEvent.setup();
		renderSettings();
		await waitFor(() =>
			expect(screen.getByLabelText('请求超时(秒)')).toHaveValue(loadedSettings.netTimeoutSec),
		);

		fireEvent.change(screen.getByLabelText('请求超时(秒)'), { target: { value: '120' } });
		await user.click(screen.getByRole('button', { name: '保存更改' }));

		await waitFor(() =>
			expect(settingsSave).toHaveBeenCalledWith({ ...loadedSettings, netTimeoutSec: 120 }),
		);
	});

	it('点击"恢复默认"应把本地态重置为硬编码默认值(而非回到已加载的设置)', async () => {
		const user = userEvent.setup();
		renderSettings();
		await waitFor(() =>
			expect(screen.getByRole('radio', { name: 'Beta (测试版)' })).toBeChecked(),
		);

		await user.click(screen.getByRole('button', { name: '恢复默认' }));

		expect(screen.getByLabelText('请求超时(秒)')).toHaveValue(defaultSettings.netTimeoutSec);
		expect(screen.getByRole('switch', { name: '仅同步已启用项' })).not.toBeChecked();
		expect(screen.getByRole('radio', { name: 'Stable (稳定版)' })).toBeChecked();
		expect(screen.getByRole('button', { name: '保存更改' })).toBeEnabled();
	});
});
