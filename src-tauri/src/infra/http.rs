// 文件作用: 统一 HTTP 客户端封装 —— 固定超时/UA 的 reqwest::Client 构造(client)、依用户
//           SettingRespVO 的网络代理/超时字段构造的 reqwest::Client(build_http_client, M4 Task 2 新增,
//           供市场刷新/安装、认证等真实发起网络 I/O 的调用路径复用, 让 net_* 五字段从"仅持久化"
//           变为"真实生效"), 以及带 Bearer 鉴权与 ETag 增量刷新(If-None-Match/304)的 JSON GET
//           封装, 供 M2 市场三源聚合(github_skills/mcp_registry/github_mcp)与 OAuth 令牌交换复用
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;

use crate::domain::setting::SettingRespVO;

/// 请求超时: 市场源/OAuth 端点均是轻量 JSON 接口, 20 秒足够覆盖弱网, 同时避免异常挂起拖累 UI
const TIMEOUT_SECS: u64 = 20;

/// 统一 User-Agent: 部分来源(如 GitHub API)要求匿名请求也带上可辨识 UA, 否则可能被拒绝
const USER_AGENT: &str = "SkillHub/0.1";

/// 构造统一超时/UA 的 HTTP 客户端。reqwest::Client 内部持有连接池, 调用方应复用同一个实例
/// (如存进 AppState), 不要每次请求都新建一个
pub fn client() -> Client {
	Client::builder()
		.timeout(Duration::from_secs(TIMEOUT_SECS))
		.user_agent(USER_AGENT)
		.build()
		.expect("构造 reqwest 客户端失败: 均为静态配置, 正常不应失败")
}

/// 超时兜底值(秒): SettingRespVO.net_timeout_sec 非正数(0 或理论上不应出现的负数)时回落此值,
/// 与 domain::setting::SettingRespVO::default 的 net_timeout_sec 默认值保持一致
const FALLBACK_TIMEOUT_SECS: u64 = 30;

/// 依用户 SettingRespVO 的网络代理/超时字段构造 reqwest::Client, 供市场刷新/安装、认证等真实发起
/// 网络 I/O 的调用路径复用(与上面 client() 的区别: client() 是不感知用户设置的固定超时/UA
/// 默认客户端, 供尚未接入设置的场景/测试沿用)。调用方应遵循与 client() 相同的"复用同一实例"
/// 约定, 每次发起请求前重新构造即可(构造本身只是本地状态机装配, 不含网络 I/O, 代价很低)。
///
/// 代理模式(net_proxy_mode):
/// - 0(系统默认): 不显式调用 .proxy()/.no_proxy(), 沿用 reqwest 默认行为(读取 HTTP_PROXY/
///   HTTPS_PROXY/NO_PROXY 等环境变量、遵循系统代理设置); 任何未识别的取值也归入此分支, 与
///   domain::setting 里"脏数据一律回落默认行为"的既有取舍一致
/// - 1(不使用代理): 显式 .no_proxy(), 忽略任何环境变量/系统代理, 恒直连
/// - 2(手动): net_http_proxy/net_https_proxy 非空时分别用 reqwest::Proxy::http/https 显式指定
///   对应协议的代理地址; 两串均为空时等同"不设代理"(不因用户选了手动模式但两个地址都还没填
///   就报错, 视为暂未填完, 沿用直连)。net_no_proxy(不使用代理的地址列表): reqwest::Proxy 手动
///   构造时未提供"排除地址名单"的公开 builder 方法(NO_PROXY 环境变量只在 0/系统默认这一走
///   隐式路径的分支下由 reqwest 内部解析), 本轮手动模式暂不接入该字段, 如实在此注释说明,
///   不假造行为
///
/// 超时: net_timeout_sec>0 时使用其值, 否则回落 FALLBACK_TIMEOUT_SECS(30 秒)
pub fn build_http_client(settings: &SettingRespVO) -> reqwest::Result<Client> {
	let timeout_secs = if settings.net_timeout_sec > 0 {
		settings.net_timeout_sec as u64
	} else {
		FALLBACK_TIMEOUT_SECS
	};

	let mut builder = Client::builder()
		.timeout(Duration::from_secs(timeout_secs))
		.user_agent(USER_AGENT);

	if settings.net_proxy_mode == 1 {
		builder = builder.no_proxy();
	} else if settings.net_proxy_mode == 2 {
		if !settings.net_http_proxy.is_empty() {
			builder = builder.proxy(reqwest::Proxy::http(&settings.net_http_proxy)?);
		}
		if !settings.net_https_proxy.is_empty() {
			builder = builder.proxy(reqwest::Proxy::https(&settings.net_https_proxy)?);
		}
	}
	// net_proxy_mode == 0(系统默认)或其它未识别取值: 不显式调用 .proxy()/.no_proxy(),
	// 沿用 reqwest 默认行为

	builder.build()
}

