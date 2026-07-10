// 文件作用: github_mcp 市场源 —— 聚合 GitHub 上的 MCP 服务器合集仓库(如 modelcontextprotocol/
//           servers)下 servers_dir 内的各子目录, 每个子目录视为一个 MCP 服务器候选(已用真实
//           GitHub API 核实该仓库结构: servers_dir="src", 下含 filesystem/fetch/git 等子目录,
//           TS 服务器有 package.json, Python 服务器只有 pyproject.toml, 无 package.json)。优先
//           读取子目录下的 package.json 取 name/description/version, 缺失时退化为解析 README.md
//           首段正文作描述; 服务器启动命令优先从 README 里"Configure for Claude.app"一节的示例
//           配置代码块(含 mcpServers 键的 JSON, 已用 fetch 服务器 README 实测核实存在该惯例)解出,
//           解不到再退化为兜底猜测 npx -y <包名或目录名>(与 mcp_registry 对 npm 包的既有猜测惯例
//           一致)。归一化为 MarketResource(res_type=Mcp), 按是否探测到必填环境变量分派
//           Mcp/McpTemplate(与 mcp_registry 同一惯例), 实现 SourceProvider(见 infra::source::mod)。
//           目录本身即视为一个合法的 MCP 服务器候选(不像 github_skills 要求 SKILL.md 必须存在),
//           缺失 package.json/README.md 时相关字段宽松兜底为空/猜测值, 不跳过该目录
// 创建日期: 2026-07-10

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::prelude::*;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::domain::agent::McpServerDef;
use crate::domain::market::{InstallManifest, MarketResource, Query, SourceId};
use crate::domain::resource::ResourceType;
use crate::infra::http::{get_json, HttpResult};

use super::{AuthKind, InstallPayload, SourceProvider};

/// 生产环境默认的 GitHub API 根地址; 测试通过 GithubMcpProvider::with_base_url 注入 wiremock
/// 本地地址, 绝不在测试里打真实 github.com
const DEFAULT_BASE_URL: &str = "https://api.github.com";

/// 一个待聚合的 GitHub MCP 合集仓库引用: owner/repo 定位仓库, branch 指定拉取的 git 引用,
/// servers_dir 是该仓库内存放各 MCP 服务器子目录的根路径(如官方合集仓库的 "src")
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
	pub owner: String,
	pub repo: String,
	pub branch: String,
	pub servers_dir: String,
}

/// github_mcp 市场源: 聚合 repos 列表下各仓库 servers_dir 内的子目录, 每个子目录归一化为一条
/// MCP 类 MarketResource
pub struct GithubMcpProvider {
	repos: Vec<RepoRef>,
	base_url: String,
}

impl GithubMcpProvider {
	/// 生产用构造: base_url 固定为官方 GitHub API 地址
	pub fn new(repos: Vec<RepoRef>) -> Self {
		Self {
			repos,
			base_url: DEFAULT_BASE_URL.to_string(),
		}
	}

	/// 测试用构造: 注入 base_url(指向 wiremock 本地地址), 其余行为与 new 一致
	pub fn with_base_url(repos: Vec<RepoRef>, base_url: String) -> Self {
		Self { repos, base_url }
	}
}

impl Default for GithubMcpProvider {
	/// 默认聚合官方 MCP 服务器合集仓库(modelcontextprotocol/servers, main 分支, servers_dir=
	/// "src"; 已用真实 GitHub API 核实该仓库当前下含 everything/fetch/filesystem/git/memory/
	/// sequentialthinking/time 共 7 个参考实现子目录)
	fn default() -> Self {
		Self::new(vec![RepoRef {
			owner: "modelcontextprotocol".to_string(),
			repo: "servers".to_string(),
			branch: "main".to_string(),
			servers_dir: "src".to_string(),
		}])
	}
}

