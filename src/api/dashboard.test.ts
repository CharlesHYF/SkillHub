// 文件作用: dashboard api 层单测
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => ({})),
}));

import {
	dashboardSummary,
	activityRecent,
	type DashboardSummaryRespVO,
	type ActivityRespVO,
} from './dashboard';

describe('dashboard api', () => {
	it('dashboardSummary 以 command 名 dashboard_summary 调用', async () => {
		const summary: DashboardSummaryRespVO = {
			skillCount: 3,
			mcpCount: 2,
			agentCount: 4,
			onlineCount: 3,
			pendingCount: 1,
		};
		vi.mocked(invoke).mockResolvedValueOnce(summary);
		const got = await dashboardSummary();
		expect(got).toEqual(summary);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('dashboard_summary');
	});

	it('activityRecent 以 command 名 activity_recent 调用并传 limit', async () => {
		const rows: ActivityRespVO[] = [
			{
				id: 1,
				actType: 1,
				resType: 1,
				title: '安装 x',
				detail: '',
				createTime: '2026-07-09',
			},
		];
		vi.mocked(invoke).mockResolvedValueOnce(rows);
		const got = await activityRecent(10);
		expect(got).toEqual(rows);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('activity_recent', { limit: 10 });
	});
});
