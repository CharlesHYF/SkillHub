// 文件作用: 资源领域类型 —— ResourceType/SourceType 枚举与 Resource 实体,
//           提供与 resource 表 INTEGER 列的 i64 互转(见 migrations/0001_init.sql)
// 创建日期: 2026-07-09

use serde::{Deserialize, Serialize};

/// 资源类型: 对应 resource.res_type 列
/// 1-Skill, 2-Mcp
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceType {
	Skill,
	Mcp,
}

impl ResourceType {
	/// 由数据库 INTEGER 值还原枚举; 未知值兜底为列默认值 Skill(1), 避免脏数据 panic
	pub fn from_i64(value: i64) -> Self {
		match value {
			2 => ResourceType::Mcp,
			_ => ResourceType::Skill,
		}
	}
}

impl From<ResourceType> for i64 {
	fn from(value: ResourceType) -> i64 {
		match value {
			ResourceType::Skill => 1,
			ResourceType::Mcp => 2,
		}
	}
}

/// 资源来源: 对应 resource.source_type 列
/// 0-本地导入, 1-官方仓库, 2-第三方仓库
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceType {
	LocalImport,
	Official,
	ThirdParty,
}

impl SourceType {
	/// 由数据库 INTEGER 值还原枚举; 未知值兜底为列默认值 LocalImport(0)
	pub fn from_i64(value: i64) -> Self {
		match value {
			1 => SourceType::Official,
			2 => SourceType::ThirdParty,
			_ => SourceType::LocalImport,
		}
	}
}

impl From<SourceType> for i64 {
	fn from(value: SourceType) -> i64 {
		match value {
			SourceType::LocalImport => 0,
			SourceType::Official => 1,
			SourceType::ThirdParty => 2,
		}
	}
}

/// 资源实体: 对应 resource 表一行(SkillHub 托管的 Skill/MCP 元数据)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
	pub id: i64,
	pub res_type: ResourceType,
	pub name: String,
	pub display_name: String,
	pub version: String,
	pub source_type: SourceType,
	pub local_path: String,
	pub enabled: bool,
	pub create_time: String,
	pub update_time: String,
}

#[cfg(test)]
mod tests {
	use super::*;

	// ResourceType: 已知值双向互转应精确对应枚举变体
	#[test]
	fn resource_type_from_i64_known_values_round_trip() {
		assert_eq!(ResourceType::from_i64(1), ResourceType::Skill);
		assert_eq!(ResourceType::from_i64(2), ResourceType::Mcp);
		assert_eq!(i64::from(ResourceType::Skill), 1);
		assert_eq!(i64::from(ResourceType::Mcp), 2);
	}

	// ResourceType: 未知值(脏数据)应兜底为列默认值 Skill, 不 panic
	#[test]
	fn resource_type_from_i64_unknown_value_falls_back_to_skill() {
		assert_eq!(ResourceType::from_i64(0), ResourceType::Skill);
		assert_eq!(ResourceType::from_i64(99), ResourceType::Skill);
	}

	// SourceType: 已知值双向互转应精确对应枚举变体
	#[test]
	fn source_type_from_i64_known_values_round_trip() {
		assert_eq!(SourceType::from_i64(0), SourceType::LocalImport);
		assert_eq!(SourceType::from_i64(1), SourceType::Official);
		assert_eq!(SourceType::from_i64(2), SourceType::ThirdParty);
		assert_eq!(i64::from(SourceType::LocalImport), 0);
		assert_eq!(i64::from(SourceType::Official), 1);
		assert_eq!(i64::from(SourceType::ThirdParty), 2);
	}

	// SourceType: 未知值(脏数据)应兜底为列默认值 LocalImport, 不 panic
	#[test]
	fn source_type_from_i64_unknown_value_falls_back_to_local_import() {
		assert_eq!(SourceType::from_i64(-1), SourceType::LocalImport);
		assert_eq!(SourceType::from_i64(99), SourceType::LocalImport);
	}
}
