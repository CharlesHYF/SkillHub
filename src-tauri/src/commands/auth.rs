// 文件作用: 认证相关 Tauri 命令 —— 已连接账号列表 / 手动录入 PAT 并校验入库 / 断开连接
//           (删库 + 删钥匙串)。均只负责加锁取出 conn、转换 provider 原始整数编码与错误类型,
//           具体逻辑见 services::auth。auth_login(应用内 OAuth 弹窗 + 本地 loopback 回调)留待
//           Task 8, 本文件不含
// 创建日期: 2026-07-10

use tauri::State;

use crate::domain::auth::{AuthAccount, ProviderKind, TokenSet};
use crate::infra::http;
use crate::infra::repo_auth;
use crate::services::auth;
use crate::AppState;

/// 列出全部已连接账号。纯查询无额外业务逻辑, 直接转调 repo_auth::list, 不为此单独包一层
/// 只做透传的服务函数(与 services::market::search 直接转调 repo_market::query 同一惯例)
#[tauri::command]
pub fn auth_accounts(state: State<'_, AppState>) -> Result<Vec<AuthAccount>, String> {
	let conn = state.db();
	repo_auth::list(&conn).map_err(|e| e.to_string())
}

/// 手动录入访问令牌(Personal Access Token): 先调 provider 身份接口校验(services::auth::
/// validate_pat), 成功后落库 + 令牌进系统钥匙串(services::auth::store), 返回入库后的完整
/// 账号(含真实 id/connect_time; store 本身只返回 Result<()>, 故此处再查一次)。provider 为
/// 前端传入的原始整数编码, 转换惯例同 market_search 的 res_type(见 commands::market)
#[tauri::command]
pub async fn auth_enter_token(
	state: State<'_, AppState>,
	provider: i64,
	token: String,
) -> Result<AuthAccount, String> {
	let provider_kind = ProviderKind::from_i64(provider);
	let client = http::client();
	let base = auth::default_validate_base(provider_kind);
	let account = auth::validate_pat(&client, provider_kind, base, &token)
		.await
		.map_err(|e| e.to_string())?;
	let tokens = TokenSet {
		access: token,
		refresh: None,
		expires_at: None,
	};

	let conn = state.db();
	auth::store(&conn, &account, &tokens).map_err(|e| e.to_string())?;
	repo_auth::get_by_provider(&conn, provider)
		.map_err(|e| e.to_string())?
		.ok_or_else(|| "入库后未能查到刚保存的账号".to_string())
}

/// 断开连接: 删库行 + 删系统钥匙串对应条目(见 services::auth::logout)
#[tauri::command]
pub fn auth_logout(state: State<'_, AppState>, provider: i64) -> Result<(), String> {
	let conn = state.db();
	auth::logout(&conn, provider).map_err(|e| e.to_string())
}
