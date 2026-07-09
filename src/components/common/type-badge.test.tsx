// 文件作用: TypeBadge 渲染单测(Skill/MCP 文案可辨)
// 创建日期: 2026-07-09
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TypeBadge } from './type-badge';

describe('TypeBadge', () => {
	it('type=skill 应渲染 Skill 文案', () => {
		render(<TypeBadge type="skill" />);
		expect(screen.getByText('Skill')).toBeInTheDocument();
	});

	it('type=mcp 应渲染 MCP 文案', () => {
		render(<TypeBadge type="mcp" />);
		expect(screen.getByText('MCP')).toBeInTheDocument();
	});
});
