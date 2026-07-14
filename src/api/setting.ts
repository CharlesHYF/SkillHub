// 文件作用: 设置(SettingRespVO)相关 Tauri command 的类型化封装 —— 读取设置(settings_get)、保存设置
//           (settings_save)、读取应用版本号(app_version)。SettingRespVO 字段形状与后端
//           domain::setting::SettingRespVO 逐字段对齐(serde camelCase 序列化, 字段顺序无关)
// 创建日期: 2026-07-10
import { invoke } from '@tauri-apps/api/core';

/** 网络代理模式: 0 系统默认 / 1 不使用 / 2 手动 */
export type ProxyMode = 0 | 1 | 2;

/** 更新通道: 0 Stable(稳定版) / 1 Beta(测试版) */
export type UpdateChannel = 0 | 1;

/** 应用设置, 与后端 domain::setting::SettingRespVO 一一对应(该结构体标了 #[serde(rename_all =
 * "camelCase")], 故 storage_skill_dir -> storageSkillDir 等) */
export interface SettingRespVO {
	/** 本地 Skill 目录: 存放下载的 Skill 包与配置文件 */
	storageSkillDir: string;
	/** 本地 MCP 目录: 存放 MCP 服务与配置文件 */
	storageMcpDir: string;
	/** 有新的 Agent 加入时, 是否自动同步已启用的 Skill 与 MCP */
	syncAutoNewAgent: boolean;
	/** 应用启动时是否检查 Skill 与 MCP 的更新 */
	syncCheckUpdateOnStart: boolean;
	/** 同步冲突时是否显示提示, 需手动确认后继续 */
	syncConflictPrompt: boolean;
	/** 是否仅同步当前已启用的 Skill 与 MCP(忽略未启用项) */
	syncOnlyEnabled: boolean;
	netProxyMode: ProxyMode;
	/** HTTP 代理地址, 如 http://host:port; 空串表示未设置 */
	netHttpProxy: string;
	/** HTTPS 代理地址, 如 http://host:port; 空串表示未设置 */
	netHttpsProxy: string;
	/** 不使用代理的地址列表(逗号分隔), 如 localhost, 127.0.0.1, *.local */
	netNoProxy: string;
	/** 请求超时时间(秒) */
	netTimeoutSec: number;
	updateChannel: UpdateChannel;
}

/** 读取当前设置(未曾保存过时, 后端返回其内置默认值) */
export async function settingsGet(): Promise<SettingRespVO> {
	return invoke<SettingRespVO>('settings_get');
}

/** 整份保存设置(全量覆盖), 返回落库后的设置(供调用方以此为最新基准) */
export async function settingsSave(settings: SettingRespVO): Promise<SettingRespVO> {
	return invoke<SettingRespVO>('settings_save', { settings });
}

/** 读取当前应用版本号(如 "0.1.0") */
export async function appVersion(): Promise<string> {
	return invoke<string>('app_version');
}
