// 文件作用: Codex 适配器 —— 覆盖 Codex CLI 的探测(detect)与 MCP+Skill 实际态读取(read_state)。
//           Codex 的配置文件是 `~/.codex/config.toml`, MCP 服务器登记在 `[mcp_servers.<name>]`
//           表下(字段 command/args/env), 与其余 6 款 JSON mcpServers 工具的"顶层 JSON 挂字典"
//           形态不同(TOML 语法 + 表名而非 JSON 对象键), 故不复用 JsonMcpAdapter, 单独实现。
//           Skill 的落地读取(read_state.skills)已接入 skill_target(见 SkillTarget, Task 5),
//           Codex 映射到 InstructionsFile("AGENTS.md")(见 adapter::mod::all_adapters); 落地
//           写入(apply)留 Task 7, 本文件该方法仍先占位。
// 创建日期: 2026-07-09

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use toml::{Table, Value};

use crate::domain::agent::{ActualState, AgentKind, AgentScope, DetectedAgent, McpServerDef};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DiffPlan, ItemOutcome};

use super::skill_target::SkillTarget;
use super::AgentAdapter;

/// Codex 配置文件相对家目录的固定路径; 目前只有这一种已知形态(不像其余工具存在多操作系统
/// 路径漂移), 故不设候选表
const CONFIG_REL_PATH: &str = ".codex/config.toml";

/// Codex 适配器: 读取 `~/.codex/config.toml` 的 `[mcp_servers.*]` 表, 并按 skill_target
/// 读取 Skill 落地清单
pub struct CodexAdapter {
	home: PathBuf,
	skill_target: SkillTarget,
}

impl CodexAdapter {
	/// 构造一个适配器实例; `home` 生产环境通常取自 `dirs::home_dir()`, 测试时注入临时目录,
	/// 避免探测逻辑触碰真实机器配置; `skill_target` 声明 Codex 的 Skill 落地形态(见 SkillTarget)
	pub fn new(home: PathBuf, skill_target: SkillTarget) -> Self {
		Self { home, skill_target }
	}

	/// Codex 配置文件的绝对路径(`home` 拼接固定相对路径)
	fn config_path(&self) -> PathBuf {
		self.home.join(CONFIG_REL_PATH)
	}
}

impl AgentAdapter for CodexAdapter {
	/// 本适配器对应的 Agent 种类
	fn kind(&self) -> AgentKind {
		AgentKind::Codex
	}

	/// Codex 可托管 MCP 与 Skill(Skill 的落地读取已接入 skill_target, 写入留 Task 7)
	fn supports(&self, ty: ResourceType) -> bool {
		matches!(ty, ResourceType::Skill | ResourceType::Mcp)
	}

	/// 配置文件存在即视为该工具已安装; 不存在返回空表(未安装/未配置)
	fn detect(&self) -> Vec<DetectedAgent> {
		let path = self.config_path();
		if path.is_file() {
			vec![DetectedAgent {
				kind: AgentKind::Codex,
				name: AgentKind::Codex.label().to_string(),
				config_path: path.to_string_lossy().into_owned(),
				scope: AgentScope::Global,
				online: true,
			}]
		} else {
			Vec::new()
		}
	}

