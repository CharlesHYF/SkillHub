// 文件作用: mcp_registry 市场源 —— 对接官方 MCP Registry(registry.modelcontextprotocol.io)的
//           v0/servers 列表接口, 把每个 server 归一化为 MarketResource(res_type=Mcp), 按其
//           packages/remotes 字段组装 McpServerDef, 含需用户填写的环境变量则产出
//           InstallManifest::McpTemplate(否则产出可直接安装的 InstallManifest::Mcp), 实现
//           SourceProvider(见 infra::source::mod)。registry 响应结构对外仍在演进(server.json
//           schema 有多个版本), 本文件全程用 serde_json::Value 逐字段安全提取, 不绑定强 schema,
//           缺失字段一律宽松兜底, 避免真实数据里个别字段的变化拖垮整次 search
// 创建日期: 2026-07-10

use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use crate::domain::agent::McpServerDef;
use crate::domain::market::{InstallManifest, MarketResource, Query, SourceId};
use crate::domain::resource::ResourceType;
use crate::infra::http::{get_json, HttpResult};

use super::{AuthKind, InstallPayload, SourceProvider};

/// 生产环境默认的官方 MCP Registry 根地址; 测试通过 McpRegistryProvider::with_base_url 注入
/// wiremock 本地地址, 绝不在测试里打真实 registry.modelcontextprotocol.io
const DEFAULT_BASE_URL: &str = "https://registry.modelcontextprotocol.io";

/// mcp_registry 市场源: 拉取官方 MCP Registry 的 server 列表并归一化。完全公开只读
/// (见 auth_kind), search 恒发起匿名请求, 不转发调用方传入的 token(即便非 None)
pub struct McpRegistryProvider {
	base_url: String,
}

impl McpRegistryProvider {
	/// 生产用构造: base_url 固定为官方 Registry 地址
	pub fn new() -> Self {
		Self {
			base_url: DEFAULT_BASE_URL.to_string(),
		}
	}

	/// 测试用构造: 注入 base_url(指向 wiremock 本地地址), 其余行为与 new 一致
	pub fn with_base_url(base_url: String) -> Self {
		Self { base_url }
	}
}

impl Default for McpRegistryProvider {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl SourceProvider for McpRegistryProvider {
	fn id(&self) -> SourceId {
		SourceId::McpRegistry
	}

	/// 拉取 v0/servers 首页(游标分页暂只取首页, 足够 MVP 浏览; 真正的"翻下一页"留待后续任务
	/// 按需接入响应里的 metadata.nextCursor), 把 servers 数组逐条归一化为 MarketResource。
	/// 本源完全公开, 不转发调用方传入的 token(即便非 None 也恒发匿名请求, 见结构体文档);
	/// query 参数交由聚合层做过滤(与 github_skills 同一惯例), 本方法恒返回首页全量
	async fn search(
		&self,
		client: &Client,
		_query: &Query,
		_token: Option<&str>,
	) -> Result<Vec<MarketResource>> {
		let url = format!("{}/v0/servers", self.base_url);
		let body = match get_json::<Value>(client, &url, None, None).await? {
			HttpResult::Ok { data, .. } => data,
			// 本调用未传 etag, 正常不会收到 304; 出现即视为异常, 报错而非静默兜底空列表
			HttpResult::NotModified => anyhow::bail!("意外的 304(本调用未传 etag): {url}"),
		};
		let servers = body
			.get("servers")
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();
		Ok(servers.iter().filter_map(normalize_server_entry).collect())
	}

	/// mcp_registry 的安装清单在 search 阶段已组装完整(Mcp/McpTemplate 均直接内嵌 server_def),
	/// 无需像 github_skills 那样再发起网络请求下载文件, 直接从 resource.install_manifest 取出
	/// server_def 组装 InstallPayload::Mcp; resource 必须是本源产出的 Mcp/McpTemplate 类资源,
	/// 否则报错
	async fn fetch_payload(
		&self,
		_client: &Client,
		resource: &MarketResource,
		_token: Option<&str>,
	) -> Result<InstallPayload> {
		let server_def = match &resource.install_manifest {
			InstallManifest::Mcp { server_def } => server_def.clone(),
			InstallManifest::McpTemplate { server_def, .. } => server_def.clone(),
			InstallManifest::Skill { .. } => anyhow::bail!(
				"mcp_registry 只能安装 Mcp/McpTemplate 类型的安装清单, 实际: {:?}",
				resource.install_manifest
			),
		};
		Ok(InstallPayload::Mcp { server_def })
	}

