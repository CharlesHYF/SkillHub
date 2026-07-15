// 文件作用: 认证服务 —— PKCE(RFC 7636)挑战构造与 GitHub/Google/Microsoft 授权 URL 拼接、
//           换取/校验令牌(exchange_code/validate_pat, 端点 base 可注入以便 wiremock 测试)、
//           已连接账号入库 + 令牌入系统钥匙串(store)、断开连接(logout)、供后端内部取当前令牌
//           (token_for, 刻意不做成命令/不经 IPC 暴露, 见其文档)、应用内 OAuth 弹窗登录所需的
//           纯 loopback 逻辑(random_state/build_redirect_uri/parse_callback/wait_for_callback:
//           防 CSRF 的随机 state、redirect_uri 构造、请求行/URL 解析+校验、阻塞等待恰好一次
//           回调, 均不依赖 Tauri, 可脱离 WebView 直接单测)。真正打开 WebviewWindow 承载授权页
//           那部分 Tauri 专属编排逻辑在 commands::auth::auth_login, 不下沉本文件
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::prelude::*;
use rand::RngExt;
use reqwest::{Client, Url};
use rusqlite::Connection;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::domain::auth::{AuthAccountRespVO, PkceChallenge, ProviderKind, TokenSet};
use crate::infra::keychain;
use crate::infra::repo_auth;

/// GitHub OAuth App client_id 占位常量: Charles 在 GitHub 注册 OAuth App 后替换为真实值
/// (注册步骤与本常量的填写位置见仓库根目录 OAUTH_SETUP.md "GitHub" 一节)
pub const GITHUB_CLIENT_ID: &str = "REPLACE_WITH_GITHUB_OAUTH_CLIENT_ID";
/// Google OAuth 2.0 客户端 ID 占位常量: Charles 在 Google Cloud Console 注册 OAuth 客户端后
/// 替换(见 OAUTH_SETUP.md "Google" 一节)
pub const GOOGLE_CLIENT_ID: &str = "REPLACE_WITH_GOOGLE_OAUTH_CLIENT_ID";
/// Microsoft Entra ID 应用(客户端)ID 占位常量: Charles 在 Entra 管理中心注册应用后替换
/// (见 OAUTH_SETUP.md "Microsoft" 一节)
pub const MICROSOFT_CLIENT_ID: &str = "REPLACE_WITH_MICROSOFT_OAUTH_CLIENT_ID";

// ---------- build_pkce ----------

/// PKCE code_verifier 使用的随机字节数: 32 字节 = 256 位熵, base64url(无 padding)编码后恰为
/// 43 个字符 —— RFC 7636 允许的最短长度, 字符集是 unreserved(ALPHA/DIGIT/"-"/"."/"_"/"~")的
/// 子集, 天然合规, 无需额外过滤或重试
const VERIFIER_RANDOM_BYTES: usize = 32;

/// 构造一个 PKCE(RFC 7636)挑战: 随机 verifier + 其 S256 challenge。verifier/challenge 均只在
/// 内存中流转、不落库(见 domain::auth::PkceChallenge 文档)
pub fn build_pkce() -> PkceChallenge {
	let mut bytes = [0u8; VERIFIER_RANDOM_BYTES];
	rand::rng().fill(&mut bytes);
	let verifier = BASE64_URL_SAFE_NO_PAD.encode(bytes);
	let challenge = challenge_from_verifier(&verifier);
	PkceChallenge::new(verifier, challenge)
}

/// 按 RFC 7636 S256 方法由 verifier 计算 code_challenge:
/// BASE64URL-ENCODE(SHA256(ASCII(verifier))), 不带 padding。抽成独立纯函数, 便于直接用
/// RFC 7636 Appendix B.1 的官方已知测试向量校验, 不依赖 build_pkce 内部的随机性
fn challenge_from_verifier(verifier: &str) -> String {
	let digest = Sha256::digest(verifier.as_bytes());
	BASE64_URL_SAFE_NO_PAD.encode(&digest[..])
}

// ---------- authorize_url ----------

/// 拼出跳转到 provider 授权页所需的完整 URL: 携带 client_id/redirect_uri/scope/
/// response_type=code/code_challenge/code_challenge_method=S256/state。只拼字符串、不发
/// 请求, 故 authorize 端点用真实生产地址硬编码即可, 不像 exchange_code/validate_pat 那样需要
/// 注入 base(那两者要在测试里打 wiremock, 这个不用)
pub fn authorize_url(
	provider: ProviderKind,
	challenge: &str,
	redirect: &str,
	state: &str,
) -> String {
	let endpoint = authorize_endpoint(provider);
	if endpoint.is_empty() {
		// Token(手动录入访问令牌)没有对应的 OAuth 授权页, 调用方不应对此 provider 调用本函数;
		// 返回空串而非 panic 兜底, 让误用在前端表现为"无效链接"而不是让后端崩溃
		return String::new();
	}
	let mut url = Url::parse(endpoint).expect("内置 authorize 端点常量必为合法 URL");
	url.query_pairs_mut()
		.append_pair("client_id", client_id_for(provider))
		.append_pair("redirect_uri", redirect)
		.append_pair("scope", scope_for(provider))
		.append_pair("response_type", "code")
		.append_pair("code_challenge", challenge)
		.append_pair("code_challenge_method", "S256")
		.append_pair("state", state);
	url.to_string()
}

/// 各提供方的 OAuth 授权页地址(生产环境真实地址)
fn authorize_endpoint(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "https://github.com/login/oauth/authorize",
		ProviderKind::Google => "https://accounts.google.com/o/oauth2/v2/auth",
		ProviderKind::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
		ProviderKind::Token => "",
	}
}

/// 各提供方申请的最小授权范围: GitHub 只求读到用户名(市场刷新提额本身不需要任何 scope, 匿名
/// 范围的令牌就足以抬高速率限制); Google/Microsoft 均含 openid+email/profile 以便 validate
/// 阶段能取到身份字段, Microsoft 另加 offline_access 以便拿到 refresh_token
fn scope_for(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "read:user",
		ProviderKind::Google => "openid email profile",
		ProviderKind::Microsoft => "openid email profile User.Read offline_access",
		ProviderKind::Token => "",
	}
}

