// 文件作用: AgentAdapter 抽象 —— 统一各 AI 工具(Claude Code/Desktop/Cursor/...)的探测/读态/应用接口,
//           并提供全量适配器注册表 all_adapters; Task 3 接入 6 款 JSON mcpServers 工具,
//           Task 4 追加 VS Code(复用 JsonMcpAdapter, servers_key 为 "servers")与
//           Codex(TOML 配置, 单独实现 CodexAdapter), 累计 8 款; Task 5 给 8 款工具逐一接上
//           Skill 落地形态(SkillTarget), 映射关系见 json_mcp_agent_configs 与 all_adapters
//           内 VsCode/Codex 的构造实参。
// 创建日期: 2026-07-09

pub mod codex;
pub mod json_mcp;
pub mod skill_target;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::agent::{ActualState, AgentKind, DetectedAgent};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DiffPlan, ItemOutcome};
use codex::CodexAdapter;
use json_mcp::JsonMcpAdapter;
use skill_target::SkillTarget;

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
/// 生产环境由调用方传入 `dirs::home_dir()`)。Task 3 接入的 6 款 JSON mcpServers 工具在前,
/// Task 4 追加的 VS Code(仍是 JsonMcpAdapter, 只是 servers_key 换成 "servers")与
/// Codex(TOML, CodexAdapter)在后, 累计 8 款; 每款工具的 SkillTarget(Task 5)按工具种类在此
/// 逐一指定 —— 6 款走 json_mcp_agent_configs 表里携带的 SkillTarget, VsCode/Codex 因构造
/// 逻辑单独写在本函数里, 也各自单独指定。
pub fn all_adapters(home: &Path) -> Vec<Box<dyn AgentAdapter>> {
	let mut adapters: Vec<Box<dyn AgentAdapter>> = json_mcp_agent_configs()
		.into_iter()
		.map(
			|(kind, rel_candidates, skill_target)| -> Box<dyn AgentAdapter> {
				Box::new(JsonMcpAdapter::new(
					kind,
					home.to_path_buf(),
					rel_candidates,
					"mcpServers",
					skill_target,
				))
			},
		)
		.collect();

	adapters.push(Box::new(JsonMcpAdapter::new(
		AgentKind::VsCode,
		home.to_path_buf(),
		vscode_config_candidates(),
		"servers",
		SkillTarget::RulesDir {
			dir: PathBuf::from(".github/instructions"),
			ext: "md".to_string(),
		},
	)));
	adapters.push(Box::new(CodexAdapter::new(
		home.to_path_buf(),
		SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
	)));

	adapters
}

/// 六款"顶层 JSON 对象里挂一个 mcpServers 字典"工具各自的候选配置路径(相对家目录)与 Skill
/// 落地形态(SkillTarget); 同一工具的多条候选路径按 macOS/Windows/Linux 罗列, 运行时取第一个
/// 实际存在的(见 JsonMcpAdapter::detect), 兼顾工具版本与操作系统差异导致的路径漂移。
/// SkillTarget 与候选路径合并维护在同一张表里, 因为二者都是"给定 AgentKind 该怎么构造
/// JsonMcpAdapter"这一件事的两个维度, 分开维护反而容易在新增/调整工具时漏改一处。
/// 本机(macOS)只验证过每个工具的 macOS 分支, Windows/Linux 分支按各工具官方文档路径预置,
/// 结构与已验证的 macOS 分支一致但未实机核对。
fn json_mcp_agent_configs() -> Vec<(AgentKind, Vec<PathBuf>, SkillTarget)> {
	vec![
		(
			AgentKind::ClaudeCode,
			vec![PathBuf::from(".claude.json")],
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		),
		(
			AgentKind::ClaudeDesktop,
			vec![
				PathBuf::from("Library/Application Support/Claude/claude_desktop_config.json"),
				PathBuf::from("AppData/Roaming/Claude/claude_desktop_config.json"),
				PathBuf::from(".config/Claude/claude_desktop_config.json"),
			],
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		),
		(
			AgentKind::Cursor,
			vec![PathBuf::from(".cursor/mcp.json")],
			SkillTarget::RulesDir {
				dir: PathBuf::from(".cursor/rules"),
				ext: "mdc".to_string(),
			},
		),
		(
			AgentKind::Windsurf,
			vec![PathBuf::from(".codeium/windsurf/mcp_config.json")],
			SkillTarget::RulesDir {
				dir: PathBuf::from(".windsurf/rules"),
				ext: "md".to_string(),
			},
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
			SkillTarget::RulesDir {
				dir: PathBuf::from(".clinerules"),
				ext: "md".to_string(),
			},
		),
		(
			AgentKind::GeminiCli,
			vec![PathBuf::from(".gemini/settings.json")],
			SkillTarget::InstructionsFile(PathBuf::from("GEMINI.md")),
		),
	]
}

