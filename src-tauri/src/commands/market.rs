// 文件作用: 市场相关 Tauri 命令 —— 搜索(分页)/详情查询/刷新缓存/下载安装; market_search/
//           market_detail 只负责加锁取出 conn、转换查询参数(前端原始整数编码 -> 领域枚举)与
//           错误类型, 具体逻辑见 services::market。market_refresh/market_install 均为 async
//           命令: 刻意分阶段调用 services::market 对应的纯异步(网络拉取)/纯同步(落库)函数,
//           不在同一次加锁区间跨 await, 避免 state.db() 返回的 std::sync::MutexGuard(!Send)
//           跨 await 点存活导致命令 Future 整体退化为 !Send(Tauri 要求命令 Future: Send 才能
//           被其异步运行时 spawn; 详见 services::market 文件头注释"关于拆分")。market_install
//           另在阶段一(持锁查详情)内判定该来源是否需要认证(见 needs_auth_required_error): 若
//           来源要求认证且未连接对应账号、该资源又确实 auth_required, 提前返回
//           "AUTH_REQUIRED:<Provider>" 特征错误串, 供前端据此弹出对应登录引导(不强行要求匿名可
//           读的公开资源也必须先登录)
// 创建日期: 2026-07-10

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;

use crate::domain::auth::ProviderKind;
use crate::domain::market::{MarketResource, Query, SortBy, SourceId};
use crate::domain::resource::{Resource, ResourceType};
use crate::infra::source::{self, AuthKind};
use crate::services::auth;
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

/// 把市场源要求的认证类型(AuthKind)映射为认证领域的提供方(ProviderKind), 供
/// services::auth::token_for 查询是否已连接对应账号; 三者一一对应, 无需兜底分支(AuthKind 只有
/// 这三个变体, 见 infra::source::mod 文档)
fn provider_kind_for(auth_kind: AuthKind) -> ProviderKind {
	match auth_kind {
		AuthKind::GitHub => ProviderKind::GitHub,
		AuthKind::Google => ProviderKind::Google,
		AuthKind::Microsoft => ProviderKind::Microsoft,
	}
}

/// 认证类型的人类可读标签, 供拼装 "AUTH_REQUIRED:<label>" 错误串(前端据此识别并弹出对应的登录
/// 引导, 而不是把它当一般错误直接展示)
fn auth_label(auth_kind: AuthKind) -> &'static str {
	match auth_kind {
		AuthKind::GitHub => "GitHub",
		AuthKind::Google => "Google",
		AuthKind::Microsoft => "Microsoft",
	}
}

/// 判定安装前是否应拦截并要求用户先完成认证: 来源本身无需认证(auth_kind=None, 如 mcp_registry)
/// 恒放行; 需要认证的来源(GitHub 类)若已有 token(不论该资源是否 auth_required, 有 token 就带上
/// 以便提额/访问私有仓库)也放行; 只有"需要认证的来源 + 没有可用 token + 该资源本身确实
/// auth_required"这一组合才拦截 —— 公开只读资源(auth_required=false)即便没有 token 也应放行,
/// 走匿名请求(至多受限流), 不应强迫用户为可白嫖的资源也去登录
fn needs_auth_required_error(
	auth_kind: Option<AuthKind>,
	has_token: bool,
	auth_required: bool,
) -> bool {
	match auth_kind {
		None => false,
		Some(_) => !has_token && auth_required,
	}
}

