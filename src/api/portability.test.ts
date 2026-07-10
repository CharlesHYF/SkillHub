// 文件作用: portability api 层单测
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(),
}));

import {
	exportBundle,
	importPreview,
	importBundle,
	impexpHistory,
	type ExportOptions,
	type ImportPreview,
	type ImportOutcome,
	type ImpexpRow,
	type Manifest,
} from './portability';

const baseOptions: ExportOptions = {
	includeSkills: true,
	includeMcp: true,
	scope: 0,
	format: 1,
	includeConfig: true,
	includeVersionLock: true,
};

describe('portability api', () => {
	it('exportBundle 以 command 名 export_bundle 调用并传 options/outPath, 返回 Manifest', async () => {
		const manifest: Manifest = {
			schemaVersion: 1,
			exportedAt: '2026-07-10 00:00:00',
			counts: { skill: 128, mcp: 45, config: 23, agent: 8 },
			checksums: { 'skills/data-visualizer/SKILL.md': 'abc123' },
		};
		vi.mocked(invoke).mockResolvedValueOnce(manifest);

		const got = await exportBundle(baseOptions, '/tmp/skillhub_backup.zip');

		expect(got).toEqual(manifest);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('export_bundle', {
			options: baseOptions,
			outPath: '/tmp/skillhub_backup.zip',
		});
	});

	it('importPreview 以 command 名 import_preview 调用并传 path, 返回预览计数', async () => {
		const preview: ImportPreview = {
			skill: 128,
			mcp: 45,
			config: 23,
			agent: 8,
			schemaOk: true,
		};
		vi.mocked(invoke).mockResolvedValueOnce(preview);

		const got = await importPreview('/tmp/skillhub_backup.zip');

		expect(got).toEqual(preview);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('import_preview', {
			path: '/tmp/skillhub_backup.zip',
		});
	});

	it('importBundle 以 command 名 import_bundle 调用并传 path/strategy/autoSync, 返回结果', async () => {
		const outcome: ImportOutcome = {
			imported: 128,
			skipped: 0,
			renamed: 0,
			status: 1,
		};
		vi.mocked(invoke).mockResolvedValueOnce(outcome);

		const got = await importBundle('/tmp/skillhub_backup.zip', 1, true);

		expect(got).toEqual(outcome);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('import_bundle', {
			path: '/tmp/skillhub_backup.zip',
			strategy: 1,
			autoSync: true,
		});
	});

	it('impexpHistory 以 command 名 impexp_history 调用并传 limit, 返回历史行', async () => {
		const rows: ImpexpRow[] = [
			{
				id: 1,
				direction: 0,
				fileName: 'skillhub_backup_2024-05-23.zip',
				fileFormat: 1,
				summary: 'Skill 128 · MCP 45 · 配置 23 · Agent 8',
				status: 1,
				runTime: '2026-07-10 00:00:00',
			},
		];
		vi.mocked(invoke).mockResolvedValueOnce(rows);

		const got = await impexpHistory(20);

		expect(got).toEqual(rows);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('impexp_history', { limit: 20 });
	});
});
