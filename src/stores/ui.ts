// 文件作用: 全局 UI 态(zustand) —— 选中资源/Agent、类型与关键字筛选、选中的市场资源标识,
//           供 Installed/Sync Center/Marketplace 等屏共享
// 创建日期: 2026-07-09
import { create } from 'zustand';

/** 资源类型筛选; undefined 表示不筛选(全部) */
export type ResourceTypeFilter = 'skill' | 'mcp' | undefined;

/** 选中的市场资源标识: sourceType 为 domain::market::SourceId 的 i64 编码, 与 extId 组合
 * 定位一条 MarketResource(市场资源尚未导入本地库前没有 resource.id 这样的整数主键, 见
 * domain::market::MarketResource 的复合唯一键设计, 故不能复用 selectedResourceId) */
export interface SelectedMarket {
	sourceType: number;
	extId: string;
}

interface UiState {
	/** 当前选中的资源 id(用于打开 DetailPanel); null 表示未选中 */
	selectedResourceId: number | null;
	/** 当前选中的 Agent id */
	selectedAgentId: number | null;
	/** 资源类型筛选 */
	typeFilter: ResourceTypeFilter;
	/** 关键字筛选(搜索框) */
	keyword: string;
	/** 当前选中的市场资源标识(用于打开 Marketplace 详情面板); null 表示未选中 */
	selectedMarket: SelectedMarket | null;

	setSelectedResourceId: (id: number | null) => void;
	setSelectedAgentId: (id: number | null) => void;
	setTypeFilter: (type: ResourceTypeFilter) => void;
	setKeyword: (keyword: string) => void;
	setSelectedMarket: (market: SelectedMarket | null) => void;
	/** 还原全部 UI 态到初始值(主要供测试用例之间隔离) */
	reset: () => void;
}

/** 初始态单独提出, 供 reset 复用, 避免和 create 里的默认值写两遍 */
const initialState: Pick<
	UiState,
	'selectedResourceId' | 'selectedAgentId' | 'typeFilter' | 'keyword' | 'selectedMarket'
> = {
	selectedResourceId: null,
	selectedAgentId: null,
	typeFilter: undefined,
	keyword: '',
	selectedMarket: null,
};

/** 全局 UI 态 store: 选中资源/Agent id、当前类型与关键字筛选、选中的市场资源标识 */
export const useUiStore = create<UiState>((set) => ({
	...initialState,
	setSelectedResourceId: (id) => set({ selectedResourceId: id }),
	setSelectedAgentId: (id) => set({ selectedAgentId: id }),
	setTypeFilter: (typeFilter) => set({ typeFilter }),
	setKeyword: (keyword) => set({ keyword }),
	setSelectedMarket: (selectedMarket) => set({ selectedMarket }),
	reset: () => set(initialState),
}));
