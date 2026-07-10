// 文件作用: 设置(Settings)领域类型 —— 存储目录/同步偏好/网络代理/更新通道的类型化表示,
//           与 setting 表键值对(infra::repo_setting::SettingRow 的 cfg_key/cfg_value)双向
//           映射(from_rows/to_pairs); serde camelCase 序列化与前端 src/api/setting.ts 的
//           Settings 类型逐字段对齐(见 docs/superpowers/plans/2026-07-10-skillhub-m4-polish.md
//           "设置契约"一节)。存储编码: bool 存 '0'/'1', i64 存十进制字符串, String 原样;
//           解析(from_rows)时缺键或值非法一律回落 Settings::default() 对应字段, 不 panic,
//           呼应 domain::portability 里 BundleFormat/Scope/ConflictStrategy::from_i64 对
//           "未知/脏数据一律兜底为合法默认值"的既有取舍
// 创建日期: 2026-07-10

use serde::{Deserialize, Serialize};

use crate::infra::repo_setting::SettingRow;

/// setting 表 cfg_key 命名: 与前端 src/api/setting.ts Settings 字段逐一对应, 顺序同 to_pairs
const KEY_STORAGE_SKILL_DIR: &str = "storage.skill_dir";
const KEY_STORAGE_MCP_DIR: &str = "storage.mcp_dir";
const KEY_SYNC_AUTO_NEW_AGENT: &str = "sync.auto_new_agent";
const KEY_SYNC_CHECK_UPDATE_ON_START: &str = "sync.check_update_on_start";
const KEY_SYNC_CONFLICT_PROMPT: &str = "sync.conflict_prompt";
const KEY_SYNC_ONLY_ENABLED: &str = "sync.only_enabled";
const KEY_NET_PROXY_MODE: &str = "net.proxy_mode";
const KEY_NET_HTTP_PROXY: &str = "net.http_proxy";
const KEY_NET_HTTPS_PROXY: &str = "net.https_proxy";
const KEY_NET_NO_PROXY: &str = "net.no_proxy";
const KEY_NET_TIMEOUT_SEC: &str = "net.timeout_sec";
const KEY_UPDATE_CHANNEL: &str = "update.channel";

/// 应用设置: 12 个字段整体持久化于 setting 表(每字段对应上方一个 cfg_key 常量), 缺键或值
/// 非法一律回落默认值(见 from_rows), 不因脏数据或首次运行(空表)导致解析失败。
///
/// 诚实边界(与 M4 计划"诚实边界"一节一致, 不在此类型内重复展开): 本类型全部字段均已真实
/// 持久化 + 可读可写; 其中网络代理/超时(net_* 五个字段)本轮已接入共享 HTTP 客户端真实生效
/// (见 infra::http::build_http_client), 存储目录(storage_*)/同步偏好(sync_*)/更新通道
/// (update_channel)本轮仅持久化留用, 尚未接入对应的真实行为(目录迁移/同步流程钩子/更新器)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
	/// 本地 Skill 目录(cfg_key="storage.skill_dir"); 默认空串, 空串语义为"使用应用数据目录下
	/// 默认位置"; 本轮仅持久化展示, 改动后对既有资源的迁移/重定位留待后续任务
	pub storage_skill_dir: String,
	/// 本地 MCP 目录(cfg_key="storage.mcp_dir"); 语义与 storage_skill_dir 一致
	pub storage_mcp_dir: String,
	/// 自动同步到新 Agent(cfg_key="sync.auto_new_agent"); 默认 true
	pub sync_auto_new_agent: bool,
	/// 启动时检查更新(cfg_key="sync.check_update_on_start"); 默认 true; 尚无更新器, 本轮
	/// 仅持久化留用
	pub sync_check_update_on_start: bool,
	/// 冲突时提示(cfg_key="sync.conflict_prompt"); 默认 true
	pub sync_conflict_prompt: bool,
	/// 仅同步已启用项(cfg_key="sync.only_enabled"); 默认 false
	pub sync_only_enabled: bool,
	/// 网络代理模式(cfg_key="net.proxy_mode"): 0-系统默认/1-不使用/2-手动; 默认 0
	pub net_proxy_mode: i64,
	/// HTTP 代理地址(cfg_key="net.http_proxy"); 默认空串
	pub net_http_proxy: String,
	/// HTTPS 代理地址(cfg_key="net.https_proxy"); 默认空串
	pub net_https_proxy: String,
	/// 不使用代理的地址列表(cfg_key="net.no_proxy"); 默认空串
	pub net_no_proxy: String,
	/// 请求超时(秒)(cfg_key="net.timeout_sec"); 默认 30
	pub net_timeout_sec: i64,
	/// 更新通道(cfg_key="update.channel"): 0-Stable/1-Beta; 默认 0
	pub update_channel: i64,
}