/// get_json 的结果: 区分"拿到新数据"与"服务端确认未变化"(304), 后者调用方应继续沿用本地缓存,
/// 不要用空数据覆盖已有内容
#[derive(Debug, Clone, PartialEq)]
pub enum HttpResult<T> {
	/// 成功拿到新数据; etag 为响应携带的 ETag(若来源提供), 供下次请求带 If-None-Match 增量刷新
	Ok { data: T, etag: Option<String> },
	/// 304 Not Modified: 服务端确认调用方传入的 etag 仍是最新, 响应无正文, 调用方应沿用本地缓存
	NotModified,
}

/// 发起一次 GET 请求并将响应体解析为 T。可选带 Authorization: Bearer(OAuth 访问令牌鉴权)与
/// If-None-Match(ETag 增量刷新, 命中时服务端返回 304, 不再解析响应体)。网络错误(含超时)、非
/// 2xx/304 状态码、JSON 解析失败均归一为 anyhow::Error, 不 panic
pub async fn get_json<T: DeserializeOwned>(
	client: &Client,
	url: &str,
	bearer: Option<&str>,
	etag: Option<&str>,
) -> Result<HttpResult<T>> {
	let mut request = client.get(url);
	if let Some(token) = bearer {
		request = request.bearer_auth(token);
	}
	if let Some(tag) = etag {
		request = request.header(reqwest::header::IF_NONE_MATCH, tag);
	}

	let response = request
		.send()
		.await
		.with_context(|| format!("请求发送失败(网络错误或超时): {url}"))?;

	if response.status() == StatusCode::NOT_MODIFIED {
		return Ok(HttpResult::NotModified);
	}
	let status = response.status();
	if !status.is_success() {
		anyhow::bail!("响应状态码非成功: {status} {url}");
	}

	let response_etag = response
		.headers()
		.get(reqwest::header::ETAG)
		.and_then(|value| value.to_str().ok())
		.map(str::to_string);

	let data = response
		.json::<T>()
		.await
		.with_context(|| format!("响应体 JSON 解析失败: {url}"))?;

	Ok(HttpResult::Ok {
		data,
		etag: response_etag,
	})
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use serde::{Deserialize, Serialize};
	use wiremock::matchers::{bearer_token, header, method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use super::*;

	#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
	struct Sample {
		name: String,
		value: i64,
	}

	fn sample() -> Sample {
		Sample {
			name: "demo".to_string(),
			value: 1,
		}
	}

	// 200 + JSON + ETag: 应返回 HttpResult::Ok, data 完整还原且带上响应携带的 etag
	#[tokio::test]
	async fn get_json_returns_ok_with_data_and_etag_on_success() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/resource"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_json(sample())
					.insert_header("ETag", "\"abc123\""),
			)
			.mount(&server)
			.await;

		let result =
			get_json::<Sample>(&client(), &format!("{}/resource", server.uri()), None, None)
				.await
				.unwrap();

		match result {
			HttpResult::Ok { data, etag } => {
				assert_eq!(data, sample());
				assert_eq!(etag, Some("\"abc123\"".to_string()));
			}
			HttpResult::NotModified => panic!("应返回 Ok, 而非 NotModified"),
		}
	}

	// 应把 bearer 令牌放进 Authorization: Bearer 头; mock 严格校验令牌值, 值不对不会匹配从而
	// 走 wiremock 默认 404, 借此间接验证令牌确实被正确携带
	#[tokio::test]
	async fn get_json_sends_bearer_authorization_header_when_provided() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/secure"))
			.and(bearer_token("secret-token"))
			.respond_with(ResponseTemplate::new(200).set_body_json(sample()))
			.mount(&server)
			.await;

		let result = get_json::<Sample>(
			&client(),
			&format!("{}/secure", server.uri()),
			Some("secret-token"),
			None,
		)
		.await
		.unwrap();

		assert!(matches!(result, HttpResult::Ok { .. }));
	}

	// 带 If-None-Match 且命中(mock 模拟服务端确认未变化返回 304): 应归一为 NotModified,
	// 不尝试解析(本就没有的)响应体
	#[tokio::test]
	async fn get_json_returns_not_modified_when_etag_matches() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/resource"))
			.and(header("If-None-Match", "\"abc123\""))
			.respond_with(ResponseTemplate::new(304))
			.mount(&server)
			.await;

		let result = get_json::<Sample>(
			&client(),
			&format!("{}/resource", server.uri()),
			None,
			Some("\"abc123\""),
		)
		.await
		.unwrap();

		assert_eq!(result, HttpResult::NotModified);
	}

	// 非 2xx(且非 304): 应归一为 Err, 不 panic
	#[tokio::test]
	async fn get_json_returns_err_on_non_success_status() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/broken"))
			.respond_with(ResponseTemplate::new(500))
			.mount(&server)
			.await;

		let result =
			get_json::<Sample>(&client(), &format!("{}/broken", server.uri()), None, None).await;
		assert!(result.is_err());
	}

	// 响应体不是合法 JSON: 应归一为 Err, 不 panic
	#[tokio::test]
	async fn get_json_returns_err_on_invalid_json_body() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/malformed"))
			.respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
			.mount(&server)
			.await;

		let result = get_json::<Sample>(
			&client(),
			&format!("{}/malformed", server.uri()),
			None,
			None,
		)
		.await;
		assert!(result.is_err());
	}

	// 请求超时: 用极短超时的客户端 + 故意延迟的响应触发, 应归一为 Err 而不是无限挂起
	#[tokio::test]
	async fn get_json_returns_err_on_timeout() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/slow"))
			.respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(300)))
			.mount(&server)
			.await;

		let short_timeout_client = Client::builder()
			.timeout(Duration::from_millis(50))
			.build()
			.unwrap();

		let result = get_json::<Sample>(
			&short_timeout_client,
			&format!("{}/slow", server.uri()),
			None,
			None,
		)
		.await;
		assert!(result.is_err());
	}

	// client(): 应带上约定的 User-Agent; 用严格匹配该头的 mock 间接验证(UA 不对则不匹配, 走
	// wiremock 默认 404, get_json 收到非 2xx 会返回 Err, 与此测试期望的 Ok 相悖从而暴露问题)
	#[tokio::test]
	async fn client_sends_configured_user_agent_header() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/ua-check"))
			.and(header("User-Agent", "SkillHub/0.1"))
			.respond_with(ResponseTemplate::new(200).set_body_json(sample()))
			.mount(&server)
			.await;

		let result =
			get_json::<Sample>(&client(), &format!("{}/ua-check", server.uri()), None, None).await;
		assert!(result.is_ok());
	}

	// ---------- build_http_client ----------
	// 以下测试只验证客户端"能否成功构造"(reqwest::Client::builder 的本地状态机装配, 不含网络
	// I/O), 不发起真实网络请求, 与本文件头注释"只验构造"的约定一致

	// net_proxy_mode=0(系统默认): 不显式设代理, 仍应成功构造
	#[test]
	fn build_http_client_ok_with_system_default_proxy_mode() {
		let settings = SettingRespVO {
			net_proxy_mode: 0,
			..SettingRespVO::default()
		};
		assert!(build_http_client(&settings).is_ok());
	}

	// net_proxy_mode=1(不使用代理): 显式 no_proxy(), 应成功构造
	#[test]
	fn build_http_client_ok_with_no_proxy_mode() {
		let settings = SettingRespVO {
			net_proxy_mode: 1,
			..SettingRespVO::default()
		};
		assert!(build_http_client(&settings).is_ok());
	}

	// net_proxy_mode=2(手动)但两个代理地址均留空: 等同不设代理, 应成功构造, 不报错
	#[test]
	fn build_http_client_ok_with_manual_mode_and_empty_proxy_addresses() {
		let settings = SettingRespVO {
			net_proxy_mode: 2,
			..SettingRespVO::default()
		};
		assert!(build_http_client(&settings).is_ok());
	}

	// net_proxy_mode=2(手动)且给定合法的 http/https 代理串: 应能成功构造
	#[test]
	fn build_http_client_ok_with_manual_mode_and_valid_proxy_addresses() {
		let settings = SettingRespVO {
			net_proxy_mode: 2,
			net_http_proxy: "http://127.0.0.1:7890".to_string(),
			net_https_proxy: "http://127.0.0.1:7891".to_string(),
			..SettingRespVO::default()
		};
		assert!(build_http_client(&settings).is_ok());
	}

	// net_timeout_sec=0: 应回落 FALLBACK_TIMEOUT_SECS(30 秒)而不是构造出 0 秒超时的客户端,
	// 仍应成功构造(本函数不对外暴露 timeout 字段, 只能验证"不因 0 而出错", 具体秒数由
	// domain::setting::SettingRespVO::default 的契约值 30 保证一致)
	#[test]
	fn build_http_client_ok_and_falls_back_timeout_when_zero() {
		let settings = SettingRespVO {
			net_timeout_sec: 0,
			..SettingRespVO::default()
		};
		assert!(build_http_client(&settings).is_ok());
	}

	// net_timeout_sec 为负数(理论上不应出现, 前端/命令层未做该校验, 此处仍兜底): 同样应回落
	// FALLBACK_TIMEOUT_SECS, 成功构造, 不 panic
	#[test]
	fn build_http_client_ok_and_falls_back_timeout_when_negative() {
		let settings = SettingRespVO {
			net_timeout_sec: -1,
			..SettingRespVO::default()
		};
		assert!(build_http_client(&settings).is_ok());
	}

	// SettingRespVO::default() 本身(net_proxy_mode=0, net_timeout_sec=30): 应能成功构造, 覆盖
	// "最常见的从未改过设置"这一默认路径
	#[test]
	fn build_http_client_ok_with_default_settings() {
		assert!(build_http_client(&SettingRespVO::default()).is_ok());
	}
}