/// 各提供方 client_id: 生产环境需 Charles 注册应用后, 把本文件顶部三个占位常量替换为真实值
fn client_id_for(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => GITHUB_CLIENT_ID,
		ProviderKind::Google => GOOGLE_CLIENT_ID,
		ProviderKind::Microsoft => MICROSOFT_CLIENT_ID,
		ProviderKind::Token => "",
	}
}

// ---------- exchange_code ----------

/// token 端点 JSON 响应的通用形状: 三家提供方成功时字段基本一致(access_token/refresh_token/
/// expires_in); GitHub 出错时特有地仍返回 200 状态码, 只是响应体换成 error/error_description
/// (而非非 2xx 状态码), 故 access_token 设为 Option 且显式检查, 不能只凭 HTTP 状态码判定成功;
/// 全部字段加 #[serde(default)], 容忍任意一方实际响应里缺失某个键(而非仅值为 null)
#[derive(Deserialize)]
struct TokenResponse {
	#[serde(default)]
	access_token: Option<String>,
	#[serde(default)]
	refresh_token: Option<String>,
	#[serde(default)]
	expires_in: Option<u64>,
	#[serde(default)]
	error: Option<String>,
	#[serde(default)]
	error_description: Option<String>,
}

// 手写 Debug 而非派生: access_token/refresh_token 是敏感令牌, 派生会打印明文; 此处只对二者做
// 存在性脱敏, 保留 error/error_description(非敏感, 便于排查 OAuth 失败)与 expires_in
impl std::fmt::Debug for TokenResponse {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("TokenResponse")
			.field(
				"access_token",
				&self.access_token.as_ref().map(|_| "<redacted>"),
			)
			.field(
				"refresh_token",
				&self.refresh_token.as_ref().map(|_| "<redacted>"),
			)
			.field("expires_in", &self.expires_in)
			.field("error", &self.error)
			.field("error_description", &self.error_description)
			.finish()
	}
}

/// 用授权码换取令牌: POST 到 `{base}` + 该 provider 的 token 路径, 表单体含
/// client_id/code/redirect_uri/grant_type=authorization_code/code_verifier(PKCE)。base 可
/// 注入(生产用 default_token_base 给出的真实地址, 测试指向 wiremock 本地地址)
pub async fn exchange_code(
	client: &Client,
	provider: ProviderKind,
	base: &str,
	code: &str,
	verifier: &str,
	redirect: &str,
) -> Result<TokenSet> {
	let url = format!("{base}{}", token_path(provider));
	let form = [
		("client_id", client_id_for(provider)),
		("code", code),
		("redirect_uri", redirect),
		("grant_type", "authorization_code"),
		("code_verifier", verifier),
	];

	let response = client
		.post(&url)
		.header(reqwest::header::ACCEPT, "application/json")
		.form(&form)
		.send()
		.await
		.with_context(|| format!("换取 token 请求发送失败: {url}"))?;

	let status = response.status();
	let body: TokenResponse = response
		.json()
		.await
		.with_context(|| format!("换取 token 响应体 JSON 解析失败: {url}"))?;

	if let Some(access) = body.access_token {
		return Ok(TokenSet {
			access,
			refresh: body.refresh_token,
			expires_at: body.expires_in.map(unix_expiry),
		});
	}
	let reason = body
		.error_description
		.or(body.error)
		.unwrap_or_else(|| format!("状态码 {status}, 响应未含 access_token"));
	anyhow::bail!("换取 token 失败: {reason}")
}

/// 各提供方 token 端点的路径部分, 与 default_token_base 给出的根地址拼接
fn token_path(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "/login/oauth/access_token",
		ProviderKind::Google => "/token",
		ProviderKind::Microsoft => "/oauth2/v2.0/token",
		ProviderKind::Token => "",
	}
}

/// 各提供方 token 端点的生产环境根地址; 测试通过显式传参注入 wiremock 本地地址, 与
/// infra::source::github_skills 等既有源的 with_base_url 注入手法同一思路
pub fn default_token_base(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "https://github.com",
		ProviderKind::Google => "https://oauth2.googleapis.com",
		ProviderKind::Microsoft => "https://login.microsoftonline.com/common",
		ProviderKind::Token => "",
	}
}

/// 把 token 端点返回的相对过期秒数(expires_in)换算为绝对 Unix 时间戳(自 1970-01-01 UTC
/// 起的秒数)的字符串形式。刻意不引入任何日期时间 crate: 全仓库其余时间戳列均由 SQLite
/// datetime('now')生成(见下方 sqlite_now), 这是唯一必须在 Rust 侧计算时间的地方, 用最朴素的
/// 整数运算即可, 无需日历/时区换算; 可读时间由后续消费方按需转换
fn unix_expiry(expires_in_secs: u64) -> String {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs();
	(now + expires_in_secs).to_string()
}

// ---------- OAuth 弹窗登录: 本地 loopback 回调捕获 ----------

/// 授权回调等待的最长时长: 超过后视为用户已放弃本次登录, 优雅报错而不是无限期挂起等待
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

/// 轮询 accept 的重试间隔: 100ms 足够及时响应超时/取消, 又不会空转占用过多 CPU
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 单次读取回调请求行的缓冲区大小: 请求行(如 "GET /callback?code=...&state=... HTTP/1.1")里
/// code/state 均为几十到上百字符的 token, 通常远小于此值, 留有充分余量
const REQUEST_BUFFER_BYTES: usize = 2048;

/// 回调连接的响应体: 极简 200, 告知用户可关闭本窗口; 不引入模板引擎, 固定文案足矣
const CALLBACK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body><p>登录已完成, 可关闭此窗口。</p></body></html>";

/// 生成 OAuth "state" 参数用的随机值: 与 PKCE verifier 同规格(32 字节随机数, base64url 无
/// padding 编码), 但语义完全独立 —— state 只用于让发起授权与接收回调两端比对一致, 防御 CSRF/
/// 回调伪造, 不参与 code_verifier/code_challenge 运算, 不应与 PKCE verifier 混用同一份随机数
pub fn random_state() -> String {
	let mut bytes = [0u8; VERIFIER_RANDOM_BYTES];
	rand::rng().fill(&mut bytes);
	BASE64_URL_SAFE_NO_PAD.encode(bytes)
}

