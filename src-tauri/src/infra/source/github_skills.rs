// 文件作用: github_skills 市场源 —— 聚合若干 GitHub 仓库下 skills_dir 内含 SKILL.md 的目录,
//           解析其 YAML frontmatter(name/description/version)归一化为 MarketResourceRespVO, 并在
//           下载安装时递归拉取该 Skill 子目录下的全部文件, 实现 SourceProvider(见 infra::
//           source::mod)。GitHub contents API 细节按官方 REST v3: 目录路径返回条目数组,
//           文件路径返回单个对象(含 base64 content), 具体调用见 GithubCtx
// 创建日期: 2026-07-09

use std::collections::VecDeque;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::prelude::*;
use reqwest::Client;
use serde::Deserialize;

use crate::domain::market::{InstallManifest, MarketResourceRespVO, Query, SourceId};
use crate::domain::resource::ResourceType;
use crate::infra::http::{get_json, HttpResult};

use super::{AuthKind, FileEntry, InstallPayload, SourceProvider};

/// 生产环境默认的 GitHub API 根地址; 测试通过 GithubSkillsProvider::with_base_url 注入
/// wiremock 本地地址, 绝不在测试里打真实 github.com
const DEFAULT_BASE_URL: &str = "https://api.github.com";

/// 一个待聚合的 GitHub 仓库引用: owner/repo 定位仓库, branch 指定拉取的 git 引用(分支/tag),
/// skills_dir 是该仓库内存放各 Skill 子目录的根路径(如官方仓库的 "skills")
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
	pub owner: String,
	pub repo: String,
	pub branch: String,
	pub skills_dir: String,
}

/// github_skills 市场源: 聚合 repos 列表下各仓库 skills_dir 内含 SKILL.md 的子目录
pub struct GithubSkillsProvider {
	repos: Vec<RepoRef>,
	base_url: String,
}

impl GithubSkillsProvider {
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

impl Default for GithubSkillsProvider {
	/// 默认聚合 Anthropic 官方 skills 仓库(anthropics/skills, main 分支, skills_dir="skills";
	/// 经真实 GitHub API 核实的目录结构, 如 skills/docx、skills/pdf 等, 每个子目录下都有一份
	/// SKILL.md)。用户可配置追加/替换仓库留待后续任务接入配置持久化, 本任务先固定该默认值
	fn default() -> Self {
		Self::new(vec![RepoRef {
			owner: "anthropics".to_string(),
			repo: "skills".to_string(),
			branch: "main".to_string(),
			skills_dir: "skills".to_string(),
		}])
	}
}

/// GitHub contents API 单条目的归一化视图: 目录列表响应是本结构体的数组(此时 content/encoding
/// 缺失, 天然反序列化为 None); 单文件内容响应是单个本结构体(多出 content/encoding)。两种
/// 响应共用同一形状, 调用方按上下文(当前 path 是目录还是文件)决定按数组还是单对象反序列化
#[derive(Debug, Clone, Deserialize)]
struct ContentsItem {
	name: String,
	path: String,
	#[serde(rename = "type")]
	kind: String,
	content: Option<String>,
	encoding: Option<String>,
}

/// 打包一次 GitHub contents API 调用所需的定位信息(base_url/owner/repo/git_ref)与鉴权令牌,
/// 避免 list_contents/fetch_file_content 各自罗列一长串同样的参数(clippy too_many_arguments),
/// 也让 search/fetch_payload 的调用点更简洁
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
	/// (本文件内两处调用点均已在上一层遍历中确认过, 不会误把文件路径传进来)
	async fn list_contents(&self, path: &str) -> Result<Vec<ContentsItem>> {
		let url = self.contents_url(path);
		match get_json::<Vec<ContentsItem>>(self.client, &url, self.token, None).await? {
			HttpResult::Ok { data, .. } => Ok(data),
			// 本调用未传 etag(见上面的 None 参数), 正常不会收到 304; 出现即视为异常, 报错而非
			// 静默兜底空列表, 避免掩盖潜在的服务端/mock 配置问题
			HttpResult::NotModified => anyhow::bail!("意外的 304(本调用未传 etag): {url}"),
		}
	}

	/// 取 `path` 指向的单个文件的内容条目(含 content/encoding), 供 decode_content 解出原始字节;
	/// 调用方需确保 path 指向文件
	async fn fetch_file_content(&self, path: &str) -> Result<ContentsItem> {
		let url = self.contents_url(path);
		match get_json::<ContentsItem>(self.client, &url, self.token, None).await? {
			HttpResult::Ok { data, .. } => Ok(data),
			HttpResult::NotModified => anyhow::bail!("意外的 304(本调用未传 etag): {url}"),
		}
	}

