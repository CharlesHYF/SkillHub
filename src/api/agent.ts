// 文件作用: Agent(本机 AI 工具实例)相关 Tauri command 的类型化封装
// 创建日期: 2026-07-09
import { invoke } from '@tauri-apps/api/core';

/** Agent 种类, 与后端 AgentKind 枚举变体名一一对应 */
export type AgentKind =
	| 'ClaudeCode'
	| 'ClaudeDesktop'
	| 'Cursor'
	| 'Windsurf'
	| 'Cline'
	| 'VsCode'
	| 'GeminiCli'
	| 'Codex';

/** Agent 作用域: 全局 / 项目级, 与后端 AgentScope 枚举变体名一一对应 */
export type AgentScope = 'Global' | 'Project';

/** agent 表一行 */
export interface AgentRow {
	id: number;
	agentKind: AgentKind;
	name: string;
	configPath: string;
	scope: AgentScope;
	status: boolean;
	lastSyncTime: string;
	createTime: string;
	updateTime: string;
}

/** 探测本机全部已知 AI 工具实例并落库, 返回 Agent 表当前全量 */
export async function agentDetect(): Promise<AgentRow[]> {
	return invoke<AgentRow[]>('agent_detect');
}

/** 查询 Agent 表当前全量, 不触发探测 */
export async function agentList(): Promise<AgentRow[]> {
	return invoke<AgentRow[]>('agent_list');
}