/// 拼出本机 loopback 回调地址: 固定用 127.0.0.1(不用 "localhost", 避免依赖系统 hosts/DNS 解析
/// 的不确定性)+ 实际监听到的随机端口 + 固定路径 "/callback"
pub fn build_redirect_uri(port: u16) -> String {
	format!("http://127.0.0.1:{port}/callback")
}

/// 从 loopback 收到的一行 HTTP 请求行(如 "GET /callback?code=X&state=Y HTTP/1.1"), 或一个完整
/// 重定向 URL(如 "http://127.0.0.1:9999/callback?code=X&state=Y")中取出 code, 并就地校验 state
/// 与发起授权时生成的 expected_state 一致。两种输入形式共用同一套逻辑: 先定位首个 '?', 再在其后
/// 截到首个空白处(请求行结尾还有 " HTTP/1.1" 协议版本尾巴; URL 没有这条尾巴, 天然在字符串末尾
/// 截止)。state 缺失或不匹配一律报错拒绝(不区分是被篡改还是重放, 统一按 CSRF 处理), 这一校验
/// 故意做成 parse_callback 自身职责的一部分而非留给调用方另行判断: 调用方不可能在不提供
/// expected_state 的情况下拿到 code, 结构上杜绝"忘记校验 state"的误用
pub fn parse_callback(request_line_or_url: &str, expected_state: &str) -> Result<String> {
	let query = request_line_or_url
		.split_once('?')
		.map(|(_, rest)| rest)
		.unwrap_or("");
	let query = query.split_whitespace().next().unwrap_or("");

	// 借 Url::query_pairs 完成 '&' 切分 + percent-decode(不手写, 避免遗漏 "+"/"%XX" 等边角情况);
	// 拼一个占位 scheme+host 只为满足 Url::parse 必须是合法绝对 URL 的要求, 不会发起任何网络访问
	let dummy = format!("http://127.0.0.1/?{query}");
	let parsed = Url::parse(&dummy).context("回调查询串解析失败")?;

	let mut code = None;
	let mut state = None;
	for (key, value) in parsed.query_pairs() {
		match key.as_ref() {
			"code" => code = Some(value.into_owned()),
			"state" => state = Some(value.into_owned()),
			_ => {}
		}
	}

	let state = state.ok_or_else(|| anyhow::anyhow!("回调缺少 state 参数"))?;
	if state != expected_state {
		anyhow::bail!("state 与发起时不一致, 已拒绝该回调(可能是 CSRF 或链接已过期)");
	}
	code.ok_or_else(|| anyhow::anyhow!("回调缺少 code 参数"))
}

/// 阻塞等待 loopback 监听口上恰好一次授权回调: 轮询 accept(非阻塞), 直至(a)收到连接并读到
/// 请求行、(b)超过 timeout, 或(c)cancelled 被置位(用户关闭了登录窗口)。命中(a)时无论解析
/// 成败都会先回一段 CALLBACK_RESPONSE, 让用户的浏览器/WebView 不再停留在转圈状态;(b)/(c)路径
/// 下压根没有连接可回应。本函数只做阻塞轮询, 调用方应在专用线程(如
/// tauri::async_runtime::spawn_blocking)上调用, 不要在异步任务里直接调用(会占住执行器线程)
pub fn wait_for_callback(
	listener: TcpListener,
	expected_state: &str,
	timeout: Duration,
	cancelled: &AtomicBool,
) -> Result<String> {
	listener
		.set_nonblocking(true)
		.context("loopback 监听设置非阻塞模式失败")?;
	let deadline = Instant::now() + timeout;

	loop {
		if cancelled.load(Ordering::SeqCst) {
			anyhow::bail!("登录窗口已被关闭, 已取消本次登录");
		}
		if Instant::now() >= deadline {
			anyhow::bail!("等待授权回调超时, 请重试");
		}

		match listener.accept() {
			Ok((stream, _addr)) => return handle_callback_connection(stream, expected_state),
			Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
				std::thread::sleep(POLL_INTERVAL);
			}
			Err(e) => return Err(e).context("接受 loopback 连接失败"),
		}
	}
}

/// 处理已接受的一条 loopback 连接: 读取其请求行、解析+校验 code/state, 无论成败都回一段极简
/// 200 响应, 最终把解析结果透传给调用方
fn handle_callback_connection(mut stream: TcpStream, expected_state: &str) -> Result<String> {
	// 5 秒读超时: 正常回调会在建连后立即发送请求行, 超过此值大概率是异常连接(如端口探测), 不应
	// 无限期占住这唯一一次 accept 机会
	stream
		.set_read_timeout(Some(Duration::from_secs(5)))
		.context("设置回调连接读超时失败")?;

	let mut buf = [0u8; REQUEST_BUFFER_BYTES];
	let n = stream.read(&mut buf).context("读取回调请求失败")?;
	let request = String::from_utf8_lossy(&buf[..n]);
	let request_line = request.lines().next().unwrap_or("");
	let result = parse_callback(request_line, expected_state);

	let _ = stream.write_all(CALLBACK_RESPONSE.as_bytes());
	let _ = stream.flush();

	result
}

// ---------- validate_pat ----------

/// 校验一个访问令牌(手动录入的 PAT, 或 OAuth 换来的 access token 均可): 调该 provider 的身份
/// 接口取账号标识, 组装成待入库的 AuthAccountRespVO(id/connect_time 均为占位值, 由 store 落库时
/// 填充真实值, 见其文档)。base 可注入(生产用 default_validate_base, 测试指向 wiremock)。
/// Token(手动录入的通用访问令牌, 不属于 GitHub/Google/Microsoft 任一品牌)没有可调的身份接口,
/// 跳过网络请求, 直接返回一个固定账号标签的 AuthAccountRespVO(见 (provider,account) 唯一键约束下,
/// 这相当于"仅此一条"的通用令牌槽位)
pub async fn validate_pat(
	client: &Client,
	provider: ProviderKind,
	base: &str,
	token: &str,
) -> Result<AuthAccountRespVO> {
	let (account, scope) = match provider {
		ProviderKind::Token => ("access-token".to_string(), String::new()),
		_ => fetch_identity(client, provider, base, token).await?,
	};
	Ok(AuthAccountRespVO {
		id: 0,
		provider,
		account,
		scope,
		status: true,
		connect_time: String::new(),
	})
}

