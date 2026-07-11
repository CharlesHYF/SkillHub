// 文件作用: 资源详情/安装界面(还原原型第 3 屏) —— 大图标+名称+版本+发布者+类别+更新时间等元
//           信息(大小/下载量/兼容性当前领域模型未提供, 如实占位)、版本历史(仅现有当前版本,
//           无历史字段故如实置空)、兼容 Agent(与 Task 10 MarketDetailPanel 同一"暂未接入"口径)、
//           权限说明、安装步骤、下载并安装 + 收藏(占位); 下载并安装若因 market_install 返回
//           "AUTH_REQUIRED:<provider>" 错误触发, 打开 AuthModal 完成认证后自动重试安装, 成功后
//           提示已安装并可跳转已安装页
// 创建日期: 2026-07-10
import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, Plug, Sparkles, Star } from 'lucide-react';

import {
	marketDetail,
	marketInstall,
	parseAuthRequiredProvider,
	type MarketResource,
} from '@/api/market';
import type { AuthAccount } from '@/api/auth';
import { AuthModal } from '@/components/auth/auth-modal';
import { INSTALL_PERMISSIONS } from '@/components/auth/auth-display';
import {
	formatCategory,
	formatStars,
	formatUpdatedAt,
	formatVersion,
	sourceTypeToCode,
	toResourceKind,
} from '@/components/marketplace/market-display';
import { TypeBadge } from '@/components/common/type-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

const MARKET_DETAIL_KEY = 'market-detail';
// 与 pages/marketplace.tsx 共享同一字面量, 安装成功后一并失效 Installed 页面的本地库列表查询
const LIBRARY_LIST_KEY = 'library-list';

/** 当前领域模型未提供的字段统一占位文案, 与 Task 10 MarketDetailPanel"暂无兼容性数据,
 * 待后续任务补齐"同一口径, 避免同一类空态在不同屏出现不同措辞 */
const NOT_AVAILABLE = '暂无数据, 待后续任务补齐';

/** 安装步骤: 与具体资源无关的通用静态说明, 还原原型"安装步骤"卡片文案 */
const INSTALL_STEPS = [
	{ title: '下载安装包', description: '点击"下载并安装"按钮, 获取最新版本。' },
	{ title: '验证与配置', description: '根据提示完成认证与配置。' },
	{ title: '同步到 Agent', description: '选择目标 Agent 并完成同步。' },
	{ title: '开始使用', description: '在 Agent 中配置并使用该资源。' },
];

/** 把 (sourceType 数值编码, extId) 编码为 /marketplace/:id 的单段路由参数: extId 本身可能含
 * "/" 与 ":"(如 "owner/repo:path", 见 api/market.ts MarketResource.extId 文档), 不能直接拼进
 * 路径段, 整体 encodeURIComponent 后以 "<sourceType>:<encoded extId>" 拼接(sourceType 数值
 * 编码本身不含 ":", 解码时按第一个 ":" 切分即可还原, 不会与 extId 内部的 ":"/"/" 混淆)。
 * 未来"查看详情"入口应据此构造链接, 与 parseMarketDetailId 互为逆运算 */
export function buildMarketDetailId(sourceType: number, extId: string): string {
	return `${sourceType}:${encodeURIComponent(extId)}`;
}

/** 解析路由参数还原 (sourceType, extId); 参数缺失/格式不合法均返回 null(由页面展示"资源不存在") */
export function parseMarketDetailId(
	id: string | undefined,
): { sourceType: number; extId: string } | null {
	if (!id) return null;
	const sepIndex = id.indexOf(':');
	if (sepIndex < 0) return null;
	const sourceType = Number(id.slice(0, sepIndex));
	const extId = decodeURIComponent(id.slice(sepIndex + 1));
	if (!Number.isFinite(sourceType) || !extId) return null;
	return { sourceType, extId };
}

interface InfoRowProps {
	label: string;
	value: string;
}

/** 元信息行: 标签 + 值, 供发布者/类别/大小/下载量/最新更新/兼容性等复用同一排布 */
function InfoRow({ label, value }: InfoRowProps) {
	return (
		<div className="flex flex-col">
			<span className="text-xs text-muted-foreground">{label}</span>
			<span className="text-foreground">{value}</span>
		</div>
	);
}

