// 文件作用: AgentAdapter 抽象 —— 统一各 AI 工具(Claude Code/Desktop/Cursor/...)的探测/读态/应用接口,
//           并提供全量适配器注册表 all_adapters; Task 3 接入 6 款 JSON mcpServers 工具,
//           Task 4 追加 VS Code(复用 JsonMcpAdapter, servers_key 为 "servers")与
//           Codex(TOML 配置, 单独实现 CodexAdapter), 累计 8 款; Task 5 给 8 款工具逐一接上
//           Skill 落地形态(SkillTarget), 映射关系见 json_mcp_agent_configs 与 all_adapters
//           内 VsCode/Codex 的构造实参。M5 Task B1 追加 Hermes(YAML 配置, 单独实现
//           HermesAdapter), 累计 9 款, Skill 落地形态复用 ClaudeSkillsDir(".hermes/skills")。
//           本任务追加腾讯 CodeBuddy 与 WorkBuddy 两款工具, 累计 11 款; 二者配置文件均为 JSON
//           mcpServers 形态, 故直接追加进 json_mcp_agent_configs 表复用 JsonMcpAdapter, 不新增
//           适配器实现文件 —— CodeBuddy 官方文档未提供 Skill 落地约定, SkillTarget 传
//           SkillTarget::None(纯 MCP, supports(Skill)=false); WorkBuddy 的 Skill(.workbuddy/
//           skills)与 MCP(.workbuddy/mcp.json)均已核实, 按 ClaudeSkillsDir 形态正常接入。
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

pub mod codex;
pub mod hermes;
pub mod json_mcp;
pub mod skill_target;
pub mod util;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::agent::{ActualState, AgentKind, DetectedAgent};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DiffPlanRespVO, ItemOutcome};
use codex::CodexAdapter;
use hermes::HermesAdapter;
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
	fn apply(&self, agent: &DetectedAgent, plan: &DiffPlanRespVO) -> Result<Vec<ItemOutcome>>;

	/// 把本适配器对应工具里名为 `name` 的已装 Skill 内容导出到 `dest_dir`(统一整理为"含
	/// SKILL.md 的目录"形态), 供 M6 Task BE-2(services::agent_import, 从已检测 Agent 反向导入
	/// 已装 Skill/MCP 到本地库)复用 —— read_state().skills 只还原出 SkillRef{name,version}
	/// 两个元数据字段, 不足以取到真正可落地的内容, 必须由本方法按各工具的 SkillTarget 落地形态
	/// 回到磁盘上取原始内容(见 SkillTarget::export_skill, 各实现均只是转发给自身持有的
	/// skill_target 字段, 不重复实现)。该 Skill 名不存在/内容缺失时返回 `Ok(false)`(不算错误,
	/// 调用方应静默跳过), 成功导出返回 `Ok(true)`
	fn export_skill(&self, name: &str, dest_dir: &Path) -> Result<bool>;
}

/// 全量适配器注册表; `home` 为家目录(测试时可注入临时目录, 避免探测逻辑触碰真实机器配置;
/// 生产环境由调用方传入 `dirs::home_dir()`)。Task 3 接入的 6 款 JSON mcpServers 工具在前,
/// Task 4 追加的 VS Code(仍是 JsonMcpAdapter, 只是 servers_key 换成 "servers")与
/// Codex(TOML, CodexAdapter)在后, 累计 8 款; 每款工具的 SkillTarget(Task 5)按工具种类在此
/// 逐一指定 —— 6 款走 json_mcp_agent_configs 表里携带的 SkillTarget, VsCode/Codex 因构造
/// 逻辑单独写在本函数里, 也各自单独指定。M5 Task B1 在末尾追加 Hermes(YAML, HermesAdapter),
/// 累计 9 款; Skill 落地形态固定为 ClaudeSkillsDir(".hermes/skills"), 与 Claude 家族同形态。
/// 本任务追加的 CodeBuddy/WorkBuddy 同样走 json_mcp_agent_configs 表复用 JsonMcpAdapter(与前 6
/// 款+VsCode 同一套构造逻辑), 累计 11 款; 不新增 push 语句, 只需把两条目加进该表即可。
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
	adapters.push(Box::new(HermesAdapter::new(
		home.to_path_buf(),
		SkillTarget::ClaudeSkillsDir(PathBuf::from(".hermes/skills")),
	)));

	adapters
}

