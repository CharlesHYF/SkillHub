// 文件作用: 市场(Marketplace)相关 Tauri command 的类型化封装 —— 搜索(分页)/详情查询/刷新缓存;
//           另预置 marketInstall 的调用约定(对应后端 market_install 命令由并行任务实现, 本文件
//           先行定义前端类型与调用形状, 待后端合入后即可直接生效); parseAuthRequiredProvider
//           解析 marketInstall 因鉴权失败拒绝时 "AUTH_REQUIRED:<provider>" 的错误约定, 供
//           pages/marketplace-detail 定位需要弹出 AuthModal(components/auth/auth-modal)完成
//           认证的 provider
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { invoke } from '@tauri-apps/api/core';

import type { ResourceRespVO, ResourceType } from './library';
import type { McpServerDef } from './sync';

/** 市场资源来源, 与后端 domain::market::SourceId 枚举变体名一一对应 */
export type MarketSourceType = 'GithubSkills' | 'McpRegistry' | 'GithubMcp';

/** 安装清单: 归一化的"如何安装这条市场资源"描述, 与后端 InstallManifest 外部标签枚举一一对应
 * (标签保持 PascalCase, 仅字段名转 camelCase, 与 api/sync.ts 的 DesiredPayload 同一约定) */
export type InstallManifest =
	| { Skill: { repo: string; path: string; gitRef: string } }
	| { Mcp: { serverDef: McpServerDef } }
	| { McpTemplate: { serverDef: McpServerDef; requiredEnv: string[] } };

/** 市场资源: 一条可浏览/可安装的 Skill/MCP, 与后端 domain::market::MarketResourceRespVO 一一对应 */
export interface MarketResourceRespVO {
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
	items: MarketResourceRespVO[];
	total: number;
}

/** market_refresh 结果: 本次刷新写入市场缓存的资源条数 */
export interface MarketRefreshRespVO {
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
): Promise<MarketResourceRespVO | null> {
	return invoke<MarketResourceRespVO | null>('market_detail', { sourceType, extId });
}

/** 刷新市场缓存: 并发拉取三源(github_skills/mcp_registry/github_mcp)全量资源并写入本地缓存 */
export async function marketRefresh(): Promise<MarketRefreshRespVO> {
	return invoke<MarketRefreshRespVO>('market_refresh');
}

/** 下载并安装一条市场资源, 落地为本地库的一条 ResourceRespVO; envOverrides 供 McpTemplate 类资源
 * 填充 installManifest.requiredEnv 所需的环境变量取值(非 McpTemplate 资源可不传)。
 * 对应的后端 market_install 命令由并行任务实现, 本函数先行接好调用约定: 若因鉴权失败被拒绝,
 * 约定拒绝原因形如 "AUTH_REQUIRED:<provider>"(provider 为 domain::auth::ProviderKind 的
 * i64 编码), 调用方应以 parseAuthRequiredProvider 解析出 provider 后打开 AuthModal(见
 * pages/marketplace-detail 与 components/auth/auth-modal), 完成认证后重试本函数即可 */
export async function marketInstall(
	sourceType: number,
	extId: string,
	envOverrides?: Record<string, string>,
): Promise<ResourceRespVO> {
	return invoke<ResourceRespVO>('market_install', {
		sourceType,
		extId,
		envOverrides: envOverrides ?? null,
	});
}

/** market_install 因鉴权失败拒绝时的错误前缀约定, 与 parseAuthRequiredProvider 配套 */
const AUTH_REQUIRED_PREFIX = 'AUTH_REQUIRED:';

/** 从 marketInstall 的拒绝原因中解析出需要完成认证的 provider 数值编码(与 api/auth.ts 的
 * authLogin/authEnterToken 同一编码约定); 无法识别为 AUTH_REQUIRED 错误时返回 null, 调用方
 * 应按普通安装失败处理, 不误判为需要登录。Tauri command 的 Err(String) 拒绝值可能是裸字符串,
 * 也可能被上层包成 Error 实例(如测试里的 mockRejectedValue(new Error(...))), 两种形态都兼容
 * 取出文本再匹配前缀; 前缀匹配但取不出合法数字时兜底为 1(GitHub), 与后端
 * domain::auth::ProviderKind::from_i64 对未知值兜底 GitHub 同一防御性约定 */
export function parseAuthRequiredProvider(error: unknown): number | null {
	const message = error instanceof Error ? error.message : String(error);
	if (!message.startsWith(AUTH_REQUIRED_PREFIX)) return null;
	const raw = message.slice(AUTH_REQUIRED_PREFIX.length).trim();
	// Number('') === 0(而非 NaN), 空串必须先单独兜底, 否则会被误判成"合法解析出 0"
	if (!raw) return 1;
	const provider = Number(raw);
	return Number.isFinite(provider) ? provider : 1;
}