/** 资源详情/安装界面: 还原原型第 3 屏 */
export default function MarketplaceDetail() {
	const { id } = useParams();
	const navigate = useNavigate();
	const queryClient = useQueryClient();
	const parsedId = useMemo(() => parseMarketDetailId(id), [id]);

	// authProvider 非 null 表示 AuthModal 应打开, 其值即弹窗应定位的 provider 数值编码
	const [authProvider, setAuthProvider] = useState<number | null>(null);
	const [installed, setInstalled] = useState(false);
	const [installError, setInstallError] = useState<string | null>(null);

	const detailQuery = useQuery({
		queryKey: [MARKET_DETAIL_KEY, parsedId?.sourceType, parsedId?.extId],
		queryFn: () => marketDetail(parsedId!.sourceType, parsedId!.extId),
		enabled: parsedId !== null,
	});

	const resource = detailQuery.data ?? null;

	// 路由 id 一变, 之前残留的安装反馈状态就该清空, 避免"上一条资源装完的成功提示"错误地
	// 叠在新资源页面上(本页当前没有站内跳转到另一 id 的入口, 属防御性处理)
	useEffect(() => {
		setInstalled(false);
		setInstallError(null);
		setAuthProvider(null);
	}, [parsedId?.sourceType, parsedId?.extId]);

	const installMutation = useMutation({
		mutationFn: (target: MarketResource) =>
			marketInstall(sourceTypeToCode(target.sourceType), target.extId),
		onMutate: () => {
			setInstalled(false);
			setInstallError(null);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [LIBRARY_LIST_KEY] });
			setInstalled(true);
		},
		onError: (error) => {
			const provider = parseAuthRequiredProvider(error);
			if (provider !== null) {
				setAuthProvider(provider);
			} else {
				setInstallError(error instanceof Error ? error.message : String(error));
			}
		},
	});

	function handleInstall() {
		if (resource) installMutation.mutate(resource);
	}

	/** AuthModal 认证成功回调: 关闭弹窗并自动重试安装, 不需要用户再点一次"下载并安装" */
	function handleAuthenticated(_account: AuthAccount) {
		setAuthProvider(null);
		if (resource) installMutation.mutate(resource);
	}

	const Icon = resource?.resType === 'Mcp' ? Plug : Sparkles;

	return (
		<div className="flex h-full flex-col gap-4">
			<div>
				<Button variant="ghost" size="sm" onClick={() => navigate('/marketplace')}>
					<ArrowLeft size={14} />
					返回 Marketplace
				</Button>
			</div>
			<h1 className="text-2xl font-bold">资源详情 / Install</h1>

			{!parsedId ? (
				<p className="text-sm text-muted-foreground">资源不存在 / Resource not found</p>
			) : detailQuery.isLoading ? (
				<p className="text-sm text-muted-foreground">加载中...</p>
			) : !resource ? (
				<p className="text-sm text-muted-foreground">资源不存在 / Resource not found</p>
			) : (
				<div className="grid grid-cols-3 gap-4">
					<div className="col-span-2 flex flex-col gap-4">
						<div className="flex items-start gap-4">
							<span
								className="flex size-16 shrink-0 items-center justify-center rounded-xl"
								style={{ background: 'var(--sh-brand-tint)' }}
							>
								<Icon size={28} color="var(--sh-brand)" />
							</span>
							<div className="min-w-0 flex-1">
								<div className="flex flex-wrap items-center gap-2">
									<h2 className="text-xl font-semibold text-foreground">
										{resource.name}
									</h2>
									<Badge variant="outline">
										{formatVersion(resource.version)}
									</Badge>
									<TypeBadge type={toResourceKind(resource.resType)} />
									<span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
										<Star size={14} />
										{formatStars(resource.stars)}
									</span>
								</div>
								<p className="mt-1 text-sm text-muted-foreground">
									{resource.description || '暂无简介'}
								</p>
							</div>
						</div>

						<div className="grid grid-cols-2 gap-x-6 gap-y-3 text-sm">
							<InfoRow label="发布者" value={resource.author} />
							<InfoRow label="类别" value={formatCategory(resource.category)} />
							<InfoRow label="大小" value={NOT_AVAILABLE} />
							<InfoRow label="下载量" value={NOT_AVAILABLE} />
							<InfoRow label="最新更新" value={formatUpdatedAt(resource.updatedAt)} />
							<InfoRow label="兼容性" value={NOT_AVAILABLE} />
						</div>

						<Card>
							<CardHeader>
								<CardTitle>版本历史</CardTitle>
							</CardHeader>
							<CardContent>
								<ul className="flex flex-col gap-2 text-sm">
									<li className="flex items-center justify-between">
										<span className="font-medium text-foreground">
											{formatVersion(resource.version)}(当前)
										</span>
										<span className="text-muted-foreground">
											{formatUpdatedAt(resource.updatedAt)}
										</span>
									</li>
								</ul>
								<p className="mt-2 text-xs text-muted-foreground">
									更早版本历史暂未接入, 待后续任务补齐
								</p>
							</CardContent>
						</Card>

						<Card>
							<CardHeader>
								<CardTitle>兼容 Agent</CardTitle>
							</CardHeader>
							<CardContent>
								<p className="text-sm text-muted-foreground">{NOT_AVAILABLE}</p>
							</CardContent>
						</Card>
					</div>

					<div className="flex flex-col gap-4">
						<div className="flex flex-col gap-2">
							{installed ? (
								<div
									role="status"
									className="flex items-center gap-2 rounded-md p-2.5 text-sm"
									style={{
										background: 'var(--sh-brand-tint)',
										color: 'var(--sh-brand)',
									}}
								>
									<span>已安装成功</span>
									<Button
										variant="link"
										size="sm"
										className="h-auto p-0"
										onClick={() => navigate('/installed')}
									>
										前往已安装
									</Button>
								</div>
							) : null}
							{installError ? (
								<p
									role="alert"
									className="text-sm"
									style={{ color: 'var(--sh-danger)' }}
								>
									{installError}
								</p>
							) : null}
							<Button onClick={handleInstall} disabled={installMutation.isPending}>
								{installMutation.isPending ? '安装中...' : '下载并安装'}
							</Button>
							{/* 收藏: 原型占位操作, 当前无对应后端 command, 仅展示按钮不做实际持久化,
							    待后续任务补齐真正的收藏能力 */}
							<Button variant="outline" onClick={() => undefined}>
								<Star size={14} />
								收藏
							</Button>
						</div>

						<Card>
							<CardHeader>
								<CardTitle>权限说明</CardTitle>
								<CardAction>
									<Badge
										variant={resource.authRequired ? 'outline' : 'secondary'}
									>
										{resource.authRequired ? '需要登录' : '无需登录'}
									</Badge>
								</CardAction>
							</CardHeader>
							<CardContent>
								<ul className="flex flex-col gap-2 text-sm">
									{INSTALL_PERMISSIONS.map((item) => (
										<li key={item.title}>
											<span className="font-medium text-foreground">
												{item.title}
											</span>
											<p className="text-xs text-muted-foreground">
												{item.description}
											</p>
										</li>
									))}
								</ul>
							</CardContent>
						</Card>

						<Card>
							<CardHeader>
								<CardTitle>安装步骤</CardTitle>
							</CardHeader>
							<CardContent>
								<ol className="flex flex-col gap-2.5 text-sm">
									{INSTALL_STEPS.map((step, index) => (
										<li key={step.title} className="flex gap-2">
											<span
												className="flex size-5 shrink-0 items-center justify-center rounded-full text-xs font-medium"
												style={{
													background: 'var(--sh-brand-tint)',
													color: 'var(--sh-brand)',
												}}
											>
												{index + 1}
											</span>
											<span>
												<span className="font-medium text-foreground">
													{step.title}
												</span>
												<p className="text-xs text-muted-foreground">
													{step.description}
												</p>
											</span>
										</li>
									))}
								</ol>
							</CardContent>
						</Card>
					</div>
				</div>
			)}

			<AuthModal
				open={authProvider !== null}
				onOpenChange={(open) => !open && setAuthProvider(null)}
				defaultProvider={authProvider ?? undefined}
				onAuthenticated={handleAuthenticated}
			/>
		</div>
	);
}