/// GitHub contents API 单条目的归一化视图, 与 github_skills::ContentsItem 同构; 各 provider
/// 模块各自独立维护一份(体量小, 不值得跨文件共享), 目录列表响应是本结构体的数组(content/encoding
/// 天然缺失反序列化为 None), 单文件内容响应是单个本结构体(多出 content/encoding)
#[derive(Debug, Clone, Deserialize)]
struct ContentsItem {
	name: String,
	path: String,
	#[serde(rename = "type")]
	kind: String,
	content: Option<String>,
	encoding: Option<String>,
}

/// 打包一次 GitHub contents API 调用所需的定位信息与鉴权令牌, 同 github_skills::GithubCtx 惯例,
/// 避免各方法参数过多(clippy too_many_arguments)
struct GithubCtx<'a> {
	client: &'a Client,
	base_url: &'a str,
	owner: &'a str,
	repo: &'a str,
	git_ref: &'a str,
	token: Option<&'a str>,
}

impl GithubCtx<'_> {
	/// 拼出 GitHub contents API 的完整 URL: `{base_url}/repos/{owner}/{repo}/contents/{path}?ref={git_ref}`
	fn contents_url(&self, path: &str) -> String {
		format!(
			"{}/repos/{}/{}/contents/{}?ref={}",
			self.base_url, self.owner, self.repo, path, self.git_ref
		)
	}

	/// 列出 `path` 目录下的直接子项(文件与子目录混合); 调用方需确保 path 指向目录
	async fn list_contents(&self, path: &str) -> Result<Vec<ContentsItem>> {
		let url = self.contents_url(path);
		match get_json::<Vec<ContentsItem>>(self.client, &url, self.token, None).await? {
			HttpResult::Ok { data, .. } => Ok(data),
			// 本调用未传 etag, 正常不会收到 304; 出现即视为异常, 报错而非静默兜底空列表
			HttpResult::NotModified => anyhow::bail!("意外的 304(本调用未传 etag): {url}"),
		}
	}

	/// 尝试取 `path` 指向的单个文件的解码后文本内容; 文件不存在(404)/响应非成功状态/非 base64
	/// 编码/解码失败均视为"取不到", 返回 None 而不是报错 —— 本源允许 package.json/README.md
	/// 任一或两者皆缺失(该子目录仍是一个合法的 MCP 服务器候选, 只是暂无法归一化出更丰富的元数据,
	/// 见文件头注释"字段宽松兜底"), 与 github_skills 要求 SKILL.md 必须存在的强约束不同
	async fn try_fetch_text(&self, path: &str) -> Option<String> {
		let url = self.contents_url(path);
		let item = match get_json::<ContentsItem>(self.client, &url, self.token, None).await {
			Ok(HttpResult::Ok { data, .. }) => data,
			_ => return None,
		};
		let encoding = item.encoding.as_deref()?;
		if encoding != "base64" {
			return None;
		}
		let raw = item.content.as_deref()?;
		let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
		let bytes = BASE64_STANDARD.decode(cleaned).ok()?;
		Some(String::from_utf8_lossy(&bytes).into_owned())
	}
}

/// package.json 里我们关心的三个字段, 均可缺省(部分服务器用 Python 等非 npm 生态实现, 没有
/// package.json, 或字段本身未填); 缺省时对应 Option 为 None, 由调用方决定兜底值
#[derive(Debug, Clone, Deserialize)]
struct PackageJson {
	name: Option<String>,
	description: Option<String>,
	version: Option<String>,
}

/// 从 README 解出的示例配置代码块中提取的启动提示: command/args 直接取自代码块(维护者亲自写的
/// 文档, 比"npx -y <目录名>"的兜底猜测更权威), required_env 取 env 对象的键名(值多是占位说明
/// 文字如 "<your-api-key>", 只取键名, 具体值交安装时用户填写)
#[derive(Debug)]
struct ReadmeConfigHint {
	command: Option<String>,
	args: Vec<String>,
	required_env: Vec<String>,
}

