// 文件作用: 资源中心右侧详情面板 —— 图标/名称/类型徽标/作者+认证标记/版本/更新时间/星标数、
//           简介、标签、兼容 Agent(占位)、安装要求、认证与授权说明、下载并安装操作; 纯展示 +
//           回调, 数据获取/选中态/安装 mutation 由 pages/marketplace 统一持有
// 创建日期: 2026-07-10
import { BadgeCheck, Sparkles, Plug, Star, ShieldCheck } from 'lucide-react';

import type { MarketResource } from '@/api/market';
import { DetailPanel } from '@/components/common/detail-panel';
import { TypeBadge } from '@/components/common/type-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { formatDateTime } from '@/lib/utils';
import {
	deriveAuthNotice,
	deriveInstallRequirements,
	formatStars,
	toResourceKind,
} from './market-display';

interface MarketDetailPanelProps {
	resource: MarketResource;
	onClose: () => void;
	onDownload: (resource: MarketResource) => void;
	/** 是否正在执行"下载并安装"(禁用按钮 + 文案提示, 避免重复触发) */
	isInstalling?: boolean;
	/** 该资源最近一次安装失败的提示文案(如需要登录/授权的占位提示); 非空时展示在操作按钮上方 */
	installError?: string;
}

/** 资源中心右侧详情面板: 还原原型第 2 屏 —— 简介/标签/兼容 Agent/安装要求/认证与授权说明 +
 * 下载并安装操作 */
export function MarketDetailPanel({
	resource,
	onClose,
	onDownload,
	isInstalling = false,
	installError,
}: MarketDetailPanelProps) {
	const Icon = resource.resType === 'Mcp' ? Plug : Sparkles;
	const installRequirements = deriveInstallRequirements(resource);
	const authNotice = deriveAuthNotice(resource);

	return (
		<DetailPanel title={resource.name} onClose={onClose} className="w-90 shrink-0">
			<div className="flex flex-col gap-5">
				<div className="flex items-center gap-3">
					<span
						className="flex size-10 shrink-0 items-center justify-center rounded-lg"
						style={{ background: 'var(--sh-brand-tint)' }}
					>
						<Icon size={20} color="var(--sh-brand)" />
					</span>
					<TypeBadge type={toResourceKind(resource.resType)} />
				</div>

				<div className="flex flex-col gap-1 text-sm text-muted-foreground">
					<span className="inline-flex items-center gap-1">
						作者: {resource.author}
						<BadgeCheck
							size={14}
							color="var(--sh-brand)"
							role="img"
							aria-label="已认证市场源"
						/>
					</span>
					<span>
						版本: v{resource.version || '-'} · 更新于:{' '}
						{formatDateTime(resource.updatedAt)}
					</span>
					<span className="inline-flex items-center gap-1">
						<Star size={14} />
						{formatStars(resource.stars)}
					</span>
				</div>

				<section>
					<h3 className="mb-1 text-xs font-medium text-muted-foreground">简介</h3>
					<p className="text-sm text-foreground">{resource.description || '暂无简介'}</p>
				</section>

				{resource.tags.length > 0 ? (
					<section>
						<h3 className="mb-1.5 text-xs font-medium text-muted-foreground">标签</h3>
						<div className="flex flex-wrap gap-1.5">
							{resource.tags.map((tag) => (
								<Badge key={tag} variant="secondary">
									{tag}
								</Badge>
							))}
						</div>
					</section>
				) : null}

				<section>
					<h3 className="mb-1.5 text-xs font-medium text-muted-foreground">兼容 Agent</h3>
					{/* 当前领域模型(domain::market::MarketResource)未提供逐资源的 Agent 兼容性字段,
					    也没有对应的后端查询维度; 原型截图中的"Agent Alpha/Beta/Gamma/Delta"本身即为
					    示意性占位名(与真实 domain::agent::AgentKind 的产品名完全不同), 故这里如实
					    展示"暂未接入"占位, 不虚构兼容列表, 见本任务报告"与原型差异"一节 */}
					<p className="text-sm text-muted-foreground">暂无兼容性数据, 待后续任务补齐</p>
				</section>

				{installRequirements.length > 0 ? (
					<section>
						<h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
							安装要求
						</h3>
						<ul className="list-inside list-disc text-sm text-foreground">
							{installRequirements.map((line) => (
								<li key={line}>{line}</li>
							))}
						</ul>
					</section>
				) : null}

				<section>
					<h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
						认证与授权(可选)
					</h3>
					<div className="flex items-start gap-2 text-sm text-muted-foreground">
						<ShieldCheck size={16} className="mt-0.5 shrink-0" />
						<span>{authNotice}</span>
					</div>
				</section>

				<div className="flex flex-col gap-2 pt-2">
					{installError ? (
						<p role="alert" className="text-sm" style={{ color: 'var(--sh-danger)' }}>
							{installError}
						</p>
					) : null}
					<Button onClick={() => onDownload(resource)} disabled={isInstalling}>
						{isInstalling ? '安装中...' : '下载并安装'}
					</Button>
				</div>
			</div>
		</DetailPanel>
	);
}
