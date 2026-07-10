// 文件作用: 泛型数据表(列配置驱动), 支持可选行点击/选中/斑马纹, hover 行由 Table 基元自带
// 创建日期: 2026-07-09
import type { ReactNode } from 'react';
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '@/components/ui/table';
import { cn } from '@/lib/utils';

/** 一列的配置: key 用于 React key 与(无 render 时的)取值, header 为表头内容, render 可选自定义渲染 */
export interface DataTableColumn<T> {
	key: string;
	header: ReactNode;
	render?: (row: T) => ReactNode;
}

interface DataTableProps<T> {
	columns: DataTableColumn<T>[];
	rows: T[];
	/** 取行的唯一 key, 用于 React key 与选中态比对 */
	rowKey: (row: T) => string | number;
	/** 行点击回调; 提供时该行会有 hover 手型光标 */
	onRowClick?: (row: T) => void;
	/** 当前选中行的 key; 命中时用品牌轻染背景高亮(呼应 DESIGN.md 的"当前项/选中行") */
	selectedRowKey?: string | number;
	/** 是否开启斑马纹(偶数行加中性轻染背景) */
	zebra?: boolean;
}

/** 泛型数据表: 列配置驱动渲染, 可选行点击/选中态/斑马纹 */
export function DataTable<T>({
	columns,
	rows,
	rowKey,
	onRowClick,
	selectedRowKey,
	zebra = false,
}: DataTableProps<T>) {
	return (
		<Table>
			<TableHeader>
				<TableRow>
					{columns.map((col) => (
						<TableHead key={col.key}>{col.header}</TableHead>
					))}
				</TableRow>
			</TableHeader>
			<TableBody>
				{rows.map((row) => {
					const key = rowKey(row);
					const selected = selectedRowKey !== undefined && key === selectedRowKey;
					return (
						<TableRow
							key={key}
							data-state={selected ? 'selected' : undefined}
							onClick={onRowClick ? () => onRowClick(row) : undefined}
							className={cn(
								onRowClick && 'cursor-pointer',
								zebra && 'even:bg-muted/50',
							)}
							style={selected ? { background: 'var(--sh-brand-tint)' } : undefined}
						>
							{columns.map((col) => (
								<TableCell key={col.key}>
									{col.render
										? col.render(row)
										: String((row as Record<string, unknown>)[col.key] ?? '')}
								</TableCell>
							))}
						</TableRow>
					);
				})}
			</TableBody>
		</Table>
	);
}
