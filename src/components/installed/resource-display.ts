// 文件作用: 已安装界面资源展示态的派生逻辑(来源文案/描述兜底/类型与同步状态映射),
//           供 resource-list/resource-detail-panel 两处共用, 避免同一套映射写两遍
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import type { ResourceRespVO, ResourceType, SourceType } from '@/api/library';
import type { ResourceKind } from '@/components/common/type-badge';
import type { SyncStatus } from '@/components/common/sync-status-badge';

/** 来源类型 -> 中文展示文案, 措辞与原型截图一致 */
export const SOURCE_LABEL: Record<SourceType, string> = {
	LocalImport: '本地导入',
	Official: '官方仓库',
	ThirdParty: '第三方仓库',
};

/** 资源描述兜底: 后端 resource 表暂无独立的 description 字段(见后端 domain::resource::ResourceRespVO
 * 与迁移脚本 0001_init.sql), displayName 在当前导入逻辑下默认与 name 相同; 仅当两者确实不同时
 * 才把 displayName 当描述展示, 避免与主标题重复展示同一串文本 */
export function deriveDescription(resource: ResourceRespVO): string | undefined {
	if (resource.displayName && resource.displayName !== resource.name) {
		return resource.displayName;
	}
	return undefined;
}

/** 后端 ResourceType('Skill'|'Mcp', 首字母大写) -> TypeBadge 所需的 ResourceKind(小写) */
export function toResourceKind(resType: ResourceType): ResourceKind {
	return resType.toLowerCase() as ResourceKind;
}

/** 由资源启用态派生同步状态展示值。M1 简化: 后端 resource_agent.sync_status 是"资源-Agent"
 * 关联行上的字段(每个关联各自的状态), 而非资源级的单一状态; 在按 Agent 聚合的口径尚未接入
 * 前端前, 这里先用 resource.enabled 派生二态(已禁用/已同步), 精确的待同步/本地修改/失败态
 * 需要后续任务在聚合口径明确后补齐 */
export function deriveSyncStatus(enabled: boolean): SyncStatus {
	return enabled ? '已同步' : '已禁用';
}
