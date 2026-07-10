// 文件作用: TanStack Query 通用配置片段 —— 易变数据(Agent 列表/在线态、Dashboard 概览、导入
//           导出历史)的实时保鲜策略, 供 dashboard/sync-center/portability 等页面对应的 useQuery
//           调用按需展开复用, 避免同一组 refetch 参数在多个页面各写一遍、改一处漏别处(M5 Task F1:
//           去掉手动"刷新"按钮后, 这些数据改由此策略自动保鲜, 不再依赖用户手动点按钮)
// 创建日期: 2026-07-10

/** 易变数据的实时保鲜 Query 配置: 5 秒轮询 + 窗口重新聚焦时刷新 + 每次挂载都强制刷新一次
 * (而非只信任 staleTime 判断是否需要刷新), 使这些随后端状态持续变化的数据在界面停留期间
 * 保持新鲜。市场资源(Marketplace)数据较重, 不适用本配置, 见 pages/marketplace.tsx 的
 * "挂载拉一次 + 用户搜索即可"策略 */
export const LIVE_QUERY_OPTIONS = {
	refetchInterval: 5000,
	refetchOnWindowFocus: true,
	refetchOnMount: 'always',
} as const;
