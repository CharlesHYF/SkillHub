// 文件作用: 全局 UI 态(zustand) —— 选中资源/Agent、类型与关键字筛选, 供 Installed/Sync Center 等屏共享
// 创建日期: 2026-07-09
import { create } from 'zustand';

/** 资源类型筛选; undefined 表示不筛选(全部) */
export type ResourceTypeFilter = 'skill' | 'mcp' | undefined;

interface UiState {
	/** 当前选中的资源 id(用于打开 DetailPanel); null 表示未选中 */
	selectedResourceId: number | null;
	/** 当前选中的 Agent id */
	selectedAgentId: number | null;
	/** 资源类型筛选 */
	typeFilter: ResourceTypeFilter;
	/** 关键字筛选(搜索框) */
	keyword: string;

	setSelectedResourceId: (id: number | null) => void;
	setSelectedAgentId: (id: number | null) => void;
	setTypeFilter: (type: ResourceTypeFilter) => void;
	setKeyword: (keyword: string) => void;
	/** 还原全部 UI 态到初始值(主要供测试用例之间隔离) */
	reset: () => void;
}

/** 初始态单独提出, 供 reset 复用, 避免和 create 里的默认值写两遍 */
const initialState: Pick<
	UiState,
	'selectedResourceId' | 'selectedAgentId' | 'typeFilter' | 'keyword'
> = {
	selectedResourceId: null,
	selectedAgentId: null,
	typeFilter: undefined,
	keyword: '',
};

/** 全局 UI 态 store: 选中资源/Agent id、当前类型与关键字筛选 */
export const useUiStore = create<UiState>((set) => ({
	...initialState,
	setSelectedResourceId: (id) => set({ selectedResourceId: id }),
	setSelectedAgentId: (id) => set({ selectedAgentId: id }),
	setTypeFilter: (typeFilter) => set({ typeFilter }),
	setKeyword: (keyword) => set({ keyword }),
	reset: () => set(initialState),
}));
