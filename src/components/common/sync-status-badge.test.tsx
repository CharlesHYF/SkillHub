// 文件作用: SyncStatusBadge 渲染单测(5 种状态文案可辨 + 语义色落在圆点上)
// 创建日期: 2026-07-09
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SyncStatusBadge, type SyncStatus } from './sync-status-badge';

describe('SyncStatusBadge', () => {
	const cases: Array<[SyncStatus, string]> = [
		['已同步', 'var(--sh-ok)'],
		['待同步', 'var(--sh-warn)'],
		['失败', 'var(--sh-danger)'],
		['本地修改', 'var(--sh-muted)'],
		['已禁用', 'var(--sh-muted)'],
		// 以下 4 种供 Sync Center 的 Agent 在线状态列复用(见 components/sync/agent-display.ts
		// 的 deriveAgentSyncStatus), 与上面 5 种资源同步状态共用同一套徽标渲染逻辑
		['在线', 'var(--sh-ok)'],
		['部分同步', 'var(--sh-warn)'],
		['同步失败', 'var(--sh-danger)'],
		['离线', 'var(--sh-muted)'],
		// 以下 2 种供导入导出历史表复用(见 components/portability/impexp-display.ts)
		['成功', 'var(--sh-ok)'],
		['部分成功', 'var(--sh-warn)'],
	];

	it.each(cases)('status=%s 应渲染对应文案且圆点用对应语义色', (status, expectedColor) => {
		render(<SyncStatusBadge status={status} />);
		expect(screen.getByText(status)).toBeInTheDocument();
		const dot = screen.getByTestId('sync-status-dot');
		expect(dot.style.background).toBe(expectedColor);
	});
});
