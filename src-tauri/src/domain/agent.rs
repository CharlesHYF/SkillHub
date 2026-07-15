// 文件作用: Agent 领域类型 —— AgentKind/AgentScope 枚举与检测态实体(DetectedAgent/McpServerDef/
//           SkillRef/ActualState), 提供与 agent 表 INTEGER 列的互转(见 migrations/0001_init.sql)。
//           本任务追加 CodeBuddy(10, 纯 MCP, 见 SkillTarget::None)与 WorkBuddy(11, MCP+Skill
//           俱全)两款腾讯 AI 工具, 均追加在末尾, 不改动既有 9 款的判别值。
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// AI 工具种类: 对应 agent.agent_kind 列
/// 1-ClaudeCode, 2-ClaudeDesktop, 3-Cursor, 4-Windsurf, 5-Cline, 6-VsCode, 7-GeminiCli, 8-Codex,
/// 9-Hermes(M5 Task B1 追加), 10-CodeBuddy, 11-WorkBuddy(本任务追加, 均追加在末尾, 不改动既有
/// 9 款的判别值)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind {
	ClaudeCode,
	ClaudeDesktop,
	Cursor,
	Windsurf,
	Cline,
	VsCode,
	GeminiCli,
	Codex,
	Hermes,
	CodeBuddy,
	WorkBuddy,
}

impl AgentKind {
	/// 转为持久化编码(与 agent.agent_kind 列取值一致)
	pub fn code(&self) -> i64 {
		match self {
			AgentKind::ClaudeCode => 1,
			AgentKind::ClaudeDesktop => 2,
			AgentKind::Cursor => 3,
			AgentKind::Windsurf => 4,
			AgentKind::Cline => 5,
			AgentKind::VsCode => 6,
			AgentKind::GeminiCli => 7,
			AgentKind::Codex => 8,
			AgentKind::Hermes => 9,
			AgentKind::CodeBuddy => 10,
			AgentKind::WorkBuddy => 11,
		}
	}

	/// 人类可读展示名, 供 UI 直接展示(如 Agent 列表/侧栏)
	pub fn label(&self) -> &'static str {
		match self {
			AgentKind::ClaudeCode => "Claude Code",
			AgentKind::ClaudeDesktop => "Claude Desktop",
			AgentKind::Cursor => "Cursor",
			AgentKind::Windsurf => "Windsurf",
			AgentKind::Cline => "Cline",
			AgentKind::VsCode => "VS Code",
			AgentKind::GeminiCli => "Gemini CLI",
			AgentKind::Codex => "Codex",
			AgentKind::Hermes => "Hermes",
			AgentKind::CodeBuddy => "CodeBuddy",
			AgentKind::WorkBuddy => "WorkBuddy",
		}
	}

	/// 由持久化编码还原枚举; 未知值(脏数据, 含列默认值 0)兜底为最小合法编码 ClaudeCode(1),
	/// 与 ResourceType/SourceType::from_i64 的兜底惯例一致
	pub fn from_code(value: i64) -> Self {
		match value {
			1 => AgentKind::ClaudeCode,
			2 => AgentKind::ClaudeDesktop,
			3 => AgentKind::Cursor,
			4 => AgentKind::Windsurf,
			5 => AgentKind::Cline,
			6 => AgentKind::VsCode,
			7 => AgentKind::GeminiCli,
			8 => AgentKind::Codex,
			9 => AgentKind::Hermes,
			10 => AgentKind::CodeBuddy,
			11 => AgentKind::WorkBuddy,
			_ => AgentKind::ClaudeCode,
		}
	}
}

/// Agent 作用域: 对应 agent.scope 列
/// 0-全局, 1-项目
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentScope {
	Global,
	Project,
}

impl AgentScope {
	/// 由数据库 INTEGER 值还原枚举; 未知值兜底为列默认值 Global(0)
	pub fn from_i64(value: i64) -> Self {
		match value {
			1 => AgentScope::Project,
			_ => AgentScope::Global,
		}
	}
}

impl From<AgentScope> for i64 {
	fn from(value: AgentScope) -> i64 {
		match value {
			AgentScope::Global => 0,
			AgentScope::Project => 1,
		}
	}
}

/// 探测到的一个本机 Agent 实例(未落库前的领域态, 由各 AgentAdapter::detect 产出)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAgent {
	pub kind: AgentKind,
	pub name: String,
	pub config_path: String,
	pub scope: AgentScope,
	pub online: bool,
}

/// 单个 MCP 服务器定义(Agent 配置文件里一条 mcpServers 条目的归一化形态)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDef {
	pub name: String,
	pub command: Option<String>,
	pub args: Vec<String>,
	pub env: BTreeMap<String, String>,
	pub url: Option<String>,
}

