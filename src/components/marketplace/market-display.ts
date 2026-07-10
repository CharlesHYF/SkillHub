// 文件作用: Marketplace 展示态的派生逻辑(来源编码互转/类型徽标映射/星标数格式化/安装要求/
//           认证说明), 供 market-card/market-list/market-detail-panel 与 pages/marketplace 共用,
//           避免同一套映射写多份
// 创建日期: 2026-07-10
import type { InstallManifest, MarketResource, MarketSourceType } from '@/api/market';
import type { McpServerDef } from '@/api/sync';
import type { ResourceKind } from '@/components/common/type-badge';

/** 市场资源来源编码, 与后端 domain::market::SourceId 的 i64 互转约定一一对应
 * (1-GithubSkills, 2-McpRegistry, 3-GithubMcp) */
const MARKET_SOURCE_TYPE_CODE: Record<MarketSourceType, number> = {
	GithubSkills: 1,
	McpRegistry: 2,
	GithubMcp: 3,
};

/** 把 MarketResource.sourceType(字符串变体)转回 market_detail/market_install 等命令所需的
 * i64 编码入参 */
export function sourceTypeToCode(sourceType: MarketSourceType): number {
	return MARKET_SOURCE_TYPE_CODE[sourceType];
}

/** 由 sourceType + extId 拼出一条市场资源的复合唯一键, 供选中态/安装错误态按 key 索引
 * (市场资源没有 resource.id 那样的整数主键, 见 domain::market::MarketResource 的复合唯一键设计) */
export function marketResourceKey(resource: Pick<MarketResource, 'sourceType' | 'extId'>): string {
	return `${resource.sourceType}:${resource.extId}`;
}

/** 后端 ResourceType('Skill'|'Mcp', 首字母大写) -> TypeBadge 所需的 ResourceKind(小写);
 * 与 components/installed/resource-display.ts 的同名函数逻辑一致, 因两个 feature 目录彼此独立,
 * 这里保留一份轻量副本, 不做跨 feature 目录引用 */
export function toResourceKind(resType: MarketResource['resType']): ResourceKind {
	return resType.toLowerCase() as ResourceKind;
}

/** 数字保留 1 位小数并去掉多余的 ".0"(如 12.0 -> "12") */
function trimTrailingZero(value: number): string {
	return value.toFixed(1).replace(/\.0$/, '');
}

/** 星标数展示格式化: >=100 万用 m 后缀, >=1000 用 k 后缀, 均保留 1 位小数并去掉多余的 .0,
 * 否则原样展示整数; 与原型截图的数字格式(如 "12.3k")一致 */
export function formatStars(stars: number): string {
	if (stars >= 1_000_000) {
		return `${trimTrailingZero(stars / 1_000_000)}m`;
	}
	if (stars >= 1000) {
		return `${trimTrailingZero(stars / 1000)}k`;
	}
	return String(stars);
}

/** 由 McpServerDef 派生安装要求展示行: 有本地启动命令则展示命令+参数, 有远程地址则展示地址
 * (两者理论上互斥, 见 domain::agent::McpServerDef 的 command/url 语义) */
function mcpServerRequirementLines(serverDef: McpServerDef): string[] {
	const lines: string[] = [];
	if (serverDef.command) {
		const args = serverDef.args.length > 0 ? ` ${serverDef.args.join(' ')}` : '';
		lines.push(`启动命令: ${serverDef.command}${args}`);
	}
	if (serverDef.url) {
		lines.push(`远程地址: ${serverDef.url}`);
	}
	return lines;
}

/** 由安装清单派生"安装要求"展示行: 三种 variant 各自呈现其真实可用的信息(仓库/子目录/版本引用,
 * 或启动命令/远程地址, 以及 McpTemplate 需要用户填充的环境变量名)。不填充原型截图中那些当前
 * 领域模型未提供数据来源的通用占位项(如"SkillHub 版本"/"运行环境"/"权限"), 避免展示虚构信息,
 * 见本任务报告"与原型差异"一节 */
export function deriveInstallRequirements(resource: MarketResource): string[] {
	const manifest: InstallManifest = resource.installManifest;
	if ('Skill' in manifest) {
		const { repo, path, gitRef } = manifest.Skill;
		return [`来源仓库: ${repo}`, `子目录: ${path}`, `版本引用: ${gitRef}`];
	}
	if ('Mcp' in manifest) {
		return mcpServerRequirementLines(manifest.Mcp.serverDef);
	}
	const { serverDef, requiredEnv } = manifest.McpTemplate;
	const lines = mcpServerRequirementLines(serverDef);
	if (requiredEnv.length > 0) {
		lines.push(`需配置环境变量: ${requiredEnv.join(', ')}`);
	}
	return lines;
}

/** 认证与授权说明文案: 直接由 authRequired 派生, 需要授权时的措辞与原型截图一致, 无需授权则
 * 给出对应的中性说明, 不引入当前领域模型未提供的独立字段 */
export function deriveAuthNotice(resource: MarketResource): string {
	return resource.authRequired
		? '部分功能需要授权访问第三方服务, 若需要登录或授权, 将在 SkillHub 内部打开完成。'
		: '该资源无需登录或授权, 可直接下载安装。';
}
