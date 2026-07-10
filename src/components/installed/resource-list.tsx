// 文件作用: 已安装界面左侧列表区 —— 分段筛选(全部/Skills/MCP) + 搜索 + 筛选/排序/批量操作
//           (占位) + 资源表(DataTable) + 分页; 纯展示 + 回调, 数据获取/选中态由 pages/installed
//           统一持有(便于本组件单测无需接入真实 Tauri/Query 环境)
// 创建日期: 2026-07-09
import { useEffect, useState, Fragment } from 'react';
import {
	Search,
	Filter,
	ArrowUpDown,
	ChevronDown,
	MoreVertical,
	ChevronLeft,
	ChevronRight,
	Sparkles,
	Plug,
} from 'lucide-react';

import type { Resource } from '@/api/library';
import type { ResourceTypeFilter } from '@/stores/ui';
import { DataTable, type DataTableColumn } from '@/components/common/data-table';
import { TypeBadge } from '@/components/common/type-badge';
import { SyncStatusBadge } from '@/components/common/sync-status-badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from '@/components/ui/select';
import { formatRelativeTime } from '@/lib/utils';
import {
	SOURCE_LABEL,
	deriveDescription,
	deriveSyncStatus,
	toResourceKind,
} from './resource-display';

const PAGE_SIZE_OPTIONS = [10, 20, 50];

interface ResourceListProps {
	resources: Resource[];
	/** resource.id -> 已关联(desired=1) Agent 数, 由 pages/installed 从 resourceAgentLinks 聚合而来 */
	linkCountByResource: Map<number, number>;
	selectedId: number | null;
	typeFilter: ResourceTypeFilter;
	keyword: string;
	onTypeFilterChange: (filter: ResourceTypeFilter) => void;
	onKeywordChange: (keyword: string) => void;
	onSelectResource: (resource: Resource) => void;
	onToggleEnabled: (resource: Resource) => void;
	onRequestDelete: (resource: Resource) => void;
}

/** 分页页码按钮列表: 总页数不多(<=7)时全量展示, 否则展示首尾各 2 页 + 当前页前后 1 页,
 * 中间的空隙由调用方渲染省略号, 避免页多时按钮铺满一整行 */
function pageNumbers(current: number, total: number): number[] {
	if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1);
	const set = new Set<number>([1, 2, total - 1, total, current - 1, current, current + 1]);
	return Array.from(set)
		.filter((n) => n >= 1 && n <= total)
		.sort((a, b) => a - b);
}

