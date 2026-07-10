// 文件作用: 全局 UI 态 store 单测(选中态/筛选态的读写与复位)
// 创建日期: 2026-07-09
import { describe, it, expect, beforeEach } from 'vitest';
import { useUiStore } from './ui';

describe('useUiStore', () => {
	beforeEach(() => {
		useUiStore.getState().reset();
	});

	it('初始态: 未选中资源/Agent, 无类型筛选, 关键字为空', () => {
		const state = useUiStore.getState();
		expect(state.selectedResourceId).toBeNull();
		expect(state.selectedAgentId).toBeNull();
		expect(state.typeFilter).toBeUndefined();
		expect(state.keyword).toBe('');
		expect(state.selectedMarket).toBeNull();
		expect(state.marketRefreshed).toBe(false);
	});

	it('setSelectedResourceId 应更新选中资源 id', () => {
		useUiStore.getState().setSelectedResourceId(42);
		expect(useUiStore.getState().selectedResourceId).toBe(42);
	});

	it('setSelectedAgentId 应更新选中 Agent id', () => {
		useUiStore.getState().setSelectedAgentId(7);
		expect(useUiStore.getState().selectedAgentId).toBe(7);
	});

	it('setTypeFilter/setKeyword 应更新筛选态', () => {
		useUiStore.getState().setTypeFilter('mcp');
		useUiStore.getState().setKeyword('搜索词');
		const state = useUiStore.getState();
		expect(state.typeFilter).toBe('mcp');
		expect(state.keyword).toBe('搜索词');
	});

	it('reset 应把所有态还原为初始值', () => {
		useUiStore.getState().setSelectedResourceId(1);
		useUiStore.getState().setTypeFilter('skill');
		useUiStore.getState().setSelectedMarket({ sourceType: 1, extId: 'acme/skills:demo' });
		useUiStore.getState().reset();
		const state = useUiStore.getState();
		expect(state.selectedResourceId).toBeNull();
		expect(state.typeFilter).toBeUndefined();
		expect(state.selectedMarket).toBeNull();
	});

	it('setSelectedMarket 应更新选中的市场资源标识(sourceType 编码 + extId 复合键)', () => {
		useUiStore.getState().setSelectedMarket({ sourceType: 2, extId: 'demo/mcp-server' });
		expect(useUiStore.getState().selectedMarket).toEqual({
			sourceType: 2,
			extId: 'demo/mcp-server',
		});
	});

	it('setSelectedMarket(null) 应清空选中的市场资源标识', () => {
		useUiStore.getState().setSelectedMarket({ sourceType: 1, extId: 'demo/skill' });
		useUiStore.getState().setSelectedMarket(null);
		expect(useUiStore.getState().selectedMarket).toBeNull();
	});

	it('setMarketRefreshed 应把市场刷新态置为 true, reset 应还原为 false', () => {
		useUiStore.getState().setMarketRefreshed();
		expect(useUiStore.getState().marketRefreshed).toBe(true);

		useUiStore.getState().reset();
		expect(useUiStore.getState().marketRefreshed).toBe(false);
	});
});
