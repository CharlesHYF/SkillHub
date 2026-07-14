// 文件作用: 导入导出展示态派生逻辑单测(状态码/方向码映射 + 可选项文案)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import {
	IMPEXP_STATUS_LABEL,
	DIRECTION_LABEL,
	DIRECTION_ICON,
	SCOPE_OPTIONS,
	FORMAT_OPTIONS,
	CONFLICT_STRATEGY_OPTIONS,
} from './impexp-display';

describe('impexp-display', () => {
	it('IMPEXP_STATUS_LABEL 应将 0/1/2 映射为 失败/成功/部分成功', () => {
		expect(IMPEXP_STATUS_LABEL[0]).toBe('失败');
		expect(IMPEXP_STATUS_LABEL[1]).toBe('成功');
		expect(IMPEXP_STATUS_LABEL[2]).toBe('部分成功');
	});

	it('DIRECTION_LABEL 应将 0/1 映射为 导出/导入', () => {
		expect(DIRECTION_LABEL[0]).toBe('导出');
		expect(DIRECTION_LABEL[1]).toBe('导入');
	});

	it('DIRECTION_ICON 应为 0/1 各提供一个图标组件', () => {
		expect(DIRECTION_ICON[0]).toBeDefined();
		expect(DIRECTION_ICON[1]).toBeDefined();
		expect(DIRECTION_ICON[0]).not.toBe(DIRECTION_ICON[1]);
	});

	it('SCOPE_OPTIONS 应含 3 项, 措辞与原型一致', () => {
		expect(SCOPE_OPTIONS.map((o) => o.label)).toEqual(['全部数据', '按类型选择', '按时间范围']);
		expect(SCOPE_OPTIONS.map((o) => o.value)).toEqual([0, 1, 2]);
	});

	it('FORMAT_OPTIONS 应含 3 项(zip/json/tar)', () => {
		expect(FORMAT_OPTIONS.map((o) => o.label)).toEqual(['zip', 'json', 'tar']);
		expect(FORMAT_OPTIONS.map((o) => o.value)).toEqual([1, 2, 3]);
	});

	it('CONFLICT_STRATEGY_OPTIONS 应含 3 项且带说明文案, 措辞与原型一致', () => {
		expect(CONFLICT_STRATEGY_OPTIONS).toEqual([
			{ value: 0, label: '覆盖 (推荐)', description: '覆盖已存在的同名项' },
			{ value: 1, label: '跳过', description: '跳过已存在的同名项' },
			{ value: 2, label: '保留两者', description: '重命名导入项以保留两者' },
		]);
	});
});
