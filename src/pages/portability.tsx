// 文件作用: 导入导出(Import/Export)界面(原型第 6 屏) —— 左"导出 Export"面板(export-panel) + 右
//           "导入 Import"面板(import-panel) + 底部"导入导出历史 History"表(impexp-history-table);
//           导出经 exportBundle 提交, 导入路径变化时经 importPreview 自动取内容预览, 经
//           importBundle 提交; 两者成功后失效历史列表 Query 触发刷新, 导入成功后额外清空路径。
//           导出目标路径/导入文件路径均可直接在文本框输入, 也可点"选择保存位置"/"选择文件"经
//           src/lib/dialog.ts 弹出原生对话框拿真实路径回填(取消则维持原值不变); 拖拽区(见
//           import-panel)取到路径后走的是同一个 setImportPath, 三条路径殊途同归。
//           M5 Task F1: 移除手动"刷新"按钮, 历史列表改由 refetchInterval 等策略自动保鲜
//           (见 lib/query.ts)
// 创建日期: 2026-07-10
import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import {
	exportBundle,
	importBundle,
	importPreview,
	impexpHistory,
	type ConflictStrategy,
	type ExportOptions,
} from '@/api/portability';
import { PageHeader } from '@/components/common/page-header';
import { ExportPanel } from '@/components/portability/export-panel';
import { ImportPanel } from '@/components/portability/import-panel';
import { ImpexpHistoryTable } from '@/components/portability/impexp-history-table';
import { FORMAT_OPTIONS } from '@/components/portability/impexp-display';
import { LIVE_QUERY_OPTIONS } from '@/lib/query';
import { pickOpenFile, pickSaveFile } from '@/lib/dialog';

/** 导入包"选择文件"对话框的固定过滤器: 支持 zip/json/tar/gz 四种格式, 不区分当前未选定的导出
 * 格式(导入包格式由文件本身决定, 与导出侧当前单选值无关) */
const IMPORT_FILE_FILTERS = [{ name: '导入包', extensions: ['zip', 'json', 'tar', 'gz'] }];

/** 导出保存对话框的过滤器: 按当前所选目标格式(zip/json/tar)生成单一扩展名过滤器, 扩展名取自
 * FORMAT_OPTIONS 的既有措辞, 避免与导出面板的格式文案重复维护一份映射 */
function exportSaveDialogFilters(format: ExportOptions['format']) {
	const ext = FORMAT_OPTIONS.find((opt) => opt.value === format)?.label ?? 'zip';
	return [{ name: `导出包 (.${ext})`, extensions: [ext] }];
}

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
	// 导出目标路径: 可直接在文本框输入, 也可经"选择保存位置"原生对话框拿路径回填
	const [exportOutPath, setExportOutPath] = useState('');

	const [importPath, setImportPath] = useState('');
	const [conflictStrategy, setConflictStrategy] = useState<ConflictStrategy>(0);
	const [autoSync, setAutoSync] = useState(true);

	const historyQuery = useQuery({
		queryKey: [IMPEXP_HISTORY_KEY, HISTORY_LIMIT],
		queryFn: () => impexpHistory(HISTORY_LIMIT),
		...LIVE_QUERY_OPTIONS,
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

	// "选择保存位置": 弹出原生保存对话框, 过滤器按当前所选导出格式生成; 用户取消(结果为 null)
	// 时维持 exportOutPath 原值不变
	async function handleBrowseExportOutPath() {
		const result = await pickSaveFile({
			filters: exportSaveDialogFilters(exportOptions.format),
		});
		if (result !== null) setExportOutPath(result);
	}

	// "选择文件": 弹出原生打开文件对话框拿导入包路径; 结果写入 importPath 后与拖拽/文本输入
	// 拿到路径走的是同一条状态更新路径, 自动触发既有的 importPreview 查询。用户取消(结果为
	// null)时维持 importPath 原值不变
	async function handleBrowseImportFile() {
		const result = await pickOpenFile({ filters: IMPORT_FILE_FILTERS });
		if (result !== null) setImportPath(result);
	}

	return (
		<div className="flex h-full flex-col gap-4">
			<PageHeader
				title="导入导出 / Import & Export"
				description="把本地配置导出为迁移包, 或从迁移包导入到本地库"
			/>

			<div className="grid grid-cols-2 gap-4">
				<ExportPanel
					options={exportOptions}
					onOptionsChange={setExportOptions}
					outPath={exportOutPath}
					onOutPathChange={setExportOutPath}
					onBrowseOutPath={handleBrowseExportOutPath}
					onExport={() => exportMutation.mutate()}
					isExporting={exportMutation.isPending}
				/>
				<ImportPanel
					path={importPath}
					onPathChange={setImportPath}
					onBrowseFile={handleBrowseImportFile}
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

			<ImpexpHistoryTable rows={historyQuery.data ?? []} isLoading={historyQuery.isLoading} />
		</div>
	);
}
