// 文件作用: DetailPanel 渲染与交互单测(标题/内容可见, 关闭按钮触发回调)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { DetailPanel } from './detail-panel';

describe('DetailPanel', () => {
	it('应显示标题与内容插槽', () => {
		render(
			<DetailPanel title="资源详情" onClose={() => {}}>
				<p>正文内容</p>
			</DetailPanel>,
		);
		expect(screen.getByText('资源详情')).toBeInTheDocument();
		expect(screen.getByText('正文内容')).toBeInTheDocument();
	});

	it('点击关闭按钮应触发 onClose', () => {
		const onClose = vi.fn();
		render(<DetailPanel title="资源详情" onClose={onClose} />);
		fireEvent.click(screen.getByRole('button', { name: '关闭' }));
		expect(onClose).toHaveBeenCalledTimes(1);
	});
});
