// 文件作用: 市场领域类型 —— SourceId 来源枚举、MarketResourceRespVO 归一化实体、InstallManifest
//           安装清单、SortBy/Query 查询参数, 提供与 market_cache 表 INTEGER 列的 i64 互转
//           (见 migrations/0001_init.sql market_cache 表注释)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use serde::{Deserialize, Serialize};

use crate::domain::agent::McpServerDef;
use crate::domain::resource::ResourceType;

/// 市场资源来源: 对应 market_cache.source_type 列
/// 1-github_skills(GitHub Skills 仓库聚合), 2-mcp_registry(官方 MCP Registry), 3-github_mcp(GitHub
/// 上的 MCP 服务器合集仓库聚合)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceId {
	GithubSkills,
	McpRegistry,
	GithubMcp,
}

impl SourceId {
	/// 由数据库 INTEGER 值还原枚举; 未知值(含列默认值 0)兜底为最小合法编码 GithubSkills(1)
	pub fn from_i64(value: i64) -> Self {
		match value {
			2 => SourceId::McpRegistry,
			3 => SourceId::GithubMcp,
			_ => SourceId::GithubSkills,
		}
	}
}

impl From<SourceId> for i64 {
	fn from(value: SourceId) -> i64 {
		match value {
			SourceId::GithubSkills => 1,
			SourceId::McpRegistry => 2,
			SourceId::GithubMcp => 3,
		}
	}
}

/// 安装清单: 归一化后的"如何安装这条市场资源"描述, 按 res_type 呼应的形状分派。变体标签本身
/// 不转 case(PascalCase: Skill/Mcp/McpTemplate), 只转字段名(与 domain::sync::DesiredPayload
/// "标签保持 PascalCase, 只转字段名"的既有约定一致)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum InstallManifest {
	/// Skill: 记录源仓库 + 子目录路径 + git 引用(分支/tag/commit), 供安装时拉取该子树
	#[serde(rename_all = "camelCase")]
	Skill {
		repo: String,
		path: String,
		git_ref: String,
	},
	/// Mcp: 服务定义已完整(无需用户再填参数), 可直接落地为某 Agent 的 mcpServers 配置项
	#[serde(rename_all = "camelCase")]
	Mcp { server_def: McpServerDef },
	/// McpTemplate: 服务定义是模板, required_env 列出安装时需用户填充的环境变量名(占位值需
	/// 由调用方替换后才能落地使用)
	#[serde(rename_all = "camelCase")]
	McpTemplate {
		server_def: McpServerDef,
		required_env: Vec<String>,
	},
}

/// 市场资源: 归一化后的一条可浏览/可安装资源(对应 market_cache 表一行的完整载荷,
/// 落库时整份序列化进 raw_json, 见 infra::repo_market)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketResourceRespVO {
	pub source_type: SourceId,
	pub res_type: ResourceType,
	/// 来源内唯一标识(如 "owner/repo:path"), 与 source_type 组合唯一(对应 uk_market_cache_src_ext)
	pub ext_id: String,
	pub name: String,
	pub display_name: String,
	pub description: String,
	pub author: String,
	pub version: String,
	pub stars: i64,
	pub category: String,
	pub tags: Vec<String>,
	pub auth_required: bool,
	pub install_manifest: InstallManifest,
	/// 该资源在来源侧的最后更新时间(如 GitHub 仓库的 pushed_at), 非本地缓存拉取时间
	/// (缓存拉取时间见 infra::repo_market 维护的 fetch_time 列, 不进入本结构体)
	pub updated_at: String,
}

/// 市场查询排序方式: 对应前端筛选栏的排序下拉
/// 0-推荐(暂无独立评分信号, 兜底按写入顺序), 1-星标数降序, 2-最近更新(按本地缓存刷新时间降序)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
	Recommended,
	Stars,
	Updated,
}

impl SortBy {
	/// 由前端传入的排序编码(见本类型文档的编码约定)还原枚举; 未知值兜底为最小合法编码
	/// Recommended(0), 与 SourceId/ResourceType/ProviderKind 的既有 from_i64 惯例一致
	pub fn from_i64(value: i64) -> Self {
		match value {
			1 => SortBy::Stars,
			2 => SortBy::Updated,
			_ => SortBy::Recommended,
		}
	}
}

impl From<SortBy> for i64 {
	fn from(value: SortBy) -> i64 {
		match value {
			SortBy::Recommended => 0,
			SortBy::Stars => 1,
			SortBy::Updated => 2,
		}
	}
}

