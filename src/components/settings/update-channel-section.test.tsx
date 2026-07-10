// 文件作用: UpdateChannelSection 组件单测(Stable/Beta 单选渲染与切换回调)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { UpdateChannelSection } from './update-channel-section';

function renderSection(overrides: Partial<React.ComponentProps<typeof UpdateChannelSection>> = {}) {
	const onChange = vi.fn();
	render(
		<UpdateChannelSection settings={{ updateChannel: 0 }} onChange={onChange} {...overrides} />,
	);
	return { onChange };
}

describe('UpdateChannelSection', () => {
	it('应渲染标题与 Stable/Beta 两个选项及说明文案', () => {
		renderSection();
		expect(screen.getByText('更新通道 Update Channel')).toBeInTheDocument();
		expect(screen.getByText('Stable (稳定版)')).toBeInTheDocument();
		expect(screen.getByText('推荐用于生产环境, 提供稳定可靠的功能')).toBeInTheDocument();
		expect(screen.getByText('Beta (测试版)')).toBeInTheDocument();
		expect(screen.getByText('提前体验新功能, 可能包含未完全稳定的特性')).toBeInTheDocument();
	});

	it('updateChannel=0 时 Stable 应为选中态', () => {
		renderSection({ settings: { updateChannel: 0 } });
		expect(screen.getByRole('radio', { name: 'Stable (稳定版)' })).toBeChecked();
		expect(screen.getByRole('radio', { name: 'Beta (测试版)' })).not.toBeChecked();
	});

	it('选择 Beta 应以 updateChannel=1 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.click(screen.getByRole('radio', { name: 'Beta (测试版)' }));

		expect(onChange).toHaveBeenCalledWith({ updateChannel: 1 });
	});
});
