// 文件作用: Button 组件单测(ref 转发) —— 验证 React.forwardRef 修复后, 传给 Button 的 ref 能
//           落到真实 <button> DOM 节点上(asChild=false), 以及 asChild=true 时落到被合并的子
//           元素上(而非丢失/停留在 null); 修复前 Button 是普通函数组件, 传入的 ref 会被 React
//           丢弃并在控制台告警
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as React from 'react';
import { Button } from './button';

describe('Button', () => {
	it('ref 应转发到真实的 <button> DOM 节点', () => {
		const ref = React.createRef<HTMLButtonElement>();
		render(<Button ref={ref}>点击</Button>);

		expect(ref.current).toBeInstanceOf(HTMLButtonElement);
		expect(ref.current?.textContent).toBe('点击');
	});

	it('asChild=true 时, 传给 Button 的 ref 应转发到被合并的子元素上', () => {
		// Button 的公开 ref 类型固定为 HTMLButtonElement(asChild 场景下 Slot 实际会把 ref 转发到
		// 被合并的子元素, 与 Button 声明的 ref 类型不完全一致, 这是该 asChild 模式固有的类型
		// 局限, 不属于本次 forwardRef 修复的范围); 这里仍按 Button 声明的类型创建 ref, 只在断言
		// 阶段用运行时检查验证其真实落到的 DOM 节点
		const ref = React.createRef<HTMLButtonElement>();
		render(
			<Button asChild ref={ref}>
				<a href="#test">链接</a>
			</Button>,
		);

		expect(ref.current).toBeInstanceOf(HTMLAnchorElement);
		expect(ref.current?.tagName).toBe('A');
	});

	it('应设置 displayName 便于调试与 React DevTools 识别', () => {
		expect(Button.displayName).toBe('Button');
	});

	it('variant/className/onClick 等既有行为不应被 ref 转发改动影响', async () => {
		const onClick = vi.fn();
		render(
			<Button variant="outline" className="custom-cls" onClick={onClick}>
				确定
			</Button>,
		);

		const btn = screen.getByRole('button', { name: '确定' });
		expect(btn).toHaveAttribute('data-variant', 'outline');
		expect(btn.className).toContain('custom-cls');

		await userEvent.click(btn);
		expect(onClick).toHaveBeenCalledTimes(1);
	});
});
