// 文件作用: AgentAdapter 抽象 —— 统一各 AI 工具(Claude Code/Desktop/Cursor/...)的探测/读态/应用接口,
//           并提供全量适配器注册表 all_adapters; Task 3 起接入具体适配器(本任务: 6 款 JSON
//           mcpServers 工具), VS Code/Codex(Task 4)与 Skill 落地(Task 5)陆续补齐
// 创建日期: 2026-07-09

pub mod json_mcp;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::agent::{ActualState, AgentKind, DetectedAgent};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DiffPlan, ItemOutcome};
use json_mcp::JsonMcpAdapter;

/// 统一封装"探测本机某类 AI 工具 + 读取其配置实际态 + 把差异计划应用回配置文件"的能力。
/// 每种 AgentKind(Claude Code/Cursor/...) 各实现一个适配器; 方法均不含泛型/`Self` 返回值,
/// trait 对象安全, 供 all_adapters 统一装箱为 `Vec<Box<dyn AgentAdapter>>`。
pub trait AgentAdapter {
	/// 本适配器对应的 Agent 种类
	fn kind(&self) -> AgentKind;

	/// 本适配器是否支持同步给定资源类型(如某工具暂不支持 MCP)
	fn supports(&self, ty: ResourceType) -> bool;

	/// 探测本机是否安装/配置了该工具; 可能发现多个实例(全局 + 若干项目级)
	fn detect(&self) -> Vec<DetectedAgent>;

	/// 读取某个已探测到的 Agent 实例当前的实际态(已配置的 MCP/Skill 清单)
	fn read_state(&self, agent: &DetectedAgent) -> Result<ActualState>;

	/// 把差异计划应用到该 Agent 的配置文件, 返回每一项的执行结果
	fn apply(&self, agent: &DetectedAgent, plan: &DiffPlan) -> Result<Vec<ItemOutcome>>;
}

/// 全量适配器注册表; `home` 为家目录(测试时可注入临时目录, 避免探测逻辑触碰真实机器配置;
/// 生产环境由调用方传入 `dirs::home_dir()`)。Task 3 接入 6 款 JSON mcpServers 适配器;
/// VS Code/Codex(Task 4)与 Skill 落地(Task 5)陆续 push 进本函数。
pub fn all_adapters(home: &Path) -> Vec<Box<dyn AgentAdapter>> {
	json_mcp_agent_configs()
		.into_iter()
		.map(|(kind, rel_candidates)| -> Box<dyn AgentAdapter> {
			Box::new(JsonMcpAdapter::new(
				kind,
				home.to_path_buf(),
				rel_candidates,
				"mcpServers",
			))
		})
		.collect()
}

