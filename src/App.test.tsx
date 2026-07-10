// 文件作用: App 根组件集成测试(mock src/api) —— 应用启动时应各自动触发一次 agent_detect 与
//           market_refresh(fire-and-forget), 且在 StrictMode 的双重挂载模拟下仍只各触发一次
//           (不形成重复触发循环), 修复此前"进入即空, 必须先手动点刷新"的问题(M5 Task F1)
// 创建日期: 2026-07-10
import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

vi.mock('@/api/agent', () => ({
	agentDetect: vi.fn(),
	agentList: vi.fn(),
}));
vi.mock('@/api/market', () => ({
	marketRefresh: vi.fn(),
}));
vi.mock('@/api/dashboard', () => ({
	dashboardSummary: vi.fn(),
	activityRecent: vi.fn(),
}));

import { agentDetect, agentList } from '@/api/agent';
import { marketRefresh } from '@/api/market';
import { dashboardSummary, activityRecent } from '@/api/dashboard';

describe('App 根组件', () => {
	beforeEach(() => {
		vi.mocked(agentDetect).mockReset().mockResolvedValue([]);
		vi.mocked(agentList).mockReset().mockResolvedValue([]);
		vi.mocked(marketRefresh).mockReset().mockResolvedValue({ count: 0 });
		vi.mocked(dashboardSummary).mockReset().mockResolvedValue({
			skillCount: 0,
			mcpCount: 0,
			agentCount: 0,
			onlineCount: 0,
			pendingCount: 0,
		});
		vi.mocked(activityRecent).mockReset().mockResolvedValue([]);
	});

	it('挂载后应各触发一次 agent_detect 与 market_refresh(启动自动初始化), 且不形成重复触发循环', async () => {
		// 用 StrictMode 包裹, 复现开发环境下的"挂载->卸载->重新挂载"双重调用模拟, 验证
		// initializedRef 守卫在这种场景下仍能保证只各触发一次(而非"至少一次"这种弱断言)
		render(
			<React.StrictMode>
				<App />
			</React.StrictMode>,
		);

		expect(await screen.findByText('首页 / Dashboard')).toBeInTheDocument();
		await vi.waitFor(() => {
			expect(agentDetect).toHaveBeenCalledTimes(1);
			expect(marketRefresh).toHaveBeenCalledTimes(1);
		});
	});
});
