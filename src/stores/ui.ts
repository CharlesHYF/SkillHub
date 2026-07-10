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
	/** 本次应用会话内 market_refresh 是否已成功执行过至少一次(不区分是应用启动初始化触发还是
	 * Marketplace 页面挂载时触发); 供二者共享同一份守卫, 避免在同一会话内重复触发这个较重的
	 * 网络请求 —— 某次筛选/关键字搜索恰好 0 命中不代表市场缓存本身为空, 不应据此重复刷新
	 * (见 App.tsx 启动初始化与 pages/marketplace.tsx 挂载时的自动刷新逻辑) */
	marketRefreshed: boolean;

	setSelectedResourceId: (id: number | null) => void;
	setSelectedAgentId: (id: number | null) => void;
	setTypeFilter: (type: ResourceTypeFilter) => void;
	setKeyword: (keyword: string) => void;
	setSelectedMarket: (market: SelectedMarket | null) => void;
	/** 标记本次会话已成功刷新过市场缓存 */
	setMarketRefreshed: () => void;
	/** 还原全部 UI 态到初始值(主要供测试用例之间隔离) */
	reset: () => void;
}

/** 初始态单独提出, 供 reset 复用, 避免和 create 里的默认值写两遍 */
const initialState: Pick<
	UiState,
	| 'selectedResourceId'
	| 'selectedAgentId'
	| 'typeFilter'
	| 'keyword'
	| 'selectedMarket'
	| 'marketRefreshed'
> = {
	selectedResourceId: null,
	selectedAgentId: null,
	typeFilter: undefined,
	keyword: '',
	selectedMarket: null,
	marketRefreshed: false,
};

/** 全局 UI 态 store: 选中资源/Agent id、当前类型与关键字筛选、选中的市场资源标识、市场刷新态 */
export const useUiStore = create<UiState>((set) => ({
	...initialState,
	setSelectedResourceId: (id) => set({ selectedResourceId: id }),
	setSelectedAgentId: (id) => set({ selectedAgentId: id }),
	setTypeFilter: (typeFilter) => set({ typeFilter }),
	setKeyword: (keyword) => set({ keyword }),
	setSelectedMarket: (selectedMarket) => set({ selectedMarket }),
	setMarketRefreshed: () => set({ marketRefreshed: true }),
	reset: () => set(initialState),
}));
