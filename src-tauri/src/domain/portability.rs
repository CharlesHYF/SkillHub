// 文件作用: 导入导出领域类型 —— BundleFormat/Scope/ConflictStrategy 枚举、导出选项(ExportOptions)、
//           导出清单(Manifest/Counts)与导入预览(ImportPreview), 提供与 import_export_log 表
//           file_format 列一致的 i64 互转(见 migrations/0001_init.sql)
// 创建日期: 2026-07-10

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 导出包格式: 对应 import_export_log.file_format 列
/// 1-Zip, 2-Json, 3-Tar
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleFormat {
	Zip,
	Json,
	Tar,
}

impl BundleFormat {
	/// 由数据库 INTEGER 值还原枚举; 未知值兜底为列默认值 Zip(1), 避免脏数据 panic
	pub fn from_i64(value: i64) -> Self {
		match value {
			2 => BundleFormat::Json,
			3 => BundleFormat::Tar,
			_ => BundleFormat::Zip,
		}
	}
}

impl From<BundleFormat> for i64 {
	fn from(value: BundleFormat) -> i64 {
		match value {
			BundleFormat::Zip => 1,
			BundleFormat::Json => 2,
			BundleFormat::Tar => 3,
		}
	}
}

/// 导出范围: 前端"导出"面板的范围选项(不落库, 仅供 services::portability 收集资源时判定)
/// 0-全部, 1-按类型, 2-按时间
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
	All,
	ByType,
	ByTime,
}

impl Scope {
	/// 由持久化编码还原枚举; 未知值兜底为最小合法编码 All(0)
	pub fn from_i64(value: i64) -> Self {
		match value {
			1 => Scope::ByType,
			2 => Scope::ByTime,
			_ => Scope::All,
		}
	}
}

impl From<Scope> for i64 {
	fn from(value: Scope) -> i64 {
		match value {
			Scope::All => 0,
			Scope::ByType => 1,
			Scope::ByTime => 2,
		}
	}
}

/// 导入冲突处理策略: import_bundle 命令入参之一(不落库, 仅驱动 services::portability::import_bundle)
/// 0-覆盖, 1-跳过, 2-保留两者(重命名落地)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictStrategy {
	Overwrite,
	Skip,
	KeepBoth,
}

impl ConflictStrategy {
	/// 由持久化编码还原枚举; 未知值兜底为最小合法编码 Overwrite(0)
	pub fn from_i64(value: i64) -> Self {
		match value {
			1 => ConflictStrategy::Skip,
			2 => ConflictStrategy::KeepBoth,
			_ => ConflictStrategy::Overwrite,
		}
	}
}

impl From<ConflictStrategy> for i64 {
	fn from(value: ConflictStrategy) -> i64 {
		match value {
			ConflictStrategy::Overwrite => 0,
			ConflictStrategy::Skip => 1,
			ConflictStrategy::KeepBoth => 2,
		}
	}
}

/// 导出选项: 前端"导出"面板的表单值, 驱动 services::portability::export_bundle 收集范围与打包格式
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
	pub include_skills: bool,
	pub include_mcp: bool,
	pub scope: Scope,
	pub format: BundleFormat,
	pub include_config: bool,
	pub include_version_lock: bool,
}

/// 导出内容计数: 供 Manifest 与 ImportPreview 共用的资源类别统计
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
	pub skill: i64,
	pub mcp: i64,
	pub config: i64,
	pub agent: i64,
}

/// 导出清单: 打包产物内 manifest.json 的内容 —— schema 版本、导出时间、内容计数、各文件 sha256
/// 校验和(供导入时逐一比对, 见 M3 Task 3 的 parse_bundle 校验)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
	pub schema_version: i64,
	pub exported_at: String,
	pub counts: Counts,
	pub checksums: BTreeMap<String, String>,
}

/// 导入预览: import_preview 命令返回给前端"将导入内容"面板的计数 + schema 兼容性判定结果
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
	pub skill: i64,
	pub mcp: i64,
	pub config: i64,
	pub agent: i64,
	pub schema_ok: bool,
}

#[cfg(test)]
mod tests {
	use super::*;

	// BundleFormat: 已知编码应与枚举变体精确往返(与 import_export_log.file_format 列一致)
	#[test]
	fn bundle_format_from_i64_known_values_round_trip() {
		assert_eq!(BundleFormat::from_i64(1), BundleFormat::Zip);
		assert_eq!(BundleFormat::from_i64(2), BundleFormat::Json);
		assert_eq!(BundleFormat::from_i64(3), BundleFormat::Tar);
		assert_eq!(i64::from(BundleFormat::Zip), 1);
		assert_eq!(i64::from(BundleFormat::Json), 2);
		assert_eq!(i64::from(BundleFormat::Tar), 3);
	}

	// BundleFormat: 未知值(脏数据)应兜底为列默认值 Zip(1), 不 panic
	#[test]
	fn bundle_format_from_i64_unknown_value_falls_back_to_zip() {
		assert_eq!(BundleFormat::from_i64(0), BundleFormat::Zip);
		assert_eq!(BundleFormat::from_i64(99), BundleFormat::Zip);
	}