	/// 拉取本仓库的星标数与最后一次 push 时间(GitHub repos API `GET /repos/{owner}/{repo}`);
	/// 每个仓库在一次 search 里只调用一次, 供 search 施于该仓库下产出的全部 MarketResourceRespVO,
	/// 而不是逐资源调用, 呼应"匿名限流 60 次/时, 严禁逐资源调用把额度打爆"的约束。请求失败
	/// (网络错误/限流/非 2xx)时把 Err 交回调用方, 由调用方 unwrap_or_default() 兜底为
	/// stars=0/pushed_at=空串(不劣于富化前"恒为 0/空串"的既有行为), 不让这一次额外调用连累
	/// 整次 search 失败
	async fn fetch_repo_meta(&self) -> Result<RepoMeta> {
		let url = format!("{}/repos/{}/{}", self.base_url, self.owner, self.repo);
		match get_json::<RepoMeta>(self.client, &url, self.token, None).await? {
			HttpResult::Ok { data, .. } => Ok(data),
			HttpResult::NotModified => anyhow::bail!("意外的 304(本调用未传 etag): {url}"),
		}
	}
}

/// GitHub repos API 单次仓库详情响应的归一化视图, 只取本任务需要的两个字段。派生 Default
/// 供 fetch_repo_meta 失败时由调用方 unwrap_or_default() 优雅降级; 派生 Deserialize 时对两个
/// 字段都标 #[serde(default)], 即便响应体意外缺失该字段也兜底为 0/空串而不是解析报错
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
struct RepoMeta {
	/// 仓库星标数, 落库到 MarketResourceRespVO.stars, 施于该仓库下的全部资源
	#[serde(default, rename = "stargazers_count")]
	stars: i64,
	/// 仓库最后一次 push 时间(ISO 8601), 落库到 MarketResourceRespVO.updated_at, 施于该仓库下的
	/// 全部资源; 用仓库级 push 时间而非逐资源的提交历史, 避免为每条资源单独查一次昂贵的
	/// commits API
	#[serde(default, rename = "pushed_at")]
	pushed_at: String,
}

/// 解出 ContentsItem.content 携带的原始字节: GitHub 返回 base64 编码, 且每行按 60 字符换行,
/// 解码前需先剔除所有空白字符; encoding 非 "base64" 或 content 缺失都视为错误, 不静默返回
/// 空字节(那样会误装出一个内容为空的文件, 比直接报错更危险)
fn decode_content(item: &ContentsItem) -> Result<Vec<u8>> {
	let encoding = item
		.encoding
		.as_deref()
		.context("文件内容响应缺失 encoding 字段")?;
	if encoding != "base64" {
		anyhow::bail!("不支持的内容编码: {encoding}(仅支持 base64)");
	}
	let raw = item
		.content
		.as_deref()
		.context("文件内容响应缺失 content 字段")?;
	let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
	BASE64_STANDARD
		.decode(cleaned)
		.context("base64 解码文件内容失败")
}

/// 从 SKILL.md 全文解析出的四个 frontmatter 字段, 均可缺省(缺省给空串)。M6 市场元数据富化任务
/// 新增 category: 经核实官方 anthropics/skills 仓库当前 SKILL.md frontmatter 只有 name/
/// description/license 三个字段(无 category, 也无 version, 见 search 文档"关于 version"一节),
/// 故该默认仓库产出的资源 category 恒为空串, 如实留空不臆造分类; 一旦某仓库的 SKILL.md 补上
/// 该字段即可自动被下面的 parse_frontmatter 识别生效
#[derive(Debug, Clone, Default, PartialEq)]
struct SkillFrontmatter {
	name: String,
	description: String,
	version: String,
	category: String,
}