/// 逐行扫描 markdown 全文, 收集所有围栏代码块(``` 包裹, 起始围栏可带语言标注如 ```json)内部的
/// 原始文本(不含围栏本身与语言标注行); 只做最简单的开关状态机, 不处理"围栏本身即为转义内容"这类
/// 边界情况(真实 README 未见过这类写法, 不值得为此增加复杂度)
fn fenced_code_blocks(text: &str) -> Vec<String> {
	let mut blocks = Vec::new();
	let mut current: Option<Vec<&str>> = None;
	for line in text.lines() {
		if line.trim_start().starts_with("```") {
			match current.take() {
				Some(collected) => blocks.push(collected.join("\n")),
				None => current = Some(Vec::new()),
			}
			continue;
		}
		if let Some(collected) = current.as_mut() {
			collected.push(line);
		}
	}
	blocks
}

/// 从 README 全文里挑出首个能解析为 JSON 且含 mcpServers 键的代码块, 取其首个 server 条目组装
/// ReadmeConfigHint; 合集仓库各服务器 README 的既有惯例是在"Configure for Claude.app"一节给出
/// 这样一份示例配置(已用 modelcontextprotocol/servers 的 fetch 服务器 README 实测核实存在该
/// 惯例), 是比逐行猜测更权威的来源。全文找不到任何一个可用代码块则返回 None, 交调用方走兜底猜测
fn extract_config_hint_from_readme(text: &str) -> Option<ReadmeConfigHint> {
	for block in fenced_code_blocks(text) {
		let Ok(value) = serde_json::from_str::<Value>(&block) else {
			continue;
		};
		let Some(first_server) = value
			.get("mcpServers")
			.and_then(Value::as_object)
			.and_then(|servers| servers.values().next())
		else {
			continue;
		};
		let command = first_server
			.get("command")
			.and_then(Value::as_str)
			.map(str::to_string);
		let args = first_server
			.get("args")
			.and_then(Value::as_array)
			.map(|items| {
				items
					.iter()
					.filter_map(Value::as_str)
					.map(str::to_string)
					.collect()
			})
			.unwrap_or_default();
		let required_env = first_server
			.get("env")
			.and_then(Value::as_object)
			.map(|env| env.keys().cloned().collect())
			.unwrap_or_default();
		return Some(ReadmeConfigHint {
			command,
			args,
			required_env,
		});
	}
	None
}

/// 从 README.md 全文提取一句简介: 跳过空行/标题行(# 开头)/HTML 注释行(<!-- 开头)/图片或徽章行
/// (![ 或 [![ 开头)/引用块行(> 开头, 常见于 GitHub 提示框如 "> [!CAUTION]"), 取第一行"看起来像
/// 正文"的整行文字作为描述; 全文找不到则返回 None(交调用方兜底空串)。以上噪音行模式均已用真实
/// modelcontextprotocol/servers 仓库(filesystem/fetch 两份 README)核实过
fn extract_description_from_readme(text: &str) -> Option<String> {
	for line in text.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty()
			|| trimmed.starts_with('#')
			|| trimmed.starts_with("<!--")
			|| trimmed.starts_with("![")
			|| trimmed.starts_with("[![")
			|| trimmed.starts_with('>')
		{
			continue;
		}
		return Some(trimmed.to_string());
	}
	None
}

