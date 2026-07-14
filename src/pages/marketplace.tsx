// 文件作用: 资源中心(Marketplace)界面(还原原型第 2 屏) —— 顶部标题, 左侧列表区(MarketList:
//           搜索/Skills-MCP 分段/筛选 chips/分类/排序/卡片网格/分页), 右侧详情面板
//           (MarketDetailPanel); 数据经 market_search/market_detail 获取, 市场缓存刷新经
//           market_refresh, 下载安装经 market_install(该后端命令由并行任务实现, 本页先接好调用
//           链路: 若安装失败, 先占位提示引导用户到详情页登录, 正式的认证弹窗由后续任务实现)。
//           M5 Task F1: 移除手动"刷新"按钮, 改为挂载时若首次搜索发现市场缓存为空则自动刷新一次
//           (不做高频轮询, 市场数据较重, 挂载拉一次 + 用户搜索即可)。
//           M5 Task F2: 详情面板改为常驻(还原原型默认选中 data-visualizer 的第 2 屏) —— 挂载后
//           首次搜索结果非空且尚无选中项时自动选中第一条(见 hasAutoSelectedRef), 面板本身不再
//           随"有无选中"整体消失, 无内容时改为在同一宽度容器内展示空态
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { useEffect, useMemo, useRef, useState } from 'react';
import { Info, PackageSearch, SearchX } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import {
	marketDetail,
	marketInstall,
	marketRefresh,
	marketSearch,
	type MarketResourceRespVO,
} from '@/api/market';
import { DetailPanel } from '@/components/common/detail-panel';
import { EmptyState } from '@/components/common/empty-state';
import { PageHeader } from '@/components/common/page-header';
import { MarketDetailPanel } from '@/components/marketplace/market-detail-panel';
import { MarketList, type MarketChip } from '@/components/marketplace/market-list';
import { marketResourceKey, sourceTypeToCode } from '@/components/marketplace/market-display';
import { useUiStore } from '@/stores/ui';

/** 前端 Skills/MCP 分段 -> 后端 market_search 的 resType 数值编码(1-Skill, 2-Mcp),
 * 与 pages/installed.tsx 的 RES_TYPE_CODE 同一编码约定 */
const RES_TYPE_CODE: Record<'skill' | 'mcp', number> = { skill: 1, mcp: 2 };

const DEFAULT_PAGE_SIZE = 10;

const MARKET_SEARCH_KEY = 'market-search';
const MARKET_DETAIL_KEY = 'market-detail';
// 与 pages/installed.tsx 内的同名字面量共享同一个 QueryClient 缓存条目(query key 按值而非引用
// 匹配), 使市场安装成功后 Installed 页面的本地库列表也能被一并失效重取
const LIBRARY_LIST_KEY = 'library-list';

/** 安装失败时的占位提示: market_install 尚未接入真正的鉴权错误分类(后端命令由并行任务实现),
 * 这里先统一按"需要登录/授权"提示, 引导用户后续到详情页完成登录(正式的认证弹窗见后续任务) */
const INSTALL_ERROR_PLACEHOLDER = '需在详情页登录';

/** 资源中心(Marketplace)界面: 还原原型第 2 屏 —— 搜索/分段/筛选/排序/分类 + 卡片网格 + 分页 +
 * 右侧详情面板 */