/// 调 GitHub/Google/Microsoft 各自身份接口, 用给定的 bearer token 换取 (账号标识, 授权范围)。
/// 三家响应体形状互不相同, 且这里只需要从中摘取一个字符串字段, 故用 serde_json::Value 通用
/// 解析 + identity_field 按 provider 分派取字段, 不为每家单独定义一个只用一次的强类型结构体
async fn fetch_identity(
	client: &Client,
	provider: ProviderKind,
	base: &str,
	token: &str,
) -> Result<(String, String)> {
	let url = format!("{base}{}", identity_path(provider));
	let response = client
		.get(&url)
		.bearer_auth(token)
		.send()
		.await
		.with_context(|| format!("校验令牌请求发送失败: {url}"))?;

	if !response.status().is_success() {
		anyhow::bail!("令牌校验失败, 状态码 {}: {url}", response.status());
	}

	// GitHub 特有: 授权范围不在响应体里, 在 X-OAuth-Scopes 响应头(逗号分隔); 需在消费响应体
	// (下面的 .json())前先取出。HeaderMap 按 header 名大小写不敏感匹配, 小写字面量即可命中
	let scope = response
		.headers()
		.get("x-oauth-scopes")
		.and_then(|value| value.to_str().ok())
		.unwrap_or("")
		.to_string();

	let body: serde_json::Value = response
		.json()
		.await
		.with_context(|| format!("校验令牌响应体 JSON 解析失败: {url}"))?;

	let account = identity_field(provider, &body)
		.with_context(|| format!("响应体缺少可识别账号的字段: {url}"))?;
	Ok((account, scope))
}

/// 各提供方身份接口(whoami)的路径部分, 与 default_validate_base 给出的根地址拼接; Token 无
/// 对应品牌接口, validate_pat 已在派发时短路掉这一分支, 此处仅为保持 match 穷尽
fn identity_path(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "/user",
		ProviderKind::Google => "/v1/userinfo",
		ProviderKind::Microsoft => "/v1.0/me",
		ProviderKind::Token => "",
	}
}

/// 各提供方身份接口的生产环境根地址; 测试通过显式传参注入 wiremock 本地地址
pub fn default_validate_base(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "https://api.github.com",
		ProviderKind::Google => "https://openidconnect.googleapis.com",
		ProviderKind::Microsoft => "https://graph.microsoft.com",
		ProviderKind::Token => "",
	}
}

/// 从身份接口响应体取出用作 AuthAccountRespVO.account 的字段: GitHub 用 login; Google 优先 email,
/// 未开放 email scope 时退回 sub(始终存在的稳定用户 ID); Microsoft 优先 mail(部分租户为空)
/// 退回 userPrincipalName
fn identity_field(provider: ProviderKind, body: &serde_json::Value) -> Option<String> {
	let value = match provider {
		ProviderKind::GitHub => body.get("login"),
		ProviderKind::Google => body.get("email").or_else(|| body.get("sub")),
		ProviderKind::Microsoft => body.get("mail").or_else(|| body.get("userPrincipalName")),
		ProviderKind::Token => None,
	}?;
	value.as_str().map(str::to_string)
}

// ---------- store / logout / token_for ----------

/// 把 provider 映射为钥匙串条目键里用的简短小写英文 slug(而非中文或枚举 Debug 输出), 保持
/// 钥匙串条目名称稳定可读, 不随枚举派生的 Debug 格式变化而失配
fn provider_slug(provider: ProviderKind) -> &'static str {
	match provider {
		ProviderKind::GitHub => "github",
		ProviderKind::Google => "google",
		ProviderKind::Microsoft => "microsoft",
		ProviderKind::Token => "token",
	}
}

/// 拼出该账号在系统钥匙串里的 account 键(与 infra::keychain 固定的 service 名 "skillhub"
/// 组合定位一条钥匙串记录), 形如 "github:demo@example.com"; 同一个键也落进
/// auth_account.keyring_ref 列(见 repo_auth::upsert), 供 token_for/logout 按行反查
fn build_keyring_ref(provider: ProviderKind, account: &str) -> String {
	format!("{}:{}", provider_slug(provider), account)
}

/// refresh token 复用同一账号的 keyring_ref, 拼后缀单独存一条钥匙串记录, 与 access token 分开
/// 存放, 避免互相覆盖; 该键不落库(auth_account.keyring_ref 只记 access token 那一个), 需要时
/// 按 access 的 keyring_ref 现拼即可, 无需额外持久化
fn refresh_keyring_ref(primary_ref: &str) -> String {
	format!("{primary_ref}:refresh")
}

/// 借同一个 SQLite 连接取当前 UTC 时间(与其它时间戳列的 datetime('now')默认值同格式), 供
/// store 落库前填充 AuthAccountRespVO.connect_time; 不引入日期时间 crate, 与全库时间戳保持同一
/// 权威时间源与格式(另见 unix_expiry 处"为什么不用日期库"的说明)
fn sqlite_now(conn: &Connection) -> rusqlite::Result<String> {
	conn.query_row("SELECT datetime('now')", [], |row| row.get(0))
}

/// 落库一个已校验通过的账号(PAT 校验或 OAuth 换 token 之后均调用本函数): account 入
/// auth_account 表(connect_time 由本函数取当前时间填充, 覆盖调用方传入的占位值), access/
/// refresh 令牌入系统钥匙串, 绝不落库(见 domain::auth::TokenSet 文档)
pub fn store(conn: &Connection, account: &AuthAccountRespVO, tokens: &TokenSet) -> Result<()> {
	let mut final_account = account.clone();
	final_account.connect_time = sqlite_now(conn)?;
	let keyring_ref = build_keyring_ref(account.provider, &account.account);

	repo_auth::upsert(conn, &final_account, &keyring_ref)?;
	keychain::set_token(&keyring_ref, &tokens.access)?;
	if let Some(refresh) = &tokens.refresh {
		keychain::set_token(&refresh_keyring_ref(&keyring_ref), refresh)?;
	}
	Ok(())
}