/// 由 README 解出的配置提示(若有且含 command)组装 server_def; 没有提示或提示缺 command 时,
/// 退化为兜底猜测: command 固定 "npx", args 给 "-y"(免交互确认, 与 mcp_registry 对 npm 包的
/// 既有猜测惯例一致)后追加 resolved_name(有 package.json 则是真实可安装的包名, 否则是目录名,
/// 不保证一定能装, 仅为安装前的合理起点, 用户可在装好后自行修正)。返回值第二项是必填环境变量名
/// 列表, 空则调用方应产出 InstallManifest::Mcp, 否则产出 McpTemplate(与 mcp_registry 同一惯例)
fn build_server_def(
	resolved_name: &str,
	config_hint: Option<ReadmeConfigHint>,
) -> (McpServerDef, Vec<String>) {
	if let Some(hint) = config_hint.filter(|hint| hint.command.is_some()) {
		let mut env = BTreeMap::new();
		for key in &hint.required_env {
			env.insert(key.clone(), String::new());
		}
		return (
			McpServerDef {
				name: resolved_name.to_string(),
				command: hint.command,
				args: hint.args,
				env,
				url: None,
			},
			hint.required_env,
		);
	}
	(
		McpServerDef {
			name: resolved_name.to_string(),
			command: Some("npx".to_string()),
			args: vec!["-y".to_string(), resolved_name.to_string()],
			env: BTreeMap::new(),
			url: None,
		},
		Vec::new(),
	)
}

#[async_trait]
impl SourceProvider for GithubMcpProvider {
	fn id(&self) -> SourceId {
		SourceId::GithubMcp
	}

	/// 遍历 repos 列表, 对每个仓库列出 servers_dir 下的子目录, 每个子目录本身即视为一个合法的
	/// MCP 服务器候选(不要求任何特定文件存在); 尝试读取其 package.json(取 name/description/
	/// version)与 README.md(取描述兜底 + 示例配置提示), 归一化为 MarketResource。关键字/分类
	/// 过滤留给聚合层(services::market, Task 6), 本方法恒返回全量, query 参数暂未使用(签名与
	/// 其它源统一)
	async fn search(
		&self,
		client: &Client,
		_query: &Query,
		token: Option<&str>,
	) -> Result<Vec<MarketResource>> {
		let mut resources = Vec::new();
		for repo_ref in &self.repos {
			let ctx = GithubCtx {
				client,
				base_url: &self.base_url,
				owner: &repo_ref.owner,
				repo: &repo_ref.repo,
				git_ref: &repo_ref.branch,
				token,
			};
			let entries = ctx
				.list_contents(&repo_ref.servers_dir)
				.await
				.with_context(|| {
					format!(
						"列出 {}/{} 的 servers_dir 失败: {}",
						repo_ref.owner, repo_ref.repo, repo_ref.servers_dir
					)
				})?;

			for entry in entries.iter().filter(|item| item.kind == "dir") {
				let package_json: Option<PackageJson> = ctx
					.try_fetch_text(&format!("{}/package.json", entry.path))
					.await
					.and_then(|text| serde_json::from_str(&text).ok());
				let readme = ctx
					.try_fetch_text(&format!("{}/README.md", entry.path))
					.await;

				let pkg_name = package_json
					.as_ref()
					.and_then(|p| p.name.clone())
					.filter(|s| !s.is_empty());
				let pkg_description = package_json
					.as_ref()
					.and_then(|p| p.description.clone())
					.filter(|s| !s.is_empty());
				let version = package_json
					.as_ref()
					.and_then(|p| p.version.clone())
					.unwrap_or_default();
				let resolved_name = pkg_name.unwrap_or_else(|| entry.name.clone());

				let description = pkg_description
					.or_else(|| readme.as_deref().and_then(extract_description_from_readme))
					.unwrap_or_default();
				let config_hint = readme.as_deref().and_then(extract_config_hint_from_readme);

				let (server_def, required_env) = build_server_def(&resolved_name, config_hint);
				let install_manifest = if required_env.is_empty() {
					InstallManifest::Mcp { server_def }
				} else {
					InstallManifest::McpTemplate {
						server_def,
						required_env,
					}
				};

				resources.push(MarketResource {
					source_type: SourceId::GithubMcp,
					res_type: ResourceType::Mcp,
					ext_id: format!("{}/{}/{}", repo_ref.owner, repo_ref.repo, entry.name),
					name: resolved_name.clone(),
					display_name: resolved_name,
					description,
					author: repo_ref.owner.clone(),
					version,
					stars: 0,
					category: String::new(),
					tags: Vec::new(),
					auth_required: false,
					install_manifest,
					updated_at: String::new(),
				});
			}
		}
		Ok(resources)
	}