/// 下载安装某条市场资源到本地库: 按 (sourceType, extId) 查市场缓存详情(不存在报错) -> 若该来源
/// 需要认证且未连接对应账号、该资源又确实 auth_required, 提前返回 "AUTH_REQUIRED:<Provider>"
/// (见 needs_auth_required_error)-> 异步拉取安装内容(services::market::fetch_install_payload,
/// 不持锁, 与 market_refresh 同一 Send 安全惯例, 详见文件头注释) -> 落地入库
/// (services::market::write_installed), 返回安装后的完整 Resource。envOverrides 供 McpTemplate
/// 类资源安装时填充 required_env(纯 Skill/Mcp 资源通常不传或传空, 见 services::market::
/// write_installed 文档); 已连接账号时会把令牌转发给 GitHub 类源用于提额/访问私有仓库
#[tauri::command]
pub async fn market_install(
	state: State<'_, AppState>,
	source_type: i64,
	ext_id: String,
	env_overrides: Option<BTreeMap<String, String>>,
) -> Result<Resource, String> {
	let sources = source::all_sources();
	let target_id = SourceId::from_i64(source_type);
	let auth_kind = sources
		.iter()
		.find(|item| item.id() == target_id)
		.and_then(|item| item.auth_kind());

	// 阶段一(同步, 持锁): 查详情 + 按需查已连接账号令牌 + 判定是否需要拦截认证, 不跨 await
	let (detail, token) = {
		let conn = state.db();
		let detail = market::detail(&conn, source_type, &ext_id)
			.map_err(|e| e.to_string())?
			.ok_or_else(|| format!("市场资源不存在: source_type={source_type}, ext_id={ext_id}"))?;

		let token = match auth_kind {
			Some(kind) => auth::token_for(&conn, i64::from(provider_kind_for(kind)))
				.map_err(|e| e.to_string())?,
			None => None,
		};

		if needs_auth_required_error(auth_kind, token.is_some(), detail.auth_required) {
			// unwrap: needs_auth_required_error 仅在 auth_kind 为 Some 时才可能返回 true
			return Err(format!("AUTH_REQUIRED:{}", auth_label(auth_kind.unwrap())));
		}
		(detail, token)
	};

	// 阶段二(异步, 不持锁): 网络拉取安装内容
	let payload = market::fetch_install_payload(&sources, source_type, token.as_deref(), &detail)
		.await
		.map_err(|e| e.to_string())?;

	// 阶段三(同步, 持锁): 落地写入 data_dir + 登记资源 + 记一条"下载"活动
	let conn = state.db();
	let overrides = env_overrides.unwrap_or_default();
	market::write_installed(&conn, &state.data_dir, &detail, payload, &overrides)
		.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

	// provider_kind_for: 三个 AuthKind 变体应精确映射到对应的 ProviderKind
	#[test]
	fn provider_kind_for_maps_three_variants() {
		assert_eq!(provider_kind_for(AuthKind::GitHub), ProviderKind::GitHub);
		assert_eq!(provider_kind_for(AuthKind::Google), ProviderKind::Google);
		assert_eq!(
			provider_kind_for(AuthKind::Microsoft),
			ProviderKind::Microsoft
		);
	}

	// auth_label: 应返回人类可读的英文标签, 供拼装 AUTH_REQUIRED 错误串
	#[test]
	fn auth_label_returns_readable_names() {
		assert_eq!(auth_label(AuthKind::GitHub), "GitHub");
		assert_eq!(auth_label(AuthKind::Google), "Google");
		assert_eq!(auth_label(AuthKind::Microsoft), "Microsoft");
	}

	// needs_auth_required_error: 来源无需认证(auth_kind=None)时恒不应拦截, 不论是否有 token/
	// 该资源是否 auth_required
	#[test]
	fn needs_auth_required_error_false_when_source_has_no_auth_kind() {
		assert!(!needs_auth_required_error(None, false, true));
		assert!(!needs_auth_required_error(None, true, true));
		assert!(!needs_auth_required_error(None, false, false));
	}

	// needs_auth_required_error: 已有 token 时恒放行(不论资源是否 auth_required), 供已连接
	// 账号的用户正常提额/访问私有资源
	#[test]
	fn needs_auth_required_error_false_when_token_present() {
		assert!(!needs_auth_required_error(
			Some(AuthKind::GitHub),
			true,
			true
		));
		assert!(!needs_auth_required_error(
			Some(AuthKind::GitHub),
			true,
			false
		));
	}

	// needs_auth_required_error: 没有 token 且该资源 auth_required=true 才应拦截, 返回 true
	#[test]
	fn needs_auth_required_error_true_when_no_token_and_resource_requires_auth() {
		assert!(needs_auth_required_error(
			Some(AuthKind::GitHub),
			false,
			true
		));
	}

	// needs_auth_required_error: 没有 token 但该资源本身不强制要求认证(公开只读资源)时不应
	// 拦截, 应放行走匿名请求(至多受限流)
	#[test]
	fn needs_auth_required_error_false_when_no_token_but_resource_not_auth_required() {
		assert!(!needs_auth_required_error(
			Some(AuthKind::GitHub),
			false,
			false
		));
	}
}