	/// 读取 `agent.config_path` 指向的 TOML 配置文件, 解析出 mcp_servers 表, 并按 skill_target
	/// 读出 Skill 清单一并装进 ActualState。mcp 配置文件不存在视为该维度"实际态为空"(工具装了
	/// 但还没配任何 MCP, 不算错误, 且不影响 Skill 照常读取, 二者落地位置本就互相独立); 文件
	/// 存在但 TOML 解析失败才报错, 因为那属于配置文件本身损坏, 不应被静默吞掉
	fn read_state(&self, agent: &DetectedAgent) -> Result<ActualState> {
		let path = &agent.config_path;
		let text = match fs::read_to_string(path) {
			Ok(text) => text,
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
				return Ok(ActualState {
					mcp: Vec::new(),
					skills: self.skill_target.read_skills(&self.home),
				});
			}
			Err(err) => return Err(err).with_context(|| format!("读取配置文件失败: {path}")),
		};
		// 注意: 必须解析为 Table(TOML 文档根)而非 Value —— Value::from_str 只认单个 TOML 值
		// 表达式, 不认 `[section]` 表头语法, 会把整份文档解析炸掉; Table::from_str 才是文档级解析
		let root: Table = text
			.parse()
			.with_context(|| format!("解析配置文件 TOML 失败: {path}"))?;
		Ok(ActualState {
			mcp: parse_mcp_servers(&root),
			skills: self.skill_target.read_skills(&self.home),
		})
	}

	/// 写回配置文件留给 Task 7(声明式协调引擎与写入应用)实现
	fn apply(&self, _agent: &DetectedAgent, _plan: &DiffPlan) -> Result<Vec<ItemOutcome>> {
		todo!("Task 7 实现: 按 DiffPlan 写回配置文件, 写前对目标文件做时间戳备份")
	}
}

