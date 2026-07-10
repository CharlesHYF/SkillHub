// 文件作用: 导入导出(Import/Export)界面(原型第 6 屏) —— 左"导出 Export"面板(export-panel) + 右
//           "导入 Import"面板(import-panel) + 底部"导入导出历史 History"表(impexp-history-table);
//           导出经 exportBundle 提交, 导入路径变化时经 importPreview 自动取内容预览, 经
//           importBundle 提交; 两者成功后失效历史列表 Query 触发刷新, 导入成功后额外清空路径
// 创建日期: 2026-07-10
import { useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import {
	exportBundle,
	importBundle,
	importPreview,
	impexpHistory,
	type ConflictStrategy,
	type ExportOptions,
} from '@/api/portability';
import { ExportPanel } from '@/components/portability/export-panel';
import { ImportPanel } from '@/components/portability/import-panel';
import { ImpexpHistoryTable } from '@/components/portability/impexp-history-table';
import { Button } from '@/components/ui/button';

const IMPEXP_HISTORY_KEY = 'impexp-history';
const IMPORT_PREVIEW_KEY = 'import-preview';
/** 历史表 M3 暂不做分页/查看全部, 固定拉取近 20 条(见任务说明: 未提供的字段/交互不臆造) */
const HISTORY_LIMIT = 20;

/** 导出选项默认值: 与原型截图一致 —— 全部勾选/全部数据/zip/含配置/含版本锁定 */
const DEFAULT_EXPORT_OPTIONS: ExportOptions = {
	includeSkills: true,
	includeMcp: true,
	scope: 0,
	format: 1,
	includeConfig: true,
	includeVersionLock: true,
};

/** 导入导出(Import/Export)界面: 还原原型第 6 屏 —— 导出面板 + 导入面板 + 历史表 */
export default function Portability() {
	const queryClient = useQueryClient();

	const [exportOptions, setExportOptions] = useState<ExportOptions>(DEFAULT_EXPORT_OPTIONS);
	// 导出目标路径: M3 先用文本输入框占位, 原生保存对话框留后续(见 pages/portability 任务说明)
	const [exportOutPath, setExportOutPath] = useState('');

	const [importPath, setImportPath] = useState('');
	const [conflictStrategy, setConflictStrategy] = useState<ConflictStrategy>(0);
	const [autoSync, setAutoSync] = useState(true);

	const historyQuery = useQuery({
		queryKey: [IMPEXP_HISTORY_KEY, HISTORY_LIMIT],
		queryFn: () => impexpHistory(HISTORY_LIMIT),
	});

	// 选定路径后自动预览(不必再点一次"预览"), 路径为空时不发请求
	const previewQuery = useQuery({
		queryKey: [IMPORT_PREVIEW_KEY, importPath],
		queryFn: () => importPreview(importPath),
		enabled: importPath.trim().length > 0,
		retry: false,
	});

	function invalidateHistory() {
		queryClient.invalidateQueries({ queryKey: [IMPEXP_HISTORY_KEY] });
	}

	const exportMutation = useMutation({
		mutationFn: () => exportBundle(exportOptions, exportOutPath),
		onSuccess: invalidateHistory,
	});

	const importMutation = useMutation({
		mutationFn: () => importBundle(importPath, conflictStrategy, autoSync),
		onSuccess: () => {
			invalidateHistory();
			// 导入完成后清空路径: 预览态随之失效, 避免误导用户"当前预览仍对应最新一次导入"
			setImportPath('');
		},
	});

	return (
		<div className="flex h-full flex-col gap-4">
			<header className="flex items-center justify-between">
				<h1 className="text-2xl font-bold">导入导出 / Import & Export</h1>
				<Button variant="outline" onClick={() => historyQuery.refetch()}>
					<RefreshCw
						size={14}
						className={historyQuery.isFetching ? 'animate-spin' : undefined}
					/>
					刷新
				</Button>
			</header>

			<div className="grid grid-cols-2 gap-4">
				<ExportPanel
					options={exportOptions}
					onOptionsChange={setExportOptions}
					outPath={exportOutPath}
					onOutPathChange={setExportOutPath}
					onExport={() => exportMutation.mutate()}
					isExporting={exportMutation.isPending}
				/>
				<ImportPanel
					path={importPath}
					onPathChange={setImportPath}
					preview={previewQuery.data}
					isPreviewLoading={previewQuery.isFetching}
					conflictStrategy={conflictStrategy}
					onConflictStrategyChange={setConflictStrategy}
					autoSync={autoSync}
					onAutoSyncChange={setAutoSync}
					onStartImport={() => importMutation.mutate()}
					isImporting={importMutation.isPending}
				/>
			</div>

			<ImpexpHistoryTable rows={historyQuery.data ?? []} />
		</div>
	);
}