/// 断开某 provider 下全部已连接账号: 逐个删除其钥匙串条目(access + refresh, 后者即便未曾
/// 写入也是幂等删除, 见 infra::keychain::delete_token 文档), 再统一删库行。先删钥匙串再删
/// 库行(顺序颠倒会导致库已清空、却还留着查不到 account 名从而无法反查删除的孤儿钥匙串条目)
pub fn logout(conn: &Connection, provider: i64) -> Result<()> {
	let accounts = repo_auth::list(conn)?;
	for account in accounts
		.into_iter()
		.filter(|item| i64::from(item.provider) == provider)
	{
		let keyring_ref = build_keyring_ref(account.provider, &account.account);
		keychain::delete_token(&keyring_ref)?;
		keychain::delete_token(&refresh_keyring_ref(&keyring_ref))?;
	}
	repo_auth::delete(conn, provider)?;
	Ok(())
}

/// 取某 provider 当前可用的 access token, 供后端内部使用(如 market 刷新拿 GitHub 令牌提额)。
/// **刻意不做成 Tauri 命令、不经 IPC 暴露**: 按 provider 查已连接账号(不存在返回 None), 再用
/// 其 keyring_ref 读钥匙串——令牌只应活在 Rust 进程内存里, 传到前端就等于把凭证暴露给了
/// WebView 的 JS 上下文, 与 domain::auth::TokenSet"绝不落库/绝不可序列化"是同一约束的延伸
pub fn token_for(conn: &Connection, provider: i64) -> Result<Option<String>> {
	let Some(account) = repo_auth::get_by_provider(conn, provider)? else {
		return Ok(None);
	};
	let keyring_ref = build_keyring_ref(account.provider, &account.account);
	keychain::get_token(&keyring_ref)
}

#[cfg(test)]
mod tests {
	use std::collections::HashMap;