impl Default for Settings {
	/// 契约约定的默认值(见 docs/superpowers/plans/2026-07-10-skillhub-m4-polish.md "设置契约");
	/// 首次运行(setting 表为空)或任意字段缺键/非法时, from_rows 均回落到本实现
	fn default() -> Self {
		Settings {
			storage_skill_dir: String::new(),
			storage_mcp_dir: String::new(),
			sync_auto_new_agent: true,
			sync_check_update_on_start: true,
			sync_conflict_prompt: true,
			sync_only_enabled: false,
			net_proxy_mode: 0,
			net_http_proxy: String::new(),
			net_https_proxy: String::new(),
			net_no_proxy: String::new(),
			net_timeout_sec: 30,
			update_channel: 0,
		}
	}
}

/// 在若干行里按 cfg_key 查找对应的 cfg_value; 找不到返回 None, 供各 decode_* 函数统一处理
/// "键缺失"这一种场景(与"键存在但值非法"一样, 当前策略下二者结果一致, 均回落默认值)
fn find<'a>(rows: &'a [SettingRow], key: &str) -> Option<&'a str> {
	rows.iter()
		.find(|row| row.cfg_key == key)
		.map(|row| row.cfg_value.as_str())
}

/// 解码 bool: 严格要求存储值恰为 '0'/'1'(见本文件头"存储编码"约定), 其余任何取值(键缺失
/// 传入的 None、或历史脏数据留下的其它字符串, 如 "true"/"x")一律回落 default, 不 panic
fn decode_bool(value: Option<&str>, default: bool) -> bool {
	match value {
		Some("1") => true,
		Some("0") => false,
		_ => default,
	}
}

/// 解码 i64: 解析失败(键缺失或不是合法十进制数字, 如 "x")一律回落 default
fn decode_i64(value: Option<&str>, default: i64) -> i64 {
	value.and_then(|v| v.parse::<i64>().ok()).unwrap_or(default)
}

/// 解码字符串: 原样使用, 只有键缺失(value 为 None)才回落 default —— 字符串字段没有
/// "值非法"这一概念, 任意字符串本身都是合法的
fn decode_string(value: Option<&str>, default: &str) -> String {
	value
		.map(|v| v.to_string())
		.unwrap_or_else(|| default.to_string())
}

/// 编码 bool: true -> "1", false -> "0"(与本文件头"存储编码"约定一致)
fn encode_bool(value: bool) -> String {
	(if value { "1" } else { "0" }).to_string()
}

impl Settings {
	/// 由 setting 表全量行还原为 Settings: 逐字段按对应 cfg_key 查值并解码, 缺键或值非法均
	/// 回落 Settings::default() 里对应字段的值(见 decode_bool/decode_i64/decode_string 各自
	/// 文档), 因此本函数对任意输入(包括空切片、任意脏数据组合)恒不 panic、恒有确定结果
	pub fn from_rows(rows: &[SettingRow]) -> Settings {
		let fallback = Settings::default();
		Settings {
			storage_skill_dir: decode_string(
				find(rows, KEY_STORAGE_SKILL_DIR),
				&fallback.storage_skill_dir,
			),
			storage_mcp_dir: decode_string(
				find(rows, KEY_STORAGE_MCP_DIR),
				&fallback.storage_mcp_dir,
			),
			sync_auto_new_agent: decode_bool(
				find(rows, KEY_SYNC_AUTO_NEW_AGENT),
				fallback.sync_auto_new_agent,
			),
			sync_check_update_on_start: decode_bool(
				find(rows, KEY_SYNC_CHECK_UPDATE_ON_START),
				fallback.sync_check_update_on_start,
			),
			sync_conflict_prompt: decode_bool(
				find(rows, KEY_SYNC_CONFLICT_PROMPT),
				fallback.sync_conflict_prompt,
			),
			sync_only_enabled: decode_bool(
				find(rows, KEY_SYNC_ONLY_ENABLED),
				fallback.sync_only_enabled,
			),
			net_proxy_mode: decode_i64(find(rows, KEY_NET_PROXY_MODE), fallback.net_proxy_mode),
			net_http_proxy: decode_string(find(rows, KEY_NET_HTTP_PROXY), &fallback.net_http_proxy),
			net_https_proxy: decode_string(
				find(rows, KEY_NET_HTTPS_PROXY),
				&fallback.net_https_proxy,
			),
			net_no_proxy: decode_string(find(rows, KEY_NET_NO_PROXY), &fallback.net_no_proxy),
			net_timeout_sec: decode_i64(find(rows, KEY_NET_TIMEOUT_SEC), fallback.net_timeout_sec),
			update_channel: decode_i64(find(rows, KEY_UPDATE_CHANNEL), fallback.update_channel),
		}
	}

