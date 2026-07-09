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
	];

	it.each(cases)('status=%s 应渲染对应文案且圆点用对应语义色', (status, expectedColor) => {
		render(<SyncStatusBadge status={status} />);
		expect(screen.getByText(status)).toBeInTheDocument();
		const dot = screen.getByTestId('sync-status-dot');
		expect(dot.style.background).toBe(expectedColor);
	});
});
