// 文件作用: DataTable 渲染与交互单测(渲染行/自定义 render 列/行点击回调)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { DataTable, type DataTableColumn } from './data-table';

interface Row {
	id: number;
	name: string;
}

const columns: DataTableColumn<Row>[] = [
	{ key: 'name', header: '名称' },
	{ key: 'tag', header: '标记', render: (row) => `#${row.id}` },
];

const rows: Row[] = [
	{ id: 1, name: 'Alpha' },
	{ id: 2, name: 'Beta' },
];

describe('DataTable', () => {
	it('应渲染表头与每行数据(含自定义 render 列)', () => {
		render(<DataTable columns={columns} rows={rows} rowKey={(r) => r.id} />);
		expect(screen.getByText('名称')).toBeInTheDocument();
		expect(screen.getByText('标记')).toBeInTheDocument();
		expect(screen.getByText('Alpha')).toBeInTheDocument();
		expect(screen.getByText('Beta')).toBeInTheDocument();
		expect(screen.getByText('#1')).toBeInTheDocument();
		expect(screen.getByText('#2')).toBeInTheDocument();
	});

	it('点击行应触发 onRowClick 并回传该行数据', () => {
		const onRowClick = vi.fn();
		render(
			<DataTable
				columns={columns}
				rows={rows}
				rowKey={(r) => r.id}
				onRowClick={onRowClick}
			/>,
		);
		fireEvent.click(screen.getByText('Beta'));
		expect(onRowClick).toHaveBeenCalledWith(rows[1]);
	});
});
