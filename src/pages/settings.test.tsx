// 文件作用: SettingRespVO 页面集成测试(mock src/api/setting、src/api/auth 与 src/lib/dialog) ——
//           五个分区渲染、切换开关/修改超时改变本地态、保存更改携带完整 SettingRespVO、恢复默认回到
//           硬编码默认值(存储目录两项例外: 保留已加载值, 不清空为空串)、账号区登录/退出对应
//           auth api、脏态下保存按钮可用性、存储目录两个"浏览"按钮接原生目录对话框(pickDirectory)
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { SettingRespVO } from '@/api/setting';
import type { AuthAccountRespVO } from '@/api/auth';
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
vi.mock('@/lib/dialog', () => ({
	pickDirectory: vi.fn(),
}));

import { settingsGet, settingsSave } from '@/api/setting';
import { authAccounts, authLogin, authLogout, authEnterToken } from '@/api/auth';
import { pickDirectory } from '@/lib/dialog';

/** 与页面内硬编码的默认设置口径一致(见 pages/settings.tsx DEFAULT_SETTINGS), 独立重复定义,
 * 使本测试文件作为一份不依赖实现内部常量的独立规格(与 portability.test.tsx 的
 * defaultExportOptions 同一惯例) */
const defaultSettings: SettingRespVO = {
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
const loadedSettings: SettingRespVO = {
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

function renderSettings() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<Settings_ />
		</QueryClientProvider>,
	);
}

describe('SettingRespVO 页面', () => {
	beforeEach(() => {
		vi.mocked(settingsGet).mockReset().mockResolvedValue(loadedSettings);
		vi.mocked(settingsSave)
			.mockReset()
			.mockImplementation(async (next: SettingRespVO) => next);
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
		vi.mocked(pickDirectory).mockReset().mockResolvedValue(null);
	});

	it('应渲染标题与五个分区标题', async () => {
		renderSettings();

		expect(screen.getByText('设置 / SettingRespVO')).toBeInTheDocument();
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

	it('修改请求超时输入后点击"保存更改"应以完整 SettingRespVO(含被改字段)调用 settingsSave', async () => {
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

	// 存储目录两项是本次任务的例外: 后端 settings_get 起已回填真实默认目录并持久化(空则填
	// data_dir/skills·mcp, 见 commit 319be75), "恢复默认"如果仍把这两项清成硬编码空串, 用户会
	// 看到目录被清空这一明显倒退, 故这两项应保留 settingsGet 已加载到的值, 与上面这条"其余字段
	// 回到硬编码默认值"的用例刻意区分开、各自独立断言, 避免同一条用例里出现相互矛盾的期望
	it('点击"恢复默认"应保留当前已加载的存储目录值, 不清空为硬编码空串', async () => {
		const user = userEvent.setup();
		renderSettings();
		await waitFor(() =>
			expect(screen.getByLabelText('本地 Skill 目录')).toHaveValue(
				loadedSettings.storageSkillDir,
			),
		);

		await user.click(screen.getByRole('button', { name: '恢复默认' }));

		expect(screen.getByLabelText('本地 Skill 目录')).toHaveValue(
			loadedSettings.storageSkillDir,
		);
		expect(screen.getByLabelText('本地 MCP 目录')).toHaveValue(loadedSettings.storageMcpDir);
		// 其余字段确实被重置为与已加载设置不同的硬编码默认值, 保存按钮应仍可用
		expect(screen.getByRole('button', { name: '保存更改' })).toBeEnabled();
	});

	it('点击本地 Skill 目录的"浏览"应调用 pickDirectory, 结果写入该输入框并使保存按钮可用', async () => {
		const user = userEvent.setup();
		vi.mocked(pickDirectory).mockResolvedValue('/Users/demo/.skillhub/skills-new');
		renderSettings();
		await waitFor(() =>
			expect(screen.getByLabelText('本地 Skill 目录')).toHaveValue(
				loadedSettings.storageSkillDir,
			),
		);

		const browseButtons = screen.getAllByRole('button', { name: '浏览' });
		await user.click(browseButtons[0]);

		expect(pickDirectory).toHaveBeenCalled();
		await waitFor(() =>
			expect(screen.getByLabelText('本地 Skill 目录')).toHaveValue(
				'/Users/demo/.skillhub/skills-new',
			),
		);
		expect(screen.getByRole('button', { name: '保存更改' })).toBeEnabled();
	});

	it('点击本地 MCP 目录的"浏览"应调用 pickDirectory, 结果写入该输入框', async () => {
		const user = userEvent.setup();
		vi.mocked(pickDirectory).mockResolvedValue('/Users/demo/.skillhub/mcp-new');
		renderSettings();
		await waitFor(() =>
			expect(screen.getByLabelText('本地 MCP 目录')).toHaveValue(
				loadedSettings.storageMcpDir,
			),
		);

		const browseButtons = screen.getAllByRole('button', { name: '浏览' });
		await user.click(browseButtons[1]);

		expect(pickDirectory).toHaveBeenCalled();
		await waitFor(() =>
			expect(screen.getByLabelText('本地 MCP 目录')).toHaveValue(
				'/Users/demo/.skillhub/mcp-new',
			),
		);
	});

	it('"浏览"取消(pickDirectory 返回 null)不应改变目录输入框的值', async () => {
		const user = userEvent.setup();
		renderSettings();
		await waitFor(() =>
			expect(screen.getByLabelText('本地 Skill 目录')).toHaveValue(
				loadedSettings.storageSkillDir,
			),
		);

		const browseButtons = screen.getAllByRole('button', { name: '浏览' });
		await user.click(browseButtons[0]);

		await waitFor(() => expect(pickDirectory).toHaveBeenCalled());
		expect(screen.getByLabelText('本地 Skill 目录')).toHaveValue(
			loadedSettings.storageSkillDir,
		);
	});
});
