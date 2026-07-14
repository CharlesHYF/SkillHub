// 文件作用: agent api 层单测
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => []),
}));

import { agentDetect, agentList, type AgentRespVO } from './agent';

const sampleAgent: AgentRespVO = {
	id: 1,
	agentKind: 'ClaudeCode',
	name: 'Claude Code',
	configPath: '/home/demo/.claude.json',
	scope: 'Global',
	status: true,
	lastSyncTime: '',
	createTime: '2026-07-09 00:00:00',
	updateTime: '2026-07-09 00:00:00',
};

describe('agent api', () => {
	it('agentDetect 以 command 名 agent_detect 调用并返回探测落库后的结果', async () => {
		vi.mocked(invoke).mockResolvedValueOnce([sampleAgent]);
		const rows = await agentDetect();
		expect(rows).toEqual([sampleAgent]);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('agent_detect');
	});

	it('agentList 以 command 名 agent_list 调用并返回全量列表', async () => {
		vi.mocked(invoke).mockResolvedValueOnce([sampleAgent]);
		const rows = await agentList();
		expect(rows).toEqual([sampleAgent]);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('agent_list');
	});
});
