// 文件作用: 资源中心卡片 —— 图标/名称/类型徽标/描述/作者+认证标记/版本/星标数/查看详情与下载
//           操作; 纯展示 + 回调, 数据获取/选中态/安装 mutation 由 pages/marketplace 统一持有
// 创建日期: 2026-07-10
import { BadgeCheck, Sparkles, Plug, Star } from 'lucide-react';

import type { MarketResourceRespVO } from '@/api/market';
import { TypeBadge } from '@/components/common/type-badge';
import { Button } from '@/components/ui/button';
import { formatStars, formatVersion, toResourceKind } from './market-display';

interface MarketCardProps {
	resource: MarketResourceRespVO;
	/** 当前卡片是否为详情面板正在展示的选中项 */
	selected: boolean;
	onSelect: (resource: MarketResourceRespVO) => void;
	onDownload: (resource: MarketResourceRespVO) => void;
	/** 该资源最近一次下载安装失败的提示文案; 非空时展示在卡片底部 */
	installError?: string;
}

/** 资源中心卡片: 还原原型第 2 屏的卡片布局, 点击卡片主体或"查看详情"选中该项(打开右侧详情面板),
 * "下载"按钮直接触发安装, 二者互不影响 */
export function MarketCard({
	resource,
	selected,
	onSelect,
	onDownload,
	installError,
}: MarketCardProps) {
	const Icon = resource.resType === 'Mcp' ? Plug : Sparkles;

	// 卡片整体可点选(打开详情), 键盘可达性由内部"查看详情"/"下载"按钮承载(与列表可点行同一
	// 分层惯例); 此处只做视觉反馈: hover 轻微抬升 + 阴影加深, selected 用品牌描边 + 轻染底,
	// 过渡克制(200ms)并尊重 prefers-reduced-motion
	return (
		<div
			data-state={selected ? 'selected' : undefined}
			onClick={() => onSelect(resource)}
			// min-w-0 不可省略: 卡片是两列 grid 的 grid item, grid item 默认 min-width:auto(不
			// 缩小到低于内容固有最小宽度), 名称/描述等一长, 就会把列撑宽进而挤压甚至溢出整个网格
			// (与 components/layout/app-shell.tsx 的 min-w-0 注释同一原理)
			className="flex min-w-0 cursor-pointer flex-col gap-3 rounded-lg border p-4 shadow-xs transition-[transform,box-shadow,border-color,background-color] duration-200 hover:-translate-y-0.5 hover:shadow-md active:translate-y-0 motion-reduce:transition-none motion-reduce:hover:translate-y-0"
			style={{
				borderColor: selected ? 'var(--sh-brand)' : 'var(--sh-border)',
				background: selected ? 'var(--sh-brand-tint)' : 'var(--sh-surface)',
			}}
		>
			<div className="flex items-start gap-3">
				<span
					className="flex size-10 shrink-0 items-center justify-center rounded-lg"
					style={{ background: 'var(--sh-brand-tint)' }}
				>
					<Icon size={20} color="var(--sh-brand)" />
				</span>
				<div className="min-w-0 flex-1">
					<div className="flex flex-wrap items-center gap-1.5">
						<h3 className="truncate font-medium text-foreground">{resource.name}</h3>
						<TypeBadge type={toResourceKind(resource.resType)} />
					</div>
					<p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
						{resource.description}
					</p>
				</div>
			</div>

			<div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
				<span className="inline-flex min-w-0 items-center gap-1 truncate">
					作者: {resource.author}
					{/* 市场资源均来自 SkillHub 聚合的已知市场源(见 infra::source), 用统一的认证标记
					    标识"来自已聚合市场源", 不对应某个逐条资源的独立字段(当前领域模型未提供
					    单条资源级别的发布者认证信息), 见本任务报告"疑虑"一节 */}
					<BadgeCheck
						size={14}
						color="var(--sh-brand)"
						aria-hidden={false}
						role="img"
						aria-label="已认证市场源"
					/>
				</span>
				<span className="shrink-0">{formatVersion(resource.version)}</span>
			</div>

			<div className="flex items-center justify-between gap-2">
				<span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
					<Star size={14} />
					{formatStars(resource.stars)}
				</span>
				<div className="flex shrink-0 gap-2" onClick={(e) => e.stopPropagation()}>
					<Button variant="outline" size="sm" onClick={() => onSelect(resource)}>
						查看详情
					</Button>
					<Button
						variant={selected ? 'default' : 'outline'}
						size="sm"
						onClick={() => onDownload(resource)}
					>
						下载
					</Button>
				</div>
			</div>

			{installError ? (
				<p role="alert" className="text-xs" style={{ color: 'var(--sh-danger)' }}>
					{installError}
				</p>
			) : null}
		</div>
	);
}
