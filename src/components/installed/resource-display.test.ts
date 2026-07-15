// 文件作用: 已安装界面资源展示派生逻辑单测(来源文案/描述兜底/类型与状态映射)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import type { ResourceRespVO } from '@/api/library';
import {
	SOURCE_LABEL,
	deriveDescription,
	toResourceKind,
	deriveSyncStatus,
} from './resource-display';

function makeResource(overrides: Partial<ResourceRespVO> = {}): ResourceRespVO {
	return {
		id: 1,
		resType: 'Skill',
		name: 'demo-skill',
		displayName: 'demo-skill',
		version: '1.0.0',
		sourceType: 'LocalImport',
		localPath: '/tmp/demo-skill',
		enabled: true,
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

describe('SOURCE_LABEL', () => {
	it('应覆盖三种来源的中文文案', () => {
		expect(SOURCE_LABEL.LocalImport).toBe('本地导入');
		expect(SOURCE_LABEL.Official).toBe('官方仓库');
		expect(SOURCE_LABEL.ThirdParty).toBe('第三方仓库');
	});
});

describe('deriveDescription', () => {
	it('displayName 与 name 相同时应返回 undefined(不重复展示)', () => {
		expect(deriveDescription(makeResource())).toBeUndefined();
	});

	it('displayName 与 name 不同时应返回 displayName 作为描述', () => {
		expect(deriveDescription(makeResource({ displayName: '数据可视化工具集合' }))).toBe(
			'数据可视化工具集合',
		);
	});
});

describe('toResourceKind', () => {
	it('Skill 应映射为小写 skill', () => {
		expect(toResourceKind('Skill')).toBe('skill');
	});

	it('Mcp 应映射为小写 mcp', () => {
		expect(toResourceKind('Mcp')).toBe('mcp');
	});
});

describe('deriveSyncStatus', () => {
	it('enabled=false 应映射为已禁用', () => {
		expect(deriveSyncStatus(false)).toBe('已禁用');
	});

	it('enabled=true 应映射为已同步', () => {
		expect(deriveSyncStatus(true)).toBe('已同步');
	});
});
