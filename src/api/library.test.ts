// 文件作用: library api 层单测
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => []),
}));

import {
	libraryList,
	libraryGet,
	libraryCounts,
	resourceImportLocal,
	resourceSetEnabled,
	resourceDelete,
	type Resource,
} from './library';

const sampleResource: Resource = {
	id: 1,
	resType: 'Skill',
	name: 'demo-skill',
	displayName: 'Demo Skill',
	version: '1.0.0',
	sourceType: 'LocalImport',
	localPath: '/tmp/demo-skill',
	enabled: true,
	createTime: '2026-07-09 00:00:00',
	updateTime: '2026-07-09 00:00:00',
};

describe('library api', () => {
	it('libraryList 缺省参数时以 command 名 library_list 调用, 过滤条件传 null', async () => {
		vi.mocked(invoke).mockResolvedValueOnce([sampleResource]);
		const list = await libraryList();
		expect(list).toEqual([sampleResource]);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('library_list', {
			resType: null,
			keyword: null,
		});
	});

	it('libraryList 应把 resType/keyword 原样传给后端', async () => {
		vi.mocked(invoke).mockResolvedValueOnce([]);
		await libraryList(2, 'demo');
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('library_list', {
			resType: 2,
			keyword: 'demo',
		});
	});

	it('libraryGet 以 command 名 library_get 调用并返回结果', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(sampleResource);
		const got = await libraryGet(1);
		expect(got).toEqual(sampleResource);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('library_get', { id: 1 });
	});

	it('libraryCounts 以 command 名 library_counts 调用', async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ skill: 3, mcp: 2 });
		const counts = await libraryCounts();
		expect(counts).toEqual({ skill: 3, mcp: 2 });
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('library_counts');
	});

	it('resourceImportLocal 以 command 名 resource_import_local 调用并传 path', async () => {
		vi.mocked(invoke).mockResolvedValueOnce(sampleResource);
		const got = await resourceImportLocal('/tmp/demo-skill');
		expect(got).toEqual(sampleResource);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('resource_import_local', {
			path: '/tmp/demo-skill',
		});
	});

	it('resourceSetEnabled 以 command 名 resource_set_enabled 调用并传 id/enabled', async () => {
		await resourceSetEnabled(1, false);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('resource_set_enabled', {
			id: 1,
			enabled: false,
		});
	});

	it('resourceDelete 以 command 名 resource_delete 调用并传 id', async () => {
		await resourceDelete(1);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('resource_delete', { id: 1 });
	});
});
