// 文件作用: StorageSection 组件单测(输入回调、浏览按钮可点击占位回调)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Settings } from '@/api/setting';
import { StorageSection } from './storage-section';

const baseSettings: Pick<Settings, 'storageSkillDir' | 'storageMcpDir'> = {
	storageSkillDir: '',
	storageMcpDir: '',
};

function renderSection(overrides: Partial<React.ComponentProps<typeof StorageSection>> = {}) {
	const onChange = vi.fn();
	const onBrowseSkillDir = vi.fn();
	const onBrowseMcpDir = vi.fn();
	render(
		<StorageSection
			settings={baseSettings}
			onChange={onChange}
			onBrowseSkillDir={onBrowseSkillDir}
			onBrowseMcpDir={onBrowseMcpDir}
			{...overrides}
		/>,
	);
	return { onChange, onBrowseSkillDir, onBrowseMcpDir };
}

describe('StorageSection', () => {
	it('应渲染标题与两个目录输入框', () => {
		renderSection();
		expect(screen.getByText('存储目录 Storage')).toBeInTheDocument();
		expect(screen.getByLabelText('本地 Skill 目录')).toBeInTheDocument();
		expect(screen.getByLabelText('本地 MCP 目录')).toBeInTheDocument();
	});

	it('在 Skill 目录输入框中输入应以 storageSkillDir 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.type(screen.getByLabelText('本地 Skill 目录'), 'a');

		expect(onChange).toHaveBeenCalledWith({ storageSkillDir: 'a' });
	});

	it('在 MCP 目录输入框中输入应以 storageMcpDir 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.type(screen.getByLabelText('本地 MCP 目录'), 'b');

		expect(onChange).toHaveBeenCalledWith({ storageMcpDir: 'b' });
	});

	it('两个"浏览"按钮均应可点击并各自调用对应占位回调', async () => {
		const user = userEvent.setup();
		const { onBrowseSkillDir, onBrowseMcpDir } = renderSection();

		const browseButtons = screen.getAllByRole('button', { name: '浏览' });
		expect(browseButtons).toHaveLength(2);
		expect(browseButtons[0]).toBeEnabled();
		expect(browseButtons[1]).toBeEnabled();

		await user.click(browseButtons[0]);
		await user.click(browseButtons[1]);

		expect(onBrowseSkillDir).toHaveBeenCalledTimes(1);
		expect(onBrowseMcpDir).toHaveBeenCalledTimes(1);
	});
});
