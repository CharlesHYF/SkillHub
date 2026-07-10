// 文件作用: NetworkSection 组件单测(代理模式下拉/三个地址输入/超时数字输入的渲染与回调)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Settings } from '@/api/setting';
import { NetworkSection } from './network-section';

// jsdom 未实现 Pointer Capture 相关 API, 而 Radix Select 触发器的指针事件处理会调用它们;
// 仅在本测试文件内 polyfill 为 no-op, 不改动全局 vitest.setup.ts(其余组件测试不需要这些 API)
if (!Element.prototype.hasPointerCapture) {
	Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.setPointerCapture) {
	Element.prototype.setPointerCapture = () => {};
}
if (!Element.prototype.releasePointerCapture) {
	Element.prototype.releasePointerCapture = () => {};
}
if (!Element.prototype.scrollIntoView) {
	Element.prototype.scrollIntoView = () => {};
}

const baseSettings: Pick<
	Settings,
	'netProxyMode' | 'netHttpProxy' | 'netHttpsProxy' | 'netNoProxy' | 'netTimeoutSec'
> = {
	netProxyMode: 0,
	netHttpProxy: '',
	netHttpsProxy: '',
	netNoProxy: '',
	netTimeoutSec: 30,
};

function renderSection(overrides: Partial<React.ComponentProps<typeof NetworkSection>> = {}) {
	const onChange = vi.fn();
	render(<NetworkSection settings={baseSettings} onChange={onChange} {...overrides} />);
	return { onChange };
}

describe('NetworkSection', () => {
	it('应渲染标题、代理模式下拉与四个输入项', () => {
		renderSection();
		expect(screen.getByText('网络与代理 Network')).toBeInTheDocument();
		expect(screen.getByText('代理模式')).toBeInTheDocument();
		expect(screen.getByText('系统默认')).toBeInTheDocument();
		expect(screen.getByLabelText('HTTP 代理')).toBeInTheDocument();
		expect(screen.getByLabelText('HTTPS 代理')).toBeInTheDocument();
		expect(screen.getByLabelText('不使用代理的地址')).toBeInTheDocument();
		expect(screen.getByLabelText('请求超时(秒)')).toBeInTheDocument();
	});

	it('切换代理模式为"手动"应以 netProxyMode=2 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.click(screen.getByRole('combobox'));
		await user.click(await screen.findByRole('option', { name: '手动' }));

		expect(onChange).toHaveBeenCalledWith({ netProxyMode: 2 });
	});

	it('在 HTTP 代理输入框中输入应以 netHttpProxy 调用 onChange', async () => {
		const user = userEvent.setup();
		const { onChange } = renderSection();

		await user.type(screen.getByLabelText('HTTP 代理'), 'a');

		expect(onChange).toHaveBeenCalledWith({ netHttpProxy: 'a' });
	});

	it('修改请求超时输入应以数字 netTimeoutSec 调用 onChange', () => {
		const { onChange } = renderSection();

		// 受控输入在本组件测试里由静态 props 提供 value, 不经父级状态回灌; 用 fireEvent 直接
		// 派发一次目标值的 change 事件, 避免 userEvent 逐字符输入时每次按键之间被重渲染重置
		// (与 export-panel/import-panel 对纯展示输入框仅断言"回调被调用"而非累积值同一顾虑)
		fireEvent.change(screen.getByLabelText('请求超时(秒)'), { target: { value: '60' } });

		expect(onChange).toHaveBeenLastCalledWith({ netTimeoutSec: 60 });
	});
});
