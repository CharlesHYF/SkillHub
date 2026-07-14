// 文件作用: 设置界面"更新通道 Update Channel"分区(原型第 7 屏底部通栏) —— Stable(稳定版)/
//           Beta(测试版)单选, 各带说明文案(措辞与原型截图一致); 纯展示 + onChange(patch), 数据
//           由 pages/settings 统一持有(与 export-panel 的 options/onOptionsChange 同一惯例)
// 创建日期: 2026-07-10
import { GitBranch } from 'lucide-react';

import type { SettingRespVO } from '@/api/setting';
import { UPDATE_CHANNEL_OPTIONS } from './settings-display';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';

interface UpdateChannelSectionProps {
	settings: Pick<SettingRespVO, 'updateChannel'>;
	onChange: (patch: Partial<SettingRespVO>) => void;
}

/** 设置界面"更新通道"分区: 还原原型第 7 屏底部通栏 */
export function UpdateChannelSection({ settings, onChange }: UpdateChannelSectionProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<GitBranch size={16} color="var(--sh-brand)" />
					更新通道 Update Channel
				</CardTitle>
			</CardHeader>
			<CardContent>
				<RadioGroup
					value={String(settings.updateChannel)}
					onValueChange={(value) =>
						onChange({ updateChannel: Number(value) as SettingRespVO['updateChannel'] })
					}
					className="flex items-center gap-8"
					aria-label="更新通道"
				>
					{UPDATE_CHANNEL_OPTIONS.map((opt) => (
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
			</CardContent>
		</Card>
	);
}
