// 文件作用: Sync Center 展示态派生逻辑单测(类型本地/远程归类、可同步判定、在线状态徽标、
//           diff 计划按 action 分组统计、按 action 过滤)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import type { AgentKind, AgentRespVO } from '@/api/agent';
import type { DiffItem, DiffPlanRespVO, SyncSummaryRespVO } from '@/api/sync';
import {
	agentInstallKind,
	countDiffByAction,
	deriveAgentSyncStatus,
	filterDiffItems,
	isAgentSyncable,
	lastResultLabel,
} from './agent-display';

function makeAgent(overrides: Partial<AgentRespVO> = {}): AgentRespVO {
	return {
		id: 1,
		agentKind: 'ClaudeCode',
		name: 'Claude Code',
		configPath: '/home/demo/.claude.json',
		scope: 'Global',
		status: true,
		lastSyncTime: '',
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

function makeDiffItem(overrides: Partial<DiffItem> = {}): DiffItem {
	return {
		resType: 'Skill',
		name: 'demo-skill',
		action: 'Add',
		localVer: '1.0.0',
		agentVer: '',
		payload: null,
		...overrides,
	};
}

describe('agentInstallKind', () => {
	it.each<AgentKind>([
		'ClaudeCode',
		'ClaudeDesktop',
		'Cursor',
		'Windsurf',
		'Cline',
		'VsCode',
		'GeminiCli',
		'Codex',
	])('已知 kind=%s 应归类为本地', (kind) => {
		expect(agentInstallKind(kind)).toBe('本地');
	});

	it('未知 kind(后端契约漂移防御)应归类为远程', () => {
		expect(agentInstallKind('FutureRemoteAgent' as unknown as AgentKind)).toBe('远程');
	});
});

describe('isAgentSyncable', () => {
	it('在线 + 本地 kind 应可同步', () => {
		expect(isAgentSyncable(makeAgent({ status: true, agentKind: 'ClaudeCode' }))).toBe(true);
	});

	it('离线不可同步(即便 kind 是本地)', () => {
		expect(isAgentSyncable(makeAgent({ status: false, agentKind: 'ClaudeCode' }))).toBe(false);
	});

	it('远程 kind 不可同步(即便在线)', () => {
		expect(
			isAgentSyncable(
				makeAgent({ status: true, agentKind: 'FutureRemoteAgent' as unknown as AgentKind }),
			),
		).toBe(false);
	});
});

describe('deriveAgentSyncStatus', () => {
	it('离线应展示"离线", 不论是否有历史同步结果', () => {
		const bad: SyncSummaryRespVO = { success: 0, failed: 1, skipped: 0 };
		expect(deriveAgentSyncStatus(makeAgent({ status: false }), bad)).toBe('离线');
	});

	it('在线且无历史同步结果应展示"在线"', () => {
		expect(deriveAgentSyncStatus(makeAgent({ status: true }))).toBe('在线');
	});

	it('在线且最近一次同步全部失败(success=0)应展示"同步失败"', () => {
		const outcome: SyncSummaryRespVO = { success: 0, failed: 2, skipped: 0 };
		expect(deriveAgentSyncStatus(makeAgent({ status: true }), outcome)).toBe('同步失败');
	});

	it('在线且最近一次同步部分失败(success>0 且 failed>0)应展示"部分同步"', () => {
		const outcome: SyncSummaryRespVO = { success: 1, failed: 1, skipped: 0 };
		expect(deriveAgentSyncStatus(makeAgent({ status: true }), outcome)).toBe('部分同步');
	});

	it('在线且最近一次同步全部成功应展示"在线"(非失败态不覆盖)', () => {
		const outcome: SyncSummaryRespVO = { success: 2, failed: 0, skipped: 0 };
		expect(deriveAgentSyncStatus(makeAgent({ status: true }), outcome)).toBe('在线');
	});
});

describe('countDiffByAction', () => {
	it('应按 Add/Update/Remove 分组计数并给出 total', () => {
		const plan: DiffPlanRespVO = {
			items: [
				makeDiffItem({ action: 'Add', name: 'a' }),
				makeDiffItem({ action: 'Add', name: 'b' }),
				makeDiffItem({ action: 'Update', name: 'c' }),
				makeDiffItem({ action: 'Remove', name: 'd' }),
			],
		};
		expect(countDiffByAction(plan)).toEqual({ add: 2, update: 1, remove: 1, total: 4 });
	});

	it('plan 为 undefined 时应返回全 0(未选中 Agent 或尚未加载)', () => {
		expect(countDiffByAction(undefined)).toEqual({ add: 0, update: 0, remove: 0, total: 0 });
	});
});

describe('lastResultLabel', () => {
	it('undefined(本次会话尚未同步过)应展示"暂无记录"', () => {
		expect(lastResultLabel(undefined)).toBe('暂无记录');
	});

	it('failed=0 应展示"全部同步"', () => {
		expect(lastResultLabel({ success: 3, failed: 0, skipped: 0 })).toBe('全部同步');
	});

	it('failed>0 且 success>0 应展示"部分同步"', () => {
		expect(lastResultLabel({ success: 1, failed: 1, skipped: 0 })).toBe('部分同步');
	});

	it('failed>0 且 success=0 应展示"同步失败"', () => {
		expect(lastResultLabel({ success: 0, failed: 2, skipped: 0 })).toBe('同步失败');
	});
});

describe('filterDiffItems', () => {
	const items: DiffItem[] = [
		makeDiffItem({ action: 'Add', name: 'a' }),
		makeDiffItem({ action: 'Update', name: 'b' }),
		makeDiffItem({ action: 'Remove', name: 'c' }),
	];

	it('action="All" 应原样返回全部条目', () => {
		expect(filterDiffItems(items, 'All')).toEqual(items);
	});

	it('指定 action 应只返回该动作的条目', () => {
		expect(filterDiffItems(items, 'Update')).toEqual([items[1]]);
	});
});