	/// github_mcp 的安装清单在 search 阶段已组装完整(Mcp/McpTemplate 均直接内嵌 server_def),
	/// 无需再发起网络请求, 直接从 resource.install_manifest 取出 server_def 组装
	/// InstallPayload::Mcp(与 mcp_registry 同一惯例); resource 必须是本源产出的 Mcp/McpTemplate
	/// 类资源, 否则报错
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
				"github_mcp 只能安装 Mcp/McpTemplate 类型的安装清单, 实际: {:?}",
				resource.install_manifest
			),
		};
		Ok(InstallPayload::Mcp { server_def })
	}

	/// 匿名可读公开仓库但受限流, 登录 GitHub 可提额; 同 github_skills(均命中真实 GitHub
	/// contents API)
	fn auth_kind(&self) -> Option<AuthKind> {
		Some(AuthKind::GitHub)
	}
}

#[cfg(test)]
mod tests {
	use serde_json::json;
	use wiremock::matchers::{bearer_token, method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use super::*;
	use crate::domain::market::SortBy;
	use crate::infra::http::client;

	fn sample_repo_ref() -> RepoRef {
		RepoRef {
			owner: "acme".to_string(),
			repo: "mcp-collection".to_string(),
			branch: "main".to_string(),
			servers_dir: "src".to_string(),
		}
	}

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

	/// 在 `server` 上为某个具体文件路径挂一个 GitHub 单文件内容响应(content 按 base64 编码),
	/// 同 github_skills 测试里的 mount_file 惯例
	async fn mount_file(server: &MockServer, owner: &str, repo: &str, rel_path: &str, body: &str) {
		Mock::given(method("GET"))
			.and(path(format!("/repos/{owner}/{repo}/contents/{rel_path}")))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"name": rel_path.rsplit('/').next().unwrap_or(rel_path),
				"path": rel_path,
				"type": "file",
				"content": BASE64_STANDARD.encode(body),
				"encoding": "base64",
			})))
			.mount(server)
			.await;
	}

	fn sample_market_resource(install_manifest: InstallManifest) -> MarketResource {
		MarketResource {
			source_type: SourceId::GithubMcp,
			res_type: ResourceType::Mcp,
			ext_id: "acme/mcp-collection/foo".to_string(),
			name: "foo".to_string(),
			display_name: "foo".to_string(),
			description: String::new(),
			author: "acme".to_string(),
			version: String::new(),
			stars: 0,
			category: String::new(),
			tags: Vec::new(),
			auth_required: false,
			install_manifest,
			updated_at: String::new(),
		}
	}

	// search: 应从 servers_dir 下的 2 个目录归一化出 2 条 MarketResource, 非目录条目(README.md)
	// 应被过滤; 含 package.json 且 README 无配置提示的目录应兜底猜测 npx -y <包名>产出 Mcp; 无
	// package.json 但 README 含 mcpServers 示例配置且需环境变量的目录应产出 McpTemplate,
	// command/args 取自 README 示例而非兜底猜测
	#[tokio::test]
	async fn search_normalizes_two_server_directories_from_src_listing() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/mcp-collection/contents/src"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "foo", "path": "src/foo", "type": "dir"},
				{"name": "bar", "path": "src/bar", "type": "dir"},
				{"name": "README.md", "path": "src/README.md", "type": "file"},
			])))
			.mount(&server)
			.await;
		mount_file(
			&server,
			"acme",
			"mcp-collection",
			"src/foo/package.json",
			&json!({
				"name": "@acme/server-foo",
				"description": "Foo 的描述",
				"version": "1.2.0",
			})
			.to_string(),
		)
		.await;
		// foo 故意不 mount README.md(默认 404), package.json 已够用

		let bar_readme = format!(
			"# Bar Server\n\nBar 服务器的简介\n\n```json\n{}\n```\n",
			json!({
				"mcpServers": {
					"bar": {
						"command": "uvx",
						"args": ["mcp-server-bar"],
						"env": {"BAR_API_KEY": ""}
					}
				}
			})
		);
		mount_file(
			&server,
			"acme",
			"mcp-collection",
			"src/bar/README.md",
			&bar_readme,
		)
		.await;
		// bar 故意不 mount package.json(默认 404), 纯靠 README 归一化

		let provider = GithubMcpProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 2, "README.md(非目录)应被过滤");

		let foo = resources
			.iter()
			.find(|r| r.ext_id == "acme/mcp-collection/foo")
			.expect("应含 foo");
		assert_eq!(foo.name, "@acme/server-foo");
		assert_eq!(foo.display_name, "@acme/server-foo");
		assert_eq!(foo.description, "Foo 的描述");
		assert_eq!(foo.version, "1.2.0");
		assert_eq!(foo.author, "acme");
		assert_eq!(foo.res_type, ResourceType::Mcp);
		assert!(!foo.auth_required);
		assert_eq!(
			foo.install_manifest,
			InstallManifest::Mcp {
				server_def: McpServerDef {
					name: "@acme/server-foo".to_string(),
					command: Some("npx".to_string()),
					args: vec!["-y".to_string(), "@acme/server-foo".to_string()],
					env: BTreeMap::new(),
					url: None,
				}
			},
			"无 README 配置提示时应兜底猜测 npx -y <包名>"
		);

		let bar = resources
			.iter()
			.find(|r| r.ext_id == "acme/mcp-collection/bar")
			.expect("应含 bar");
		assert_eq!(bar.name, "bar", "无 package.json 应回退目录名");
		assert_eq!(bar.description, "Bar 服务器的简介");
		let InstallManifest::McpTemplate {
			server_def,
			required_env,
		} = &bar.install_manifest
		else {
			panic!("需要环境变量的 server 应产出 McpTemplate");
		};
		assert_eq!(server_def.command, Some("uvx".to_string()));
		assert_eq!(server_def.args, vec!["mcp-server-bar".to_string()]);
		assert_eq!(required_env, &vec!["BAR_API_KEY".to_string()]);
		assert_eq!(server_def.env.get("BAR_API_KEY"), Some(&String::new()));
	}

	// search: 目录既无 package.json 也无 README.md(默认 404)时, 仍应产出一条 MarketResource,
	// 各字段宽松兜底为空/猜测值, 不报错也不跳过该目录
	#[tokio::test]
	async fn search_defaults_missing_metadata_for_directory_without_package_or_readme() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/mcp-collection/contents/src"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "baz", "path": "src/baz", "type": "dir"},
			])))
			.mount(&server)
			.await;
		// 故意不 mount package.json/README.md, 全走默认 404

		let provider = GithubMcpProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 1);
		let baz = &resources[0];
		assert_eq!(baz.ext_id, "acme/mcp-collection/baz");
		assert_eq!(baz.name, "baz");
		assert_eq!(baz.description, "");
		assert_eq!(baz.version, "");
		assert_eq!(
			baz.install_manifest,
			InstallManifest::Mcp {
				server_def: McpServerDef {
					name: "baz".to_string(),
					command: Some("npx".to_string()),
					args: vec!["-y".to_string(), "baz".to_string()],
					env: BTreeMap::new(),
					url: None,
				}
			}
		);
	}

	// search: 应把 token 作为 Bearer 携带到 GitHub API 请求; mock 严格校验令牌值, 值不对不会
	// 匹配从而走 wiremock 默认 404, search 会因此报错, 与本测试期望的 Ok 相悖从而暴露问题
	#[tokio::test]
	async fn search_sends_bearer_token_when_provided() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/mcp-collection/contents/src"))
			.and(bearer_token("secret-token"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
			.mount(&server)
			.await;

		let provider = GithubMcpProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let result = provider
			.search(&client(), &sample_query(), Some("secret-token"))
			.await;

		assert!(result.is_ok());
		assert!(result.unwrap().is_empty());
	}

	// fetch_payload: Mcp/McpTemplate 两种安装清单均应直接取出内嵌的 server_def 组装
	// InstallPayload::Mcp, 不发起任何网络请求(mock server 全程未挂载任何响应, 命中即报错)
	#[tokio::test]
	async fn fetch_payload_assembles_mcp_payload_from_manifest_without_network_call() {
		let server = MockServer::start().await; // 故意不挂载任何 Mock, 验证确实不发请求
		let provider = GithubMcpProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let server_def = McpServerDef {
			name: "foo".to_string(),
			command: Some("npx".to_string()),
			args: vec!["-y".to_string(), "foo".to_string()],
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
		let provider = GithubMcpProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resource = sample_market_resource(InstallManifest::Skill {
			repo: "acme/demo".to_string(),
			path: "skills/demo".to_string(),
			git_ref: "main".to_string(),
		});

		let result = provider.fetch_payload(&client(), &resource, None).await;
		assert!(result.is_err());
	}

	// id/auth_kind: 应分别报告 GithubMcp 与 Some(GitHub)(匿名可读但限流, 同 github_skills),
	// 不需要网络
	#[test]
	fn provider_reports_github_mcp_id_and_github_auth_kind() {
		let provider = GithubMcpProvider::default();
		assert_eq!(provider.id(), SourceId::GithubMcp);
		assert_eq!(provider.auth_kind(), Some(AuthKind::GitHub));
	}

	// extract_description_from_readme: 应跳过标题/HTML 注释/徽章/引用块等噪音行, 取第一行正文
	#[test]
	fn extract_description_from_readme_skips_noise_lines() {
		let text = "# Title\n\n<!-- mcp-name: foo -->\n\n![badge](https://example.com/badge.svg)\n\n> [!CAUTION]\n> 说明\n\n真正的描述在这里\n";
		assert_eq!(
			extract_description_from_readme(text),
			Some("真正的描述在这里".to_string())
		);
	}

	// extract_description_from_readme: 全文只有噪音行(无正文)应返回 None
	#[test]
	fn extract_description_from_readme_returns_none_when_only_noise() {
		let text = "# Title\n\n<!-- comment -->\n";
		assert_eq!(extract_description_from_readme(text), None);
	}

	// extract_config_hint_from_readme: 应从首个含 mcpServers 键的 JSON 代码块解出 command/args/env
	#[test]
	fn extract_config_hint_from_readme_parses_first_mcp_servers_json_block() {
		let text = format!(
			"# Demo\n\n```json\n{}\n```\n",
			json!({
				"mcpServers": {
					"demo": {
						"command": "uvx",
						"args": ["mcp-server-demo"],
						"env": {"DEMO_TOKEN": ""}
					}
				}
			})
		);
		let hint = extract_config_hint_from_readme(&text).expect("应解出配置提示");
		assert_eq!(hint.command, Some("uvx".to_string()));
		assert_eq!(hint.args, vec!["mcp-server-demo".to_string()]);
		assert_eq!(hint.required_env, vec!["DEMO_TOKEN".to_string()]);
	}

	// extract_config_hint_from_readme: 全文没有可解析出 mcpServers 的代码块应返回 None
	#[test]
	fn extract_config_hint_from_readme_returns_none_when_absent() {
		let text = "# Demo\n\n```bash\nnpm install demo\n```\n";
		assert!(extract_config_hint_from_readme(text).is_none());
	}
}
