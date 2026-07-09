// 文件作用: 本地库(Skill/MCP 资源)相关 Tauri command 的类型化封装
// 创建日期: 2026-07-09
import { invoke } from '@tauri-apps/api/core';

/** 资源类型, 与后端 ResourceType 枚举变体名一一对应 */
export type ResourceType = 'Skill' | 'Mcp';

/** 资源来源, 与后端 SourceType 枚举变体名一一对应 */
export type SourceType = 'LocalImport' | 'Official' | 'ThirdParty';

/** resource 表一行 */
export interface Resource {
	id: number;
	resType: ResourceType;
	name: string;
	displayName: string;
	version: string;
	sourceType: SourceType;
	localPath: string;
	enabled: boolean;
	createTime: string;
	updateTime: string;
}

/** 本地库 Skill/MCP 各自数量统计 */
export interface LibraryCounts {
	skill: number;
	mcp: number;
}

/** 按类型/关键字查询本地库资源列表; 均缺省表示不过滤 */
export async function libraryList(resType?: number, keyword?: string): Promise<Resource[]> {
	return invoke<Resource[]>('library_list', {
		resType: resType ?? null,
		keyword: keyword ?? null,
	});
}

/** 按主键查询单条资源, 不存在返回 null */
export async function libraryGet(id: number): Promise<Resource | null> {
	return invoke<Resource | null>('library_get', { id });
}

/** 统计本地库 Skill/MCP 各自数量, 供首页/侧栏角标展示 */
export async function libraryCounts(): Promise<LibraryCounts> {
	return invoke<LibraryCounts>('library_counts');
}

/** 把本地路径(MCP 单定义 json 文件或含 SKILL.md 的 Skill 目录)导入为一条资源 */
export async function resourceImportLocal(path: string): Promise<Resource> {
	return invoke<Resource>('resource_import_local', { path });
}

/** 设置资源启用/禁用状态 */
export async function resourceSetEnabled(id: number, enabled: boolean): Promise<void> {
	return invoke('resource_set_enabled', { id, enabled });
}

/** 删除一条资源: 删库记录 + 清理其在 SkillHub 存储目录下的内容 */
export async function resourceDelete(id: number): Promise<void> {
	return invoke('resource_delete', { id });
}
