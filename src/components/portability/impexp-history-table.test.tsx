// 文件作用: ImpexpHistoryTable 组件单测(渲染历史行/空态)
// 创建日期: 2026-07-10
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ImpexpRespVO } from '@/api/portability';
import { ImpexpHistoryTable } from './impexp-history-table';

function makeRow(overrides: Partial<ImpexpRespVO> = {}): ImpexpRespVO {
	return {
		id: 1,
		direction: 0,
		fileName: 'skillhub_backup_2024-05-23.zip',
		fileFormat: 1,
		summary: 'Skill 128 · MCP 45 · 配置 23 · Agent 8',
		status: 1,
		runTime: '2026-07-10 00:00:00',
		...overrides,
	};
}

describe('ImpexpHistoryTable', () => {
	it('应渲染标题与表头', () => {
		render(<ImpexpHistoryTable rows={[makeRow()]} />);
		expect(screen.getByText('导入导出历史 History')).toBeInTheDocument();
		expect(screen.getByText('文件名')).toBeInTheDocument();
		expect(screen.getByText('内容摘要')).toBeInTheDocument();
		expect(screen.getByText('状态')).toBeInTheDocument();
	});

	it('无记录时应展示空态文案', () => {
		render(<ImpexpHistoryTable rows={[]} />);
		expect(screen.getByText('暂无导入导出记录')).toBeInTheDocument();
	});

	it('应渲染文件名/类型/摘要/状态徽标', () => {
		const rows = [
			makeRow({ id: 1, direction: 0, fileName: 'skillhub_backup_2024-05-23.zip', status: 1 }),
			makeRow({
				id: 2,
				direction: 1,
				fileName: 'import_migration_2024-05-22.json',
				fileFormat: 2,
				summary: 'Skill 96 · MCP 30 · 配置 18 · Agent 6',
				status: 2,
			}),
		];
		render(<ImpexpHistoryTable rows={rows} />);

		expect(screen.getByText('skillhub_backup_2024-05-23.zip')).toBeInTheDocument();
		expect(screen.getByText('import_migration_2024-05-22.json')).toBeInTheDocument();
		expect(screen.getAllByText('导出')).toHaveLength(1);
		expect(screen.getAllByText('导入')).toHaveLength(1);
		expect(screen.getByText('Skill 96 · MCP 30 · 配置 18 · Agent 6')).toBeInTheDocument();
		expect(screen.getByText('成功')).toBeInTheDocument();
		expect(screen.getByText('部分成功')).toBeInTheDocument();
	});
});
