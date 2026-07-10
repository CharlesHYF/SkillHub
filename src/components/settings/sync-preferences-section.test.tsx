// 文件作用: SyncPreferencesSection 组件单测(4 个开关的渲染文案与切换回调)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Settings } from '@/api/setting';
import { SyncPreferencesSection } from './sync-preferences-section';

const baseSettings: Pick<
	Settings,
	'syncAutoNewAgent' | 'syncCheckUpdateOnStart' | 'syncConflictPrompt' | 'syncOnlyEnabled'
> = {
	syncAutoNewAgent: true,
	syncCheckUpdateOnStart: true,
	syncConflictPrompt: true,
	syncOnlyEnabled: false,
};

function renderSection(
	overrides: Partial<React.ComponentProps<typeof SyncPreferencesSection>> = {},
) {
	const onChange = vi.fn();
	render(<SyncPreferencesSection settings={baseSettings} onChange={onChange} {...overrides} />);
	return { onChange };
}

describe('SyncPreferencesSection', () => {
	it('应渲染标题与四个开关及其说明文案', () => {
		renderSection();
		expect(screen.getByText('同步偏好 Sync Preferences')).toBeInTheDocument();
		expect(screen.getByText('自动同步到新 Agent')).toBeInTheDocument();
		expect(
			screen.getByText('当有新的 Agent 加入时, 自动同步已启用的 Skill 与 MCP'),
		).toBeInTheDocument();
		expect(screen.getByText('启动时检查更新')).toBeInTheDocument();
		expect(screen.getByText('冲突时提示')).toBeInTheDocument();
		expect(screen.getByText('仅同步已启用项')).toBeInTheDocument();
	});

	it('四个开关的初始受控态应与传入的 settings 一致', () => {
		renderSection();
		expect(screen.getByRole('switch', { name: '自动同步到新 Agent' })).toBeChecked();
		expect(screen.getByRole('switch', { name: '仅同步已启用项' })).not.toBeChecked();
	});

	it('切换"仅同步已启用项"应以 syncOnlyEnabled=true 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.click(screen.getByRole('switch', { name: '仅同步已启用项' }));

		expect(onChange).toHaveBeenCalledWith({ syncOnlyEnabled: true });
	});

	it('切换"冲突时提示"应以 syncConflictPrompt=false 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.click(screen.getByRole('switch', { name: '冲突时提示' }));

		expect(onChange).toHaveBeenCalledWith({ syncConflictPrompt: false });
	});
});
