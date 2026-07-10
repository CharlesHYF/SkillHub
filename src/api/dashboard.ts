// 文件作用: 首页汇总相关 Tauri command 的类型化封装
// 创建日期: 2026-07-09
import { invoke } from '@tauri-apps/api/core';

/** 首页统计卡片数据 */
export interface DashboardSummary {
	skillCount: number;
	mcpCount: number;
	agentCount: number;
	onlineCount: number;
	pendingCount: number;
}

/** activity_log 表一行 */
export interface ActivityRow {
	id: number;
	actType: number;
	resType: number;
	title: string;
	detail: string;
	createTime: string;
}

/** 查询首页统计卡片数据(Skill/MCP 数量、Agent 总数与在线数、待同步数) */
export async function dashboardSummary(): Promise<DashboardSummary> {
	return invoke<DashboardSummary>('dashboard_summary');
}

/** 查询最近若干条活动记录, 供首页"最近变更"列表 */
export async function activityRecent(limit: number): Promise<ActivityRow[]> {
	return invoke<ActivityRow[]>('activity_recent', { limit });
}
