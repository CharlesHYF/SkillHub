// 文件作用: 导入导出界面右侧"导入 Import"面板 —— 拖拽区(经 Tauri v2 webview 的
//           onDragDropEvent 拿真实文件路径, 无需后端插件) + "选择文件"按钮(经 pages/portability
//           接 src/lib/dialog.ts 的 pickOpenFile 拿真实文件路径) + 文本路径输入兜底、导入内容
//           预览(按 import_preview 结果渲染 Skill/MCP/配置/Agent 计数与 schema 校验提示)、冲突
//           处理策略/导入后自动同步勾选、开始导入按钮; 纯展示 + 回调, 数据与提交由
//           pages/portability 统一持有
// 创建日期: 2026-07-10
import { useEffect, useState } from 'react';
import { CircleHelp, CloudUpload, FolderOpen, Plug, Sparkles, Upload, Users } from 'lucide-react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { UnlistenFn } from '@tauri-apps/api/event';

import type { ConflictStrategy, ImportPreview } from '@/api/portability';
import { CONFLICT_STRATEGY_OPTIONS } from './impexp-display';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

interface ImportPanelProps {
	/** 待导入文件路径(拖拽、"选择文件"原生对话框或文本输入拿到的真实路径); 空串表示尚未选择 */
	path: string;
	onPathChange: (path: string) => void;
	/** "选择文件"按钮点击回调, 由 pages/portability 接 dialog.ts 的 pickOpenFile 实现 */
	onBrowseFile: () => void;
	preview: ImportPreview | undefined;
	isPreviewLoading: boolean;
	conflictStrategy: ConflictStrategy;
	onConflictStrategyChange: (strategy: ConflictStrategy) => void;
	autoSync: boolean;
	onAutoSyncChange: (autoSync: boolean) => void;
	onStartImport: () => void;
	isImporting: boolean;
}

/** 预览列表一行: 图标 + 名称 + 计数 */
function PreviewRow({
	icon,
	label,
	count,
}: {
	icon: React.ReactNode;
	label: string;
	count: number;
}) {
	return (
		<div className="flex items-center justify-between rounded-md border px-3 py-2 text-sm">
			<span className="flex items-center gap-2 text-muted-foreground">
				{icon}
				{label}
			</span>
			<span className="font-semibold text-foreground">{count}</span>
		</div>
	);
}