	// Scope: 已知值双向互转应精确对应枚举变体
	#[test]
	fn scope_from_i64_known_values_round_trip() {
		assert_eq!(Scope::from_i64(0), Scope::All);
		assert_eq!(Scope::from_i64(1), Scope::ByType);
		assert_eq!(Scope::from_i64(2), Scope::ByTime);
		assert_eq!(i64::from(Scope::All), 0);
		assert_eq!(i64::from(Scope::ByType), 1);
		assert_eq!(i64::from(Scope::ByTime), 2);
	}

	// Scope: 未知值(脏数据)应兜底为 All(0), 不 panic
	#[test]
	fn scope_from_i64_unknown_value_falls_back_to_all() {
		assert_eq!(Scope::from_i64(-1), Scope::All);
		assert_eq!(Scope::from_i64(99), Scope::All);
	}

	// ConflictStrategy: 已知值双向互转应精确对应枚举变体
	#[test]
	fn conflict_strategy_from_i64_known_values_round_trip() {
		assert_eq!(ConflictStrategy::from_i64(0), ConflictStrategy::Overwrite);
		assert_eq!(ConflictStrategy::from_i64(1), ConflictStrategy::Skip);
		assert_eq!(ConflictStrategy::from_i64(2), ConflictStrategy::KeepBoth);
		assert_eq!(i64::from(ConflictStrategy::Overwrite), 0);
		assert_eq!(i64::from(ConflictStrategy::Skip), 1);
		assert_eq!(i64::from(ConflictStrategy::KeepBoth), 2);
	}

	// ConflictStrategy: 未知值(脏数据)应兜底为 Overwrite(0), 不 panic
	#[test]
	fn conflict_strategy_from_i64_unknown_value_falls_back_to_overwrite() {
		assert_eq!(ConflictStrategy::from_i64(-1), ConflictStrategy::Overwrite);
		assert_eq!(ConflictStrategy::from_i64(99), ConflictStrategy::Overwrite);
	}

	// ExportOptions: 序列化应使用 camelCase 字段名, 且能通过 JSON 往返还原
	#[test]
	fn export_options_round_trips_through_json_with_camel_case_fields() {
		let opts = ExportOptions {
			include_skills: true,
			include_mcp: false,
			scope: Scope::ByType,
			format: BundleFormat::Zip,
			include_config: true,
			include_version_lock: false,
		};
		let json = serde_json::to_value(&opts).unwrap();
		assert_eq!(json["includeSkills"], true);
		assert_eq!(json["includeMcp"], false);
		assert_eq!(json["scope"], "ByType");
		assert_eq!(json["format"], "Zip");
		assert_eq!(json["includeConfig"], true);
		assert_eq!(json["includeVersionLock"], false);
		assert!(json.get("include_skills").is_none());

		let text = serde_json::to_string(&opts).unwrap();
		let back: ExportOptions = serde_json::from_str(&text).unwrap();
		assert_eq!(back, opts);
	}

	// Manifest: 内嵌 Counts/checksums 应整体序列化为 camelCase, 且能 JSON 往返还原
	#[test]
	fn manifest_round_trips_through_json_with_nested_counts_and_checksums() {
		let mut checksums = BTreeMap::new();
		checksums.insert("skills/demo/SKILL.md".to_string(), "abc123".to_string());
		let manifest = Manifest {
			schema_version: 1,
			exported_at: "2026-07-10T00:00:00Z".to_string(),
			counts: Counts {
				skill: 3,
				mcp: 2,
				config: 1,
				agent: 0,
			},
			checksums,
		};
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["schemaVersion"], 1);
		assert_eq!(json["exportedAt"], "2026-07-10T00:00:00Z");
		assert_eq!(json["counts"]["skill"], 3);
		assert_eq!(json["counts"]["mcp"], 2);
		assert_eq!(json["checksums"]["skills/demo/SKILL.md"], "abc123");
		assert!(json.get("schema_version").is_none());

		let text = serde_json::to_string(&manifest).unwrap();
		let back: Manifest = serde_json::from_str(&text).unwrap();
		assert_eq!(back, manifest);
	}

	// ImportPreview: 序列化应使用 camelCase(schemaOk), 且能 JSON 往返还原
	#[test]
	fn import_preview_round_trips_through_json_with_camel_case_fields() {
		let preview = ImportPreview {
			skill: 2,
			mcp: 1,
			config: 1,
			agent: 0,
			schema_ok: true,
		};
		let json = serde_json::to_value(&preview).unwrap();
		assert_eq!(json["schemaOk"], true);
		assert!(json.get("schema_ok").is_none());

		let text = serde_json::to_string(&preview).unwrap();
		let back: ImportPreview = serde_json::from_str(&text).unwrap();
		assert_eq!(back, preview);
	}
}