/// 从配置文件根 TOML 取出 `mcp_servers` 表, 逐条转为 McpServerDef; 表名即服务器名。
/// 字段逐个宽松提取: 字段缺失或类型不符都退回默认值, 不让单条脏数据拖垮整份读取
/// (Codex 官方文档目前只有 command 型服务器, 无 url 型, 但仍尝试读取 url 字段以防未来支持,
/// 读不到则为 None, 与 JsonMcpAdapter::parse_mcp_servers 的宽松策略保持一致)
fn parse_mcp_servers(root: &Table) -> Vec<McpServerDef> {
	let Some(servers) = root.get("mcp_servers").and_then(Value::as_table) else {
		return Vec::new();
	};
	servers
		.iter()
		.map(|(name, raw)| McpServerDef {
			name: name.clone(),
			command: raw
				.get("command")
				.and_then(Value::as_str)
				.map(str::to_string),
			args: raw
				.get("args")
				.and_then(Value::as_array)
				.map(|items| {
					items
						.iter()
						.filter_map(Value::as_str)
						.map(str::to_string)
						.collect::<Vec<String>>()
				})
				.unwrap_or_default(),
			env: raw
				.get("env")
				.and_then(Value::as_table)
				.map(|obj| {
					obj.iter()
						.filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
						.collect::<BTreeMap<String, String>>()
				})
				.unwrap_or_default(),
			url: raw.get("url").and_then(Value::as_str).map(str::to_string),
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::fs;

	use tempfile::tempdir;

	use super::*;

	/// fixture: 一条 command 型服务器 foo(含 args/env), 覆盖 brief 要求的最小场景
	const FIXTURE_TOML: &str =
		"[mcp_servers.foo]\ncommand = \"node\"\nargs = [\"x\"]\nenv = { K = \"V\" }\n";

	// detect + read_state: 配置文件存在时应命中并解析出 foo 这一条 command 型服务器
	#[test]
	fn detect_and_read_state_parse_command_server() {
		let dir = tempdir().unwrap();
		let abs = dir.path().join(".codex/config.toml");
		fs::create_dir_all(abs.parent().unwrap()).unwrap();
		fs::write(&abs, FIXTURE_TOML).unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1);
		assert_eq!(detected[0].kind, AgentKind::Codex);
		assert!(detected[0].online);
		assert_eq!(detected[0].scope, AgentScope::Global);
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 1);
		let foo = &state.mcp[0];
		assert_eq!(foo.name, "foo");
		assert_eq!(foo.command, Some("node".to_string()));
		assert_eq!(foo.args, vec!["x".to_string()]);
		let mut expected_env = BTreeMap::new();
		expected_env.insert("K".to_string(), "V".to_string());
		assert_eq!(foo.env, expected_env);
		assert_eq!(foo.url, None);
		assert!(
			state.skills.is_empty(),
			"本测试未落地任何 AGENTS.md fixture, 应为空"
		);
	}

	// detect 应在配置文件不存在时返回空表(Codex 未安装/未配置), 不 panic 不报错
	#[test]
	fn detect_returns_empty_when_config_file_missing() {
		let dir = tempdir().unwrap();
		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		assert!(adapter.detect().is_empty());
	}

	// read_state: 配置文件不存在时视为"实际态为空"而非错误(工具已装但尚未配置任何 MCP)
	#[test]
	fn read_state_returns_empty_actual_state_when_config_file_missing() {
		let dir = tempdir().unwrap();
		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = DetectedAgent {
			kind: AgentKind::Codex,
			name: "Codex".to_string(),
			config_path: dir
				.path()
				.join(".codex/config.toml")
				.to_string_lossy()
				.into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
		assert!(state.skills.is_empty());
	}

	// read_state: 文件存在但内容不是合法 TOML, 属配置文件本身损坏, 应报错而不是静默兜底成空态
	#[test]
	fn read_state_returns_err_on_malformed_toml() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(&path, "this is not [ valid toml").unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = DetectedAgent {
			kind: AgentKind::Codex,
			name: "Codex".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		assert!(adapter.read_state(&probe).is_err());
	}

	// read_state: TOML 合法但没有 mcp_servers 表(如刚安装还没写任何 MCP 配置段),
	// mcp 应为空表而非报错
	#[test]
	fn read_state_returns_empty_mcp_when_mcp_servers_table_absent() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(&path, "model = \"gpt-5\"\n").unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = DetectedAgent {
			kind: AgentKind::Codex,
			name: "Codex".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
	}

	// read_state: 单条 mcp_servers 条目格式异常(值不是表)应被当作"全默认"兜底, 不拖累
	// 其余条目与整体读取(呼应"宽松解析"要求, 对齐 JsonMcpAdapter 同类测试)
	#[test]
	fn read_state_tolerates_malformed_single_server_entry() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(
			&path,
			"[mcp_servers]\nbroken = \"not-a-table\"\n\n[mcp_servers.ok]\ncommand = \"node\"\n",
		)
		.unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = DetectedAgent {
			kind: AgentKind::Codex,
			name: "Codex".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert_eq!(state.mcp.len(), 2);
		let broken = state.mcp.iter().find(|s| s.name == "broken").unwrap();
		assert_eq!(broken.command, None);
		assert!(broken.args.is_empty());
		let ok = state.mcp.iter().find(|s| s.name == "ok").unwrap();
		assert_eq!(ok.command, Some("node".to_string()));
	}

	// read_state: 应同时返回 mcp([mcp_servers.*] 表)与 skills(skill_target 描述的 AGENTS.md
	// 标记块), 二者落地位置互不相干, 应各自独立解析并一并装进 ActualState
	#[test]
	fn read_state_combines_mcp_and_instructions_file_skills() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, FIXTURE_TOML).unwrap();

		fs::write(
			dir.path().join("AGENTS.md"),
			"<!-- skillhub:start:demo-skill@2.0.0 -->\n内容\n<!-- skillhub:end:demo-skill -->\n",
		)
		.unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);

		let detected = adapter.detect();
		let state = adapter.read_state(&detected[0]).unwrap();

		assert_eq!(state.mcp.len(), 1);
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, "demo-skill");
		assert_eq!(state.skills[0].version, "2.0.0");
	}

	// read_state: Codex 配置文件缺失只代表"还没配任何 MCP", 不应连带影响 Skill 的读取
	// (二者落地位置独立, 缺一不代表另一个也该为空)
	#[test]
	fn read_state_reads_skills_even_when_config_file_missing() {
		let dir = tempdir().unwrap();
		fs::write(
			dir.path().join("AGENTS.md"),
			"<!-- skillhub:start:demo-skill@0.1.0 -->\n内容\n<!-- skillhub:end:demo-skill -->\n",
		)
		.unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = DetectedAgent {
			kind: AgentKind::Codex,
			name: "Codex".to_string(),
			config_path: dir
				.path()
				.join(".codex/config.toml")
				.to_string_lossy()
				.into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, "demo-skill");
	}
}
