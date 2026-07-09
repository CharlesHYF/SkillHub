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
		useUiStore.getState().reset();
		const state = useUiStore.getState();
		expect(state.selectedResourceId).toBeNull();
		expect(state.typeFilter).toBeUndefined();
	});
});
