// 文件作用: 统一 HTTP 客户端封装 —— 固定超时/UA 的 reqwest::Client 构造, 以及带 Bearer 鉴权与
//           ETag 增量刷新(If-None-Match/304)的 JSON GET 封装, 供 M2 市场三源聚合
//           (github_skills/mcp_registry/github_mcp)与 OAuth 令牌交换复用
// 创建日期: 2026-07-09

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;

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
}
