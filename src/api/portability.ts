// 文件作用: 导入导出(Import/Export)相关 Tauri command 的类型化封装 —— 导出包(export_bundle)、
//           导入预览(import_preview)、导入执行(import_bundle)、导入导出历史(impexp_history)。
//           ManifestRespVO 字段形状取自 M3 计划 Task 1(domain::portability::ManifestRespVO, camelCase
//           序列化); ImportOutcomeRespVO 与后端 domain::portability::ImportOutcomeRespVO 逐字段对齐(见类型注释)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { invoke } from '@tauri-apps/api/core';

/** 导出目标文件格式: 1 zip / 2 json / 3 tar */
export type BundleFormat = 1 | 2 | 3;

/** 导出范围: 0 全部数据 / 1 按类型选择 / 2 按时间范围 */
export type Scope = 0 | 1 | 2;

/** 导入冲突处理策略: 0 覆盖(推荐) / 1 跳过 / 2 保留两者 */
export type ConflictStrategy = 0 | 1 | 2;

/** 导出选项, 与后端 export_bundle 的 options 参数一一对应 */
export interface ExportReqVO {
	/** 导出全部 Skill */
	includeSkills: boolean;
	/** 导出全部 MCP */
	includeMcp: boolean;
	scope: Scope;
	format: BundleFormat;
	/** 是否包含 SkillHub 自身配置(如 Agent 列表等) */
	includeConfig: boolean;
	/** 是否包含各 Skill/MCP 的精确版本号(版本锁定) */
	includeVersionLock: boolean;
}

/** 导出内容计数, 与 ImportPreviewRespVO 的 skill/mcp/config/agent 同口径 */
export interface ManifestCounts {
	skill: number;
	mcp: number;
	config: number;
	agent: number;
}

/** 导出清单: export_bundle 的返回值(字段对应 M3 计划 Task 1 的 domain::portability::ManifestRespVO,
 * camelCase 序列化)。M3 前端暂不在 UI 上展示其字段(导出后的记录改走 impexpHistory 表刷新得到),
 * 仅用 Promise 是否 resolve 判断导出是否成功 */
export interface ManifestRespVO {
	schemaVersion: number;
	exportedAt: string;
	counts: ManifestCounts;
	checksums: Record<string, string>;
}

/** 导入内容预览: 将要导入的各类内容计数 + 包结构/版本 schema 校验结果 */
export interface ImportPreviewRespVO {
	skill: number;
	mcp: number;
	config: number;
	agent: number;
	/** 导入包结构/版本 schema 校验是否通过; 为 false 时应提示风险, 不宜直接放行导入 */
	schemaOk: boolean;
}

/** 一次导入执行的结果, 与后端 domain::portability::ImportOutcomeRespVO 逐字段对齐
 * (解析/校验硬失败在 importBundle 之前就已抛错, 故 status 不含 0 失败) */
export interface ImportOutcomeRespVO {
	/** 新导入(含覆盖策略下的覆盖)的资源数 */
	imported: number;
	/** 因同名已存在而跳过的资源数(跳过策略) */
	skipped: number;
	/** 因冲突改名后落地的资源数(保留两者策略) */
	renamed: number;
	/** 结果状态: 1 成功 / 2 部分成功(如某些关联在本机找不到对应 Agent), 与 ImpexpRespVO.status 同口径 */
	status: 1 | 2;
}

/** 一条导入导出历史记录 */
export interface ImpexpRespVO {
	id: number;
	/** 操作方向: 0 导出 / 1 导入 */
	direction: 0 | 1;
	fileName: string;
	fileFormat: BundleFormat;
	/** 内容摘要, 后端已格式化好的展示文案(如 "Skill 128 · MCP 45 · 配置 23 · Agent 8") */
	summary: string;
	/** 结果状态: 0 失败 / 1 成功 / 2 部分成功 */
	status: 0 | 1 | 2;
	runTime: string;
}

/** 按 options 导出一个包到 outPath, 返回导出清单 */
export async function exportBundle(options: ExportReqVO, outPath: string): Promise<ManifestRespVO> {
	return invoke<ManifestRespVO>('export_bundle', { options, outPath });
}

/** 预览某导入包路径将会导入的内容, 不落地写入 */
export async function importPreview(path: string): Promise<ImportPreviewRespVO> {
	return invoke<ImportPreviewRespVO>('import_preview', { path });
}

/** 按冲突策略执行导入; autoSync 表示导入完成后是否自动同步到各 Agent */
export async function importBundle(
	path: string,
	strategy: ConflictStrategy,
	autoSync: boolean,
): Promise<ImportOutcomeRespVO> {
	return invoke<ImportOutcomeRespVO>('import_bundle', { path, strategy, autoSync });
}

/** 查询最近 limit 条导入导出历史记录 */
export async function impexpHistory(limit: number): Promise<ImpexpRespVO[]> {
	return invoke<ImpexpRespVO[]>('impexp_history', { limit });
}