	use rusqlite::Connection;
	use wiremock::matchers::{bearer_token, body_string_contains, header, method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use super::*;
	use crate::infra::keychain::tests::{lock_keychain_tests, random_account};

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	/// 构造一个待落库的样例账号(provider 固定 GitHub, id/connect_time 均为占位值, 与
	/// validate_pat 实际产出的形状一致), account 由调用方传入(测试传随机名避免撞真实钥匙串条目)
	fn sample_account(account: &str) -> AuthAccountRespVO {
		AuthAccountRespVO {
			id: 0,
			provider: ProviderKind::GitHub,
			account: account.to_string(),
			scope: "read:user".to_string(),
			status: true,
			connect_time: String::new(),
		}
	}

	// ---------- build_pkce / challenge_from_verifier ----------

	// RFC 7636 Appendix B.1 官方已知测试向量: 该 verifier 的 S256 challenge 必须精确等于给定
	// 值(已独立用 Python hashlib+base64 复核过, 与本仓库实现无关联, 是真正的外部真值)
	#[test]
	fn challenge_from_verifier_matches_rfc7636_known_test_vector() {
		let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
		let challenge = challenge_from_verifier(verifier);
		assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
	}

	// build_pkce: verifier 长度落在 RFC 7636 允许的 [43,128] 区间, 且全部字符属于 unreserved
	// 的 base64url 子集(A-Za-z0-9-_), method 固定 S256
	#[test]
	fn build_pkce_verifier_matches_rfc7636_length_and_charset() {
		let pkce = build_pkce();
		assert!(
			(43..=128).contains(&pkce.verifier.len()),
			"verifier 长度应在 43-128 之间, 实际 {}",
			pkce.verifier.len()
		);
		assert!(
			pkce.verifier
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
			"verifier 应只含 base64url 字符集: {}",
			pkce.verifier
		);
		assert_eq!(pkce.method, "S256");
	}

	// build_pkce: challenge 应与独立调用 challenge_from_verifier(基于同一 verifier)算出的值
	// 一致 —— 验证两者"接线"正确(加密正确性已由上一条已知向量测试单独覆盖)
	#[test]
	fn build_pkce_challenge_is_consistent_with_its_own_verifier() {
		let pkce = build_pkce();
		assert_eq!(pkce.challenge, challenge_from_verifier(&pkce.verifier));
	}

	// build_pkce: 两次调用应产出不同 verifier(基本随机性检查, 防止退化为定值实现)
	#[test]
	fn build_pkce_generates_different_verifier_each_call() {
		let a = build_pkce();
		let b = build_pkce();
		assert_ne!(a.verifier, b.verifier);
	}

	// ---------- authorize_url ----------

	// GitHub: 应含全部必需 query 参数, 且 host/path 精确指向 GitHub 授权页
	#[test]
	fn authorize_url_contains_required_params_for_github() {
		let url = authorize_url(
			ProviderKind::GitHub,
			"chal-abc",
			"http://127.0.0.1:9999/cb",
			"state-123",
		);
		let parsed = Url::parse(&url).unwrap();
		assert_eq!(parsed.host_str(), Some("github.com"));
		assert_eq!(parsed.path(), "/login/oauth/authorize");

		let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
		assert_eq!(pairs["client_id"], GITHUB_CLIENT_ID);
		assert_eq!(pairs["redirect_uri"], "http://127.0.0.1:9999/cb");
		assert_eq!(pairs["response_type"], "code");
		assert_eq!(pairs["code_challenge"], "chal-abc");
		assert_eq!(pairs["code_challenge_method"], "S256");
		assert_eq!(pairs["state"], "state-123");
		assert!(!pairs["scope"].is_empty());
	}

	// Google: host 指向 Google 授权页, 其余必需参数齐全
	#[test]
	fn authorize_url_contains_required_params_for_google() {
		let url = authorize_url(ProviderKind::Google, "chal", "http://127.0.0.1:1/cb", "st");
		let parsed = Url::parse(&url).unwrap();
		assert_eq!(parsed.host_str(), Some("accounts.google.com"));

		let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
		assert_eq!(pairs["client_id"], GOOGLE_CLIENT_ID);
		assert_eq!(pairs["code_challenge"], "chal");
		assert_eq!(pairs["code_challenge_method"], "S256");
		assert_eq!(pairs["state"], "st");
	}

	// Microsoft: host 指向 Microsoft 授权页, 其余必需参数齐全
	#[test]
	fn authorize_url_contains_required_params_for_microsoft() {
		let url = authorize_url(
			ProviderKind::Microsoft,
			"chal",
			"http://127.0.0.1:1/cb",
			"st",
		);
		let parsed = Url::parse(&url).unwrap();
		assert_eq!(parsed.host_str(), Some("login.microsoftonline.com"));

		let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
		assert_eq!(pairs["client_id"], MICROSOFT_CLIENT_ID);
		assert_eq!(pairs["code_challenge"], "chal");
		assert_eq!(pairs["code_challenge_method"], "S256");
		assert_eq!(pairs["state"], "st");
	}

	// Token: 没有 OAuth 授权页, 应返回空串而非拼出某个错误地址
	#[test]
	fn authorize_url_returns_empty_string_for_token_provider() {
		assert_eq!(authorize_url(ProviderKind::Token, "c", "r", "s"), "");
	}

	// ---------- random_state / build_redirect_uri / parse_callback / wait_for_callback ----------

	// random_state: 应与 build_pkce verifier 同规格(同一随机源实现, 长度区间借用 RFC 7636 只是
	// 恰好一致, state 本身不受该 RFC 约束), 且字符集只含 base64url
	#[test]
	fn random_state_has_expected_length_and_charset() {
		let state = random_state();
		assert!(
			(43..=128).contains(&state.len()),
			"state 长度应在 43-128 之间, 实际 {}",
			state.len()
		);
		assert!(
			state
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
			"state 应只含 base64url 字符集: {}",
			state
		);
	}

	// random_state: 两次调用应产出不同值(基本随机性检查)
	#[test]
	fn random_state_generates_different_value_each_call() {
		assert_ne!(random_state(), random_state());
	}

	// build_redirect_uri: 应固定用 127.0.0.1 + 给定端口 + /callback 路径
	#[test]
	fn build_redirect_uri_formats_loopback_url_with_given_port() {
		assert_eq!(build_redirect_uri(54321), "http://127.0.0.1:54321/callback");
	}

	// parse_callback: 正常请求行(state 匹配)应取出 code
	#[test]
	fn parse_callback_extracts_code_from_request_line() {
		let code = parse_callback("GET /callback?code=abc123&state=st-1 HTTP/1.1", "st-1").unwrap();
		assert_eq!(code, "abc123");
	}

	// parse_callback: 也应支持直接传入完整重定向 URL(而非请求行), 函数名 request_line_or_url
	// 即表明两种输入形式都要支持
	#[test]
	fn parse_callback_extracts_code_from_full_url() {
		let code = parse_callback(
			"http://127.0.0.1:9999/callback?code=xyz789&state=st-2",
			"st-2",
		)
		.unwrap();
		assert_eq!(code, "xyz789");
	}

	// parse_callback: 缺 code 应报错
	#[test]
	fn parse_callback_returns_err_when_code_missing() {
		let err = parse_callback("GET /callback?state=st-3 HTTP/1.1", "st-3").unwrap_err();
		assert!(err.to_string().contains("code"));
	}

	// parse_callback: state 缺失应报错(与"不符"同归为拒绝, 不单独放行)
	#[test]
	fn parse_callback_returns_err_when_state_missing() {
		let err = parse_callback("GET /callback?code=abc HTTP/1.1", "expected").unwrap_err();
		assert!(err.to_string().contains("state"));
	}

	// parse_callback: state 与期望值不符应报错(核心防 CSRF 场景)
	#[test]
	fn parse_callback_returns_err_when_state_mismatches() {
		let err =
			parse_callback("GET /callback?code=abc&state=wrong HTTP/1.1", "expected").unwrap_err();
		assert!(err.to_string().contains("state"));
	}

	// wait_for_callback: 真实建立一次 TCP 连接发送合法回调, 应返回 Ok(code), 且客户端应收到
	// 200 响应(不能让浏览器/WebView 停在转圈状态)
	#[test]
	fn wait_for_callback_returns_code_on_valid_connection() {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let port = listener.local_addr().unwrap().port();
		let cancelled = AtomicBool::new(false);

		let client = std::thread::spawn(move || {
			let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
			stream
				.write_all(b"GET /callback?code=real-code&state=real-state HTTP/1.1\r\n\r\n")
				.unwrap();
			let mut response = String::new();
			stream.read_to_string(&mut response).unwrap();
			response
		});

		let result = wait_for_callback(listener, "real-state", Duration::from_secs(5), &cancelled);
		let response = client.join().unwrap();

		assert_eq!(result.unwrap(), "real-code");
		assert!(
			response.starts_with("HTTP/1.1 200"),
			"应回 200 响应: {response}"
		);
	}

	// wait_for_callback: 无连接到达且超过 timeout, 应超时报错而非无限期挂起
	#[test]
	fn wait_for_callback_times_out_when_no_connection_arrives() {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let cancelled = AtomicBool::new(false);

		let started = Instant::now();
		let result = wait_for_callback(
			listener,
			"any-state",
			Duration::from_millis(300),
			&cancelled,
		);
		let elapsed = started.elapsed();

		assert!(result.is_err());
		assert!(
			elapsed < Duration::from_secs(2),
			"超时应接近设定值而非长期挂起, 实际耗时 {elapsed:?}"
		);
	}

	// wait_for_callback: cancelled 标志被置位(模拟用户关闭登录窗口)应提前报错退出, 不等到超时
	#[test]
	fn wait_for_callback_stops_early_when_cancelled() {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let cancelled = AtomicBool::new(true);

		let started = Instant::now();
		let result = wait_for_callback(listener, "any-state", Duration::from_secs(60), &cancelled);
		let elapsed = started.elapsed();

		assert!(result.is_err());
		assert!(
			elapsed < Duration::from_secs(1),
			"取消应立即生效而非等待 60 秒超时, 实际耗时 {elapsed:?}"
		);
	}

	// wait_for_callback: state 不符时也应先回 200 响应再报错(不能让浏览器停在转圈状态), 即便
	// 这次登录最终会被判定失败
	#[test]
	fn wait_for_callback_still_responds_when_state_mismatches() {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let port = listener.local_addr().unwrap().port();
		let cancelled = AtomicBool::new(false);

		let client = std::thread::spawn(move || {
			let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
			stream
				.write_all(b"GET /callback?code=c&state=wrong HTTP/1.1\r\n\r\n")
				.unwrap();
			let mut response = String::new();
			stream.read_to_string(&mut response).unwrap();
			response
		});

		let result = wait_for_callback(listener, "expected", Duration::from_secs(5), &cancelled);
		let response = client.join().unwrap();

		assert!(result.is_err());
		assert!(
			response.starts_with("HTTP/1.1 200"),
			"即便失败也应回 200: {response}"
		);
	}

	// ---------- exchange_code ----------

	// 成功: 应返回解析出的 TokenSet, expires_at 换算为"晚于当前时间"的 Unix 时间戳字符串
	#[tokio::test]
	async fn exchange_code_returns_token_set_on_success() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
				"access_token": "gho_abc123",
				"refresh_token": "ghr_xyz789",
				"expires_in": 28800,
			})))
			.mount(&server)
			.await;

		let tokens = exchange_code(
			&crate::infra::http::client(),
			ProviderKind::GitHub,
			&server.uri(),
			"test-code",
			"test-verifier",
			"http://127.0.0.1:9999/cb",
		)
		.await
		.unwrap();

		assert_eq!(tokens.access, "gho_abc123");
		assert_eq!(tokens.refresh, Some("ghr_xyz789".to_string()));
		let expires_at: u64 = tokens.expires_at.unwrap().parse().unwrap();
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		assert!(expires_at > now, "expires_at 应晚于当前时间");
	}

	// 应把 code/code_verifier/redirect_uri 放进表单体, 且带 Accept: application/json(GitHub
	// 需要这个头才会返回 JSON 而非默认的 form-urlencoded 响应体); mock 严格匹配, 值不对/头
	// 缺失都不会命中, 走 wiremock 默认 404, exchange_code 会返回 Err, 与此测试期望的 Ok 相悖
	// 从而暴露问题
	#[tokio::test]
	async fn exchange_code_sends_form_fields_and_accept_json_header() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.and(header("Accept", "application/json"))
			.and(body_string_contains("code=test-code"))
			.and(body_string_contains("code_verifier=test-verifier"))
			.and(body_string_contains("redirect_uri=http"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_json(serde_json::json!({"access_token": "tok"})),
			)
			.mount(&server)
			.await;

		let result = exchange_code(
			&crate::infra::http::client(),
			ProviderKind::GitHub,
			&server.uri(),
			"test-code",
			"test-verifier",
			"http://127.0.0.1:9999/cb",
		)
		.await;

		assert!(result.is_ok());
	}

	// GitHub 出错时特有: 状态码仍是 200, 但响应体是 error/error_description、没有
	// access_token; 应归一为 Err 且带上 error_description 内容, 不能只看状态码判定成功
	#[tokio::test]
	async fn exchange_code_returns_err_when_body_reports_error_with_200_status() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
				"error": "bad_verification_code",
				"error_description": "The code passed is incorrect or expired.",
			})))
			.mount(&server)
			.await;

		let err = exchange_code(
			&crate::infra::http::client(),
			ProviderKind::GitHub,
			&server.uri(),
			"bad-code",
			"verifier",
			"http://127.0.0.1:9999/cb",
		)
		.await
		.unwrap_err();

		assert!(err.to_string().contains("incorrect or expired"));
	}

	// 非 2xx 状态码且响应体不是合法 TokenResponse JSON: 应归一为 Err, 不 panic
	#[tokio::test]
	async fn exchange_code_returns_err_on_non_success_status() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
			.mount(&server)
			.await;

		let result = exchange_code(
			&crate::infra::http::client(),
			ProviderKind::GitHub,
			&server.uri(),
			"code",
			"verifier",
			"http://127.0.0.1:9999/cb",
		)
		.await;
		assert!(result.is_err());
	}

	// ---------- validate_pat ----------

	// GitHub: 应取 login 作 account, X-OAuth-Scopes 响应头作 scope
	#[tokio::test]
	async fn validate_pat_returns_account_for_github_using_login_and_scope_header() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/user"))
			.and(bearer_token("ghp_test123"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_json(serde_json::json!({"login": "demo-user"}))
					.insert_header("X-OAuth-Scopes", "repo, read:org"),
			)
			.mount(&server)
			.await;

		let account = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::GitHub,
			&server.uri(),
			"ghp_test123",
		)
		.await
		.unwrap();

		assert_eq!(account.provider, ProviderKind::GitHub);
		assert_eq!(account.account, "demo-user");
		assert_eq!(account.scope, "repo, read:org");
		assert!(account.status);
	}

	// Google: 应优先取 email 作 account
	#[tokio::test]
	async fn validate_pat_returns_account_for_google_using_email() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v1/userinfo"))
			.respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
				"email": "demo@gmail.com",
				"sub": "1234567890",
			})))
			.mount(&server)
			.await;

		let account = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::Google,
			&server.uri(),
			"google-token",
		)
		.await
		.unwrap();

		assert_eq!(account.account, "demo@gmail.com");
	}

	// Google: 没有 email(未开放该 scope)时应退回 sub
	#[tokio::test]
	async fn validate_pat_falls_back_to_sub_when_google_email_missing() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v1/userinfo"))
			.respond_with(
				ResponseTemplate::new(200).set_body_json(serde_json::json!({"sub": "1234567890"})),
			)
			.mount(&server)
			.await;

		let account = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::Google,
			&server.uri(),
			"google-token",
		)
		.await
		.unwrap();

		assert_eq!(account.account, "1234567890");
	}

	// Microsoft: 应取 mail 作 account
	#[tokio::test]
	async fn validate_pat_returns_account_for_microsoft_using_mail() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v1.0/me"))
			.respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
				"mail": "demo@contoso.com",
				"userPrincipalName": "demo@contoso.onmicrosoft.com",
			})))
			.mount(&server)
			.await;

		let account = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::Microsoft,
			&server.uri(),
			"ms-token",
		)
		.await
		.unwrap();

		assert_eq!(account.account, "demo@contoso.com");
	}

	// Microsoft: mail 为空(常见于部分租户)时应退回 userPrincipalName
	#[tokio::test]
	async fn validate_pat_falls_back_to_upn_when_microsoft_mail_missing() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v1.0/me"))
			.respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
				"userPrincipalName": "demo@contoso.onmicrosoft.com",
			})))
			.mount(&server)
			.await;

		let account = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::Microsoft,
			&server.uri(),
			"ms-token",
		)
		.await
		.unwrap();

		assert_eq!(account.account, "demo@contoso.onmicrosoft.com");
	}

	// 401: 应归一为 Err, 不 panic
	#[tokio::test]
	async fn validate_pat_returns_err_on_unauthorized_status() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/user"))
			.respond_with(ResponseTemplate::new(401))
			.mount(&server)
			.await;

		let result = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::GitHub,
			&server.uri(),
			"bad-token",
		)
		.await;
		assert!(result.is_err());
	}

	// Token(手动录入的通用访问令牌): 无品牌身份接口可调, 应跳过网络请求直接返回固定账号标签;
	// 用一个几乎必然被拒连的地址作 base(而非启动 wiremock), 若实现误发起了请求, 本测试会因
	// 连接失败而 Err, 从而反证"确实跳过了网络调用"这一约定
	#[tokio::test]
	async fn validate_pat_for_token_provider_skips_network_call() {
		let account = validate_pat(
			&crate::infra::http::client(),
			ProviderKind::Token,
			"http://127.0.0.1:1",
			"raw-token-value",
		)
		.await
		.unwrap();

		assert_eq!(account.provider, ProviderKind::Token);
		assert_eq!(account.account, "access-token");
		assert_eq!(account.scope, "");
	}

	// ---------- store / logout / token_for ----------

	// store: 应把账号 upsert 进库(id/connect_time 由 store 自行确定, 覆盖传入的占位值),
	// access token 写入系统钥匙串且可原样取回
	#[test]
	fn store_upserts_account_and_writes_access_token_to_keychain() {
		let _guard = lock_keychain_tests();
		let conn = setup_conn();
		let test_account = random_account();
		let account = sample_account(&test_account);
		let tokens = TokenSet {
			access: "access-value".to_string(),
			refresh: None,
			expires_at: None,
		};

		store(&conn, &account, &tokens).unwrap();

		let stored = repo_auth::get_by_provider(&conn, i64::from(ProviderKind::GitHub))
			.unwrap()
			.expect("store 后应能查到该账号");
		assert_eq!(stored.account, test_account);
		assert_eq!(stored.scope, "read:user");
		assert!(
			!stored.connect_time.is_empty(),
			"connect_time 应已由 store 填充"
		);

		let keyring_ref = build_keyring_ref(ProviderKind::GitHub, &test_account);
		assert_eq!(
			keychain::get_token(&keyring_ref).unwrap(),
			Some("access-value".to_string())
		);

		keychain::delete_token(&keyring_ref).unwrap();
	}

	// store: refresh token(若提供)应额外写入 ":refresh" 后缀的钥匙串条目, 与 access 分开存放
	#[test]
	fn store_also_writes_refresh_token_under_suffixed_keyring_ref() {
		let _guard = lock_keychain_tests();
		let conn = setup_conn();
		let test_account = random_account();
		let account = sample_account(&test_account);
		let tokens = TokenSet {
			access: "access-value".to_string(),
			refresh: Some("refresh-value".to_string()),
			expires_at: None,
		};

		store(&conn, &account, &tokens).unwrap();

		let keyring_ref = build_keyring_ref(ProviderKind::GitHub, &test_account);
		let refresh_ref = refresh_keyring_ref(&keyring_ref);
		assert_eq!(
			keychain::get_token(&refresh_ref).unwrap(),
			Some("refresh-value".to_string())
		);

		keychain::delete_token(&keyring_ref).unwrap();
		keychain::delete_token(&refresh_ref).unwrap();
	}

	// token_for: store 之后应能按 provider 取回同一 access token
	#[test]
	fn token_for_returns_stored_access_token_after_store() {
		let _guard = lock_keychain_tests();
		let conn = setup_conn();
		let test_account = random_account();
		let account = sample_account(&test_account);
		let tokens = TokenSet {
			access: "access-for-token-for".to_string(),
			refresh: None,
			expires_at: None,
		};
		store(&conn, &account, &tokens).unwrap();

		let got = token_for(&conn, i64::from(ProviderKind::GitHub)).unwrap();
		assert_eq!(got, Some("access-for-token-for".to_string()));

		let keyring_ref = build_keyring_ref(ProviderKind::GitHub, &test_account);
		keychain::delete_token(&keyring_ref).unwrap();
	}

	// token_for: 该 provider 从未连接过账号时应返回 None, 不是 Err, 也不接触钥匙串
	#[test]
	fn token_for_returns_none_when_no_account_connected() {
		let conn = setup_conn();
		let got = token_for(&conn, i64::from(ProviderKind::GitHub)).unwrap();
		assert_eq!(got, None);
	}

	// logout: 应删库行 + 删钥匙串条目(access + refresh), 之后 token_for 应回落 None
	#[test]
	fn logout_removes_db_row_and_keychain_entries() {
		let _guard = lock_keychain_tests();
		let conn = setup_conn();
		let test_account = random_account();
		let account = sample_account(&test_account);
		let tokens = TokenSet {
			access: "to-be-logged-out".to_string(),
			refresh: Some("refresh-to-be-logged-out".to_string()),
			expires_at: None,
		};
		store(&conn, &account, &tokens).unwrap();
		let keyring_ref = build_keyring_ref(ProviderKind::GitHub, &test_account);

		logout(&conn, i64::from(ProviderKind::GitHub)).unwrap();

		assert_eq!(
			repo_auth::get_by_provider(&conn, i64::from(ProviderKind::GitHub)).unwrap(),
			None
		);
		assert_eq!(keychain::get_token(&keyring_ref).unwrap(), None);
		assert_eq!(
			keychain::get_token(&refresh_keyring_ref(&keyring_ref)).unwrap(),
			None
		);
	}

	// logout: 该 provider 下无任何账号时应视为幂等成功, 不报错
	#[test]
	fn logout_is_idempotent_when_no_accounts_for_provider() {
		let conn = setup_conn();
		assert!(logout(&conn, i64::from(ProviderKind::GitHub)).is_ok());
	}
}
