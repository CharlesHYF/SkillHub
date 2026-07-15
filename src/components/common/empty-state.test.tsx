// 文件作用: EmptyState 渲染单测(标题/说明/图标/自动保鲜提示/行动区/尺寸变体)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Inbox } from 'lucide-react';
import { EmptyState } from './empty-state';

describe('EmptyState', () => {
	it('应显示标题', () => {
		render(<EmptyState icon={Inbox} title="暂无数据" />);
		expect(screen.getByText('暂无数据')).toBeInTheDocument();
	});

	it('提供 description 时应显示说明', () => {
		render(<EmptyState icon={Inbox} title="暂无数据" description="换个关键字再试试" />);
		expect(screen.getByText('换个关键字再试试')).toBeInTheDocument();
	});

	it('autoRefresh 为真时应显示自动保鲜提示', () => {
		render(<EmptyState icon={Inbox} title="暂无数据" autoRefresh />);
		expect(screen.getByText(/自动保持最新/)).toBeInTheDocument();
	});

	it('autoRefresh 默认关闭时不显示自动保鲜提示', () => {
		render(<EmptyState icon={Inbox} title="暂无数据" />);
		expect(screen.queryByText(/自动保持最新/)).not.toBeInTheDocument();
	});

	it('提供 action 时应渲染行动区内容', () => {
		render(<EmptyState icon={Inbox} title="暂无数据" action={<button>去添加</button>} />);
		expect(screen.getByRole('button', { name: '去添加' })).toBeInTheDocument();
	});
});
