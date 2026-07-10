// 文件作用: 资源中心左侧列表区 —— 搜索框 + Skills/MCP 分段 + 筛选 chips(全部/推荐/已认证/免费/
//           最近更新) + 分类下拉 + 结果计数 + 排序下拉 + 卡片网格(MarketCard) + 分页; 纯展示 +
//           回调, 查询态(关键字/分段/分类/排序/分页)与安装 mutation 由 pages/marketplace 统一持有
// 创建日期: 2026-07-10
import { Fragment } from 'react';
import { Search, ChevronDown, ChevronLeft, ChevronRight } from 'lucide-react';

import type { MarketResource } from '@/api/market';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MarketCard } from './market-card';
import { marketResourceKey } from './market-display';

/** 筛选 chip 取值: all=全部, recommended/updated 是"排序"的快捷方式, certified/free 是对
 * authRequired 的客户端筛选快捷方式(见 services::market::search 文档注释"已认证/免费筛选
 * 可由前端在已取回的当页数据上直接筛选") */
export type MarketChip = 'all' | 'recommended' | 'certified' | 'free' | 'updated';

const CHIPS: { value: MarketChip; label: string }[] = [
	{ value: 'all', label: '全部' },
	{ value: 'recommended', label: '推荐' },
	{ value: 'certified', label: '已认证' },
	{ value: 'free', label: '免费' },
	{ value: 'updated', label: '最近更新' },
];

/** 排序选项: value 与后端 SortBy 的 i64 编码一一对应(0-推荐, 1-星标数, 2-最近更新) */
const SORT_OPTIONS: { value: number; label: string }[] = [
	{ value: 0, label: '推荐' },
	{ value: 1, label: '星标数' },
	{ value: 2, label: '最近更新' },
];

const PAGE_SIZE_OPTIONS = [10, 20, 50];

/** 分页页码按钮列表: 总页数不多(<=7)时全量展示, 否则展示首尾各 2 页 + 当前页前后 1 页,
 * 中间空隙由调用方渲染省略号(与 components/installed/resource-list.tsx 同一算法, 因两个
 * feature 目录彼此独立各自维护, 不做跨目录引用) */
function pageNumbers(current: number, total: number): number[] {
	if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1);
	const set = new Set<number>([1, 2, total - 1, total, current - 1, current, current + 1]);
	return Array.from(set)
		.filter((n) => n >= 1 && n <= total)
		.sort((a, b) => a - b);
}

interface MarketListProps {
	/** market_search 当页命中(未经 chip 客户端筛选) */
	items: MarketResource[];
	/** 服务端总命中数(不受分页/chip 客户端筛选影响) */
	total: number;
	/** 分类下拉选项(由 pages/marketplace 从当前已加载数据派生, 不含"全部分类") */
	categories: string[];
	resTypeFilter: 'skill' | 'mcp';
	keyword: string;
	chip: MarketChip;
	category: string | undefined;
	sort: number;
	page: number;
	pageSize: number;
	/** 当前选中项的复合键(marketResourceKey), 无选中为 null */
	selectedKey: string | null;
	/** 复合键 -> 该资源最近一次安装失败的提示文案; 缺省为空对象。调用方(pages/marketplace)
	 * 目前只把安装错误喂给 MarketDetailPanel, 不重复喂给这里的卡片(提示文案本身即"请到详情页
	 * 登录", 在卡片上重复展示同一句会造成语义指向不清 + 同文案两处渲染的可及性噪音), 但此 prop
	 * 仍保留给调用方按需接入(如未来想在网格上也做轻量提示) */
	installErrors?: Record<string, string>;
	onResTypeFilterChange: (value: 'skill' | 'mcp') => void;
	onKeywordChange: (value: string) => void;
	onChipChange: (chip: MarketChip) => void;
	onCategoryChange: (category: string | undefined) => void;
	onSortChange: (sort: number) => void;
	onPageChange: (page: number) => void;
	onPageSizeChange: (pageSize: number) => void;
	onSelectItem: (item: MarketResource) => void;
	onDownload: (item: MarketResource) => void;
}

