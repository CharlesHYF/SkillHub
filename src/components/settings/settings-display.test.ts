// 文件作用: 设置(Settings)界面展示态可选项文案单测(措辞与原型截图第 7 屏一致)
// 创建日期: 2026-07-10
import { describe, it, expect } from 'vitest';
import { PROXY_MODE_OPTIONS, UPDATE_CHANNEL_OPTIONS } from './settings-display';

describe('settings-display', () => {
	it('PROXY_MODE_OPTIONS 应含 3 项(系统默认/不使用/手动), 措辞与原型一致', () => {
		expect(PROXY_MODE_OPTIONS).toEqual([
			{ value: 0, label: '系统默认' },
			{ value: 1, label: '不使用' },
			{ value: 2, label: '手动' },
		]);
	});

	it('UPDATE_CHANNEL_OPTIONS 应含 2 项(Stable/Beta)且带说明文案, 措辞与原型一致', () => {
		expect(UPDATE_CHANNEL_OPTIONS).toEqual([
			{
				value: 0,
				label: 'Stable (稳定版)',
				description: '推荐用于生产环境, 提供稳定可靠的功能',
			},
			{
				value: 1,
				label: 'Beta (测试版)',
				description: '提前体验新功能, 可能包含未完全稳定的特性',
			},
		]);
	});
});
