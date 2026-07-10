// 文件作用: PageHeader 渲染单测(主标题为 h1/可选副标题/可选右侧操作区)
// 创建日期: 2026-07-10
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { PageHeader } from './page-header';

describe('PageHeader', () => {
	it('主标题应渲染为 level 1 标题', () => {
		render(<PageHeader title="首页 / Dashboard" />);
		expect(
			screen.getByRole('heading', { level: 1, name: '首页 / Dashboard' }),
		).toBeInTheDocument();
	});

	it('提供 description 时应显示副标题', () => {
		render(<PageHeader title="首页 / Dashboard" description="总览与同步状态" />);
		expect(screen.getByText('总览与同步状态')).toBeInTheDocument();
	});

	it('提供 actions 时应渲染右侧操作区', () => {
		render(<PageHeader title="首页 / Dashboard" actions={<button>操作</button>} />);
		expect(screen.getByRole('button', { name: '操作' })).toBeInTheDocument();
	});
});
