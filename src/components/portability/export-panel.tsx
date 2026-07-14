// 文件作用: 导入导出界面左侧"导出 Export"面板 —— 导出内容勾选、范围/格式/是否含配置/是否含
//           版本锁定单选、目标路径输入(文本框可直接输入, 亦可点"选择保存位置"经原生保存对话框
//           拿路径, 由 pages/portability 接 src/lib/dialog.ts 的 pickSaveFile 后回填)、一键导出
//           按钮; 纯展示 + 回调, 数据与提交由 pages/portability 统一持有
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { CircleHelp, Download, FolderOpen } from 'lucide-react';

import type { ExportReqVO } from '@/api/portability';
import { FORMAT_OPTIONS, SCOPE_OPTIONS } from './impexp-display';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

interface ExportPanelProps {
	options: ExportReqVO;
	onOptionsChange: (options: ExportReqVO) => void;
	/** 导出目标路径(可直接在文本框输入, 或点"选择保存位置"经原生对话框拿路径后回填) */
	outPath: string;
	onOutPathChange: (path: string) => void;
	/** "选择保存位置"按钮点击回调, 由 pages/portability 接 dialog.ts 的 pickSaveFile 实现 */
	onBrowseOutPath: () => void;
	onExport: () => void;
	isExporting: boolean;
}

/** 字段行的说明图标: 悬浮显示 tooltip, 不占用额外的可视布局宽度 */
function FieldHint({ text }: { text: string }) {
	return (
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
			<TooltipContent>{text}</TooltipContent>
		</Tooltip>
	);
}

/** 导入导出界面左侧"导出 Export"面板: 还原原型第 6 屏左半部分 */
export function ExportPanel({
	options,
	onOptionsChange,
	outPath,
	onOutPathChange,
	onBrowseOutPath,
	onExport,
	isExporting,
}: ExportPanelProps) {
	function patch(next: Partial<ExportReqVO>) {
		onOptionsChange({ ...options, ...next });
	}

	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<Download size={16} color="var(--sh-brand)" />
					导出 Export
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-1 flex-col gap-4">
				<div className="flex items-center justify-between gap-4">
					<span className="text-sm text-muted-foreground">导出内容</span>
					<div className="flex items-center gap-4">
						<label className="flex items-center gap-1.5 text-sm">
							<Checkbox
								checked={options.includeSkills}
								onCheckedChange={(checked) =>
									patch({ includeSkills: checked === true })
								}
								aria-label="导出全部 Skill"
							/>
							导出全部 Skill
						</label>
						<label className="flex items-center gap-1.5 text-sm">
							<Checkbox
								checked={options.includeMcp}
								onCheckedChange={(checked) =>
									patch({ includeMcp: checked === true })
								}
								aria-label="导出全部 MCP"
							/>
							导出全部 MCP
						</label>
					</div>
				</div>

				<div className="flex items-center justify-between gap-4">
					<span className="text-sm text-muted-foreground">选择导出范围</span>
					<RadioGroup
						value={String(options.scope)}
						onValueChange={(value) =>
							patch({ scope: Number(value) as ExportReqVO['scope'] })
						}
						className="flex items-center gap-4"
						aria-label="选择导出范围"
					>
						{SCOPE_OPTIONS.map((opt) => (
							<label key={opt.value} className="flex items-center gap-1.5 text-sm">
								<RadioGroupItem value={String(opt.value)} aria-label={opt.label} />
								{opt.label}
							</label>
						))}
					</RadioGroup>
				</div>

				<div className="flex items-center justify-between gap-4">
					<span className="text-sm text-muted-foreground">目标文件格式</span>
					<RadioGroup
						value={String(options.format)}
						onValueChange={(value) =>
							patch({ format: Number(value) as ExportReqVO['format'] })
						}
						className="flex items-center gap-4"
						aria-label="目标文件格式"
					>
						{FORMAT_OPTIONS.map((opt) => (
							<label key={opt.value} className="flex items-center gap-1.5 text-sm">
								<RadioGroupItem value={String(opt.value)} aria-label={opt.label} />
								{opt.label}
							</label>
						))}
					</RadioGroup>
				</div>

				<div className="flex items-center justify-between gap-4">
					<span className="flex items-center gap-1 text-sm text-muted-foreground">
						是否包含配置
						<FieldHint text="包含 SkillHub 自身的配置(如 Agent 列表、用户偏好设置)" />
					</span>
					<RadioGroup
						value={String(options.includeConfig)}
						onValueChange={(value) => patch({ includeConfig: value === 'true' })}
						className="flex items-center gap-4"
						aria-label="是否包含配置"
					>
						<label className="flex items-center gap-1.5 text-sm">
							<RadioGroupItem value="true" aria-label="配置-包含" />
							包含
						</label>
						<label className="flex items-center gap-1.5 text-sm">
							<RadioGroupItem value="false" aria-label="配置-不包含" />
							不包含
						</label>
					</RadioGroup>
				</div>

				<div className="flex items-center justify-between gap-4">
					<span className="flex items-center gap-1 text-sm text-muted-foreground">
						是否包含版本锁定
						<FieldHint text="包含各 Skill/MCP 的精确版本号, 便于在目标环境完全还原当前版本" />
					</span>
					<RadioGroup
						value={String(options.includeVersionLock)}
						onValueChange={(value) => patch({ includeVersionLock: value === 'true' })}
						className="flex items-center gap-4"
						aria-label="是否包含版本锁定"
					>
						<label className="flex items-center gap-1.5 text-sm">
							<RadioGroupItem value="true" aria-label="版本锁定-包含" />
							包含
						</label>
						<label className="flex items-center gap-1.5 text-sm">
							<RadioGroupItem value="false" aria-label="版本锁定-不包含" />
							不包含
						</label>
					</RadioGroup>
				</div>

				<p
					className="rounded-md px-3 py-2 text-xs text-muted-foreground"
					style={{ background: 'var(--sh-brand-tint)' }}
				>
					导出将包含所选内容的所有数据和元信息, 便于备份或迁移到其他环境。
				</p>

				<div className="flex flex-col gap-1.5">
					<label htmlFor="export-out-path" className="text-sm text-muted-foreground">
						导出目标路径
					</label>
					<div className="flex items-center gap-2">
						<Input
							id="export-out-path"
							value={outPath}
							onChange={(e) => onOutPathChange(e.target.value)}
							placeholder="请输入导出文件的保存路径, 如 /Users/name/skillhub_backup.zip"
							className="flex-1"
						/>
						<Button variant="outline" onClick={onBrowseOutPath}>
							<FolderOpen size={14} />
							选择保存位置
						</Button>
					</div>
				</div>

				<Button className="mt-auto w-full" onClick={onExport} disabled={isExporting}>
					<Download size={14} />
					一键导出全部
				</Button>
			</CardContent>
		</Card>
	);
}