/** 导入导出界面右侧"导入 Import"面板: 还原原型第 6 屏右半部分 */
export function ImportPanel({
	path,
	onPathChange,
	onBrowseFile,
	preview,
	isPreviewLoading,
	conflictStrategy,
	onConflictStrategyChange,
	autoSync,
	onAutoSyncChange,
	onStartImport,
	isImporting,
}: ImportPanelProps) {
	const [isDragOver, setIsDragOver] = useState(false);

	// 订阅 Tauri v2 webview 的拖拽事件拿真实文件路径(payload.paths); 浏览器原生 dataTransfer 在
	// Tauri 桌面 webview 里拿不到真实文件系统路径, 故不用原生 onDrop。组件卸载时取消订阅,
	// cancelled 兜底避免 Promise resolve 前已卸载导致的悬空订阅(与 sync-center 的既有写法一致)
	useEffect(() => {
		let unlisten: UnlistenFn | undefined;
		let cancelled = false;
		getCurrentWebview()
			.onDragDropEvent((event) => {
				const { payload } = event;
				if (payload.type === 'drop') {
					setIsDragOver(false);
					if (payload.paths.length > 0) onPathChange(payload.paths[0]);
				} else if (payload.type === 'enter' || payload.type === 'over') {
					setIsDragOver(true);
				} else {
					setIsDragOver(false);
				}
			})
			.then((fn) => {
				if (cancelled) fn();
				else unlisten = fn;
			});
		return () => {
			cancelled = true;
			unlisten?.();
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	const hasPath = path.trim().length > 0;
	const canImport = hasPath && !!preview && preview.schemaOk !== false;

	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<Upload size={16} color="var(--sh-brand)" />
					导入 Import
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-1 flex-col gap-4">
				<div
					className={cn(
						'flex flex-col items-center gap-2 rounded-lg border border-dashed px-6 py-8 text-center transition-colors',
					)}
					style={{
						borderColor: isDragOver ? 'var(--sh-brand)' : 'var(--sh-border)',
						background: isDragOver ? 'var(--sh-brand-tint)' : undefined,
					}}
				>
					<CloudUpload size={28} color="var(--sh-brand)" />
					<p className="text-sm text-foreground">拖拽文件到此处, 或点击选择文件</p>
					<p className="text-xs text-muted-foreground">支持 zip、json、tar 格式</p>
				</div>

				<div className="flex items-center gap-2">
					<Button variant="outline" size="sm" onClick={onBrowseFile}>
						<FolderOpen size={14} />
						选择文件
					</Button>
					<Input
						value={path}
						onChange={(e) => onPathChange(e.target.value)}
						placeholder="未选择文件 · 可拖拽文件, 或在此粘贴完整路径"
						className="flex-1"
						aria-label="导入文件路径"
					/>
				</div>

				<div className="grid grid-cols-2 gap-4">
					<div className="flex flex-col gap-2">
						<p className="text-sm text-muted-foreground">将导入的内容预览</p>
						{!hasPath ? (
							<p className="text-sm text-muted-foreground">
								请先拖拽或选择要导入的文件
							</p>
						) : isPreviewLoading ? (
							<p className="text-sm text-muted-foreground">正在解析导入包...</p>
						) : preview ? (
							<>
								<PreviewRow
									icon={<Sparkles size={14} />}
									label="Skill"
									count={preview.skill}
								/>
								<PreviewRow
									icon={<Plug size={14} />}
									label="MCP"
									count={preview.mcp}
								/>
								<PreviewRow
									icon={<FolderOpen size={14} />}
									label="配置项"
									count={preview.config}
								/>
								<PreviewRow
									icon={<Users size={14} />}
									label="Agent"
									count={preview.agent}
								/>
								{!preview.schemaOk ? (
									<p
										role="alert"
										className="text-xs"
										style={{ color: 'var(--sh-danger)' }}
									>
										文件结构校验未通过, 请检查导入包是否完整
									</p>
								) : null}
							</>
						) : null}
					</div>

					<div className="flex flex-col gap-2">
						<span className="flex items-center gap-1 text-sm text-muted-foreground">
							冲突处理策略
							<Tooltip>
								<TooltipTrigger asChild>
									<button
										type="button"
										aria-label="说明"
										className="rounded-sm text-muted-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
									>
										<CircleHelp size={14} />
									</button>
								</TooltipTrigger>
								<TooltipContent>
									导入项与本地已有同名项冲突时的处理方式
								</TooltipContent>
							</Tooltip>
						</span>
						<RadioGroup
							value={String(conflictStrategy)}
							onValueChange={(value) =>
								onConflictStrategyChange(Number(value) as ConflictStrategy)
							}
							aria-label="冲突处理策略"
						>
							{CONFLICT_STRATEGY_OPTIONS.map((opt) => (
								<label key={opt.value} className="flex items-start gap-2 text-sm">
									<RadioGroupItem
										value={String(opt.value)}
										aria-label={opt.label}
										className="mt-0.5"
									/>
									<span>
										<span className="block font-medium text-foreground">
											{opt.label}
										</span>
										<span className="block text-xs text-muted-foreground">
											{opt.description}
										</span>
									</span>
								</label>
							))}
						</RadioGroup>
					</div>
				</div>

				<div className="mt-auto flex items-center justify-between gap-4">
					<label className="flex items-center gap-1.5 text-sm">
						<Checkbox
							checked={autoSync}
							onCheckedChange={(checked) => onAutoSyncChange(checked === true)}
							aria-label="导入后自动同步 Agent"
						/>
						导入后自动同步 Agent
					</label>
					<Button onClick={onStartImport} disabled={!canImport || isImporting}>
						<Upload size={14} />
						开始导入
					</Button>
				</div>
			</CardContent>
		</Card>
	);
}
