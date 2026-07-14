// 文件作用: UpdateChannelSection 组件单测(Stable/Beta 单选渲染与切换回调)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
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

	// 真实浏览器里 after: 伪元素必须显式声明 content 才会生成盒子, 缺失时选中态圆点不可见;
	// jsdom 不渲染伪元素测不出这一点, 故退而求其次在"指示点元素存在 + 类名带 content-['']"这一
	// 层面锁定回归(见 ui/radio-group.tsx 的 after:content-[''])
	it("选中项应渲染选中态指示点(带 content-[''] 类名), 未选中项不应渲染指示点", () => {
		const { container } = render(
			<UpdateChannelSection settings={{ updateChannel: 0 }} onChange={vi.fn()} />,
		);

		const indicator = container.querySelector('[data-slot="radio-group-indicator"]');
		expect(indicator).not.toBeNull();
		expect(indicator).toHaveClass("after:content-['']");

		// 未选中项(Beta)不应渲染指示点(Radix 默认只在选中态挂载 Indicator)
		const betaRadio = screen.getByRole('radio', { name: 'Beta (测试版)' });
		expect(betaRadio.querySelector('[data-slot="radio-group-indicator"]')).toBeNull();
	});
});
