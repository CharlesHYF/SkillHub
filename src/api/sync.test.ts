// 文件作用: sync api 层单测
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => ({ success: 0, failed: 0, skipped: 0 })),
}));

vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async () => vi.fn()),
}));

import {
	assocSet,
	syncDiff,
	syncApply,
	onSyncProgress,
	type DiffPlan,
	type SyncSummary,
} from './sync';

describe('sync api', () => {
	it('assocSet 以 command 名 assoc_set 调用并传 resourceId/agentId/desired', async () => {
		await assocSet(10, 20, true);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('assoc_set', {
			resourceId: 10,
			agentId: 20,
			desired: true,
		});
	});

	it('syncDiff 以 command 名 sync_diff 调用并传 agentId, 返回差异计划', async () => {
		const plan: DiffPlan = { items: [] };
		vi.mocked(invoke).mockResolvedValueOnce(plan);
		const got = await syncDiff(20);
		expect(got).toEqual(plan);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('sync_diff', { agentId: 20 });
	});

	it('syncApply 以 command 名 sync_apply 调用并传 agentIds, 返回汇总结果', async () => {
		const summary: SyncSummary = { success: 2, failed: 0, skipped: 0 };
		vi.mocked(invoke).mockResolvedValueOnce(summary);
		const got = await syncApply([1, 2]);
		expect(got).toEqual(summary);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('sync_apply', { agentIds: [1, 2] });
	});

	it('onSyncProgress 订阅 "sync://progress" 频道并把事件 payload 转发给回调', async () => {
		const cb = vi.fn();
		await onSyncProgress(cb);

		expect(vi.mocked(listen)).toHaveBeenCalledWith('sync://progress', expect.any(Function));

		const handler = vi.mocked(listen).mock.calls[0][1] as (event: { payload: unknown }) => void;
		const payload = { agentId: 20, done: 1, total: 3, currentName: 'Claude Code' };
		handler({ payload });

		expect(cb).toHaveBeenCalledWith(payload);
	});
});
