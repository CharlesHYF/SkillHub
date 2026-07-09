// 文件作用: StatCard 渲染单测(label/value/hint 可见)
// 创建日期: 2026-07-09
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Box } from 'lucide-react';
import { StatCard } from './stat-card';

describe('StatCard', () => {
	it('应显示 label 与 value', () => {
		render(<StatCard icon={Box} label="Skill 总数" value={12} />);
		expect(screen.getByText('Skill 总数')).toBeInTheDocument();
		expect(screen.getByText('12')).toBeInTheDocument();
	});

	it('提供 hint 时应显示 hint', () => {
		render(<StatCard icon={Box} label="待同步" value={3} hint="较昨日 +1" />);
		expect(screen.getByText('较昨日 +1')).toBeInTheDocument();
	});
});
