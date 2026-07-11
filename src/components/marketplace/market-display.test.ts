// 文件作用: Marketplace 展示态派生逻辑单测(来源编码互转/星标数格式化/安装要求/认证说明)
// 创建日期: 2026-07-10
import { describe, it, expect } from 'vitest';
import type { MarketResource } from '@/api/market';
import {
	sourceTypeToCode,
	toResourceKind,
	formatStars,
	formatVersion,
	formatUpdatedAt,
	formatCategory,
	deriveInstallRequirements,
	deriveAuthNotice,
	marketResourceKey,
} from './market-display';

function makeMarketResource(overrides: Partial<MarketResource> = {}): MarketResource {
	return {
		sourceType: 'GithubSkills',
		resType: 'Skill',
		extId: 'acme/skills:demo',
		name: 'demo-skill',
		displayName: 'Demo Skill',
		description: '一个示例 Skill',
		author: 'acme',
		version: '1.0.0',
		stars: 42,
		category: 'productivity',
		tags: ['demo', 'sample'],
		authRequired: false,
		installManifest: { Skill: { repo: 'acme/skills', path: 'skills/demo', gitRef: 'main' } },
		updatedAt: '2026-07-01 00:00:00',
		...overrides,
	};
}

describe('sourceTypeToCode', () => {
	it('应把 SourceId 字符串变体转回后端 i64 编码', () => {
		expect(sourceTypeToCode('GithubSkills')).toBe(1);
		expect(sourceTypeToCode('McpRegistry')).toBe(2);
		expect(sourceTypeToCode('GithubMcp')).toBe(3);
	});
});

describe('toResourceKind', () => {
	it('应把后端 ResourceType 转为 TypeBadge 所需的小写 ResourceKind', () => {
		expect(toResourceKind('Skill')).toBe('skill');
		expect(toResourceKind('Mcp')).toBe('mcp');
	});
});

describe('formatStars', () => {
	it('小于 1000 时原样展示整数', () => {
		expect(formatStars(0)).toBe('0');
		expect(formatStars(42)).toBe('42');
		expect(formatStars(900)).toBe('900');
	});

	it('大于等于 1000 时用 k 后缀并保留 1 位小数, 去掉多余的 .0', () => {
		expect(formatStars(1000)).toBe('1k');
		expect(formatStars(12300)).toBe('12.3k');
		expect(formatStars(12340)).toBe('12.3k');
	});

	it('大于等于 100 万时用 m 后缀', () => {
		expect(formatStars(1_000_000)).toBe('1m');
		expect(formatStars(2_500_000)).toBe('2.5m');
	});
});

describe('formatVersion', () => {
	it('有版本号时应加 v 前缀', () => {
		expect(formatVersion('1.2.0')).toBe('v1.2.0');
	});

	it('空版本号时应展示占位符, 不展示裸 "v-"', () => {
		expect(formatVersion('')).toBe('—');
	});
});

describe('formatUpdatedAt', () => {
	it('有值时应复用 formatDateTime 裁剪到分钟精度', () => {
		expect(formatUpdatedAt('2026-07-01 12:30:00')).toBe('2026-07-01 12:30');
	});

	it('空值时应展示占位符, 不展示"更新于: "后拖一段空白', () => {
		expect(formatUpdatedAt('')).toBe('—');
	});
});

describe('formatCategory', () => {
	it('有值时原样展示', () => {
		expect(formatCategory('productivity')).toBe('productivity');
	});

	it('空值时应展示占位符', () => {
		expect(formatCategory('')).toBe('—');
	});
});

describe('deriveInstallRequirements', () => {
	it('Skill 变体应展示来源仓库/子目录/版本引用', () => {
		const lines = deriveInstallRequirements(
			makeMarketResource({
				installManifest: {
					Skill: { repo: 'acme/skills', path: 'skills/demo', gitRef: 'v1.0.0' },
				},
			}),
		);
		expect(lines).toEqual(['来源仓库: acme/skills', '子目录: skills/demo', '版本引用: v1.0.0']);
	});

	it('Mcp 变体应展示启动命令(含参数)', () => {
		const lines = deriveInstallRequirements(
			makeMarketResource({
				resType: 'Mcp',
				installManifest: {
					Mcp: {
						serverDef: {
							name: 'filesystem',
							command: 'npx',
							args: ['-y', 'server-fs'],
							env: {},
							url: null,
						},
					},
				},
			}),
		);
		expect(lines).toEqual(['启动命令: npx -y server-fs']);
	});

	it('Mcp 变体为远程地址时应展示远程地址', () => {
		const lines = deriveInstallRequirements(
			makeMarketResource({
				resType: 'Mcp',
				installManifest: {
					Mcp: {
						serverDef: {
							name: 'remote-mcp',
							command: null,
							args: [],
							env: {},
							url: 'https://example.com/mcp',
						},
					},
				},
			}),
		);
		expect(lines).toEqual(['远程地址: https://example.com/mcp']);
	});

	it('McpTemplate 变体应额外展示需配置的环境变量', () => {
		const lines = deriveInstallRequirements(
			makeMarketResource({
				resType: 'Mcp',
				installManifest: {
					McpTemplate: {
						serverDef: {
							name: 'templated',
							command: 'npx',
							args: ['-y', 'templated-mcp'],
							env: {},
							url: null,
						},
						requiredEnv: ['API_KEY', 'API_SECRET'],
					},
				},
			}),
		);
		expect(lines).toEqual([
			'启动命令: npx -y templated-mcp',
			'需配置环境变量: API_KEY, API_SECRET',
		]);
	});
});

describe('marketResourceKey', () => {
	it('应由 sourceType + extId 拼接出复合唯一键', () => {
		const resource = makeMarketResource({ sourceType: 'McpRegistry', extId: 'demo/mcp' });
		expect(marketResourceKey(resource)).toBe('McpRegistry:demo/mcp');
	});
});

describe('deriveAuthNotice', () => {
	it('authRequired 为真时应展示需要登录或授权的说明', () => {
		expect(deriveAuthNotice(makeMarketResource({ authRequired: true }))).toBe(
			'部分功能需要授权访问第三方服务, 若需要登录或授权, 将在 SkillHub 内部打开完成。',
		);
	});

	it('authRequired 为假时应展示无需登录或授权的说明', () => {
		expect(deriveAuthNotice(makeMarketResource({ authRequired: false }))).toBe(
			'该资源无需登录或授权, 可直接下载安装。',
		);
	});
});
