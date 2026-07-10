// 文件作用: 已安装(Installed)界面(原型第 4 屏) —— 顶部标题+刷新, 左侧资源列表(ResourceList),
//           右侧详情面板(ResourceDetailPanel), 卸载二次确认(UninstallDialog); 数据经
//           library_list/resourceAgentLinks 获取, 操作后失效相关 Query 触发刷新
// 创建日期: 2026-07-09
import { useMemo, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { libraryList, resourceDelete, resourceSetEnabled, type Resource } from '@/api/library';
import { agentList } from '@/api/agent';
import { resourceAgentLinks, syncApply } from '@/api/sync';
import { ResourceDetailPanel } from '@/components/installed/resource-detail-panel';
import { ResourceList } from '@/components/installed/resource-list';
import { UninstallDialog } from '@/components/installed/uninstall-dialog';
import { Button } from '@/components/ui/button';
import { useUiStore } from '@/stores/ui';

/** 前端小写 ResourceKind -> 后端 library_list 的 res_type 数值编码(1-Skill, 2-Mcp) */
const RES_TYPE_CODE: Record<'skill' | 'mcp', number> = { skill: 1, mcp: 2 };

const LIBRARY_LIST_KEY = 'library-list';
const RESOURCE_AGENT_LINKS_KEY = 'resource-agent-links';
const AGENT_LIST_KEY = 'agent-list';

/** 已安装(Installed)界面: 还原原型第 4 屏 —— 分段筛选 + 搜索 + 资源表 + 详情面板 */
export default function Installed() {
	const queryClient = useQueryClient();
	const {
		typeFilter,
		keyword,
		selectedResourceId,
		setTypeFilter,
		setKeyword,
		setSelectedResourceId,
	} = useUiStore();
	const [pendingDelete, setPendingDelete] = useState<Resource | null>(null);

	const resourcesQuery = useQuery({
		queryKey: [LIBRARY_LIST_KEY, typeFilter, keyword],
		queryFn: () =>
			libraryList(
				typeFilter ? RES_TYPE_CODE[typeFilter] : undefined,
				keyword.trim() || undefined,
			),
	});

	const linksQuery = useQuery({
		queryKey: [RESOURCE_AGENT_LINKS_KEY],
		queryFn: resourceAgentLinks,
	});

	const agentsQuery = useQuery({
		queryKey: [AGENT_LIST_KEY],
		queryFn: agentList,
	});

	// 后端按 id 升序返回, 这里按最后更新时间倒序重排, 呼应原型"最近更新在前"的展示顺序
	const resources = useMemo(() => {
		const list = resourcesQuery.data ?? [];
		return [...list].sort((a, b) => b.updateTime.localeCompare(a.updateTime));
	}, [resourcesQuery.data]);

	const linkCountByResource = useMemo(() => {
		const counts = new Map<number, number>();
		for (const link of linksQuery.data ?? []) {
			counts.set(link.resourceId, (counts.get(link.resourceId) ?? 0) + 1);
		}
		return counts;
	}, [linksQuery.data]);

	const selectedResource = resources.find((r) => r.id === selectedResourceId) ?? null;

	const linkedAgentNames = useMemo(
		() =>
			(linksQuery.data ?? [])
				.filter((link) => link.resourceId === selectedResourceId)
				.map((link) => link.agentName),
		[linksQuery.data, selectedResourceId],
	);

	/** 启用/禁用与卸载都可能改变资源列表与关联展示, 统一失效这两处 Query */
	function invalidateResourceQueries() {
		queryClient.invalidateQueries({ queryKey: [LIBRARY_LIST_KEY] });
		queryClient.invalidateQueries({ queryKey: [RESOURCE_AGENT_LINKS_KEY] });
	}

	const toggleEnabledMutation = useMutation({
		mutationFn: (resource: Resource) => resourceSetEnabled(resource.id, !resource.enabled),
		onSuccess: invalidateResourceQueries,
	});

	const deleteMutation = useMutation({
		mutationFn: (resource: Resource) => resourceDelete(resource.id),
		onSuccess: (_result, resource) => {
			invalidateResourceQueries();
			if (selectedResourceId === resource.id) setSelectedResourceId(null);
			setPendingDelete(null);
		},
	});

	// "同步到全部 Agent": 按钮字面意思是全部, 取当前全部在线(status=true)的 Agent 一起同步
	// (而非仅限该资源已关联的 Agent), 与 sync_apply 逐 Agent 全量协调差异的既有语义一致
	const syncAllMutation = useMutation({
		mutationFn: () => {
			const onlineAgentIds = (agentsQuery.data ?? [])
				.filter((agent) => agent.status)
				.map((agent) => agent.id);
			return syncApply(onlineAgentIds);
		},
		onSuccess: invalidateResourceQueries,
	});

	return (
		<div className="flex h-full flex-col gap-4">
			<header className="flex items-center justify-between">
				<h1 className="text-2xl font-bold">已安装 / Installed</h1>
				<Button variant="outline" onClick={() => resourcesQuery.refetch()}>
					<RefreshCw
						size={14}
						className={resourcesQuery.isFetching ? 'animate-spin' : undefined}
					/>
					刷新
				</Button>
			</header>

			<div className="flex min-h-0 flex-1 gap-4">
				<ResourceList
					resources={resources}
					linkCountByResource={linkCountByResource}
					selectedId={selectedResourceId}
					typeFilter={typeFilter}
					keyword={keyword}
					onTypeFilterChange={setTypeFilter}
					onKeywordChange={setKeyword}
					onSelectResource={(resource) => setSelectedResourceId(resource.id)}
					onToggleEnabled={(resource) => toggleEnabledMutation.mutate(resource)}
					onRequestDelete={(resource) => setPendingDelete(resource)}
				/>
				{selectedResource ? (
					<ResourceDetailPanel
						resource={selectedResource}
						linkedAgentNames={linkedAgentNames}
						onClose={() => setSelectedResourceId(null)}
						onSyncToAllAgents={() => syncAllMutation.mutate()}
						onRequestDelete={(resource) => setPendingDelete(resource)}
						isSyncing={syncAllMutation.isPending}
					/>
				) : null}
			</div>

			<UninstallDialog
				resource={pendingDelete}
				onConfirm={() => pendingDelete && deleteMutation.mutate(pendingDelete)}
				onCancel={() => setPendingDelete(null)}
				isDeleting={deleteMutation.isPending}
			/>
		</div>
	);
}
