// 文件作用: api 层单测
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => ({ version: '0.1.0', dbOk: true })),
}));

import { appHealth } from './index';

describe('appHealth', () => {
	it('返回后端健康信息', async () => {
		const h = await appHealth();
		expect(h.version).toBe('0.1.0');
		expect(h.dbOk).toBe(true);
	});
});
