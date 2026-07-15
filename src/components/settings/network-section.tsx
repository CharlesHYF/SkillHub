// 文件作用: 设置界面"网络与代理 Network"分区(原型第 7 屏右下卡片) —— 代理模式(系统默认/不使用/
//           手动)下拉、HTTP/HTTPS 代理地址、不使用代理的地址、请求超时(秒)四项输入; 纯展示 +
//           onChange(patch), 数据由 pages/settings 统一持有(与 export-panel 的 options/
//           onOptionsChange 同一惯例)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { Network } from 'lucide-react';

import type { SettingRespVO } from '@/api/setting';
import { PROXY_MODE_OPTIONS } from './settings-display';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from '@/components/ui/select';

interface NetworkSectionProps {
	settings: Pick<
		SettingRespVO,
		'netProxyMode' | 'netHttpProxy' | 'netHttpsProxy' | 'netNoProxy' | 'netTimeoutSec'
	>;
	onChange: (patch: Partial<SettingRespVO>) => void;
}

/** 设置界面"网络与代理"分区: 还原原型第 7 屏右下卡片 */
export function NetworkSection({ settings, onChange }: NetworkSectionProps) {
	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<Network size={16} color="var(--sh-brand)" />
					网络与代理 Network
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-1 flex-col gap-4">
				<div className="flex items-center justify-between gap-4">
					<span className="text-sm text-muted-foreground">代理模式</span>
					<Select
						value={String(settings.netProxyMode)}
						onValueChange={(value) =>
							onChange({
								netProxyMode: Number(value) as SettingRespVO['netProxyMode'],
							})
						}
					>
						<SelectTrigger size="sm" className="w-36" aria-label="代理模式">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{PROXY_MODE_OPTIONS.map((opt) => (
								<SelectItem key={opt.value} value={String(opt.value)}>
									{opt.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</div>
				<div className="flex flex-col gap-1.5">
					<label htmlFor="net-http-proxy" className="text-sm text-muted-foreground">
						HTTP 代理
					</label>
					<Input
						id="net-http-proxy"
						value={settings.netHttpProxy}
						onChange={(e) => onChange({ netHttpProxy: e.target.value })}
						placeholder="http://host:port (可选)"
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<label htmlFor="net-https-proxy" className="text-sm text-muted-foreground">
						HTTPS 代理
					</label>
					<Input
						id="net-https-proxy"
						value={settings.netHttpsProxy}
						onChange={(e) => onChange({ netHttpsProxy: e.target.value })}
						placeholder="http://host:port (可选)"
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<label htmlFor="net-no-proxy" className="text-sm text-muted-foreground">
						不使用代理的地址
					</label>
					<Input
						id="net-no-proxy"
						value={settings.netNoProxy}
						onChange={(e) => onChange({ netNoProxy: e.target.value })}
						placeholder="localhost, 127.0.0.1, *.local"
					/>
					<p className="text-xs text-muted-foreground">
						当前版本此项仅保存, 暂未接入网络请求, 将于后续版本生效
					</p>
				</div>
				<div className="flex flex-col gap-1.5">
					<label htmlFor="net-timeout" className="text-sm text-muted-foreground">
						请求超时(秒)
					</label>
					<Input
						id="net-timeout"
						type="number"
						min={1}
						value={settings.netTimeoutSec}
						onChange={(e) => onChange({ netTimeoutSec: Number(e.target.value) })}
					/>
				</div>
			</CardContent>
		</Card>
	);
}