/// 六款"顶层 JSON 对象里挂一个 mcpServers 字典"工具各自的候选配置路径(相对家目录); 同一工具
/// 的多条候选按 macOS/Windows/Linux 罗列, 运行时取第一个实际存在的(见 JsonMcpAdapter::detect),
/// 兼顾工具版本与操作系统差异导致的路径漂移。本机(macOS)只验证过每个工具的 macOS 分支,
/// Windows/Linux 分支按各工具官方文档路径预置, 结构与已验证的 macOS 分支一致但未实机核对。
fn json_mcp_agent_configs() -> Vec<(AgentKind, Vec<PathBuf>)> {
	vec![
		(AgentKind::ClaudeCode, vec![PathBuf::from(".claude.json")]),
		(
			AgentKind::ClaudeDesktop,
			vec![
				PathBuf::from("Library/Application Support/Claude/claude_desktop_config.json"),
				PathBuf::from("AppData/Roaming/Claude/claude_desktop_config.json"),
				PathBuf::from(".config/Claude/claude_desktop_config.json"),
			],
		),
		(AgentKind::Cursor, vec![PathBuf::from(".cursor/mcp.json")]),
		(
			AgentKind::Windsurf,
			vec![PathBuf::from(".codeium/windsurf/mcp_config.json")],
		),
		(
			AgentKind::Cline,
			vec![
				PathBuf::from(
					"Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
				),
				PathBuf::from(
					"AppData/Roaming/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
				),
				PathBuf::from(
					".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
				),
			],
		),
		(
			AgentKind::GeminiCli,
			vec![PathBuf::from(".gemini/settings.json")],
		),
	]
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::*;
	use crate::domain::agent::AgentScope;

	/// 占位实现: 仅用于验证 AgentAdapter trait 对象安全 —— 可被具体类型实现并装箱进
	/// `Vec<Box<dyn AgentAdapter>>`, 且各方法均可正常调用
	struct FakeAdapter;

	impl AgentAdapter for FakeAdapter {
		fn kind(&self) -> AgentKind {
			AgentKind::ClaudeCode
		}

		fn supports(&self, ty: ResourceType) -> bool {
			matches!(ty, ResourceType::Skill)
		}

		fn detect(&self) -> Vec<DetectedAgent> {
			Vec::new()
		}

		fn read_state(&self, _agent: &DetectedAgent) -> Result<ActualState> {
			Ok(ActualState {
				mcp: Vec::new(),
				skills: Vec::new(),
			})
		}

		fn apply(&self, _agent: &DetectedAgent, _plan: &DiffPlan) -> Result<Vec<ItemOutcome>> {
			Ok(Vec::new())
		}
	}

	// AgentAdapter 应对象安全: 可装箱为 trait object 并逐一调用全部方法
	#[test]
	fn agent_adapter_trait_is_object_safe() {
		let adapters: Vec<Box<dyn AgentAdapter>> = vec![Box::new(FakeAdapter)];
		let adapter = &adapters[0];
		assert_eq!(adapter.kind(), AgentKind::ClaudeCode);
		assert!(adapter.supports(ResourceType::Skill));
		assert!(!adapter.supports(ResourceType::Mcp));
		assert!(adapter.detect().is_empty());

		let probe = DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: "/tmp/does-not-matter".to_string(),
			scope: AgentScope::Global,
			online: true,
		};
		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
		assert!(state.skills.is_empty());

		let outcomes = adapter
			.apply(&probe, &DiffPlan { items: Vec::new() })
			.unwrap();
		assert!(outcomes.is_empty());
	}

	// all_adapters 从 Task 3 起接入 6 款 JSON mcpServers 适配器(VS Code/Codex 留 Task 4);
	// 数量与种类应与配置表逐一对应, 且每个都应同时声明支持 Mcp 与 Skill(Skill 落地留 Task 5)
	#[test]
	fn all_adapters_registers_six_json_mcp_tools_with_correct_kinds_and_support() {
		let home = PathBuf::from("/tmp/skillhub-test-home");
		let adapters = all_adapters(&home);

		let expected_kinds: Vec<AgentKind> = json_mcp_agent_configs()
			.into_iter()
			.map(|(kind, _)| kind)
			.collect();
		assert_eq!(
			adapters.len(),
			6,
			"本任务应恰好接入 6 款 JSON mcpServers 工具"
		);
		let actual_kinds: Vec<AgentKind> = adapters.iter().map(|a| a.kind()).collect();
		assert_eq!(actual_kinds, expected_kinds, "注册顺序与种类应与配置表一致");

		for adapter in &adapters {
			assert!(adapter.supports(ResourceType::Mcp));
			assert!(adapter.supports(ResourceType::Skill));
		}
	}

	// all_adapters 接入的适配器应是"真家伙": 在注入的 home 下按配置表的候选路径落地 fixture 后,
	// 对应适配器的 detect+read_state 应端到端命中并解析出 command/url 两种形态, 覆盖 6 工具各自路径
	#[test]
	fn all_adapters_json_mcp_entries_detect_and_read_real_candidate_paths() {
		for (kind, rel_candidates) in json_mcp_agent_configs() {
			let dir = tempfile::tempdir().unwrap();
			let rel = rel_candidates[0].clone();
			let abs = dir.path().join(&rel);
			std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
			std::fs::write(
				&abs,
				r#"{"mcpServers":{"foo":{"command":"node","args":["x"],"env":{"K":"V"}},"bar":{"url":"http://localhost:1"}}}"#,
			)
			.unwrap();

			let adapters = all_adapters(dir.path());
			let adapter = adapters
				.iter()
				.find(|a| a.kind() == kind)
				.unwrap_or_else(|| panic!("{kind:?} 应已注册"));

			let detected = adapter.detect();
			assert_eq!(detected.len(), 1, "{kind:?} 应命中 fixture");

			let state = adapter.read_state(&detected[0]).unwrap();
			assert_eq!(state.mcp.len(), 2, "{kind:?} 应解析出 2 条 McpServerDef");
			assert!(state
				.mcp
				.iter()
				.any(|s| s.name == "foo" && s.command == Some("node".to_string())));
			assert!(state
				.mcp
				.iter()
				.any(|s| s.name == "bar" && s.url == Some("http://localhost:1".to_string())));
		}
	}
}
