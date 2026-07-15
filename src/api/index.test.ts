// 文件作用: api 层单测
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => ({ version: '0.1.0', dbOk: true })),
}));

import { appHealth } from './index';

describe('appHealth', () => {
	it('以命令名 app_health 调用后端并返回健康信息', async () => {
		const h = await appHealth();
		expect(h.version).toBe('0.1.0');
		expect(h.dbOk).toBe(true);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('app_health');
	});
});
