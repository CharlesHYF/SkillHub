// 文件作用: 市场相关 Tauri 命令 —— 搜索(分页)/详情查询/刷新缓存; market_search/market_detail
//           只负责加锁取出 conn、转换查询参数(前端原始整数编码 -> 领域枚举)与错误类型, 具体
//           逻辑见 services::market。market_refresh 是本仓库首个 async 命令: 刻意分两步调用
//           services::market::fetch_all(纯异步网络拉取)与 write_refresh_results(纯同步落库),
//           不在同一次加锁区间跨 await, 避免 state.db() 返回的 std::sync::MutexGuard(!Send)
//           跨 await 点存活导致命令 Future 整体退化为 !Send(Tauri 要求命令 Future: Send 才能
//           被其异步运行时 spawn; 详见 services::market 文件头注释"关于拆分")
// 创建日期: 2026-07-10

use serde::Serialize;
use tauri::State;

use crate::domain::market::{MarketResource, Query, SortBy};
use crate::domain::resource::ResourceType;
use crate::infra::source;
use crate::services::market;
use crate::AppState;

/// market_search 返回给前端的分页结果: items 为本页命中的市场资源, total 为该组过滤条件下的
/// 总命中数(不受分页影响), 供前端渲染分页控件
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketSearchResult {
	pub items: Vec<MarketResource>,
	pub total: i64,
}

/// market_refresh 返回给前端的结果: 本次刷新写入 market_cache 的资源条数
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketRefreshResult {
	pub count: usize,
}

/// 按过滤/排序/分页条件搜索市场资源缓存(见 services::market::search); res_type/sort 为前端
/// 传入的原始整数编码, 由本命令转换为领域枚举(res_type 为 None 表示不按类型过滤, 与
/// library_list 既有的转换惯例一致)
#[tauri::command]
pub fn market_search(
	state: State<'_, AppState>,
	keyword: Option<String>,
	res_type: Option<i64>,
	category: Option<String>,
	sort: i64,
	page: i64,
	page_size: i64,
) -> Result<MarketSearchResult, String> {
	let conn = state.db();
	let query = Query {
		keyword,
		res_type: res_type.map(ResourceType::from_i64),
		category,
		sort: SortBy::from_i64(sort),
		page,
		page_size,
	};
	let (items, total) = market::search(&conn, &query).map_err(|e| e.to_string())?;
	Ok(MarketSearchResult { items, total })
}

/// 按 (sourceType, extId) 查询单条市场资源详情(见 services::market::detail)
#[tauri::command]
pub fn market_detail(
	state: State<'_, AppState>,
	source_type: i64,
	ext_id: String,
) -> Result<Option<MarketResource>, String> {
	let conn = state.db();
	market::detail(&conn, source_type, &ext_id).map_err(|e| e.to_string())
}

/// 刷新市场缓存: 并发拉取三源全量资源并写入 market_cache。github_token 暂恒传 None(匿名请求):
/// 应用内认证(Task 7)尚未实现, 且 infra::repo_auth 按设计从不暴露令牌本身或钥匙串引用键
/// (令牌只进系统钥匙串, 见其文件头注释)——即便本命令先查一次 repo_auth 确认"是否已连接 GitHub
/// 账号", 也拿不到可用于 Authorization 头的实际令牌字符串, 查了等于没查; 真正接上需要 Task 7
/// 提供一个封装好 repo_auth+keychain 的"取当前 GitHub 令牌"能力(见本任务报告"疑虑"一节)
#[tauri::command]
pub async fn market_refresh(state: State<'_, AppState>) -> Result<MarketRefreshResult, String> {
	let sources = source::all_sources();
	// 先完成全部网络 I/O(不接触数据库), 再在下方同步临界区里落库, 详见文件头注释
	let outcomes = market::fetch_all(&sources, None).await;

	let conn = state.db();
	let count = market::write_refresh_results(&conn, outcomes).map_err(|e| e.to_string())?;
	Ok(MarketRefreshResult { count })
}
