// 文件作用: Skeleton 及其组合(List/Table/Cards)渲染单测 —— 加载态以 role=status 暴露,
//           组合按传入的行/列/张数渲染对应数量的占位块
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Skeleton, SkeletonList, SkeletonTable, SkeletonCards } from './skeleton';

describe('Skeleton', () => {
	it('base Skeleton 应渲染一个脉冲占位块', () => {
		const { container } = render(<Skeleton className="h-4 w-10" />);
		const block = container.querySelector('.animate-pulse');
		expect(block).not.toBeNull();
	});

	it('SkeletonList 应以 role=status 暴露加载态, 并渲染指定行数', () => {
		render(<SkeletonList rows={3} />);
		const status = screen.getByRole('status', { name: '加载中' });
		expect(status.children).toHaveLength(3);
	});

	it('SkeletonTable 应以 role=status 暴露加载态, 并渲染指定行数', () => {
		render(<SkeletonTable rows={4} columns={5} />);
		const status = screen.getByRole('status', { name: '加载中' });
		expect(status.children).toHaveLength(4);
	});

	it('SkeletonCards 应以 role=status 暴露加载态, 并渲染指定张数', () => {
		render(<SkeletonCards count={6} />);
		const status = screen.getByRole('status', { name: '加载中' });
		expect(status.children).toHaveLength(6);
	});
});
