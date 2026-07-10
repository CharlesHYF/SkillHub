// 文件作用: 市场(Marketplace)相关 Tauri command 的类型化封装 —— 搜索(分页)/详情查询/刷新缓存;
//           另预置 marketInstall 的调用约定(对应后端 market_install 命令由并行任务实现, 本文件
//           先行定义前端类型与调用形状, 待后端合入后即可直接生效)
// 创建日期: 2026-07-10
import { invoke } from '@tauri-apps/api/core';

import type { Resource, ResourceType } from './library';
import type { McpServerDef } from './sync';

/** 市场资源来源, 与后端 domain::market::SourceId 枚举变体名一一对应 */
export type MarketSourceType = 'GithubSkills' | 'McpRegistry' | 'GithubMcp';

/** 安装清单: 归一化的"如何安装这条市场资源"描述, 与后端 InstallManifest 外部标签枚举一一对应
 * (标签保持 PascalCase, 仅字段名转 camelCase, 与 api/sync.ts 的 DesiredPayload 同一约定) */
export type InstallManifest =
	| { Skill: { repo: string; path: string; gitRef: string } }
	| { Mcp: { serverDef: McpServerDef } }
	| { McpTemplate: { serverDef: McpServerDef; requiredEnv: string[] } };

/** 市场资源: 一条可浏览/可安装的 Skill/MCP, 与后端 domain::market::MarketResource 一一对应 */
export interface MarketResource {
	sourceType: MarketSourceType;
	resType: ResourceType;
	/** 来源内唯一标识(如 "owner/repo:path"), 与 sourceType 组合唯一定位一条市场资源 */
	extId: string;
	name: string;
	displayName: string;
	description: string;
	author: string;
	version: string;
	stars: number;
	category: string;
	tags: string[];
	authRequired: boolean;
	installManifest: InstallManifest;
	/** 该资源在来源侧的最后更新时间, 非本地缓存拉取时间 */
	updatedAt: string;
}

/** market_search 查询参数; keyword/resType/category 缺省表示不按该维度过滤。resType 为后端
 * ResourceType 的 i64 编码(1-Skill, 2-Mcp), sort 为后端 SortBy 的 i64 编码
 * (0-推荐, 1-星标数, 2-最近更新), 与 pages/marketplace.tsx 的编码约定保持一致 */
export interface MarketSearchParams {
	keyword?: string;
	resType?: number;
	category?: string;
	sort: number;
	page: number;
	pageSize: number;
}

/** market_search 分页结果: items 为本页命中的市场资源, total 为该组过滤条件下的总命中数
 * (不受分页影响) */
export interface MarketSearchResult {
	items: MarketResource[];
	total: number;
}

/** market_refresh 结果: 本次刷新写入市场缓存的资源条数 */
export interface MarketRefreshResult {
	count: number;
}

/** 按过滤/排序/分页条件搜索市场资源缓存 */
export async function marketSearch(params: MarketSearchParams): Promise<MarketSearchResult> {
	return invoke<MarketSearchResult>('market_search', {
		keyword: params.keyword ?? null,
		resType: params.resType ?? null,
		category: params.category ?? null,
		sort: params.sort,
		page: params.page,
		pageSize: params.pageSize,
	});
}

/** 按 (sourceType, extId) 查询单条市场资源详情; sourceType 为 SourceId 的 i64 编码,
 * 不存在时返回 null */
export async function marketDetail(
	sourceType: number,
	extId: string,
): Promise<MarketResource | null> {
	return invoke<MarketResource | null>('market_detail', { sourceType, extId });
}

/** 刷新市场缓存: 并发拉取三源(github_skills/mcp_registry/github_mcp)全量资源并写入本地缓存 */
export async function marketRefresh(): Promise<MarketRefreshResult> {
	return invoke<MarketRefreshResult>('market_refresh');
}

/** 下载并安装一条市场资源, 落地为本地库的一条 Resource; envOverrides 供 McpTemplate 类资源
 * 填充 installManifest.requiredEnv 所需的环境变量取值(非 McpTemplate 资源可不传)。
 * 对应的后端 market_install 命令由并行任务实现, 本函数先行接好调用约定: 若返回鉴权类错误,
 * 调用方应引导用户先完成登录/授权(见 pages/marketplace.tsx 的占位提示, 正式的认证弹窗
 * 由后续任务实现) */
export async function marketInstall(
	sourceType: number,
	extId: string,
	envOverrides?: Record<string, string>,
): Promise<Resource> {
	return invoke<Resource>('market_install', {
		sourceType,
		extId,
		envOverrides: envOverrides ?? null,
	});
}
