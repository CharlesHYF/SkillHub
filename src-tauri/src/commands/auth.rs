// 文件作用: 认证相关 Tauri 命令 —— 已连接账号列表 / 手动录入 PAT 并校验入库 / 应用内 OAuth 弹窗
//           登录(起本地 loopback 监听 + 二级 WebviewWindow 承载授权页, 见 auth_login)/ 断开连接
//           (删库 + 删钥匙串)。除 auth_login 编排 Tauri 窗口与后台等待外, 其余均只负责加锁取出
//           conn、转换 provider 原始整数编码与错误类型, 具体逻辑见 services::auth。M4 Task 2 起,
//           auth_enter_token/auth_login 发起网络请求前均先短暂加锁读出当前 Settings
//           (services::setting::get_all)并立即释放锁, 再用 infra::http::build_http_client 依其
//           网络代理/超时字段现场构造 HTTP 客户端(取代此前固定配置的 infra::http::client()),
//           使这些设置对认证流程同样真实生效; 读设置与随后的网络 await 之间不跨锁, 与
//           commands::market 的既有 Send 安全惯例一致
// 创建日期: 2026-07-10

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, State, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::domain::auth::{AuthAccount, ProviderKind, TokenSet};
use crate::infra::http;
use crate::infra::repo_auth;
use crate::services::auth;
use crate::services::setting;
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
/// 前端传入的原始整数编码, 转换惯例同 market_search 的 res_type(见 commands::market)。发起
/// 校验请求所用的 HTTP 客户端依当前 Settings 的网络代理/超时字段构造, 见文件头注释
#[tauri::command]
pub async fn auth_enter_token(
	state: State<'_, AppState>,
	provider: i64,
	token: String,
) -> Result<AuthAccount, String> {
	let provider_kind = ProviderKind::from_i64(provider);
	// 先短暂加锁读出当前 Settings, 块结束后立即释放锁(不跨随后的 await), 再依其构造 HTTP 客户端
	let settings = {
		let conn = state.db();
		setting::get_all(&conn).map_err(|e| e.to_string())?
	};
	let client = http::build_http_client(&settings).map_err(|e| e.to_string())?;
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

/// 应用内 OAuth 弹窗承载登录授权页时使用的窗口标签, 与主窗口 "main" 区分
const OAUTH_WINDOW_LABEL: &str = "oauth";

/// 让 OAuth 弹窗随作用域结束自动关闭的 RAII 守卫: auth_login 后续任何一步 `?` 提前返回(甚至是
/// 等待回调的后台任务本身异常退出), 都不应该把这个弹窗遗留在桌面上; 用 Drop 兜底比在每个错误
/// 分支手动补一次 window.close() 更不容易遗漏
struct CloseWindowOnDrop(tauri::WebviewWindow);

impl Drop for CloseWindowOnDrop {
	fn drop(&mut self) {
		let _ = self.0.close();
	}
}

/// 应用内 OAuth 弹窗登录: 起一个本地 loopback 监听作 redirect_uri, 开一个二级 WebviewWindow
/// 加载 provider 授权页, 用户在其中完成登录授权后 provider 重定向到 loopback, 本地监听捕获
/// code 并校验 state(防 CSRF, 见 services::auth::parse_callback), 关闭弹窗后再换 token
/// (exchange_code)+ 调身份接口取账号标识(validate_pat, 与 auth_enter_token 同一收尾套路, 复用
/// 而非另造一份"构造 AuthAccount"逻辑)+ 落库(store), 返回入库后的完整账号。超时(3 分钟无回调,
/// 见 services::auth::LOGIN_TIMEOUT)或用户中途关闭弹窗均优雅报错而非无限期挂起(见
/// services::auth::wait_for_callback)。Token(手动录入的通用访问令牌)没有对应的 OAuth 授权页,
/// 不支持走本命令, 应改用 auth_enter_token
#[tauri::command]
pub async fn auth_login(
	app: AppHandle,
	state: State<'_, AppState>,
	provider: i64,
) -> Result<AuthAccount, String> {
	let provider_kind = ProviderKind::from_i64(provider);
	if provider_kind == ProviderKind::Token {
		return Err("Token 没有对应的 OAuth 授权页, 请改用手动录入令牌".to_string());
	}

	// 1. 起本地 loopback 监听, 得到本次登录专用的 redirect_uri
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.map_err(|e| format!("本地回调监听启动失败: {e}"))?;
	let port = listener
		.local_addr()
		.map_err(|e| format!("读取本地回调监听端口失败: {e}"))?
		.port();
	let redirect_uri = auth::build_redirect_uri(port);

	// 2. PKCE 挑战 + 防 CSRF 的随机 state + 拼出授权页 URL
	let pkce = auth::build_pkce();
	let csrf_state = auth::random_state();
	let authorize_url =
		auth::authorize_url(provider_kind, &pkce.challenge, &redirect_uri, &csrf_state);
	let authorize_url = Url::parse(&authorize_url).map_err(|e| format!("授权页地址无效: {e}"))?;

	// 3. 开二级 WebviewWindow 承载登录授权页; 用户点击关闭视为取消本次登录
	let cancelled = Arc::new(AtomicBool::new(false));
	let cancelled_for_window = cancelled.clone();
	let window = WebviewWindowBuilder::new(
		&app,
		OAUTH_WINDOW_LABEL,
		WebviewUrl::External(authorize_url),
	)
	.title("登录授权")
	.inner_size(480.0, 720.0)
	.center()
	.build()
	.map_err(|e| format!("打开登录窗口失败: {e}"))?;
	window.on_window_event(move |event| {
		if matches!(
			event,
			WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
		) {
			cancelled_for_window.store(true, Ordering::SeqCst);
		}
	});
	let window_guard = CloseWindowOnDrop(window);

	// 4. 后台线程阻塞等待恰好一次回调(超时/取消均优雅返回错误), 不占用异步执行器线程; 拿到
	// 结果后无论成败先关闭弹窗(drop 守卫), 不必等后续换 token/落库都做完才关
	let wait_result = tauri::async_runtime::spawn_blocking(move || {
		auth::wait_for_callback(listener, &csrf_state, auth::LOGIN_TIMEOUT, &cancelled)
	})
	.await
	.map_err(|e| format!("等待授权回调的后台任务异常: {e}"))?;
	drop(window_guard);
	let code = wait_result.map_err(|e| e.to_string())?;

	// 5. 换 token, 再调身份接口取账号标识(与 auth_enter_token 收尾一致), 落库; 客户端同样依当前
	// Settings 构造(见文件头注释), 读设置这一步同样短暂加锁后立即释放, 不跨随后的网络 await
	let settings = {
		let conn = state.db();
		setting::get_all(&conn).map_err(|e| e.to_string())?
	};
	let client = http::build_http_client(&settings).map_err(|e| e.to_string())?;
	let tokens = auth::exchange_code(
		&client,
		provider_kind,
		auth::default_token_base(provider_kind),
		&code,
		&pkce.verifier,
		&redirect_uri,
	)
	.await
	.map_err(|e| e.to_string())?;
	let account = auth::validate_pat(
		&client,
		provider_kind,
		auth::default_validate_base(provider_kind),
		&tokens.access,
	)
	.await
	.map_err(|e| e.to_string())?;

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