/// 从 SKILL.md 全文提取 YAML frontmatter 里的 name/description/version/category 四个字段; 只做
/// 逐行字符串扫描, 不引入 YAML 解析依赖(与 adapter::skill_target::parse_frontmatter_version 同一
/// 惯例, 该函数只需读一个字段, 这里扩到四个, 各自模块独立维护一份, 体量小不值得跨层共享)。
/// frontmatter 必须以独占一行的 `---` 开头, 扫描到下一个独占一行的 `---` 为止; 边界不存在时
/// 四个字段均返回空串。取值支持前后空白与成对单/双引号包裹(如 `description: "..."`), 引号会
/// 被裁掉
fn parse_frontmatter(text: &str) -> SkillFrontmatter {
	let mut result = SkillFrontmatter::default();
	let mut lines = text.lines();
	let Some(first) = lines.next() else {
		return result;
	};
	if first.trim() != "---" {
		return result;
	}
	for line in lines {
		if line.trim() == "---" {
			break;
		}
		let trimmed = line.trim();
		if let Some(value) = trimmed.strip_prefix("name:") {
			result.name = strip_matching_quotes(value.trim()).to_string();
		} else if let Some(value) = trimmed.strip_prefix("description:") {
			result.description = strip_matching_quotes(value.trim()).to_string();
		} else if let Some(value) = trimmed.strip_prefix("version:") {
			result.version = strip_matching_quotes(value.trim()).to_string();
		} else if let Some(value) = trimmed.strip_prefix("category:") {
			result.category = strip_matching_quotes(value.trim()).to_string();
		}
	}
	result
}

/// 裁掉字符串两端成对包裹的引号(单引号或双引号各自成对才裁, 不成对原样返回)
fn strip_matching_quotes(value: &str) -> &str {
	for quote in ['"', '\''] {
		if let Some(inner) = value
			.strip_prefix(quote)
			.and_then(|v| v.strip_suffix(quote))
		{
			return inner;
		}
	}
	value
}

#[async_trait]
impl SourceProvider for GithubSkillsProvider {
	fn id(&self) -> SourceId {
		SourceId::GithubSkills
	}

