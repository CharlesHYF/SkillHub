// 文件作用: setting api 层单测
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(),
}));

import { settingsGet, settingsSave, appVersion, type SettingRespVO } from './setting';

const sampleSettings: SettingRespVO = {
	storageSkillDir: '/Users/demo/.skillhub/skills',
	storageMcpDir: '/Users/demo/.skillhub/mcp',
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

describe('setting api', () => {
	it('settingsGet 应以 command 名 settings_get 调用且不带参数, 返回 SettingRespVO', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(sampleSettings);

		const got = await settingsGet();

		expect(got).toEqual(sampleSettings);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('settings_get');
	});

	it('settingsSave 应以 command 名 settings_save 调用并传 { settings }, 返回保存后的 SettingRespVO', async () => {
		const edited: SettingRespVO = { ...sampleSettings, netTimeoutSec: 60, netProxyMode: 2 };
		vi.mocked(invoke).mockResolvedValueOnce(edited);

		const got = await settingsSave(edited);

		expect(got).toEqual(edited);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('settings_save', { settings: edited });
	});

	it('appVersion 应以 command 名 app_version 调用且不带参数, 返回版本号字符串', async () => {
		vi.mocked(invoke).mockResolvedValueOnce('0.1.0');

		const got = await appVersion();

		expect(got).toBe('0.1.0');
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('app_version');
	});
});