	/// 官方 Registry 公开只读, 搜索与安装均无需认证
	fn auth_kind(&self) -> Option<AuthKind> {
		None
	}
}

/// 把响应体 servers 数组里的一条归一化为 MarketResource; 支持两种形状: 官方当前的
/// `{server: {...}, _meta: {...}}` 嵌套形态, 以及(防御性地)server 字段被拍平到顶层的形态,
/// "server" 键存在则取之, 否则把条目本身当作 server 对象。name 字段缺失视为条目不合法,
/// 返回 None 交调用方跳过(不让个别脏数据拖垮整次 search)
fn normalize_server_entry(entry: &Value) -> Option<MarketResource> {
	let server = entry.get("server").unwrap_or(entry);
	let name = server.get("name").and_then(Value::as_str)?.to_string();
	let description = server
		.get("description")
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string();
	let version = server
		.get("version")
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string();
	let display_name = server
		.get("title")
		.and_then(Value::as_str)
		.map(str::to_string)
		.unwrap_or_else(|| name.clone());
	let updated_at = entry
		.get("_meta")
		.and_then(|meta| meta.get("io.modelcontextprotocol.registry/official"))
		.and_then(|official| official.get("updatedAt"))
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string();

	let (server_def, required_env) = build_server_def(server, &name);
	let install_manifest = if required_env.is_empty() {
		InstallManifest::Mcp { server_def }
	} else {
		InstallManifest::McpTemplate {
			server_def,
			required_env,
		}
	};

	Some(MarketResource {
		source_type: SourceId::McpRegistry,
		res_type: ResourceType::Mcp,
		ext_id: name.clone(),
		name: name.clone(),
		display_name,
		description,
		author: extract_author(server, &name),
		version,
		stars: 0,
		category: String::new(),
		tags: Vec::new(),
		auth_required: false,
		install_manifest,
		updated_at,
	})
}

/// 提取作者/命名空间: 优先取 repository.url 里的 owner 段(如 GitHub 仓库 URL 倒数第二段);
/// 取不到则退化为 reverse-DNS name 里最后一个 '/' 之前的命名空间段(如 "io.github.acme/demo" ->
/// "io.github.acme"); 两者皆无则退化为整段 name, 保证恒有值(不返回空串)
fn extract_author(server: &Value, name: &str) -> String {
	if let Some(owner) = server
		.get("repository")
		.and_then(|repo| repo.get("url"))
		.and_then(Value::as_str)
		.map(|url| url.trim_end_matches('/'))
		.and_then(|url| url.rsplit('/').nth(1))
		.filter(|owner| !owner.is_empty())
	{
		return owner.to_string();
	}
	match name.rsplit_once('/') {
		Some((namespace, _)) => namespace.to_string(),
		None => name.to_string(),
	}
}

/// 按 packages(优先)/remotes 组装 McpServerDef 与需用户填写的环境变量名列表; 两者皆缺失时
/// 归一化为一个信息为空的 server_def(不报错, 该条资源仍会被列出, 只是暂无法直接安装)
fn build_server_def(server: &Value, name: &str) -> (McpServerDef, Vec<String>) {
	if let Some(package) = server
		.get("packages")
		.and_then(Value::as_array)
		.and_then(|list| list.first())
	{
		return build_from_package(package, name);
	}
	if let Some(remote) = server
		.get("remotes")
		.and_then(Value::as_array)
		.and_then(|list| list.first())
	{
		return build_from_remote(remote, name);
	}
	(empty_server_def(name), Vec::new())
}

/// 由 packages[0] 组装本地可执行的 server_def: command 取 runtimeHint(缺失按 registryType
/// 猜一个常见启动器), args 按惯例给 npm 加 "-y"(npx 免交互确认)后追加 identifier;
/// environmentVariables 里的每个变量都在 env 里占位(有 default 用 default, 否则空串),
/// isRequired 非 false(含字段缺失)一律视为必填收进 required_env(宁可多问用户一句, 也不要
/// 漏填导致装完的服务跑不起来)
fn build_from_package(package: &Value, name: &str) -> (McpServerDef, Vec<String>) {
	let registry_type = package
		.get("registryType")
		.and_then(Value::as_str)
		.unwrap_or_default();
	let identifier = package.get("identifier").and_then(Value::as_str);

	let command = package
		.get("runtimeHint")
		.and_then(Value::as_str)
		.map(str::to_string)
		.or_else(|| guess_runtime_hint(registry_type));

	let mut args = Vec::new();
	if registry_type == "npm" {
		args.push("-y".to_string());
	}
	if let Some(id) = identifier {
		args.push(id.to_string());
	}

	let mut env = BTreeMap::new();
	let mut required_env = Vec::new();
	if let Some(vars) = package
		.get("environmentVariables")
		.and_then(Value::as_array)
	{
		for var in vars {
			let Some(var_name) = var.get("name").and_then(Value::as_str) else {
				continue;
			};
			let default_value = var
				.get("default")
				.and_then(Value::as_str)
				.unwrap_or_default();
			env.insert(var_name.to_string(), default_value.to_string());
			let is_required = var
				.get("isRequired")
				.and_then(Value::as_bool)
				.unwrap_or(true);
			if is_required {
				required_env.push(var_name.to_string());
			}
		}
	}

	(
		McpServerDef {
			name: name.to_string(),
			command,
			args,
			env,
			url: None,
		},
		required_env,
	)
}

/// registryType 缺失 runtimeHint 时的启动器猜测: 仅覆盖生态里最常见的 npm; 其余登记类型
/// (pypi/oci/nuget/cargo 等)暂不猜测, 命令留空交由安装流程提示用户手动补全, 好过埋入一个
/// 可能是错的命令
fn guess_runtime_hint(registry_type: &str) -> Option<String> {
	if registry_type == "npm" {
		Some("npx".to_string())
	} else {
		None
	}
}

/// 由 remotes[0] 组装一个远程 server_def: 已是托管好的远程服务, 无需本地命令也无需用户填写
/// 环境变量(url 字段即可直接使用), required_env 恒为空
fn build_from_remote(remote: &Value, name: &str) -> (McpServerDef, Vec<String>) {
	let url = remote
		.get("url")
		.and_then(Value::as_str)
		.map(str::to_string);
	(
		McpServerDef {
			name: name.to_string(),
			command: None,
			args: Vec::new(),
			env: BTreeMap::new(),
			url,
		},
		Vec::new(),
	)
}

/// packages 与 remotes 均缺失时的空壳 server_def: 该条资源仍会被列出(便于用户知道 registry 里
/// 存在这条记录), 只是暂无法直接安装, 留待后续人工/后续版本补充
fn empty_server_def(name: &str) -> McpServerDef {
	McpServerDef {
		name: name.to_string(),
		command: None,
		args: Vec::new(),
		env: BTreeMap::new(),
		url: None,
	}
}

#[cfg(test)]
mod tests {
	use serde_json::json;
	use wiremock::matchers::{method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use super::*;
	use crate::domain::market::SortBy;
	use crate::infra::http::client;

	fn sample_query() -> Query {
		Query {
			keyword: None,
			res_type: None,
			category: None,
			sort: SortBy::Recommended,
			page: 1,
			page_size: 20,
		}
	}

	fn sample_market_resource(install_manifest: InstallManifest) -> MarketResource {
		MarketResource {
			source_type: SourceId::McpRegistry,
			res_type: ResourceType::Mcp,
			ext_id: "io.github.acme/weather".to_string(),
			name: "io.github.acme/weather".to_string(),
			display_name: "io.github.acme/weather".to_string(),
			description: String::new(),
			author: "io.github.acme".to_string(),
			version: "1.0.0".to_string(),
			stars: 0,
			category: String::new(),
			tags: Vec::new(),
			auth_required: false,
			install_manifest,
			updated_at: String::new(),
		}
	}

	// search: 应从 servers 数组归一化出 2 条 MarketResource; 仅含 npm 包且无环境变量的 server
	// 应产出可直接安装的 InstallManifest::Mcp(command/args 按 npm 惯例组装: npx -y <identifier>);
	// 含必填/选填环境变量的 server 应产出 InstallManifest::McpTemplate, 必填项收进 required_env,
	// 选填项(带 default)只占位进 env。author 优先取 repository.url 的 owner 段, 缺失时退化为
	// reverse-DNS name 的命名空间段
	#[tokio::test]
	async fn search_normalizes_direct_and_template_servers_from_v0_servers() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v0/servers"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"servers": [
					{
						"server": {
							"name": "io.github.acme/weather",
							"description": "Weather MCP server",
							"version": "1.0.0",
							"repository": {
								"url": "https://github.com/acme-labs/weather-mcp",
								"source": "github"
							},
							"packages": [
								{
									"registryType": "npm",
									"identifier": "@acme/weather-mcp",
									"version": "1.0.0",
									"runtimeHint": "npx",
									"transport": {"type": "stdio"}
								}
							]
						},
						"_meta": {
							"io.modelcontextprotocol.registry/official": {
								"status": "active",
								"publishedAt": "2026-01-01T00:00:00Z",
								"updatedAt": "2026-02-01T00:00:00Z",
								"isLatest": true
							}
						}
					},
					{
						"server": {
							"name": "io.github.acme/search",
							"description": "Search MCP server needing an API key",
							"version": "2.1.0",
							"packages": [
								{
									"registryType": "npm",
									"identifier": "@acme/search-mcp",
									"version": "2.1.0",
									"runtimeHint": "npx",
									"transport": {"type": "stdio"},
									"environmentVariables": [
										{
											"name": "ACME_API_KEY",
											"description": "API Key",
											"isRequired": true,
											"isSecret": true
										},
										{
											"name": "ACME_REGION",
											"description": "Region",
											"isRequired": false,
											"default": "us-east-1"
										}
									]
								}
							]
						},
						"_meta": {
							"io.modelcontextprotocol.registry/official": {
								"status": "active",
								"publishedAt": "2026-01-05T00:00:00Z",
								"updatedAt": "2026-02-05T00:00:00Z",
								"isLatest": true
							}
						}
					}
				],
				"metadata": {"nextCursor": "", "count": 2}
			})))
			.mount(&server)
			.await;

		let provider = McpRegistryProvider::with_base_url(server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 2);

		let weather = resources
			.iter()
			.find(|r| r.ext_id == "io.github.acme/weather")
			.expect("应含 weather");
		assert_eq!(weather.source_type, SourceId::McpRegistry);
		assert_eq!(weather.res_type, ResourceType::Mcp);
		assert_eq!(weather.description, "Weather MCP server");
		assert_eq!(weather.version, "1.0.0");
		assert_eq!(
			weather.author, "acme-labs",
			"应优先从 repository.url 取 owner 段"
		);
		assert_eq!(weather.updated_at, "2026-02-01T00:00:00Z");
		assert!(!weather.auth_required);
		assert_eq!(
			weather.install_manifest,
			InstallManifest::Mcp {
				server_def: McpServerDef {
					name: "io.github.acme/weather".to_string(),
					command: Some("npx".to_string()),
					args: vec!["-y".to_string(), "@acme/weather-mcp".to_string()],
					env: BTreeMap::new(),
					url: None,
				}
			}
		);

		let search_srv = resources
			.iter()
			.find(|r| r.ext_id == "io.github.acme/search")
			.expect("应含 search");
		assert_eq!(
			search_srv.author, "io.github.acme",
			"缺 repository 字段应退化为 name 的命名空间段"
		);
		let InstallManifest::McpTemplate {
			server_def,
			required_env,
		} = &search_srv.install_manifest
		else {
			panic!("需要环境变量的 server 应产出 McpTemplate");
		};
		assert_eq!(required_env, &vec!["ACME_API_KEY".to_string()]);
		assert_eq!(server_def.env.get("ACME_API_KEY"), Some(&"".to_string()));
		assert_eq!(
			server_def.env.get("ACME_REGION"),
			Some(&"us-east-1".to_string()),
			"选填且带 default 的环境变量应用 default 值占位"
		);
	}

	// search: server 只含 remotes(无 packages, 即已托管好的远程服务)应产出可直接安装的
	// InstallManifest::Mcp, server_def.url 取 remotes[0].url, command 为 None, 无需任何环境变量
	#[tokio::test]
	async fn search_normalizes_remote_only_server_without_packages() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v0/servers"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"servers": [
					{
						"server": {
							"name": "io.github.acme/hosted",
							"description": "Hosted remote MCP server",
							"version": "3.0.0",
							"remotes": [
								{"type": "streamable-http", "url": "https://hosted.acme.example/mcp"}
							]
						}
					}
				],
				"metadata": {"nextCursor": "", "count": 1}
			})))
			.mount(&server)
			.await;

		let provider = McpRegistryProvider::with_base_url(server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 1);
		assert_eq!(
			resources[0].install_manifest,
			InstallManifest::Mcp {
				server_def: McpServerDef {
					name: "io.github.acme/hosted".to_string(),
					command: None,
					args: vec![],
					env: BTreeMap::new(),
					url: Some("https://hosted.acme.example/mcp".to_string()),
				}
			}
		);
	}

	// search: 条目缺失 name 字段(脏数据)应被跳过, 不报错也不产出该条; packages[0] 缺失
	// runtimeHint 时应按 registryType=npm 猜出 command=npx; 环境变量缺失 isRequired 字段应按
	// "宁可多问"原则默认视为必填, 收进 required_env
	#[tokio::test]
	async fn search_skips_entry_without_name_and_defaults_missing_is_required_to_true() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/v0/servers"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"servers": [
					{"server": {"description": "缺 name, 应被跳过"}},
					{
						"server": {
							"name": "io.github.acme/no-flag",
							"description": "isRequired 字段缺失",
							"version": "0.1.0",
							"packages": [
								{
									"registryType": "npm",
									"identifier": "@acme/no-flag-mcp",
									"environmentVariables": [
										{"name": "SOME_TOKEN", "description": "未标注是否必填"}
									]
								}
							]
						}
					}
				],
				"metadata": {"nextCursor": "", "count": 2}
			})))
			.mount(&server)
			.await;

		let provider = McpRegistryProvider::with_base_url(server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 1, "缺 name 的条目应被跳过");
		let InstallManifest::McpTemplate {
			server_def,
			required_env,
		} = &resources[0].install_manifest
		else {
			panic!("isRequired 缺失应兜底为必填, 产出 McpTemplate");
		};
		assert_eq!(required_env, &vec!["SOME_TOKEN".to_string()]);
		assert_eq!(
			server_def.command,
			Some("npx".to_string()),
			"缺 runtimeHint 时应按 registryType=npm 猜出 npx"
		);
	}

	// fetch_payload: Mcp/McpTemplate 两种安装清单均应直接取出内嵌的 server_def 组装
	// InstallPayload::Mcp, 不发起任何网络请求(mock server 全程未挂载任何响应, 命中即报错)
	#[tokio::test]
	async fn fetch_payload_assembles_mcp_payload_from_manifest_without_network_call() {
		let server = MockServer::start().await; // 故意不挂载任何 Mock, 验证确实不发请求
		let provider = McpRegistryProvider::with_base_url(server.uri());
		let server_def = McpServerDef {
			name: "io.github.acme/weather".to_string(),
			command: Some("npx".to_string()),
			args: vec!["-y".to_string(), "@acme/weather-mcp".to_string()],
			env: BTreeMap::new(),
			url: None,
		};
		let resource = sample_market_resource(InstallManifest::Mcp {
			server_def: server_def.clone(),
		});

		let payload = provider
			.fetch_payload(&client(), &resource, None)
			.await
			.unwrap();
		assert_eq!(payload, InstallPayload::Mcp { server_def });
	}

	// fetch_payload: install_manifest 非 Mcp/McpTemplate 变体(如误传 Skill 类资源)应报错, 不 panic
	#[tokio::test]
	async fn fetch_payload_returns_err_for_skill_install_manifest() {
		let server = MockServer::start().await;
		let provider = McpRegistryProvider::with_base_url(server.uri());
		let resource = sample_market_resource(InstallManifest::Skill {
			repo: "acme/demo".to_string(),
			path: "skills/demo".to_string(),
			git_ref: "main".to_string(),
		});

		let result = provider.fetch_payload(&client(), &resource, None).await;
		assert!(result.is_err());
	}

	// id/auth_kind: 应分别报告 McpRegistry 与 None(公开只读, 无需认证), 不需要网络
	#[test]
	fn provider_reports_mcp_registry_id_and_no_auth_kind() {
		let provider = McpRegistryProvider::default();
		assert_eq!(provider.id(), SourceId::McpRegistry);
		assert_eq!(provider.auth_kind(), None);
	}
}
