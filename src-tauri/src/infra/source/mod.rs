// 文件作用: 市场源聚合抽象 —— SourceProvider trait(统一 search/fetch_payload/auth_kind 接口)、
//           AuthKind(市场源要求的认证类型)、InstallPayload/FileEntry(fetch_payload 的产物形状),
//           以及 all_sources 全量源注册表。三源(github_skills/mcp_registry/github_mcp)均已注册
// 创建日期: 2026-07-09

pub mod github_mcp;
pub mod github_skills;
pub mod mcp_registry;

use async_trait::async_trait;
use reqwest::Client;

use crate::domain::agent::McpServerDef;
use crate::domain::market::{MarketResource, Query, SourceId};
use github_mcp::GithubMcpProvider;
use github_skills::GithubSkillsProvider;
use mcp_registry::McpRegistryProvider;

/// 市场源要求的认证类型: 对应可在应用内发起 OAuth 的三家身份提供方(GitHub/Google/Microsoft),
/// 供认证服务(Task 7/8)决定弹哪一种登录方式; SourceProvider::auth_kind 返回 None 表示该源
/// 完全公开, 无需认证即可完整使用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
	GitHub,
	Google,
	Microsoft,
}

/// 单个待落地文件: 相对 Skill 根目录的相对路径 + 原始字节内容。fetch_payload 拉取 Skill 类
/// 资源时, 用一组本结构体描述"要在本地写出哪些文件"; 具体落盘(写入 data_dir/skills/<name>/)
/// 由 services::market::install(Task 9)负责, 本层只管拉取与组装
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
	pub rel_path: String,
	pub content: Vec<u8>,
}

/// fetch_payload 的产物: 按 res_type 呼应的形状分派, 变体命名呼应 domain::market::
/// InstallManifest 的既有命名习惯, 便于对照阅读(本类型不经 Tauri IPC 传输, 不需要 Serialize)。
/// 未派生 Eq: Mcp 变体内嵌 McpServerDef 本身只派生了 PartialEq(见 domain::agent), 无法整体 Eq
#[derive(Debug, Clone, PartialEq)]
pub enum InstallPayload {
	/// Skill: 该 Skill 子目录下的全部文件(递归展开子目录), 含 SKILL.md 本身
	Skill { files: Vec<FileEntry> },
	/// Mcp: 服务定义已完整, 可直接落地为某 Agent 的 mcpServers 配置项
	Mcp { server_def: McpServerDef },
}

/// 市场源统一接口: 每个源(github_skills/mcp_registry/github_mcp)各自实现一份, 由聚合层
/// (services::market, Task 6)持有 `Vec<Box<dyn SourceProvider>>` 逐源调用并合并结果。
/// 方法声明为 async fn, 借 #[async_trait] 宏改写为返回装箱 Future, 换取 trait 对象安全
/// (原生 async fn in trait 生成的匿名关联类型无法进 vtable, 不支持 dyn 分派)。要求
/// Send + Sync: 供聚合层跨 await 点/线程持有与并发调用多个源。
/// client 由调用方(聚合层持有的复用连接池)传入, 各实现不应自行构造/持有 Client
/// (呼应 infra::http::client 文档"调用方应复用同一个实例"的约束)
#[async_trait]
pub trait SourceProvider: Send + Sync {
	/// 本源对应的来源枚举(落库 market_cache.source_type 时使用)
	fn id(&self) -> SourceId;

	/// 搜索本源下的资源。关键字/分类等细粒度过滤统一交给聚合层(services::market)在合并多源
	/// 结果后处理, 各源实现可自行决定是否使用 query 参数做服务端过滤(如源本身支持关键字搜索
	/// 接口); 不支持的可直接忽略该参数, 恒返回全量
	async fn search(
		&self,
		client: &Client,
		query: &Query,
		token: Option<&str>,
	) -> anyhow::Result<Vec<MarketResource>>;

	/// 拉取某条资源的完整安装内容(如 Skill 子目录下的全部文件, 或 MCP 服务定义)
	async fn fetch_payload(
		&self,
		client: &Client,
		resource: &MarketResource,
		token: Option<&str>,
	) -> anyhow::Result<InstallPayload>;

	/// 本源要求的认证类型; None 表示无需认证即可完整使用(搜索与安装均不受限, 至多受匿名限流)
	fn auth_kind(&self) -> Option<AuthKind>;
}

/// 全量市场源注册表: github_skills/mcp_registry/github_mcp 三源均用各自的生产默认构造注册,
/// 供聚合层(services::market, Task 6)逐源调用并合并结果
pub fn all_sources() -> Vec<Box<dyn SourceProvider>> {
	vec![
		Box::new(GithubSkillsProvider::default()),
		Box::new(McpRegistryProvider::default()),
		Box::new(GithubMcpProvider::default()),
	]
}

#[cfg(test)]
mod tests {
	use super::*;

	// all_sources: 应恰好注册 3 个源(github_skills/mcp_registry/github_mcp), 顺序与 id/auth_kind
	// 均符合预期; 兼带验证 SourceProvider 对象安全(能被装箱进 Vec<Box<dyn SourceProvider>>)
	#[test]
	fn all_sources_registers_all_three_sources() {
		let sources = all_sources();
		assert_eq!(sources.len(), 3, "三源(Task 3/4/5)均应注册");
		assert_eq!(sources[0].id(), SourceId::GithubSkills);
		assert_eq!(sources[1].id(), SourceId::McpRegistry);
		assert_eq!(sources[2].id(), SourceId::GithubMcp);
		assert_eq!(sources[0].auth_kind(), Some(AuthKind::GitHub));
		assert_eq!(sources[1].auth_kind(), None);
		assert_eq!(sources[2].auth_kind(), Some(AuthKind::GitHub));
	}
}
