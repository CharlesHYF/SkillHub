// 文件作用: 导入导出界面底部"导入导出历史 History"表 —— 操作/文件名/类型/内容摘要/状态/时间,
//           纯展示, 数据经 pages/portability 调 impexpHistory 获取
// 创建日期: 2026-07-10
import type { ImpexpRow } from '@/api/portability';
import { DataTable, type DataTableColumn } from '@/components/common/data-table';
import { SyncStatusBadge } from '@/components/common/sync-status-badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { formatRelativeTime } from '@/lib/utils';
import { DIRECTION_ICON, DIRECTION_LABEL, IMPEXP_STATUS_LABEL } from './impexp-display';

interface ImpexpHistoryTableProps {
	rows: ImpexpRow[];
}

/** 导入导出界面底部历史表: 还原原型第 6 屏底部 —— 操作/文件名/类型/内容摘要/状态/时间 */
export function ImpexpHistoryTable({ rows }: ImpexpHistoryTableProps) {
	const columns: DataTableColumn<ImpexpRow>[] = [
		{
			key: 'direction-icon',
			header: '操作',
			render: (row) => {
				const Icon = DIRECTION_ICON[row.direction];
				return <Icon size={14} color="var(--sh-brand)" />;
			},
		},
		{ key: 'fileName', header: '文件名', render: (row) => row.fileName },
		{ key: 'type', header: '类型', render: (row) => DIRECTION_LABEL[row.direction] },
		{ key: 'summary', header: '内容摘要', render: (row) => row.summary },
		{
			key: 'status',
			header: '状态',
			render: (row) => <SyncStatusBadge status={IMPEXP_STATUS_LABEL[row.status]} />,
		},
		{ key: 'runTime', header: '时间', render: (row) => formatRelativeTime(row.runTime) },
	];

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">导入导出历史 History</CardTitle>
			</CardHeader>
			<CardContent>
				{rows.length === 0 ? (
					<p className="py-6 text-center text-sm text-muted-foreground">
						暂无导入导出记录
					</p>
				) : (
					<DataTable columns={columns} rows={rows} rowKey={(row) => row.id} />
				)}
			</CardContent>
		</Card>
	);
}
