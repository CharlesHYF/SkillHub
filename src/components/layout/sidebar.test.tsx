// 文件作用: 侧栏渲染与导航单测
// 创建日期: 2026-07-09
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '../../i18n';
import { Sidebar } from './sidebar';
import { NAV_ITEMS } from './nav-config';

describe('Sidebar', () => {
	it('渲染全部导航项(中文文案)', () => {
		render(
			<MemoryRouter>
				<Sidebar />
			</MemoryRouter>,
		);
		expect(screen.getByText('首页')).toBeInTheDocument();
		expect(screen.getByText('资源中心')).toBeInTheDocument();
		expect(screen.getAllByRole('link')).toHaveLength(NAV_ITEMS.length);
	});
});
