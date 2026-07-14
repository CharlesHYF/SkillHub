// 文件作用: 主题 Provider 单测
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThemeProvider, useTheme } from './theme-provider';

function Probe() {
	const { theme, toggle } = useTheme();
	return (
		<button onClick={toggle} data-testid="btn">
			{theme}
		</button>
	);
}

describe('ThemeProvider', () => {
	beforeEach(() => document.documentElement.removeAttribute('data-theme'));

	it('默认在根元素写入 data-theme', () => {
		render(
			<ThemeProvider>
				<Probe />
			</ThemeProvider>,
		);
		expect(document.documentElement.getAttribute('data-theme')).toMatch(/light|dark/);
	});

	it('toggle 在亮暗之间切换', () => {
		render(
			<ThemeProvider>
				<Probe />
			</ThemeProvider>,
		);
		const before = screen.getByTestId('btn').textContent;
		fireEvent.click(screen.getByTestId('btn'));
		const after = screen.getByTestId('btn').textContent;
		expect(after).not.toBe(before);
		expect(document.documentElement.getAttribute('data-theme')).toBe(after);
	});
});