	/// 遍历 repos 列表, 对每个仓库列出 skills_dir 下的子目录, 逐一尝试读取其 SKILL.md 并解析
	/// frontmatter, 归一化为 MarketResourceRespVO; 不含 SKILL.md 的子目录(fetch_file_content 出错,
	/// 如 404)视为"不是一个 Skill", 跳过而非让整次 search 失败。关键字/分类过滤留给聚合层
	/// (services::market, Task 6), 本方法恒返回全量, query 参数暂未使用(签名与其它源统一)。
	///
	/// M6 市场元数据富化: 每个仓库在列出 skills_dir 之外额外发起一次 fetch_repo_meta(仓库级,
	/// 不随子目录数量线性增长), 把返回的星标数/最后 push 时间施于该仓库下产出的全部资源
	/// (stars/updated_at); 该调用失败(限流/网络错误)时优雅降级为 0/空串(见 fetch_repo_meta
	/// 文档), 不影响本仓库其余资源的正常产出。关于 version: 取自 frontmatter, 若来源仓库的
	/// SKILL.md 未提供该字段(如官方 anthropics/skills 当前的实际情况, 见 SkillFrontmatter
	/// 文档), 则如实留空, 不虚构版本号
	async fn search(
		&self,
		client: &Client,
		_query: &Query,
		token: Option<&str>,
	) -> Result<Vec<MarketResourceRespVO>> {
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
			let repo_meta = ctx.fetch_repo_meta().await.unwrap_or_default();
			let entries = ctx
				.list_contents(&repo_ref.skills_dir)
				.await
				.with_context(|| {
					format!(
						"列出 {}/{} 的 skills_dir 失败: {}",
						repo_ref.owner, repo_ref.repo, repo_ref.skills_dir
					)
				})?;

			for entry in entries.iter().filter(|item| item.kind == "dir") {
				let skill_md_path = format!("{}/SKILL.md", entry.path);
				let Ok(skill_md) = ctx.fetch_file_content(&skill_md_path).await else {
					// 该子目录没有 SKILL.md(或读取失败), 不是一个合法 Skill 目录, 跳过
					continue;
				};
				let Ok(bytes) = decode_content(&skill_md) else {
					continue;
				};
				let text = String::from_utf8_lossy(&bytes).into_owned();
				let frontmatter = parse_frontmatter(&text);
				let name = if frontmatter.name.is_empty() {
					entry.name.clone()
				} else {
					frontmatter.name.clone()
				};

				resources.push(MarketResourceRespVO {
					source_type: SourceId::GithubSkills,
					res_type: ResourceType::Skill,
					ext_id: format!("{}/{}/{}", repo_ref.owner, repo_ref.repo, entry.path),
					name: name.clone(),
					display_name: name,
					description: frontmatter.description,
					author: repo_ref.owner.clone(),
					version: frontmatter.version,
					stars: repo_meta.stars,
					category: frontmatter.category,
					tags: Vec::new(),
					auth_required: false,
					install_manifest: InstallManifest::Skill {
						repo: format!("{}/{}", repo_ref.owner, repo_ref.repo),
						path: entry.path.clone(),
						git_ref: repo_ref.branch.clone(),
					},
					updated_at: repo_meta.pushed_at.clone(),
				});
			}
		}
		Ok(resources)
	}

	/// 拉取某 Skill 子目录下的全部文件(递归展开子目录), 组装 InstallPayload::Skill; resource
	/// 必须是本源产出的 Skill 类资源(install_manifest 为 Skill 变体), 否则返回错误
	async fn fetch_payload(
		&self,
		client: &Client,
		resource: &MarketResourceRespVO,
		token: Option<&str>,
	) -> Result<InstallPayload> {
		let InstallManifest::Skill {
			repo,
			path,
			git_ref,
		} = &resource.install_manifest
		else {
			anyhow::bail!(
				"github_skills 只能安装 Skill 类型的安装清单, 实际: {:?}",
				resource.install_manifest
			);
		};
		let (owner, repo_name) = repo
			.split_once('/')
			.with_context(|| format!("install_manifest.repo 格式应为 owner/repo, 实际: {repo}"))?;
		let ctx = GithubCtx {
			client,
			base_url: &self.base_url,
			owner,
			repo: repo_name,
			git_ref: git_ref.as_str(),
			token,
		};

		let mut files = Vec::new();
		let mut queue: VecDeque<String> = VecDeque::new();
		queue.push_back(path.clone());
		while let Some(dir_path) = queue.pop_front() {
			let entries = ctx
				.list_contents(&dir_path)
				.await
				.with_context(|| format!("列出目录失败: {dir_path}"))?;
			for entry in entries {
				if entry.kind == "dir" {
					queue.push_back(entry.path);
					continue;
				}
				if entry.kind != "file" {
					// symlink/submodule 等特殊类型, 本任务不处理
					continue;
				}
				let content_item = ctx
					.fetch_file_content(&entry.path)
					.await
					.with_context(|| format!("读取文件失败: {}", entry.path))?;
				let bytes = decode_content(&content_item)?;
				let rel_path = entry
					.path
					.strip_prefix(path.as_str())
					.unwrap_or(entry.path.as_str())
					.trim_start_matches('/')
					.to_string();
				files.push(FileEntry {
					rel_path,
					content: bytes,
				});
			}
		}
		Ok(InstallPayload::Skill { files })
	}

	/// 匿名可读公开仓库但受限流, 登录 GitHub 可提额/访问私有仓库; 具体某条资源是否强制要求
	/// 认证见 MarketResourceRespVO.auth_required(本源产出的资源恒为 false, 见 search 文档)
	fn auth_kind(&self) -> Option<AuthKind> {
		Some(AuthKind::GitHub)
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use base64::prelude::*;
	use serde_json::json;
	use wiremock::matchers::{bearer_token, method, path, query_param};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use super::*;
	use crate::domain::agent::McpServerDef;
	use crate::domain::market::SortBy;
	use crate::infra::http::client;

	fn sample_repo_ref() -> RepoRef {
		RepoRef {
			owner: "acme".to_string(),
			repo: "demo-repo".to_string(),
			branch: "main".to_string(),
			skills_dir: "skills".to_string(),
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

	fn sample_market_resource(path: &str) -> MarketResourceRespVO {
		MarketResourceRespVO {
			source_type: SourceId::GithubSkills,
			res_type: ResourceType::Skill,
			ext_id: format!("acme/demo-repo/{path}"),
			name: "demo-skill".to_string(),
			display_name: "demo-skill".to_string(),
			description: String::new(),
			author: "acme".to_string(),
			version: String::new(),
			stars: 0,
			category: String::new(),
			tags: Vec::new(),
			auth_required: false,
			install_manifest: InstallManifest::Skill {
				repo: "acme/demo-repo".to_string(),
				path: path.to_string(),
				git_ref: "main".to_string(),
			},
			updated_at: String::new(),
		}
	}

	/// 在 `server` 上为某个具体文件路径挂一个 GitHub 单文件内容响应(content 按 base64 编码,
	/// 每次 mount 均要求 ref 精确匹配 branch, 供多处测试复用而不必各自重复拼 JSON 结构)
	async fn mount_file(
		server: &MockServer,
		owner: &str,
		repo: &str,
		rel_path: &str,
		branch: &str,
		body: &str,
	) {
		Mock::given(method("GET"))
			.and(path(format!("/repos/{owner}/{repo}/contents/{rel_path}")))
			.and(query_param("ref", branch))
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

	// search: 应从 skills_dir 下的 2 个目录归一化出 2 条 MarketResourceRespVO, 非目录条目
	// (README.md)应被过滤; name/description/version/ext_id/install_manifest 均应正确;
	// frontmatter 缺 name/version 时应回退目录名/空串
	#[tokio::test]
	async fn search_normalizes_two_skill_directories_from_contents_api() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills"))
			.and(query_param("ref", "main"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "foo", "path": "skills/foo", "type": "dir"},
				{"name": "bar", "path": "skills/bar", "type": "dir"},
				{"name": "README.md", "path": "skills/README.md", "type": "file"},
			])))
			.mount(&server)
			.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/foo/SKILL.md",
			"main",
			"---\nname: foo-skill\ndescription: Foo 的描述\nversion: 1.0.0\ncategory: file-processing\n---\n# Foo\n",
		)
		.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/bar/SKILL.md",
			"main",
			"---\ndescription: Bar 没写 name 和 version\n---\n# Bar\n",
		)
		.await;

		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(
			resources.len(),
			2,
			"README.md(非目录)应被过滤, 只剩 foo/bar 两个 Skill 目录"
		);

		let foo = resources
			.iter()
			.find(|r| r.ext_id == "acme/demo-repo/skills/foo")
			.expect("应含 foo");
		assert_eq!(foo.name, "foo-skill");
		assert_eq!(foo.display_name, "foo-skill");
		assert_eq!(foo.description, "Foo 的描述");
		assert_eq!(foo.version, "1.0.0");
		assert_eq!(foo.category, "file-processing");
		assert_eq!(foo.author, "acme");
		assert!(!foo.auth_required);
		assert_eq!(
			foo.install_manifest,
			InstallManifest::Skill {
				repo: "acme/demo-repo".to_string(),
				path: "skills/foo".to_string(),
				git_ref: "main".to_string(),
			}
		);

		let bar = resources
			.iter()
			.find(|r| r.ext_id == "acme/demo-repo/skills/bar")
			.expect("应含 bar");
		assert_eq!(bar.name, "bar", "frontmatter 无 name 字段应回退为目录名");
		assert_eq!(bar.version, "", "frontmatter 无 version 字段应给空串");
		assert_eq!(bar.category, "", "frontmatter 无 category 字段应给空串");
	}

	// search: 应对每个仓库只调用一次 repos API(GET /repos/{owner}/{repo})取星标数与最后 push
	// 时间, 并把同一份仓库级元数据施于该仓库下产出的全部资源(foo/bar 两条), 而不是逐资源分别
	// 请求; 呼应"匿名限流 60 次/时, 严禁逐资源调用把额度打爆"的约束
	#[tokio::test]
	async fn search_enriches_stars_and_updated_at_from_repo_level_metadata() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"stargazers_count": 321,
				"pushed_at": "2026-05-01T00:00:00Z",
			})))
			.mount(&server)
			.await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "foo", "path": "skills/foo", "type": "dir"},
				{"name": "bar", "path": "skills/bar", "type": "dir"},
			])))
			.mount(&server)
			.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/foo/SKILL.md",
			"main",
			"---\nname: foo-skill\n---\n",
		)
		.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/bar/SKILL.md",
			"main",
			"---\nname: bar-skill\n---\n",
		)
		.await;

		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 2);
		for resource in &resources {
			assert_eq!(resource.stars, 321, "仓库级星标数应施于该仓库下的全部资源");
			assert_eq!(
				resource.updated_at, "2026-05-01T00:00:00Z",
				"仓库级 push 时间应施于该仓库下的全部资源"
			);
		}
	}

	// search: 仓库级元数据请求失败(本测试故意不 mount /repos/acme/demo-repo, 走 wiremock 默认
	// 404)时应优雅降级为 stars=0/updated_at=空串, 不 panic 也不让整次 search 失败, 不劣于富化前
	// "恒为 0/空串"的既有行为
	#[tokio::test]
	async fn search_defaults_stars_and_updated_at_when_repo_meta_fetch_fails() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "foo", "path": "skills/foo", "type": "dir"},
			])))
			.mount(&server)
			.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/foo/SKILL.md",
			"main",
			"---\nname: foo-skill\n---\n",
		)
		.await;
		// 故意不 mount /repos/acme/demo-repo, 模拟仓库级元数据请求失败(如限流)

		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert_eq!(resources.len(), 1, "仓库级元数据失败不应连累资源列表本身");
		assert_eq!(resources[0].stars, 0);
		assert_eq!(resources[0].updated_at, "");
	}

	// search: 子目录下没有 SKILL.md(mock 未挂载, 走 wiremock 默认 404)应被跳过, 不报错也不
	// 产出该条资源
	#[tokio::test]
	async fn search_skips_directories_without_skill_md() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "baz", "path": "skills/baz", "type": "dir"},
			])))
			.mount(&server)
			.await;
		// 故意不 mount skills/baz/SKILL.md 的响应

		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resources = provider
			.search(&client(), &sample_query(), None)
			.await
			.unwrap();

		assert!(resources.is_empty(), "无 SKILL.md 的目录应被跳过, 不报错");
	}

	// search: 应把 token 作为 Bearer 携带到 GitHub API 请求; mock 严格校验令牌值, 值不对不会
	// 匹配从而走 wiremock 默认 404, search 会因此报错, 与本测试期望的 Ok 相悖从而暴露问题
	#[tokio::test]
	async fn search_sends_bearer_token_when_provided() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills"))
			.and(bearer_token("secret-token"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
			.mount(&server)
			.await;

		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let result = provider
			.search(&client(), &sample_query(), Some("secret-token"))
			.await;

		assert!(result.is_ok());
		assert!(result.unwrap().is_empty());
	}

	// fetch_payload: 应递归展开子目录, 组装出该 Skill 目录下的全部文件(含顶层 SKILL.md 与嵌套
	// 子目录 scripts/run.sh), rel_path 应相对 Skill 根目录, 不含 skills_dir 前缀
	#[tokio::test]
	async fn fetch_payload_assembles_files_including_nested_subdirectory() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills/foo"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "SKILL.md", "path": "skills/foo/SKILL.md", "type": "file"},
				{"name": "scripts", "path": "skills/foo/scripts", "type": "dir"},
			])))
			.mount(&server)
			.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/foo/SKILL.md",
			"main",
			"skill file body",
		)
		.await;
		Mock::given(method("GET"))
			.and(path("/repos/acme/demo-repo/contents/skills/foo/scripts"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!([
				{"name": "run.sh", "path": "skills/foo/scripts/run.sh", "type": "file"},
			])))
			.mount(&server)
			.await;
		mount_file(
			&server,
			"acme",
			"demo-repo",
			"skills/foo/scripts/run.sh",
			"main",
			"#!/bin/sh\necho hi\n",
		)
		.await;

		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let resource = sample_market_resource("skills/foo");
		let payload = provider
			.fetch_payload(&client(), &resource, None)
			.await
			.unwrap();

		let InstallPayload::Skill { files } = payload else {
			panic!("应产出 InstallPayload::Skill");
		};
		assert_eq!(files.len(), 2);
		let skill_md = files
			.iter()
			.find(|f| f.rel_path == "SKILL.md")
			.expect("应含 SKILL.md");
		assert_eq!(skill_md.content, b"skill file body");
		let run_sh = files
			.iter()
			.find(|f| f.rel_path == "scripts/run.sh")
			.expect("应含嵌套子目录下的 run.sh");
		assert_eq!(run_sh.content, b"#!/bin/sh\necho hi\n");
	}

	// fetch_payload: install_manifest 非 Skill 变体(如误传 Mcp 类资源)应报错, 不 panic
	#[tokio::test]
	async fn fetch_payload_returns_err_for_non_skill_install_manifest() {
		let server = MockServer::start().await;
		let provider = GithubSkillsProvider::with_base_url(vec![sample_repo_ref()], server.uri());
		let mut resource = sample_market_resource("skills/foo");
		resource.res_type = ResourceType::Mcp;
		resource.install_manifest = InstallManifest::Mcp {
			server_def: McpServerDef {
				name: "srv".to_string(),
				command: Some("npx".to_string()),
				args: vec![],
				env: BTreeMap::new(),
				url: None,
			},
		};

		let result = provider.fetch_payload(&client(), &resource, None).await;
		assert!(result.is_err());
	}

	// id/auth_kind: 应分别报告 GithubSkills 与 Some(GitHub), 不需要网络
	#[test]
	fn provider_reports_github_skills_id_and_github_auth_kind() {
		let provider = GithubSkillsProvider::default();
		assert_eq!(provider.id(), SourceId::GithubSkills);
		assert_eq!(provider.auth_kind(), Some(AuthKind::GitHub));
	}
}