/// "顶层 JSON 对象里挂一个 mcpServers 字典"这一形态工具各自的候选配置路径(相对家目录)与 Skill
/// 落地形态(SkillTarget); 同一工具的多条候选路径按 macOS/Windows/Linux 或"推荐/废弃/旧版"
/// 罗列, 运行时取第一个实际存在的(见 JsonMcpAdapter::detect), 兼顾工具版本与操作系统差异
/// 导致的路径漂移。SkillTarget 与候选路径合并维护在同一张表里, 因为二者都是"给定 AgentKind
/// 该怎么构造 JsonMcpAdapter"这一件事的两个维度, 分开维护反而容易在新增/调整工具时漏改一处。
/// 本机(macOS)只验证过每个工具的 macOS 分支, Windows/Linux 分支按各工具官方文档路径预置,
/// 结构与已验证的 macOS 分支一致但未实机核对。
/// 本任务新增的 CodeBuddy/WorkBuddy 两款均为腾讯出品、配置文件同为本表覆盖的 JSON mcpServers
/// 形态: CodeBuddy 候选路径与优先级(`.codebuddy/.mcp.json` 推荐 > `.codebuddy/mcp.json` 已
/// 废弃 > `.codebuddy.json` 旧版)均已通过官方文档(https://www.codebuddy.ai/docs/zh/cli/mcp)
/// 核实, 其官方文档未提供任何本地 Skill/rules 目录约定, SkillTarget 传 SkillTarget::None
/// 占位(纯 MCP, 见该类型文档与 JsonMcpAdapter::supports); WorkBuddy 的 MCP 路径
/// (`.workbuddy/mcp.json`, 用户级)与 Skill 路径(`.workbuddy/skills`)均已通过官方文档
/// (https://www.codebuddy.cn/docs/workbuddy/.../MCP-Guide)核实, 按常规 ClaudeSkillsDir
/// 形态接入, 与 Claude/Hermes 家族同形态。
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
		(
			AgentKind::CodeBuddy,
			vec![
				PathBuf::from(".codebuddy/.mcp.json"),
				PathBuf::from(".codebuddy/mcp.json"),
				PathBuf::from(".codebuddy.json"),
			],
			SkillTarget::None,
		),
		(
			AgentKind::WorkBuddy,
			vec![PathBuf::from(".workbuddy/mcp.json")],
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".workbuddy/skills")),
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
	use std::path::{Path, PathBuf};

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

		fn apply(
			&self,
			_agent: &DetectedAgent,
			_plan: &DiffPlanRespVO,
		) -> Result<Vec<ItemOutcome>> {
			Ok(Vec::new())
		}

		fn export_skill(&self, _name: &str, _dest_dir: &Path) -> Result<bool> {
			Ok(false)
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
			.apply(&probe, &DiffPlanRespVO { items: Vec::new() })
			.unwrap();
		assert!(outcomes.is_empty());

		assert!(!adapter
			.export_skill("demo-skill", Path::new("/tmp/skillhub-test-export"))
			.unwrap());
	}

	// all_adapters 累计应接入 11 款工具: Task 3 的 6 款 JSON mcpServers 在前, 本任务追加的
	// CodeBuddy/WorkBuddy(同样走 json_mcp_agent_configs 表)紧随其后, Task 4 追加的
	// VS Code(仍是 JsonMcpAdapter)与 Codex(CodexAdapter), M5 Task B1 追加的 Hermes
	// (HermesAdapter)按注册顺序追加在后; 数量与种类应与配置表逐一对应。均应支持 Mcp; Skill
	// 支持与否按工具而定 —— 仅 CodeBuddy(SkillTarget::None, 纯 MCP)不支持, 其余 10 款均支持
	#[test]
	fn all_adapters_registers_eleven_tools_with_correct_kinds_and_support() {
		let home = PathBuf::from("/tmp/skillhub-test-home");
		let adapters = all_adapters(&home);

		let mut expected_kinds: Vec<AgentKind> = json_mcp_agent_configs()
			.into_iter()
			.map(|(kind, _, _)| kind)
			.collect();
		expected_kinds.push(AgentKind::VsCode);
		expected_kinds.push(AgentKind::Codex);
		expected_kinds.push(AgentKind::Hermes);

		assert_eq!(adapters.len(), 11, "本任务起应累计接入 11 款工具");
		let actual_kinds: Vec<AgentKind> = adapters.iter().map(|a| a.kind()).collect();
		assert_eq!(actual_kinds, expected_kinds, "注册顺序与种类应与配置表一致");

		for adapter in &adapters {
			assert!(
				adapter.supports(ResourceType::Mcp),
				"{:?} 应支持 Mcp",
				adapter.kind()
			);
			if adapter.kind() == AgentKind::CodeBuddy {
				assert!(
					!adapter.supports(ResourceType::Skill),
					"CodeBuddy 纯 MCP(SkillTarget::None), 不应支持 Skill"
				);
			} else {
				assert!(
					adapter.supports(ResourceType::Skill),
					"{:?} 应支持 Skill",
					adapter.kind()
				);
			}
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

	// all_adapters 里的 Hermes 条目应命中 .hermes/config.yaml(顶层 mcp_servers 映射)fixture,
	// 解析出 command 型服务器一条; 验证 HermesAdapter 已按固定相对路径正确接入
	#[test]
	fn all_adapters_hermes_entry_detects_and_reads_config_yaml_fixture() {
		let dir = tempfile::tempdir().unwrap();
		let rel = PathBuf::from(".hermes/config.yaml");
		let abs = dir.path().join(&rel);
		std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
		std::fs::write(
			&abs,
			"mcp_servers:\n  foo:\n    command: node\n    args:\n      - x\n    env:\n      K: V\n",
		)
		.unwrap();

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::Hermes)
			.expect("Hermes 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "Hermes 应命中 fixture");
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 1, "Hermes 应解析出 1 条 McpServerDef");
		let foo = &state.mcp[0];
		assert_eq!(foo.name, "foo");
		assert_eq!(foo.command, Some("node".to_string()));
		assert_eq!(foo.args, vec!["x".to_string()]);
	}

	// 按 target 的落地形态在 home 下写一份最小 fixture, 返回期望读出的 Skill 名; 供下方
	// "验证 all_adapters 里每款工具的 SkillTarget 映射均生效"系列测试复用, 覆盖三种真正落地
	// 形态; SkillTarget::None(纯 MCP 占位)不写任何东西, 调用方应改为断言读出空清单
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
			SkillTarget::None => {
				// 纯 MCP 工具(如 CodeBuddy)无 Skill 落地形态, 不生成任何 fixture
			}
		}
		name.to_string()
	}

	// all_adapters 里 JSON mcp 工具各自声明的 SkillTarget 均应生效: 分别在其 mcp 配置候选
	// 路径与 SkillTarget 描述的落地位置各放一份最小 fixture, detect+read_state 应从声明的
	// SkillTarget 读出 1 条 Skill(覆盖 ClaudeSkillsDir/RulesDir/InstructionsFile 三种形态,
	// 具体每款工具映射到哪种见 json_mcp_agent_configs); SkillTarget::None(仅 CodeBuddy)是
	// 例外 —— 不写 fixture, 应恒读出空 Skill 清单(纯 MCP, 无处落地)
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
			if matches!(skill_target, SkillTarget::None) {
				assert!(
					state.skills.is_empty(),
					"{kind:?} 为 SkillTarget::None, 应恒读出空 Skill 清单"
				);
			} else {
				assert_eq!(
					state.skills.len(),
					1,
					"{kind:?} 应从声明的 SkillTarget 读出 1 个 Skill"
				);
				assert_eq!(state.skills[0].name, expected_name);
			}
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

	// all_adapters 里的 Hermes 条目应从声明的 SkillTarget(ClaudeSkillsDir(".hermes/skills"))
	// 读出已装 Skill, 与其 .hermes/config.yaml(顶层 mcp_servers 映射)fixture 各自独立解析
	#[test]
	fn all_adapters_hermes_entry_reads_skills_from_declared_skill_target() {
		let dir = tempfile::tempdir().unwrap();
		let rel = PathBuf::from(".hermes/config.yaml");
		let abs = dir.path().join(&rel);
		std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
		std::fs::write(&abs, "mcp_servers: {}\n").unwrap();
		let expected_name = write_skill_fixture(
			dir.path(),
			&SkillTarget::ClaudeSkillsDir(PathBuf::from(".hermes/skills")),
		);

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::Hermes)
			.expect("Hermes 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "Hermes 应命中 mcp fixture");
		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, expected_name);
	}

	// all_adapters 里的 CodeBuddy 条目应按候选路径优先级探测: `.codebuddy/.mcp.json`(推荐)
	// 与 `.codebuddy/mcp.json`(已废弃)同时存在时应命中并读取前者的内容, 而非误选到后者
	// (与 JsonMcpAdapter::detect "取第一个实际存在的候选"的既有语义相印证, 这里额外验证
	// "两个候选都存在"这一更严格的场景, 不止是"前者缺失才回退"的常规容错场景)
	#[test]
	fn all_adapters_codebuddy_entry_prefers_dot_mcp_json_over_deprecated_mcp_json() {
		let dir = tempfile::tempdir().unwrap();
		let preferred = dir.path().join(".codebuddy/.mcp.json");
		let deprecated = dir.path().join(".codebuddy/mcp.json");
		std::fs::create_dir_all(preferred.parent().unwrap()).unwrap();
		std::fs::write(
			&preferred,
			r#"{"mcpServers":{"fromPreferred":{"command":"node"}}}"#,
		)
		.unwrap();
		std::fs::write(
			&deprecated,
			r#"{"mcpServers":{"fromDeprecated":{"command":"python"}}}"#,
		)
		.unwrap();

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::CodeBuddy)
			.expect("CodeBuddy 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "CodeBuddy 应命中候选路径");
		assert_eq!(
			detected[0].config_path,
			preferred.to_string_lossy(),
			"两个候选都存在时应优先命中 .mcp.json(推荐), 而非 mcp.json(已废弃)"
		);

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 1);
		assert_eq!(state.mcp[0].name, "fromPreferred");

		assert!(adapter.supports(ResourceType::Mcp));
		assert!(
			!adapter.supports(ResourceType::Skill),
			"CodeBuddy 官方文档未提供 Skill 落地约定, 应汇报不支持 Skill"
		);
	}

	// all_adapters 里的 WorkBuddy 条目应同时接好 MCP(.workbuddy/mcp.json)与 Skill
	// (.workbuddy/skills/<name>/SKILL.md), 二者各自独立解析, 与 ClaudeCode/Hermes 等既有工具
	// "MCP 与 Skill 落地位置互不相干"的惯例一致(两条路径均已通过官方文档核实, 见
	// json_mcp_agent_configs 文档注释)
	#[test]
	fn all_adapters_workbuddy_entry_detects_and_reads_mcp_and_skills() {
		let dir = tempfile::tempdir().unwrap();
		let config_abs = dir.path().join(".workbuddy/mcp.json");
		std::fs::create_dir_all(config_abs.parent().unwrap()).unwrap();
		std::fs::write(
			&config_abs,
			r#"{"mcpServers":{"foo":{"command":"node","args":["x"],"env":{"K":"V"}},"bar":{"url":"http://localhost:1"}}}"#,
		)
		.unwrap();

		let skill_dir = dir.path().join(".workbuddy/skills/demo-skill");
		std::fs::create_dir_all(&skill_dir).unwrap();
		std::fs::write(skill_dir.join("SKILL.md"), "---\nversion: 1.0.0\n---\n").unwrap();

		let adapters = all_adapters(dir.path());
		let adapter = adapters
			.iter()
			.find(|a| a.kind() == AgentKind::WorkBuddy)
			.expect("WorkBuddy 应已注册");

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1, "WorkBuddy 应命中 mcp fixture");
		assert_eq!(detected[0].config_path, config_abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 2, "WorkBuddy 应解析出 2 条 McpServerDef");
		assert!(state
			.mcp
			.iter()
			.any(|s| s.name == "foo" && s.command == Some("node".to_string())));
		assert!(state
			.mcp
			.iter()
			.any(|s| s.name == "bar" && s.url == Some("http://localhost:1".to_string())));

		assert_eq!(
			state.skills.len(),
			1,
			"WorkBuddy 应从 .workbuddy/skills 读出 1 个 Skill"
		);
		assert_eq!(state.skills[0].name, "demo-skill");
		assert_eq!(state.skills[0].version, "1.0.0");

		assert!(adapter.supports(ResourceType::Mcp));
		assert!(adapter.supports(ResourceType::Skill));
	}
}