/// VS Code 用户级 MCP 配置候选路径(相对家目录); VS Code 把 MCP 服务器配置放在独立的
/// `mcp.json` 里, 顶层直接挂 `servers` 字典, 与其余 6 款工具的 `mcpServers` 键名不同,
/// 故复用 JsonMcpAdapter 时需单独传入 servers_key="servers"(见 all_adapters)。
/// 注意: VS Code 也支持把 MCP 服务器写进 `settings.json` 的 `mcp.servers` 嵌套字段, 但那是
/// "顶层对象套一层 mcp 再挂 servers", 与 JsonMcpAdapter"顶层直接挂字典"的假设不符, 本任务
/// 只覆盖独立 mcp.json 形态, settings.json 内嵌形态留待后续任务专门处理。
/// 候选按 macOS/Windows/Linux 罗列, 运行时取第一个实际存在的(本机 macOS 已验证,
/// Windows/Linux 分支按官方文档路径预置未实机核对, 与 json_mcp_agent_configs 的惯例一致)。
fn vscode_config_candidates() -> Vec<PathBuf> {
	vec![
		PathBuf::from("Library/Application Support/Code/User/mcp.json"),
		PathBuf::from("AppData/Roaming/Code/User/mcp.json"),
		PathBuf::from(".config/Code/User/mcp.json"),
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

	// all_adapters 累计应接入 8 款工具: Task 3 的 6 款 JSON mcpServers 在前, Task 4 追加的
	// VS Code(仍是 JsonMcpAdapter)与 Codex(CodexAdapter)按注册顺序追加在后; 数量与种类应与
	// 配置表逐一对应, 且每个都应同时声明支持 Mcp 与 Skill(Skill 落地读取已接入, 写入留 Task 7)
	#[test]
	fn all_adapters_registers_eight_tools_with_correct_kinds_and_support() {
		let home = PathBuf::from("/tmp/skillhub-test-home");
		let adapters = all_adapters(&home);

		let mut expected_kinds: Vec<AgentKind> = json_mcp_agent_configs()
			.into_iter()
			.map(|(kind, _, _)| kind)
			.collect();
		expected_kinds.push(AgentKind::VsCode);
		expected_kinds.push(AgentKind::Codex);

		assert_eq!(adapters.len(), 8, "Task 4 起应累计接入 8 款工具");
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
		for (kind, rel_candidates, _) in json_mcp_agent_configs() {
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

	// all_adapters 里的 VS Code 条目应命中候选路径下的 mcp.json(顶层 servers 字典)fixture,
	// 并解析出 command 型与 url 型两条服务器; 验证复用 JsonMcpAdapter 时 servers_key="servers"
	// 确实生效(与其余 6 款工具的 "mcpServers" 键名不同)
	#[test]
	fn all_adapters_vscode_entry_detects_and_reads_servers_key_fixture() {
		let dir = tempfile::tempdir().unwrap();
		let rel = PathBuf::from("Library/Application Support/Code/User/mcp.json");
		let abs = dir.path().join(&rel);
		std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
		std::fs::write(
			&abs,
			r#"{"servers":{"foo":{"command":"node","args":["x"],"env":{"K":"V"}},"bar":{"type":"http","url":"http://localhost:1"}}}"#,
		)
		.unwrap();

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::VsCode)
			.expect("VsCode 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "VsCode 应命中 fixture");
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 2, "VsCode 应解析出 2 条 McpServerDef");
		assert!(state
			.mcp
			.iter()
			.any(|s| s.name == "foo" && s.command == Some("node".to_string())));
		assert!(state
			.mcp
			.iter()
			.any(|s| s.name == "bar" && s.url == Some("http://localhost:1".to_string())));
	}

	// all_adapters 里的 Codex 条目应命中 .codex/config.toml([mcp_servers.*] 表)fixture,
	// 解析出 command 型服务器一条; 验证 CodexAdapter 已按固定相对路径正确接入
	#[test]
	fn all_adapters_codex_entry_detects_and_reads_config_toml_fixture() {
		let dir = tempfile::tempdir().unwrap();
		let rel = PathBuf::from(".codex/config.toml");
		let abs = dir.path().join(&rel);
		std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
		std::fs::write(
			&abs,
			"[mcp_servers.foo]\ncommand = \"node\"\nargs = [\"x\"]\nenv = { K = \"V\" }\n",
		)
		.unwrap();

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::Codex)
			.expect("Codex 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "Codex 应命中 fixture");
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 1, "Codex 应解析出 1 条 McpServerDef");
		let foo = &state.mcp[0];
		assert_eq!(foo.name, "foo");
		assert_eq!(foo.command, Some("node".to_string()));
		assert_eq!(foo.args, vec!["x".to_string()]);
	}

	// 按 target 的落地形态在 home 下写一份最小 fixture, 返回期望读出的 Skill 名; 供下方
	// "验证 all_adapters 里每款工具的 SkillTarget 映射均生效"系列测试复用, 覆盖三种形态
	fn write_skill_fixture(home: &Path, target: &SkillTarget) -> String {
		let name = "demo-skill";
		match target {
			SkillTarget::ClaudeSkillsDir(rel) => {
				let skill_dir = home.join(rel).join(name);
				std::fs::create_dir_all(&skill_dir).unwrap();
				std::fs::write(skill_dir.join("SKILL.md"), "---\nversion: 9.9.9\n---\n").unwrap();
			}
			SkillTarget::RulesDir { dir, ext } => {
				let rules_dir = home.join(dir);
				std::fs::create_dir_all(&rules_dir).unwrap();
				std::fs::write(rules_dir.join(format!("{name}.{ext}")), "规则内容").unwrap();
			}
			SkillTarget::InstructionsFile(rel) => {
				std::fs::write(
					home.join(rel),
					format!(
						"<!-- skillhub:start:{name}@9.9.9 -->\n内容\n<!-- skillhub:end:{name} -->\n"
					),
				)
				.unwrap();
			}
		}
		name.to_string()
	}

	// all_adapters 里 6 款 JSON mcp 工具各自声明的 SkillTarget 均应生效: 分别在其 mcp 配置
	// 候选路径与 SkillTarget 描述的落地位置各放一份最小 fixture, detect+read_state 应从声明
	// 的 SkillTarget 读出 1 条 Skill(覆盖 ClaudeSkillsDir/RulesDir/InstructionsFile 三种形态,
	// 具体每款工具映射到哪种见 json_mcp_agent_configs)
	#[test]
	fn all_adapters_json_mcp_entries_read_skills_from_declared_skill_target() {
		for (kind, rel_candidates, skill_target) in json_mcp_agent_configs() {
			let dir = tempfile::tempdir().unwrap();
			let config_abs = dir.path().join(&rel_candidates[0]);
			std::fs::create_dir_all(config_abs.parent().unwrap()).unwrap();
			std::fs::write(&config_abs, r#"{"mcpServers":{}}"#).unwrap();
			let expected_name = write_skill_fixture(dir.path(), &skill_target);

			let adapters = all_adapters(dir.path());
			let adapter = adapters
				.iter()
				.find(|a| a.kind() == kind)
				.unwrap_or_else(|| panic!("{kind:?} 应已注册"));

			let detected = adapter.detect();
			assert_eq!(detected.len(), 1, "{kind:?} 应命中 mcp fixture");

			let state = adapter.read_state(&detected[0]).unwrap();
			assert_eq!(
				state.skills.len(),
				1,
				"{kind:?} 应从声明的 SkillTarget 读出 1 个 Skill"
			);
			assert_eq!(state.skills[0].name, expected_name);
		}
	}

	// all_adapters 里的 VS Code 条目应从声明的 SkillTarget(RulesDir(".github/instructions",
	// "md"))读出已装 Skill, 与其 mcp.json(servers 字典)fixture 各自独立解析
	#[test]
	fn all_adapters_vscode_entry_reads_skills_from_declared_skill_target() {
		let dir = tempfile::tempdir().unwrap();
		let rel = PathBuf::from("Library/Application Support/Code/User/mcp.json");
		let abs = dir.path().join(&rel);
		std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
		std::fs::write(&abs, r#"{"servers":{}}"#).unwrap();
		let expected_name = write_skill_fixture(
			dir.path(),
			&SkillTarget::RulesDir {
				dir: PathBuf::from(".github/instructions"),
				ext: "md".to_string(),
			},
		);

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::VsCode)
			.expect("VsCode 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "VsCode 应命中 mcp fixture");
		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, expected_name);
	}

	// all_adapters 里的 Codex 条目应从声明的 SkillTarget(InstructionsFile("AGENTS.md"))读出
	// 已装 Skill, 与其 .codex/config.toml([mcp_servers.*] 表)fixture 各自独立解析
	#[test]
	fn all_adapters_codex_entry_reads_skills_from_declared_skill_target() {
		let dir = tempfile::tempdir().unwrap();
		let rel = PathBuf::from(".codex/config.toml");
		let abs = dir.path().join(&rel);
		std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
		std::fs::write(&abs, "model = \"gpt-5\"\n").unwrap();
		let expected_name = write_skill_fixture(
			dir.path(),
			&SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::Codex)
			.expect("Codex 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "Codex 应命中 mcp fixture");
		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, expected_name);
	}
}