/** 已安装界面左侧列表区: 分段筛选 + 搜索 + 占位工具栏 + 资源表 + 分页 */
export function ResourceList({
	resources,
	linkCountByResource,
	selectedId,
	typeFilter,
	keyword,
	onTypeFilterChange,
	onKeywordChange,
	onSelectResource,
	onToggleEnabled,
	onRequestDelete,
}: ResourceListProps) {
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(PAGE_SIZE_OPTIONS[0]);
	const [checkedIds, setCheckedIds] = useState<Set<number>>(new Set());

	// 类型/关键字筛选变化会换一批数据, 分页回到第一页, 避免停在一个不存在的页码上
	useEffect(() => {
		setPage(1);
	}, [typeFilter, keyword]);

	const pageCount = Math.max(1, Math.ceil(resources.length / pageSize));
	const currentPage = Math.min(page, pageCount);
	const pageRows = resources.slice((currentPage - 1) * pageSize, currentPage * pageSize);
	const allChecked = pageRows.length > 0 && pageRows.every((r) => checkedIds.has(r.id));

	function toggleChecked(id: number, checked: boolean) {
		setCheckedIds((prev) => {
			const next = new Set(prev);
			if (checked) next.add(id);
			else next.delete(id);
			return next;
		});
	}

	function toggleAll(checked: boolean) {
		setCheckedIds((prev) => {
			const next = new Set(prev);
			for (const row of pageRows) {
				if (checked) next.add(row.id);
				else next.delete(row.id);
			}
			return next;
		});
	}

	const columns: DataTableColumn<Resource>[] = [
		{
			key: 'checkbox',
			header: (
				<Checkbox
					checked={allChecked}
					onCheckedChange={(checked) => toggleAll(checked === true)}
					aria-label="全选"
				/>
			),
			render: (row) => (
				<span onClick={(e) => e.stopPropagation()}>
					<Checkbox
						checked={checkedIds.has(row.id)}
						onCheckedChange={(checked) => toggleChecked(row.id, checked === true)}
						aria-label={`选中 ${row.name}`}
					/>
				</span>
			),
		},
		{
			key: 'name',
			header: '名称',
			render: (row) => {
				const description = deriveDescription(row);
				const Icon = row.resType === 'Mcp' ? Plug : Sparkles;
				return (
					<div className="flex items-center gap-3">
						<span
							className="flex size-9 shrink-0 items-center justify-center rounded-lg"
							style={{ background: 'var(--sh-brand-tint)' }}
						>
							<Icon size={16} color="var(--sh-brand)" />
						</span>
						<div className="min-w-0">
							<p className="truncate font-medium text-foreground">{row.name}</p>
							{description ? (
								<p className="truncate text-xs text-muted-foreground">
									{description}
								</p>
							) : null}
						</div>
					</div>
				);
			},
		},
		{
			key: 'type',
			header: '类型',
			render: (row) => <TypeBadge type={toResourceKind(row.resType)} />,
		},
		{
			key: 'version',
			header: '当前版本',
			render: (row) => row.version || '-',
		},
		{
			key: 'source',
			header: '来源',
			render: (row) => SOURCE_LABEL[row.sourceType],
		},
		{
			key: 'updateTime',
			header: '最后更新',
			render: (row) => formatRelativeTime(row.updateTime),
		},
		{
			key: 'syncStatus',
			header: '同步状态',
			render: (row) => <SyncStatusBadge status={deriveSyncStatus(row.enabled)} />,
		},
		{
			key: 'agentCount',
			header: '已关联 Agent',
			render: (row) => linkCountByResource.get(row.id) ?? 0,
		},
		{
			key: 'actions',
			header: '',
			render: (row) => (
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label={`${row.name} 操作`}
							onClick={(e) => e.stopPropagation()}
						>
							<MoreVertical size={16} />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
						<DropdownMenuItem onSelect={() => onSelectResource(row)}>
							查看详情
						</DropdownMenuItem>
						<DropdownMenuItem onSelect={() => onToggleEnabled(row)}>
							{row.enabled ? '禁用' : '启用'}
						</DropdownMenuItem>
						<DropdownMenuSeparator />
						<DropdownMenuItem
							variant="destructive"
							onSelect={() => onRequestDelete(row)}
						>
							卸载
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			),
		},
	];

	return (
		<div className="flex h-full min-w-0 flex-1 flex-col gap-4">
			<Tabs
				value={typeFilter ?? 'all'}
				onValueChange={(value) =>
					onTypeFilterChange(value === 'all' ? undefined : (value as 'skill' | 'mcp'))
				}
			>
				<TabsList variant="line">
					<TabsTrigger value="all">全部</TabsTrigger>
					<TabsTrigger value="skill">Skills</TabsTrigger>
					<TabsTrigger value="mcp">MCP</TabsTrigger>
				</TabsList>
			</Tabs>

			<div className="flex flex-wrap items-center gap-2">
				<div className="relative min-w-64 flex-1">
					<Search
						size={16}
						className="absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
					/>
					<Input
						value={keyword}
						onChange={(e) => onKeywordChange(e.target.value)}
						placeholder="搜索名称、描述或关键字"
						className="pl-8"
					/>
					<span className="absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-muted-foreground">
						⌘K
					</span>
				</div>
				<Button variant="outline" size="sm" disabled title="M2 再实现">
					<Filter size={14} />
					筛选
				</Button>
				<Button variant="outline" size="sm" disabled title="M2 再实现">
					<ArrowUpDown size={14} />
					排序
				</Button>
				<Button variant="outline" size="sm" disabled title="M1 不强求批量操作">
					批量操作
					<ChevronDown size={14} />
				</Button>
			</div>

			<div className="min-h-0 flex-1 overflow-auto rounded-lg border">
				{resources.length === 0 ? (
					<p className="py-6 text-center text-sm text-muted-foreground">暂无匹配的资源</p>
				) : (
					<DataTable
						columns={columns}
						rows={pageRows}
						rowKey={(row) => row.id}
						onRowClick={onSelectResource}
						selectedRowKey={selectedId ?? undefined}
					/>
				)}
			</div>

			<div className="flex flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground">
				<span>共 {resources.length} 项</span>
				<div className="flex items-center gap-1">
					<Button
						variant="outline"
						size="icon-sm"
						disabled={currentPage <= 1}
						aria-label="上一页"
						onClick={() => setPage(currentPage - 1)}
					>
						<ChevronLeft size={14} />
					</Button>
					{pageNumbers(currentPage, pageCount).map((n, i, arr) => (
						<Fragment key={n}>
							{i > 0 && n - arr[i - 1] > 1 ? <span className="px-1">...</span> : null}
							<Button
								variant={n === currentPage ? 'default' : 'outline'}
								size="icon-sm"
								onClick={() => setPage(n)}
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
						onClick={() => setPage(currentPage + 1)}
					>
						<ChevronRight size={14} />
					</Button>
				</div>
				<Select
					value={String(pageSize)}
					onValueChange={(value) => {
						setPageSize(Number(value));
						setPage(1);
					}}
				>
					<SelectTrigger size="sm">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{PAGE_SIZE_OPTIONS.map((size) => (
							<SelectItem key={size} value={String(size)}>
								{size} / 页
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</div>
		</div>
	);
}
