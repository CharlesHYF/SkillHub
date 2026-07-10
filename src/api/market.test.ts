// 文件作用: market api 层单测
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => []),
}));

import {
	marketSearch,
	marketDetail,
	marketRefresh,
	marketInstall,
	type MarketResource,
} from './market';

const sampleMarketResource: MarketResource = {
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
};

describe('market api', () => {
	it('marketSearch 应以 command 名 market_search 调用, 缺省字段转 null', async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ items: [sampleMarketResource], total: 1 });
		const result = await marketSearch({ sort: 0, page: 1, pageSize: 10 });
		expect(result).toEqual({ items: [sampleMarketResource], total: 1 });
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('market_search', {
			keyword: null,
			resType: null,
			category: null,
			sort: 0,
			page: 1,
			pageSize: 10,
		});
	});

	it('marketSearch 应把 keyword/resType/category 原样传给后端', async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ items: [], total: 0 });
		await marketSearch({
			keyword: 'demo',
			resType: 2,
			category: 'productivity',
			sort: 1,
			page: 2,
			pageSize: 20,
		});
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('market_search', {
			keyword: 'demo',
			resType: 2,
			category: 'productivity',
			sort: 1,
			page: 2,
			pageSize: 20,
		});
	});

	it('marketDetail 应以 command 名 market_detail 调用并传 sourceType/extId', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(sampleMarketResource);
		const got = await marketDetail(1, 'acme/skills:demo');
		expect(got).toEqual(sampleMarketResource);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('market_detail', {
			sourceType: 1,
			extId: 'acme/skills:demo',
		});
	});

	it('marketDetail 查无结果时应原样返回 null', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(null);
		const got = await marketDetail(1, 'nope');
		expect(got).toBeNull();
	});

	it('marketRefresh 应以 command 名 market_refresh 调用且不带参数', async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ count: 3 });
		const result = await marketRefresh();
		expect(result).toEqual({ count: 3 });
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('market_refresh');
	});

	it('marketInstall 应以 command 名 market_install 调用并传 sourceType/extId, envOverrides 缺省为 null', async () => {
		const installedResource = {
			id: 9,
			resType: 'Skill',
			name: 'demo-skill',
			displayName: 'demo-skill',
			version: '1.0.0',
			sourceType: 'Official',
			localPath: '/tmp/demo-skill',
			enabled: true,
			createTime: '2026-07-10 00:00:00',
			updateTime: '2026-07-10 00:00:00',
		};
		vi.mocked(invoke).mockResolvedValueOnce(installedResource);
		const got = await marketInstall(1, 'acme/skills:demo');
		expect(got).toEqual(installedResource);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('market_install', {
			sourceType: 1,
			extId: 'acme/skills:demo',
			envOverrides: null,
		});
	});

	it('marketInstall 应把 envOverrides 原样传给后端', async () => {
		vi.mocked(invoke).mockResolvedValueOnce({});
		await marketInstall(2, 'demo/mcp-server', { API_KEY: 'secret' });
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('market_install', {
			sourceType: 2,
			extId: 'demo/mcp-server',
			envOverrides: { API_KEY: 'secret' },
		});
	});
});
