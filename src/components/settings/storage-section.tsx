// 文件作用: 设置界面"存储目录 Storage"分区(原型第 7 屏右上卡片) —— 本地 Skill 目录/本地 MCP
//           目录, 各为输入框 + "浏览"按钮。本组件纯展示 + 回调, 不自行引入
//           @tauri-apps/plugin-dialog; "浏览"按钮的原生目录选择对话框由 pages/settings 接
//           src/lib/dialog.ts 的 pickDirectory 实现, 经 onBrowseSkillDir/onBrowseMcpDir 两个
//           回调传入, 数据与 onChange(patch)一样由 pages/settings 统一持有(与 export-panel 的
//           options/onOptionsChange 同一惯例)
// 创建日期: 2026-07-10
import { FolderOpen, HardDrive } from 'lucide-react';

import type { Settings } from '@/api/setting';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';

interface StorageSectionProps {
	settings: Pick<Settings, 'storageSkillDir' | 'storageMcpDir'>;
	onChange: (patch: Partial<Settings>) => void;
	/** "浏览"按钮点击回调, 由 pages/settings 接 dialog.ts 的 pickDirectory 实现 */
	onBrowseSkillDir: () => void;
	onBrowseMcpDir: () => void;
}

/** 设置界面"存储目录"分区: 还原原型第 7 屏右上卡片 */
export function StorageSection({
	settings,
	onChange,
	onBrowseSkillDir,
	onBrowseMcpDir,
}: StorageSectionProps) {
	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<HardDrive size={16} color="var(--sh-brand)" />
					存储目录 Storage
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-1 flex-col gap-4">
				<div className="flex flex-col gap-1.5">
					<label htmlFor="storage-skill-dir" className="text-sm text-muted-foreground">
						本地 Skill 目录
					</label>
					<div className="flex items-center gap-2">
						<Input
							id="storage-skill-dir"
							value={settings.storageSkillDir}
							onChange={(e) => onChange({ storageSkillDir: e.target.value })}
							placeholder="如 /Users/name/.skillhub/skills"
							className="flex-1"
						/>
						<Button variant="outline" onClick={onBrowseSkillDir}>
							<FolderOpen size={14} />
							浏览
						</Button>
					</div>
					<p className="text-xs text-muted-foreground">存放下载的 Skill 包与配置文件</p>
				</div>
				<div className="flex flex-col gap-1.5">
					<label htmlFor="storage-mcp-dir" className="text-sm text-muted-foreground">
						本地 MCP 目录
					</label>
					<div className="flex items-center gap-2">
						<Input
							id="storage-mcp-dir"
							value={settings.storageMcpDir}
							onChange={(e) => onChange({ storageMcpDir: e.target.value })}
							placeholder="如 /Users/name/.skillhub/mcp"
							className="flex-1"
						/>
						<Button variant="outline" onClick={onBrowseMcpDir}>
							<FolderOpen size={14} />
							浏览
						</Button>
					</div>
					<p className="text-xs text-muted-foreground">存放 MCP 服务与配置文件</p>
				</div>
			</CardContent>
		</Card>
	);
}
