// 文件作用: 设置界面"同步偏好 Sync Preferences"分区(原型第 7 屏左下卡片) —— 4 个开关(自动同步到
//           新 Agent/启动时检查更新/冲突时提示/仅同步已启用项), 各带说明文案(措辞与原型截图一致);
//           纯展示 + onChange(patch), 数据由 pages/settings 统一持有(与 export-panel 的
//           options/onOptionsChange 同一惯例)
// 创建日期: 2026-07-10
import { Repeat } from 'lucide-react';

import type { Settings } from '@/api/setting';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';

type SyncToggleKey =
	'syncAutoNewAgent' | 'syncCheckUpdateOnStart' | 'syncConflictPrompt' | 'syncOnlyEnabled';

interface SyncToggleConfig {
	key: SyncToggleKey;
	label: string;
	description: string;
}

/** 4 个同步偏好开关, 措辞与原型截图一致 */
const SYNC_TOGGLES: SyncToggleConfig[] = [
	{
		key: 'syncAutoNewAgent',
		label: '自动同步到新 Agent',
		description: '当有新的 Agent 加入时, 自动同步已启用的 Skill 与 MCP',
	},
	{
		key: 'syncCheckUpdateOnStart',
		label: '启动时检查更新',
		description: '应用启动时检查 Skill 与 MCP 的更新',
	},
	{
		key: 'syncConflictPrompt',
		label: '冲突时提示',
		description: '同步冲突时显示提示, 需手动确认后继续',
	},
	{
		key: 'syncOnlyEnabled',
		label: '仅同步已启用项',
		description: '仅同步当前已启用的 Skill 与 MCP, 忽略未启用项',
	},
];

interface SyncPreferencesSectionProps {
	settings: Pick<Settings, SyncToggleKey>;
	onChange: (patch: Partial<Settings>) => void;
}

/** 设置界面"同步偏好"分区: 还原原型第 7 屏左下卡片 */
export function SyncPreferencesSection({ settings, onChange }: SyncPreferencesSectionProps) {
	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<Repeat size={16} color="var(--sh-brand)" />
					同步偏好 Sync Preferences
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-1 flex-col gap-4">
				{SYNC_TOGGLES.map((toggle) => (
					<div key={toggle.key} className="flex items-center justify-between gap-4">
						<div className="flex flex-col gap-0.5">
							<span className="text-sm font-medium text-foreground">
								{toggle.label}
							</span>
							<span className="text-xs text-muted-foreground">
								{toggle.description}
							</span>
						</div>
						<Switch
							checked={settings[toggle.key]}
							onCheckedChange={(checked) =>
								onChange({ [toggle.key]: checked } as Partial<Settings>)
							}
							aria-label={toggle.label}
						/>
					</div>
				))}
			</CardContent>
		</Card>
	);
}