/** 资源中心左侧列表区: 搜索/分段/筛选/排序 + 卡片网格 + 分页, 还原原型第 2 屏 */
export function MarketList({
	items,
	total,
	categories,
	resTypeFilter,
	keyword,
	chip,
	category,
	sort,
	page,
	pageSize,
	selectedKey,
	installErrors = {},
	onResTypeFilterChange,
	onKeywordChange,
	onChipChange,
	onCategoryChange,
	onSortChange,
	onPageChange,
	onPageSizeChange,
	onSelectItem,
	onDownload,
}: MarketListProps) {
	// 已认证/免费两个 chip 是对当页数据的客户端筛选(见 MarketChip 文档), 故此处展示的卡片数可能
	// 少于上方"共 N 项结果"(N 为服务端未筛选的总命中数), 属已知的展示口径差异, 见本任务报告
	const visibleItems =
		chip === 'certified'
			? items.filter((item) => item.authRequired)
			: chip === 'free'
				? items.filter((item) => !item.authRequired)
				: items;

	const pageCount = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.min(Math.max(page, 1), pageCount);
	const currentSortLabel = SORT_OPTIONS.find((o) => o.value === sort)?.label ?? '推荐';

	return (
		<div className="flex h-full min-w-0 flex-1 flex-col gap-4">
			<div className="flex flex-wrap items-center gap-2">
				<div className="relative min-w-64 flex-1">
					<Search
						size={16}
						className="absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
					/>
					<Input
						value={keyword}
						onChange={(e) => onKeywordChange(e.target.value)}
						placeholder="搜索 Skills 和 MCP..."
						className="pl-8"
					/>
				</div>
				<Tabs
					value={resTypeFilter}
					onValueChange={(value) => onResTypeFilterChange(value as 'skill' | 'mcp')}
				>
					<TabsList>
						<TabsTrigger value="skill">Skills</TabsTrigger>
						<TabsTrigger value="mcp">MCP</TabsTrigger>
					</TabsList>
				</Tabs>
			</div>

			<div className="flex flex-wrap items-center gap-2">
				{CHIPS.map((c) => (
					<Button
						key={c.value}
						variant={chip === c.value ? 'default' : 'outline'}
						size="sm"
						onClick={() => onChipChange(c.value)}
					>
						{c.label}
					</Button>
				))}
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button variant="outline" size="sm">
							分类{category ? `: ${category}` : ''}
							<ChevronDown size={14} />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="start">
						<DropdownMenuItem onSelect={() => onCategoryChange(undefined)}>
							全部分类
						</DropdownMenuItem>
						{categories.map((cat) => (
							<DropdownMenuItem key={cat} onSelect={() => onCategoryChange(cat)}>
								{cat}
							</DropdownMenuItem>
						))}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>

			<div className="flex items-center justify-between gap-2 text-sm text-muted-foreground">
				<span>共 {total} 项结果</span>
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button variant="outline" size="sm">
							排序: {currentSortLabel}
							<ChevronDown size={14} />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						{SORT_OPTIONS.map((opt) => (
							<DropdownMenuItem
								key={opt.value}
								onSelect={() => onSortChange(opt.value)}
							>
								{opt.label}
							</DropdownMenuItem>
						))}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>

			<div className="grid min-h-0 flex-1 auto-rows-min grid-cols-2 gap-4 overflow-auto">
				{visibleItems.length === 0 ? (
					<p className="col-span-2 py-10 text-center text-sm text-muted-foreground">
						暂无匹配的资源
					</p>
				) : (
					visibleItems.map((item) => {
						const key = marketResourceKey(item);
						return (
							<MarketCard
								key={key}
								resource={item}
								selected={selectedKey === key}
								onSelect={onSelectItem}
								onDownload={onDownload}
								installError={installErrors[key]}
							/>
						);
					})
				)}
			</div>

			<div className="flex flex-wrap items-center justify-between gap-2">
				<div className="flex items-center gap-1">
					<Button
						variant="outline"
						size="icon-sm"
						disabled={currentPage <= 1}
						aria-label="上一页"
						onClick={() => onPageChange(currentPage - 1)}
					>
						<ChevronLeft size={14} />
					</Button>
					{pageNumbers(currentPage, pageCount).map((n, i, arr) => (
						<Fragment key={n}>
							{i > 0 && n - arr[i - 1] > 1 ? (
								<span className="px-1 text-sm text-muted-foreground">...</span>
							) : null}
							<Button
								variant={n === currentPage ? 'default' : 'outline'}
								size="icon-sm"
								onClick={() => onPageChange(n)}
							>
								{n}
							</Button>
						</Fragment>
					))}
					<Button
						variant="outline"
						size="icon-sm"
						disabled={currentPage >= pageCount}
						aria-label="下一页"
						onClick={() => onPageChange(currentPage + 1)}
					>
						<ChevronRight size={14} />
					</Button>
				</div>
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button variant="outline" size="sm">
							{pageSize} 条/页
							<ChevronDown size={14} />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						{PAGE_SIZE_OPTIONS.map((size) => (
							<DropdownMenuItem key={size} onSelect={() => onPageSizeChange(size)}>
								{size} 条/页
							</DropdownMenuItem>
						))}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
		</div>
	);
}