	/// 把 Settings 拍平为 setting 表的 12 个键值对(cfg_key, cfg_value), 供 services::setting::
	/// save 逐一 upsert; bool 编码为 '0'/'1'、i64 编码为十进制字符串、String 原样, 与
	/// from_rows 的解码规则一一对称
	pub fn to_pairs(&self) -> Vec<(String, String)> {
		vec![
			(
				KEY_STORAGE_SKILL_DIR.to_string(),
				self.storage_skill_dir.clone(),
			),
			(
				KEY_STORAGE_MCP_DIR.to_string(),
				self.storage_mcp_dir.clone(),
			),
			(
				KEY_SYNC_AUTO_NEW_AGENT.to_string(),
				encode_bool(self.sync_auto_new_agent),
			),
			(
				KEY_SYNC_CHECK_UPDATE_ON_START.to_string(),
				encode_bool(self.sync_check_update_on_start),
			),
			(
				KEY_SYNC_CONFLICT_PROMPT.to_string(),
				encode_bool(self.sync_conflict_prompt),
			),
			(
				KEY_SYNC_ONLY_ENABLED.to_string(),
				encode_bool(self.sync_only_enabled),
			),
			(
				KEY_NET_PROXY_MODE.to_string(),
				self.net_proxy_mode.to_string(),
			),
			(KEY_NET_HTTP_PROXY.to_string(), self.net_http_proxy.clone()),
			(
				KEY_NET_HTTPS_PROXY.to_string(),
				self.net_https_proxy.clone(),
			),
			(KEY_NET_NO_PROXY.to_string(), self.net_no_proxy.clone()),
			(
				KEY_NET_TIMEOUT_SEC.to_string(),
				self.net_timeout_sec.to_string(),
			),
			(
				KEY_UPDATE_CHANNEL.to_string(),
				self.update_channel.to_string(),
			),
		]
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Settings::default() 应精确给出"设置契约"一节约定的每个默认值
	#[test]
	fn default_settings_matches_contract() {
		let d = Settings::default();
		assert_eq!(d.storage_skill_dir, "");
		assert_eq!(d.storage_mcp_dir, "");
		assert!(d.sync_auto_new_agent);
		assert!(d.sync_check_update_on_start);
		assert!(d.sync_conflict_prompt);
		assert!(!d.sync_only_enabled);
		assert_eq!(d.net_proxy_mode, 0);
		assert_eq!(d.net_http_proxy, "");
		assert_eq!(d.net_https_proxy, "");
		assert_eq!(d.net_no_proxy, "");
		assert_eq!(d.net_timeout_sec, 30);
		assert_eq!(d.update_channel, 0);
	}

	/// 构造一组覆盖全部 12 键、且每个字段都取非默认值的 SettingRow, 供 from_rows 完整性/
	/// 往返测试共用
	fn full_non_default_rows() -> Vec<SettingRow> {
		vec![
			SettingRow {
				cfg_key: KEY_STORAGE_SKILL_DIR.to_string(),
				cfg_value: "/data/skills".to_string(),
			},
			SettingRow {
				cfg_key: KEY_STORAGE_MCP_DIR.to_string(),
				cfg_value: "/data/mcp".to_string(),
			},
			SettingRow {
				cfg_key: KEY_SYNC_AUTO_NEW_AGENT.to_string(),
				cfg_value: "0".to_string(),
			},
			SettingRow {
				cfg_key: KEY_SYNC_CHECK_UPDATE_ON_START.to_string(),
				cfg_value: "0".to_string(),
			},
			SettingRow {
				cfg_key: KEY_SYNC_CONFLICT_PROMPT.to_string(),
				cfg_value: "0".to_string(),
			},
			SettingRow {
				cfg_key: KEY_SYNC_ONLY_ENABLED.to_string(),
				cfg_value: "1".to_string(),
			},
			SettingRow {
				cfg_key: KEY_NET_PROXY_MODE.to_string(),
				cfg_value: "2".to_string(),
			},
			SettingRow {
				cfg_key: KEY_NET_HTTP_PROXY.to_string(),
				cfg_value: "http://127.0.0.1:7890".to_string(),
			},
			SettingRow {
				cfg_key: KEY_NET_HTTPS_PROXY.to_string(),
				cfg_value: "http://127.0.0.1:7891".to_string(),
			},
			SettingRow {
				cfg_key: KEY_NET_NO_PROXY.to_string(),
				cfg_value: "localhost,127.0.0.1".to_string(),
			},
			SettingRow {
				cfg_key: KEY_NET_TIMEOUT_SEC.to_string(),
				cfg_value: "60".to_string(),
			},
			SettingRow {
				cfg_key: KEY_UPDATE_CHANNEL.to_string(),
				cfg_value: "1".to_string(),
			},
		]
	}

	// from_rows 应把一组完整的键值对逐一还原为对应字段, 不遗漏、不错位任何一个
	#[test]
	fn from_rows_restores_all_fields_from_full_rows() {
		let settings = Settings::from_rows(&full_non_default_rows());
		assert_eq!(settings.storage_skill_dir, "/data/skills");
		assert_eq!(settings.storage_mcp_dir, "/data/mcp");
		assert!(!settings.sync_auto_new_agent);
		assert!(!settings.sync_check_update_on_start);
		assert!(!settings.sync_conflict_prompt);
		assert!(settings.sync_only_enabled);
		assert_eq!(settings.net_proxy_mode, 2);
		assert_eq!(settings.net_http_proxy, "http://127.0.0.1:7890");
		assert_eq!(settings.net_https_proxy, "http://127.0.0.1:7891");
		assert_eq!(settings.net_no_proxy, "localhost,127.0.0.1");
		assert_eq!(settings.net_timeout_sec, 60);
		assert_eq!(settings.update_channel, 1);
	}

	// from_rows 在完全没有任何行(如首次运行, migrations 只建表不预置行)时应整体回落默认值
	#[test]
	fn from_rows_falls_back_to_default_when_rows_empty() {
		assert_eq!(Settings::from_rows(&[]), Settings::default());
	}

	// from_rows 对缺失某个键的场景应只让该字段回落默认, 其余按已有行还原, 二者不相互影响
	#[test]
	fn from_rows_falls_back_missing_key_to_default_field() {
		let rows = vec![SettingRow {
			cfg_key: KEY_NET_TIMEOUT_SEC.to_string(),
			cfg_value: "99".to_string(),
		}];
		let settings = Settings::from_rows(&rows);
		assert_eq!(settings.net_timeout_sec, 99, "已提供的键应按值还原");
		assert_eq!(settings.storage_skill_dir, "", "缺失键应回落默认值");
		assert!(settings.sync_auto_new_agent, "缺失键应回落默认值 true");
	}

	// from_rows 对非法 bool 值(既非 '0' 也非 '1')应回落该字段默认值, 不 panic
	#[test]
	fn from_rows_falls_back_illegal_bool_value_to_default() {
		let rows = vec![SettingRow {
			cfg_key: KEY_SYNC_ONLY_ENABLED.to_string(),
			cfg_value: "x".to_string(),
		}];
		let settings = Settings::from_rows(&rows);
		assert_eq!(
			settings.sync_only_enabled,
			Settings::default().sync_only_enabled,
			"非法 bool 值应回落默认值"
		);
	}

	// from_rows 对非法数字值(无法解析为 i64)应回落该字段默认值, 不 panic
	#[test]
	fn from_rows_falls_back_illegal_number_value_to_default() {
		let rows = vec![SettingRow {
			cfg_key: KEY_NET_TIMEOUT_SEC.to_string(),
			cfg_value: "x".to_string(),
		}];
		let settings = Settings::from_rows(&rows);
		assert_eq!(
			settings.net_timeout_sec,
			Settings::default().net_timeout_sec,
			"非法数字值应回落默认值"
		);
	}

	// to_pairs 应恰好输出全部 12 个键, 且 bool 编码为 '0'/'1'、i64 编码为十进制字符串
	#[test]
	fn to_pairs_encodes_exactly_twelve_keys_with_correct_encoding() {
		let settings = Settings {
			storage_skill_dir: "/data/skills".to_string(),
			storage_mcp_dir: "/data/mcp".to_string(),
			sync_auto_new_agent: false,
			sync_check_update_on_start: true,
			sync_conflict_prompt: false,
			sync_only_enabled: true,
			net_proxy_mode: 2,
			net_http_proxy: "http://127.0.0.1:7890".to_string(),
			net_https_proxy: "http://127.0.0.1:7891".to_string(),
			net_no_proxy: "localhost".to_string(),
			net_timeout_sec: 60,
			update_channel: 1,
		};
		let pairs = settings.to_pairs();
		assert_eq!(pairs.len(), 12, "应恰好输出 12 个键");

		let map: std::collections::BTreeMap<String, String> = pairs.into_iter().collect();
		assert_eq!(map[KEY_STORAGE_SKILL_DIR], "/data/skills");
		assert_eq!(map[KEY_STORAGE_MCP_DIR], "/data/mcp");
		assert_eq!(map[KEY_SYNC_AUTO_NEW_AGENT], "0");
		assert_eq!(map[KEY_SYNC_CHECK_UPDATE_ON_START], "1");
		assert_eq!(map[KEY_SYNC_CONFLICT_PROMPT], "0");
		assert_eq!(map[KEY_SYNC_ONLY_ENABLED], "1");
		assert_eq!(map[KEY_NET_PROXY_MODE], "2");
		assert_eq!(map[KEY_NET_HTTP_PROXY], "http://127.0.0.1:7890");
		assert_eq!(map[KEY_NET_HTTPS_PROXY], "http://127.0.0.1:7891");
		assert_eq!(map[KEY_NET_NO_PROXY], "localhost");
		assert_eq!(map[KEY_NET_TIMEOUT_SEC], "60");
		assert_eq!(map[KEY_UPDATE_CHANNEL], "1");
	}

	// 往返: 把 to_pairs 的输出重新装回 SettingRow 再 from_rows, 应精确还原原始 Settings
	// (覆盖非默认值场景, 逐字段互不相同, 足以暴露任何字段错位/编解码不对称的问题)
	#[test]
	fn from_rows_round_trips_through_to_pairs_for_non_default_settings() {
		let original = Settings::from_rows(&full_non_default_rows());
		let rows: Vec<SettingRow> = original
			.to_pairs()
			.into_iter()
			.map(|(cfg_key, cfg_value)| SettingRow { cfg_key, cfg_value })
			.collect();
		assert_eq!(Settings::from_rows(&rows), original);
	}

	// 往返: 默认 Settings 同样应经 to_pairs -> from_rows 精确还原, 与上一测试互补覆盖
	// "全部取默认值"这一边界
	#[test]
	fn from_rows_round_trips_through_to_pairs_for_default_settings() {
		let original = Settings::default();
		let rows: Vec<SettingRow> = original
			.to_pairs()
			.into_iter()
			.map(|(cfg_key, cfg_value)| SettingRow { cfg_key, cfg_value })
			.collect();
		assert_eq!(Settings::from_rows(&rows), original);
	}

	// Settings 序列化应使用 camelCase 字段名, 与前端 src/api/setting.ts Settings 类型对齐
	// (吸取 M3 ImportOutcome 契约不符的教训, 显式核对每个字段名)
	#[test]
	fn settings_serializes_with_camel_case_field_names() {
		let settings = Settings::default();
		let json = serde_json::to_value(&settings).unwrap();
		for key in [
			"storageSkillDir",
			"storageMcpDir",
			"syncAutoNewAgent",
			"syncCheckUpdateOnStart",
			"syncConflictPrompt",
			"syncOnlyEnabled",
			"netProxyMode",
			"netHttpProxy",
			"netHttpsProxy",
			"netNoProxy",
			"netTimeoutSec",
			"updateChannel",
		] {
			assert!(json.get(key).is_some(), "缺少字段: {key}");
		}
		assert!(
			json.get("storage_skill_dir").is_none(),
			"不应残留 snake_case 字段名"
		);

		let text = serde_json::to_string(&settings).unwrap();
		let back: Settings = serde_json::from_str(&text).unwrap();
		assert_eq!(back, settings);
	}
}