/// 单个 Skill 引用(名称 + 版本)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRef {
	pub name: String,
	pub version: String,
}

/// 某 Agent 当前的实际态: 从其配置文件读出的 MCP/Skill 清单, 供与期望态 diff(见 domain::sync)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActualState {
	pub mcp: Vec<McpServerDef>,
	pub skills: Vec<SkillRef>,
}

#[cfg(test)]
mod tests {
	use super::*;

	const ALL_KINDS: [AgentKind; 11] = [
		AgentKind::ClaudeCode,
		AgentKind::ClaudeDesktop,
		AgentKind::Cursor,
		AgentKind::Windsurf,
		AgentKind::Cline,
		AgentKind::VsCode,
		AgentKind::GeminiCli,
		AgentKind::Codex,
		AgentKind::Hermes,
		AgentKind::CodeBuddy,
		AgentKind::WorkBuddy,
	];

	// AgentKind: 11 个已知编码应与枚举变体精确往返(code -> from_code -> 原变体), 含本任务
	// 追加的 CodeBuddy(10)/WorkBuddy(11)
	#[test]
	fn agent_kind_code_round_trips_all_eleven_variants() {
		for (idx, kind) in ALL_KINDS.iter().enumerate() {
			let expected_code = (idx + 1) as i64;
			assert_eq!(kind.code(), expected_code);
			assert_eq!(AgentKind::from_code(expected_code), *kind);
		}
	}

	// AgentKind: label 应为非空且两两不同, 供 UI 直接展示区分
	#[test]
	fn agent_kind_label_is_non_empty_and_unique() {
		let mut labels: Vec<&str> = ALL_KINDS.iter().map(|k| k.label()).collect();
		labels.sort_unstable();
		labels.dedup();
		assert_eq!(labels.len(), ALL_KINDS.len(), "label 应两两不同");
		assert!(ALL_KINDS.iter().all(|k| !k.label().is_empty()));
	}

	// AgentKind: 未知编码(含列默认值 0)应兜底为最小合法编码 ClaudeCode, 不 panic
	#[test]
	fn agent_kind_from_code_unknown_value_falls_back_to_claude_code() {
		assert_eq!(AgentKind::from_code(0), AgentKind::ClaudeCode);
		assert_eq!(AgentKind::from_code(99), AgentKind::ClaudeCode);
	}

	// AgentScope: 已知值双向互转应精确对应枚举变体
	#[test]
	fn agent_scope_from_i64_known_values_round_trip() {
		assert_eq!(AgentScope::from_i64(0), AgentScope::Global);
		assert_eq!(AgentScope::from_i64(1), AgentScope::Project);
		assert_eq!(i64::from(AgentScope::Global), 0);
		assert_eq!(i64::from(AgentScope::Project), 1);
	}

	// AgentScope: 未知值(脏数据)应兜底为列默认值 Global(0), 不 panic
	#[test]
	fn agent_scope_from_i64_unknown_value_falls_back_to_global() {
		assert_eq!(AgentScope::from_i64(-1), AgentScope::Global);
		assert_eq!(AgentScope::from_i64(99), AgentScope::Global);
	}

	// DetectedAgent: 序列化应使用 camelCase 字段名(前端消费契约), configPath 而非 config_path
	#[test]
	fn detected_agent_serializes_as_camel_case() {
		let agent = DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: "/home/demo/.claude.json".to_string(),
			scope: AgentScope::Global,
			online: true,
		};
		let json = serde_json::to_value(&agent).unwrap();
		assert_eq!(json["configPath"], "/home/demo/.claude.json");
		assert!(json.get("config_path").is_none());
	}

	// ActualState: 内嵌 McpServerDef/SkillRef 应整体序列化为 camelCase, 且能反序列化还原(JSON 往返)
	#[test]
	fn actual_state_round_trips_through_json() {
		let mut env = BTreeMap::new();
		env.insert("API_KEY".to_string(), "xxx".to_string());
		let state = ActualState {
			mcp: vec![McpServerDef {
				name: "filesystem".to_string(),
				command: Some("npx".to_string()),
				args: vec!["-y".to_string(), "server-fs".to_string()],
				env,
				url: None,
			}],
			skills: vec![SkillRef {
				name: "charles-coding".to_string(),
				version: "1.0.0".to_string(),
			}],
		};
		let json = serde_json::to_string(&state).unwrap();
		assert!(json.contains("\"command\""));
		let back: ActualState = serde_json::from_str(&json).unwrap();
		assert_eq!(back, state);
	}
}