/// 市场查询参数: 关键字(匹配 name/author)/类型/分类均可选, 不填表示不过滤该维度
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Query {
	pub keyword: Option<String>,
	pub res_type: Option<ResourceType>,
	pub category: Option<String>,
	pub sort: SortBy,
	pub page: i64,
	pub page_size: i64,
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use super::*;

	// SourceId: 已知值双向互转应精确对应枚举变体
	#[test]
	fn source_id_from_i64_known_values_round_trip() {
		assert_eq!(SourceId::from_i64(1), SourceId::GithubSkills);
		assert_eq!(SourceId::from_i64(2), SourceId::McpRegistry);
		assert_eq!(SourceId::from_i64(3), SourceId::GithubMcp);
		assert_eq!(i64::from(SourceId::GithubSkills), 1);
		assert_eq!(i64::from(SourceId::McpRegistry), 2);
		assert_eq!(i64::from(SourceId::GithubMcp), 3);
	}

	// SourceId: 未知值(脏数据, 含列默认值 0)兜底为最小合法编码 GithubSkills, 不 panic
	#[test]
	fn source_id_from_i64_unknown_value_falls_back_to_github_skills() {
		assert_eq!(SourceId::from_i64(0), SourceId::GithubSkills);
		assert_eq!(SourceId::from_i64(99), SourceId::GithubSkills);
	}

	// SortBy: 已知值双向互转应精确对应枚举变体
	#[test]
	fn sort_by_from_i64_known_values_round_trip() {
		assert_eq!(SortBy::from_i64(0), SortBy::Recommended);
		assert_eq!(SortBy::from_i64(1), SortBy::Stars);
		assert_eq!(SortBy::from_i64(2), SortBy::Updated);
		assert_eq!(i64::from(SortBy::Recommended), 0);
		assert_eq!(i64::from(SortBy::Stars), 1);
		assert_eq!(i64::from(SortBy::Updated), 2);
	}

	// SortBy: 未知值(脏数据)兜底为最小合法编码 Recommended, 不 panic
	#[test]
	fn sort_by_from_i64_unknown_value_falls_back_to_recommended() {
		assert_eq!(SortBy::from_i64(-1), SortBy::Recommended);
		assert_eq!(SortBy::from_i64(99), SortBy::Recommended);
	}

	fn sample_mcp_server_def() -> McpServerDef {
		McpServerDef {
			name: "filesystem".to_string(),
			command: Some("npx".to_string()),
			args: vec!["-y".to_string(), "server-fs".to_string()],
			env: BTreeMap::new(),
			url: None,
		}
	}

	// InstallManifest::Skill: 变体标签保持 PascalCase("Skill"), 字段转 camelCase(gitRef)
	#[test]
	fn install_manifest_skill_serializes_field_as_camel_case() {
		let manifest = InstallManifest::Skill {
			repo: "acme/skills".to_string(),
			path: "skills/demo".to_string(),
			git_ref: "main".to_string(),
		};
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["Skill"]["gitRef"], "main");
		assert!(json["Skill"].get("git_ref").is_none());
	}

	// InstallManifest::McpTemplate: 字段转 camelCase(serverDef/requiredEnv), 且能整体 JSON 往返
	#[test]
	fn install_manifest_mcp_template_round_trips_through_json() {
		let manifest = InstallManifest::McpTemplate {
			server_def: sample_mcp_server_def(),
			required_env: vec!["API_KEY".to_string()],
		};
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["McpTemplate"]["requiredEnv"][0], "API_KEY");
		assert_eq!(json["McpTemplate"]["serverDef"]["command"], "npx");
		assert!(json["McpTemplate"].get("required_env").is_none());

		let back: InstallManifest =
			serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
		assert_eq!(back, manifest);
	}

	// InstallManifest::Mcp: 字段转 camelCase(serverDef), 且能整体 JSON 往返
	#[test]
	fn install_manifest_mcp_round_trips_through_json() {
		let manifest = InstallManifest::Mcp {
			server_def: sample_mcp_server_def(),
		};
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["Mcp"]["serverDef"]["name"], "filesystem");

		let back: InstallManifest =
			serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
		assert_eq!(back, manifest);
	}

	fn sample_market_resource() -> MarketResourceRespVO {
		MarketResourceRespVO {
			source_type: SourceId::GithubSkills,
			res_type: ResourceType::Skill,
			ext_id: "acme/skills:demo".to_string(),
			name: "demo-skill".to_string(),
			display_name: "Demo Skill".to_string(),
			description: "一个示例 Skill".to_string(),
			author: "acme".to_string(),
			version: "1.0.0".to_string(),
			stars: 42,
			category: "productivity".to_string(),
			tags: vec!["demo".to_string(), "sample".to_string()],
			auth_required: false,
			install_manifest: InstallManifest::Skill {
				repo: "acme/skills".to_string(),
				path: "skills/demo".to_string(),
				git_ref: "main".to_string(),
			},
			updated_at: "2026-07-01T00:00:00Z".to_string(),
		}
	}

	// MarketResourceRespVO: 应整体序列化为 camelCase(sourceType/resType/extId/displayName/authRequired/
	// installManifest/updatedAt), 且能通过 JSON 往返完整还原(验证嵌套 InstallManifest 一并往返)
	#[test]
	fn market_resource_round_trips_through_json_with_camel_case_fields() {
		let resource = sample_market_resource();
		let json = serde_json::to_value(&resource).unwrap();
		assert_eq!(json["sourceType"], "GithubSkills");
		assert_eq!(json["resType"], "Skill");
		assert_eq!(json["extId"], "acme/skills:demo");
		assert_eq!(json["displayName"], "Demo Skill");
		assert_eq!(json["authRequired"], false);
		assert_eq!(json["updatedAt"], "2026-07-01T00:00:00Z");
		assert!(json.get("source_type").is_none());
		assert!(json.get("ext_id").is_none());

		let back: MarketResourceRespVO =
			serde_json::from_str(&serde_json::to_string(&resource).unwrap()).unwrap();
		assert_eq!(back, resource);
	}

	// Query: 应整体序列化为 camelCase(resType/pageSize), 且能通过 JSON 往返还原
	#[test]
	fn query_round_trips_through_json_with_camel_case_fields() {
		let query = Query {
			keyword: Some("demo".to_string()),
			res_type: Some(ResourceType::Mcp),
			category: None,
			sort: SortBy::Stars,
			page: 1,
			page_size: 20,
		};
		let json = serde_json::to_value(&query).unwrap();
		assert_eq!(json["resType"], "Mcp");
		assert_eq!(json["pageSize"], 20);
		assert_eq!(json["sort"], "Stars");
		assert!(json.get("res_type").is_none());
		assert!(json.get("page_size").is_none());

		let back: Query = serde_json::from_str(&serde_json::to_string(&query).unwrap()).unwrap();
		assert_eq!(back, query);
	}
}
