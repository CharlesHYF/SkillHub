// 文件作用: 同步相关 Tauri command 的类型化封装 + 同步进度事件("sync://progress")订阅
// 创建日期: 2026-07-09
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { ResourceType } from './library';

/** diff 动作, 与后端 DiffAction 枚举变体名一一对应 */
export type DiffAction = 'Add' | 'Update' | 'Remove';

/** 单个 MCP 服务器定义, 与后端 McpServerDef 结构一一对应 */
export interface McpServerDef {
	name: string;
	command: string | null;
	args: string[];
	env: Record<string, string>;
	url: string | null;
}

/** 期望资源的可落地内容, 与后端 DesiredPayload 枚举一一对应(外部标签形式) */
export type DesiredPayload = { Mcp: McpServerDef } | { Skill: { srcDir: string } };

/** 一条资源相对某 Agent 实际态的差异 */
export interface DiffItem {
	resType: ResourceType;
	name: string;
	action: DiffAction;
	localVer: string;
	agentVer: string;
	payload: DesiredPayload | null;
}

/** 一次同步中某 Agent 待处理的完整差异计划 */
export interface DiffPlan {
	items: DiffItem[];
}

/** 一次同步应用的结果汇总(sync_apply 返回的是各 Agent 结果相加后的总计) */
export interface SyncSummary {
	success: number;
	failed: number;
	skipped: number;
}

/** 同步进度事件负载, 经 "sync://progress" 频道推送 */
export interface SyncProgress {
	agentId: number;
	done: number;
	total: number;
	currentName: string;
}

/** 资源-Agent 关联展示行(仅取期望态 desired=1), 供"已安装"界面一次性统计每个资源已关联的
 * Agent 数量与详情面板的关联列表, 避免逐资源单独查询(N+1) */
export interface ResourceAgentLink {
	resourceId: number;
	agentId: number;
	agentName: string;
}

/** 一次性查询全部资源的关联 Agent(仅期望态), 供"已安装"界面复用同一份数据
 * (表格"已关联 Agent 数"列 + 详情面板"已关联 Agent"列表) */
export async function resourceAgentLinks(): Promise<ResourceAgentLink[]> {
	return invoke<ResourceAgentLink[]>('resource_agent_links');
}

/** 设置某资源相对某 Agent 的期望态(desired: 应存在/不应存在) */
export async function assocSet(
	resourceId: number,
	agentId: number,
	desired: boolean,
): Promise<void> {
	return invoke('assoc_set', { resourceId, agentId, desired });
}

/** 计算某 Agent 的期望态与其配置文件实际态之间的差异计划 */
export async function syncDiff(agentId: number): Promise<DiffPlan> {
	return invoke<DiffPlan>('sync_diff', { agentId });
}

/** 对给定 Agent 列表逐一应用同步, 返回各 Agent 结果相加后的总计; 过程中会持续推送
 * "sync://progress" 事件, 需配合 onSyncProgress 订阅才能拿到实时进度 */
export async function syncApply(agentIds: number[]): Promise<SyncSummary> {
	return invoke<SyncSummary>('sync_apply', { agentIds });
}

/** 订阅同步进度事件("sync://progress"), 返回取消订阅函数(组件卸载时应调用) */
export async function onSyncProgress(cb: (progress: SyncProgress) => void): Promise<UnlistenFn> {
	return listen<SyncProgress>('sync://progress', (event) => cb(event.payload));
}
