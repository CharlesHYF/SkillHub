// 文件作用: 资源中心(Marketplace)界面(还原原型第 2 屏) —— 顶部标题+刷新, 左侧列表区
//           (MarketList: 搜索/Skills-MCP 分段/筛选 chips/分类/排序/卡片网格/分页), 右侧详情面板
//           (MarketDetailPanel); 数据经 market_search/market_detail 获取, 市场缓存刷新经
//           market_refresh, 下载安装经 market_install(该后端命令由并行任务实现, 本页先接好调用
//           链路: 若安装失败, 先占位提示引导用户到详情页登录, 正式的认证弹窗由后续任务实现)
// 创建日期: 2026-07-10
import { useEffect, useMemo, useState } from 'react';
import { Info, RefreshCw } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import {
	marketDetail,
	marketInstall,
	marketRefresh,
	marketSearch,
	type MarketResource,
} from '@/api/market';
import { MarketDetailPanel } from '@/components/marketplace/market-detail-panel';
import { MarketList, type MarketChip } from '@/components/marketplace/market-list';
import { marketResourceKey, sourceTypeToCode } from '@/components/marketplace/market-display';
import { Button } from '@/components/ui/button';
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
	const { selectedMarket, setSelectedMarket } = useUiStore();

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
			resource: MarketResource;
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
		onSuccess: () => queryClient.invalidateQueries({ queryKey: [MARKET_SEARCH_KEY] }),
	});

	function handleSelectItem(resource: MarketResource) {
		setSelectedMarket({
			sourceType: sourceTypeToCode(resource.sourceType),
			extId: resource.extId,
		});
	}

	function handleDownload(resource: MarketResource) {
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
			<header className="flex items-center justify-between">
				<h1 className="text-2xl font-bold">资源中心 / Marketplace</h1>
				<Button variant="outline" onClick={() => refreshMutation.mutate()}>
					<RefreshCw
						size={14}
						className={refreshMutation.isPending ? 'animate-spin' : undefined}
					/>
					刷新
				</Button>
			</header>

			<div className="flex min-h-0 flex-1 gap-4">
				<MarketList
					items={items}
					total={searchQuery.data?.total ?? 0}
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
				) : null}
			</div>

			<footer className="flex items-center gap-1.5 text-xs text-muted-foreground">
				<Info size={14} />
				如需登录或授权, 将在 SkillHub 内部打开, 保障安全与隐私。
			</footer>
		</div>
	);
}
