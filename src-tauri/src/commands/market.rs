// 文件作用: 市场相关 Tauri 命令 —— 搜索(分页)/详情查询/刷新缓存/下载安装; market_search/
//           market_detail 只负责加锁取出 conn、转换查询参数(前端原始整数编码 -> 领域枚举)与
//           错误类型, 具体逻辑见 services::market。market_refresh/market_install 均为 async
//           命令: 刻意分阶段调用 services::market 对应的纯异步(网络拉取)/纯同步(落库)函数,
//           不在同一次加锁区间跨 await, 避免 state.db() 返回的 std::sync::MutexGuard(!Send)
//           跨 await 点存活导致命令 Future 整体退化为 !Send(Tauri 要求命令 Future: Send 才能
//           被其异步运行时 spawn; 详见 services::market 文件头注释"关于拆分")。market_refresh
//           另在阶段一(持锁)内按需查询已连接 GitHub 账号的令牌(auth::token_for), 用于提升
//           GitHub 相关源的限流额度, 未连接时该令牌为 None(仍可正常刷新, 只是走匿名请求受公共
//           限流约束); market_install 则在阶段一(持锁查详情)内判定该来源是否需要认证(见
//           needs_auth_required_error): 若来源要求认证且未连接对应账号、该资源又确实
//           auth_required, 提前返回 "AUTH_REQUIRED:<Provider>" 特征错误串, 供前端据此弹出对应
//           登录引导(不强行要求匿名可读的公开资源也必须先登录)
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

/// 刷新市场缓存: 并发拉取三源全量资源并写入 market_cache。若已连接 GitHub 账号(见
/// auth::token_for), 取出其令牌用于提升 GitHub 相关源(github_skills/github_mcp)的 API 限流
/// 额度; 未连接账号时该值为 None, fetch_all 仍会照常对各源发起匿名请求(见其文档), 不强制要求
/// 用户先登录才能刷新市场缓存。三段式调用与 market_install 同一 Send 安全惯例(阶段一持锁取
/// token、阶段二不持锁发起网络 I/O、阶段三持锁落库, 临界区内均无 await), 详见文件头注释
#[tauri::command]
pub async fn market_refresh(state: State<'_, AppState>) -> Result<MarketRefreshResult, String> {
	let sources = source::all_sources();

	// 阶段一(同步, 持锁): 若已连接 GitHub 账号则取出其令牌, 未连接时为 None(仍可正常刷新,
	// 只是走匿名请求受公共限流约束)
	let github_token = {
		let conn = state.db();
		auth::token_for(&conn, i64::from(ProviderKind::GitHub)).map_err(|e| e.to_string())?
	};

	// 阶段二(异步, 不持锁): 并发拉取三源全量资源(不接触数据库), 详见文件头注释
	let outcomes = market::fetch_all(&sources, github_token.as_deref()).await;

	// 阶段三(同步, 持锁): 落库
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

/// 拼装 "AUTH_REQUIRED:<provider>" 特征错误串, 其中 <provider> 是 ProviderKind 的 i64 编码
/// (GitHub=1/Google=2/Microsoft=3), 与前端 src/api/market.ts::parseAuthRequiredProvider 的数值
/// 解析约定一致 —— 前端据此弹出对应 provider 的登录引导, 而不是把它当一般错误直接展示
fn auth_required_error(auth_kind: AuthKind) -> String {
	format!("AUTH_REQUIRED:{}", i64::from(provider_kind_for(auth_kind)))
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
			return Err(auth_required_error(auth_kind.unwrap()));
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
	use rusqlite::Connection;

	use crate::domain::auth::{AuthAccount, TokenSet};
	use crate::infra::keychain::tests::{lock_keychain_tests, random_account};

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块 market_refresh 令牌来源相关测试复用
	/// (migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

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

	// auth_required_error: 应拼出 "AUTH_REQUIRED:<i64 编码>", 与前端 parseAuthRequiredProvider 约定一致
	#[test]
	fn auth_required_error_uses_provider_i64_code() {
		assert_eq!(auth_required_error(AuthKind::GitHub), "AUTH_REQUIRED:1");
		assert_eq!(auth_required_error(AuthKind::Google), "AUTH_REQUIRED:2");
		assert_eq!(auth_required_error(AuthKind::Microsoft), "AUTH_REQUIRED:3");
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

	// ---------- market_refresh 阶段一"取 GitHub 令牌"的真实取值逻辑 ----------
	// market_refresh 命令体本身需要真实 tauri::State<AppState>, 本仓库未引入 tauri::test 相关
	// 基础设施(其它命令的既有测试惯例均只测抽出的纯函数, 不直接调用命令本体), 故以下两个测试
	// 直接调用命令体阶段一实际执行的同一个函数(auth::token_for), 验证"未连接账号 -> None"与
	// "已连接账号 -> 能取回真实令牌"这两条路径均按预期工作, 覆盖此前 github_token 恒传 None、
	// 现在改为动态取值这一变更

	// 未连接任何 GitHub 账号时应返回 None; fetch_all 收到 None 后仍会对各源发起匿名请求(见
	// services::market::fetch_all 文档), 不应因没有账号就报错
	#[test]
	fn market_refresh_github_token_lookup_returns_none_without_connected_account() {
		let conn = setup_conn();
		let token = auth::token_for(&conn, i64::from(ProviderKind::GitHub)).unwrap();
		assert_eq!(token, None);
	}

	// 已通过 auth::store 连接 GitHub 账号后, 应能取回其真实 access token(而非像此前那样恒传
	// None), 用于提升该源的限流额度
	#[test]
	fn market_refresh_github_token_lookup_returns_stored_token_after_connect() {
		let _guard = lock_keychain_tests();
		let conn = setup_conn();
		let account_name = random_account();
		let account = AuthAccount {
			id: 0,
			provider: ProviderKind::GitHub,
			account: account_name.clone(),
			scope: "read:user".to_string(),
			status: true,
			connect_time: String::new(),
		};
		let tokens = TokenSet {
			access: "gh-connected-token".to_string(),
			refresh: None,
			expires_at: None,
		};
		auth::store(&conn, &account, &tokens).unwrap();

		let token = auth::token_for(&conn, i64::from(ProviderKind::GitHub)).unwrap();

		assert_eq!(token, Some("gh-connected-token".to_string()));

		// 清理: 避免残留污染系统钥匙串, 复用 logout(其自身行为已在 services::auth 测试里验证,
		// 此处只借用来清场)
		auth::logout(&conn, i64::from(ProviderKind::GitHub)).unwrap();
	}
}