export default function Marketplace() {
	const queryClient = useQueryClient();
	const { selectedMarket, setSelectedMarket, marketRefreshed, setMarketRefreshed } = useUiStore();

	const [resTypeFilter, setResTypeFilter] = useState<'skill' | 'mcp'>('skill');
	const [keyword, setKeyword] = useState('');
	const [chip, setChip] = useState<MarketChip>('all');
	const [category, setCategory] = useState<string | undefined>(undefined);
	const [sort, setSort] = useState(0);
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE);
	const [installingKey, setInstallingKey] = useState<string | null>(null);
	const [installErrors, setInstallErrors] = useState<Record<string, string>>({});

	// 筛选维度变化后停留在不存在的页码上没有意义, 回到第一页(与 resource-list.tsx 的既有惯例一致)
	useEffect(() => {
		setPage(1);
	}, [resTypeFilter, keyword, category, sort]);

	const searchQuery = useQuery({
		queryKey: [MARKET_SEARCH_KEY, resTypeFilter, keyword, category, sort, page, pageSize],
		queryFn: () =>
			marketSearch({
				keyword: keyword.trim() || undefined,
				resType: RES_TYPE_CODE[resTypeFilter],
				category,
				sort,
				page,
				pageSize,
			}),
	});

	const detailQuery = useQuery({
		queryKey: [MARKET_DETAIL_KEY, selectedMarket?.sourceType, selectedMarket?.extId],
		queryFn: () => marketDetail(selectedMarket!.sourceType, selectedMarket!.extId),
		enabled: selectedMarket !== null,
	});

	const items = useMemo(() => searchQuery.data?.items ?? [], [searchQuery.data]);

	// 分类下拉选项: 由当前已加载的当页数据派生(无独立的"查询全部分类"后端能力), 故只反映当前
	// 分段/关键字/分页下已加载到的分类, 见本任务报告"与原型差异"一节
	const categories = useMemo(() => {
		const set = new Set(items.map((item) => item.category).filter((c) => c.length > 0));
		return Array.from(set).sort();
	}, [items]);

	const installMutation = useMutation({
		mutationFn: ({
			resource,
			envOverrides,
		}: {
			resource: MarketResourceRespVO;
			envOverrides?: Record<string, string>;
		}) => marketInstall(sourceTypeToCode(resource.sourceType), resource.extId, envOverrides),
		onMutate: ({ resource }) => {
			const key = marketResourceKey(resource);
			setInstallingKey(key);
			setInstallErrors((prev) => {
				if (!(key in prev)) return prev;
				const next = { ...prev };
				delete next[key];
				return next;
			});
		},
		onError: (_error, { resource }) => {
			setInstallErrors((prev) => ({
				...prev,
				[marketResourceKey(resource)]: INSTALL_ERROR_PLACEHOLDER,
			}));
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [LIBRARY_LIST_KEY] });
		},
		onSettled: () => setInstallingKey(null),
	});

	const refreshMutation = useMutation({
		mutationFn: marketRefresh,
		onSuccess: () => {
			setMarketRefreshed();
			queryClient.invalidateQueries({ queryKey: [MARKET_SEARCH_KEY] });
		},
	});

	// 挂载后只依据"首次搜索"结果判断一次是否需要自动刷新市场缓存, 之后筛选条件变化不再重新判断
	// (与"挂载拉一次, 不做高频轮询"的既定策略一致); firstSearchCheckedRef 保证本组件本次挂载
	// 期间只判断一次, marketRefreshed(见 stores/ui.ts)是跨页面/跨挂载共享的会话级守卫, 二者
	// 共同确保 market_refresh 因"缓存为空"这一原因在本会话内最多被自动触发一次
	const firstSearchCheckedRef = useRef(false);
	useEffect(() => {
		if (firstSearchCheckedRef.current) return;
		if (searchQuery.isLoading) return;
		firstSearchCheckedRef.current = true;
		if (marketRefreshed) return;
		if ((searchQuery.data?.total ?? 0) > 0) return;
		refreshMutation.mutate();
	}, [searchQuery.isLoading, searchQuery.data, marketRefreshed]);

	// 详情面板默认选中: 只在"本次挂载后的首次搜索结果落地"这一时机判断一次(hasAutoSelectedRef
	// 守卫, 与上面 firstSearchCheckedRef 同一惯例), 尚无选中项且当前结果非空时自动选中第一条,
	// 还原原型"默认选中 data-visualizer, 面板始终有内容"。之后筛选/翻页变化不再重新抢占选中态
	// (不强制跟随筛选结果重新指向第一条), 也不在用户后续清空选中(如未来加"关闭"交互)时被这里
	// 重新拉回第一条——只负责"刚进这个页面时给个默认值", 不是"随时兜底选中第一条"
	const hasAutoSelectedRef = useRef(false);
	useEffect(() => {
		if (hasAutoSelectedRef.current) return;
		if (searchQuery.isLoading) return;
		hasAutoSelectedRef.current = true;
		if (selectedMarket !== null) return;
		const first = items[0];
		if (!first) return;
		setSelectedMarket({ sourceType: sourceTypeToCode(first.sourceType), extId: first.extId });
	}, [searchQuery.isLoading, items, selectedMarket, setSelectedMarket]);

	function handleSelectItem(resource: MarketResourceRespVO) {
		setSelectedMarket({
			sourceType: sourceTypeToCode(resource.sourceType),
			extId: resource.extId,
		});
	}

	function handleDownload(resource: MarketResourceRespVO) {
		installMutation.mutate({ resource });
	}

	function handleChipChange(nextChip: MarketChip) {
		setChip(nextChip);
		if (nextChip === 'recommended') setSort(0);
		if (nextChip === 'updated') setSort(2);
	}

	const selectedResource = selectedMarket ? (detailQuery.data ?? null) : null;

	return (
		<div className="flex h-full flex-col gap-4">
			<PageHeader
				title="资源中心 / Marketplace"
				description="发现并下载 Skills 与 MCP, 一键安装到本地库"
			/>

			<div className="flex min-h-0 flex-1 gap-4">
				<MarketList
					items={items}
					total={searchQuery.data?.total ?? 0}
					isLoading={searchQuery.isLoading || refreshMutation.isPending}
					categories={categories}
					resTypeFilter={resTypeFilter}
					keyword={keyword}
					chip={chip}
					category={category}
					sort={sort}
					page={page}
					pageSize={pageSize}
					selectedKey={selectedResource ? marketResourceKey(selectedResource) : null}
					// 安装错误只喂给下面的 MarketDetailPanel, 不重复喂给卡片网格: 占位提示文案
					// 本身即"需在详情页登录", 在卡片上重复展示同一句会造成语义指向不清(见
					// MarketList 的 installErrors 文档注释)
					onResTypeFilterChange={setResTypeFilter}
					onKeywordChange={setKeyword}
					onChipChange={handleChipChange}
					onCategoryChange={setCategory}
					onSortChange={setSort}
					onPageChange={setPage}
					onPageSizeChange={(size) => {
						setPageSize(size);
						setPage(1);
					}}
					onSelectItem={handleSelectItem}
					onDownload={handleDownload}
				/>
				{selectedResource ? (
					<MarketDetailPanel
						resource={selectedResource}
						onClose={() => setSelectedMarket(null)}
						onDownload={handleDownload}
						isInstalling={
							installMutation.isPending &&
							installingKey === marketResourceKey(selectedResource)
						}
						installError={installErrors[marketResourceKey(selectedResource)]}
					/>
				) : (
					// 详情面板常驻: 还原原型"面板始终有内容"的版式, 无选中项时不让整块面板从布局里
					// 消失(否则卡片网格会瞬间跳成占满宽度, 用户点选另一项时又跳回两列, 观感突兀),
					// 改在同宽度容器内展示空态; 区分"确实无搜索结果"与"有结果但暂未选中"两种文案,
					// 与 MarketList 网格区自身的空态措辞呼应但不重复(避免同一屏两处一字不差的文案)
					<DetailPanel
						title="详情"
						onClose={() => setSelectedMarket(null)}
						className="w-90 shrink-0"
					>
						<EmptyState
							icon={items.length === 0 ? SearchX : PackageSearch}
							size="sm"
							title={items.length === 0 ? '暂无可查看的详情' : '未选择资源'}
							description={
								items.length === 0
									? '没有符合当前搜索或筛选条件的资源, 换个关键字或调整筛选再试试'
									: '从左侧列表选择一项资源, 查看详情后可下载并安装'
							}
						/>
					</DetailPanel>
				)}
			</div>

			<footer className="flex items-center gap-1.5 text-xs text-muted-foreground">
				<Info size={14} />
				如需登录或授权, 将在 SkillHub 内部打开, 保障安全与隐私。
			</footer>
		</div>
	);
}
