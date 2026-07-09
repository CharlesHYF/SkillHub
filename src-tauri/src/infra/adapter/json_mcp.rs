// 文件作用: 通用 JSON mcpServers 适配器 —— 覆盖 ClaudeCode/ClaudeDesktop/Cursor/Windsurf/Cline/
//           GeminiCli 六款工具的探测(detect)与 MCP 实际态读取(read_state)。这六款工具的配置
//           文件形态一致: 顶层 JSON 对象下挂一个 mcpServers 字典, 差异只在文件路径; 故用同一
//           结构体 + 候选路径表覆盖全部六款, 具体路径表见 adapter::mod 的 json_mcp_agent_configs。
//           Skill 落地(read_state.skills)留 Task 5, apply 留 Task 7, 本文件均先占位。
// 创建日期: 2026-07-09

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::domain::agent::{ActualState, AgentKind, AgentScope, DetectedAgent, McpServerDef};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DiffPlan, ItemOutcome};

use super::AgentAdapter;

/// 通用 JSON mcpServers 适配器: `rel_candidates` 为相对 `home` 的候选配置文件路径(工具版本/
/// 操作系统可能导致路径漂移, 按声明顺序取第一个真实存在的); `servers_key` 为该工具配置文件里
/// 挂 MCP 服务器字典的键名(六款工具目前均为 "mcpServers", 保留为参数以防未来漂移)。
pub struct JsonMcpAdapter {
	kind: AgentKind,
	home: PathBuf,
	rel_candidates: Vec<PathBuf>,
	servers_key: &'static str,
}

impl JsonMcpAdapter {
	/// 构造一个适配器实例; `home` 生产环境通常取自 `dirs::home_dir()`, 测试时注入临时目录,
	/// 避免探测逻辑触碰真实机器配置
	pub fn new(
		kind: AgentKind,
		home: PathBuf,
		rel_candidates: Vec<PathBuf>,
		servers_key: &'static str,
	) -> Self {
		Self {
			kind,
			home,
			rel_candidates,
			servers_key,
		}
	}

	/// 按声明顺序在候选相对路径里找第一个真实存在的文件, 都不存在返回 None
	fn existing_config_path(&self) -> Option<PathBuf> {
		self.rel_candidates
			.iter()
			.map(|rel| self.home.join(rel))
			.find(|abs| abs.is_file())
	}
}

impl AgentAdapter for JsonMcpAdapter {
	/// 本适配器对应的 Agent 种类(构造时确定)
	fn kind(&self) -> AgentKind {
		self.kind
	}

	/// 六款工具均可托管 MCP 与 Skill(Skill 的落地读写留 Task 5/7, 能力矩阵上先声明支持)
	fn supports(&self, ty: ResourceType) -> bool {
		matches!(ty, ResourceType::Skill | ResourceType::Mcp)
	}

	/// 候选路径里第一个存在的文件即视为该工具已安装; 找不到任何候选返回空表(未安装/未配置)
	fn detect(&self) -> Vec<DetectedAgent> {
		match self.existing_config_path() {
			Some(path) => vec![DetectedAgent {
				kind: self.kind,
				name: self.kind.label().to_string(),
				config_path: path.to_string_lossy().into_owned(),
				scope: AgentScope::Global,
				online: true,
			}],
			None => Vec::new(),
		}
	}

	/// 读取 `agent.config_path` 指向的配置文件, 解析出 mcpServers 字典。配置文件不存在视为
	/// "实际态为空"(工具装了但还没配任何 MCP, 不算错误); 文件存在但 JSON 解析失败才报错,
	/// 因为那属于配置文件本身损坏, 不应被静默吞掉
	fn read_state(&self, agent: &DetectedAgent) -> Result<ActualState> {
		let path = &agent.config_path;
		let text = match fs::read_to_string(path) {
			Ok(text) => text,
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
				return Ok(ActualState {
					mcp: Vec::new(),
					skills: Vec::new(),
				});
			}
			Err(err) => return Err(err).with_context(|| format!("读取配置文件失败: {path}")),
		};
		let root: Value = serde_json::from_str(&text)
			.with_context(|| format!("解析配置文件 JSON 失败: {path}"))?;
		Ok(ActualState {
			mcp: parse_mcp_servers(&root, self.servers_key),
			skills: Vec::new(),
		})
	}

	/// 写回配置文件留给 Task 7(声明式协调引擎与写入应用)实现
	fn apply(&self, _agent: &DetectedAgent, _plan: &DiffPlan) -> Result<Vec<ItemOutcome>> {
		todo!("Task 7 实现: 按 DiffPlan 写回配置文件, 写前对目标文件做时间戳备份")
	}
}

/// 从配置文件根 JSON 取出 `servers_key` 对象, 逐条转为 McpServerDef; 键名即服务器名。
/// 值按字段逐个宽松提取: 字段缺失或类型不符都退回默认值, 不让单条脏数据拖垮整份读取
/// (同时支持 command 型 {command,args,env} 与 url 型 {url}, 两者字段可共存)
fn parse_mcp_servers(root: &Value, servers_key: &str) -> Vec<McpServerDef> {
	let Some(servers) = root.get(servers_key).and_then(Value::as_object) else {
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
				.and_then(Value::as_object)
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

	/// brief 给定的 fixture: 一条 command 型(foo) + 一条 url 型(bar), 覆盖两种服务器形态
	const FIXTURE_JSON: &str = r#"{"mcpServers":{"foo":{"command":"node","args":["x"],"env":{"K":"V"}},"bar":{"url":"http://localhost:1"}}}"#;

	/// 断言解析结果恰好是 FIXTURE_JSON 里的 foo(command 型)与 bar(url 型)两条, 字段逐一核对
	fn assert_fixture_mcp(mcp: &[McpServerDef]) {
		assert_eq!(mcp.len(), 2);

		let foo = mcp.iter().find(|s| s.name == "foo").expect("应含 foo");
		assert_eq!(foo.command, Some("node".to_string()));
		assert_eq!(foo.args, vec!["x".to_string()]);
		let mut expected_env = BTreeMap::new();
		expected_env.insert("K".to_string(), "V".to_string());
		assert_eq!(foo.env, expected_env);
		assert_eq!(foo.url, None);

		let bar = mcp.iter().find(|s| s.name == "bar").expect("应含 bar");
		assert_eq!(bar.command, None);
		assert!(bar.args.is_empty());
		assert!(bar.env.is_empty());
		assert_eq!(bar.url, Some("http://localhost:1".to_string()));
	}

	// detect + read_state: candidate 命中 fixture 后应解析出 command 型(foo)+url 型(bar)两条
	#[test]
	fn detect_and_read_state_parse_command_and_url_servers() {
		let dir = tempdir().unwrap();
		let abs = dir.path().join(".claude.json");
		fs::write(&abs, FIXTURE_JSON).unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
		);

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1);
		assert_eq!(detected[0].kind, AgentKind::ClaudeCode);
		assert!(detected[0].online);
		assert_eq!(detected[0].scope, AgentScope::Global);
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_fixture_mcp(&state.mcp);
		assert!(state.skills.is_empty(), "Skill 落地留 Task 5, 本任务应为空");
	}

	// detect 应按声明顺序容错: 排在前面的候选路径缺失时应继续尝试后面的, 命中已存在的那个
	#[test]
	fn detect_falls_back_to_later_candidate_when_earlier_ones_are_missing() {
		let dir = tempdir().unwrap();
		let present_rel = PathBuf::from("nested/config.json");
		let present_abs = dir.path().join(&present_rel);
		fs::create_dir_all(present_abs.parent().unwrap()).unwrap();
		fs::write(&present_abs, FIXTURE_JSON).unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeDesktop,
			dir.path().to_path_buf(),
			vec![PathBuf::from("does/not/exist.json"), present_rel],
			"mcpServers",
		);

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1);
		assert_eq!(detected[0].config_path, present_abs.to_string_lossy());
	}

	// detect 应在所有候选路径都不存在时返回空表(工具未安装/未配置), 不 panic 不报错
	#[test]
	fn detect_returns_empty_when_no_candidate_exists() {
		let dir = tempdir().unwrap();
		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
		);
		assert!(adapter.detect().is_empty());
	}

	// read_state: 配置文件不存在时视为"实际态为空"而非错误(工具已装但尚未配置任何 MCP)
	#[test]
	fn read_state_returns_empty_actual_state_when_config_file_missing() {
		let dir = tempdir().unwrap();
		let adapter = JsonMcpAdapter::new(
			AgentKind::Cursor,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".cursor/mcp.json")],
			"mcpServers",
		);
		let probe = DetectedAgent {
			kind: AgentKind::Cursor,
			name: "Cursor".to_string(),
			config_path: dir
				.path()
				.join(".cursor/mcp.json")
				.to_string_lossy()
				.into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
		assert!(state.skills.is_empty());
	}

	// read_state: 文件存在但内容不是合法 JSON, 属配置文件本身损坏, 应报错而不是静默兜底成空态
	#[test]
	fn read_state_returns_err_on_malformed_json() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".gemini/settings.json");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(&path, "{ not valid json").unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::GeminiCli,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".gemini/settings.json")],
			"mcpServers",
		);
		let probe = DetectedAgent {
			kind: AgentKind::GeminiCli,
			name: "Gemini CLI".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		assert!(adapter.read_state(&probe).is_err());
	}

	// read_state: JSON 合法但没有 servers_key(如刚安装还没写任何 MCP 配置段), mcp 应为空表而非报错
	#[test]
	fn read_state_returns_empty_mcp_when_servers_key_absent() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".claude.json");
		fs::write(&path, r#"{"otherField": true}"#).unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
		);
		let probe = DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
	}

	// read_state: 单条 mcpServers 条目格式异常(值不是对象)应被当作"全默认"兜底, 不拖累
	// 其余条目与整体读取(呼应"宽松解析"要求)
	#[test]
	fn read_state_tolerates_malformed_single_server_entry() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".claude.json");
		fs::write(
			&path,
			r#"{"mcpServers":{"broken":"not-an-object","ok":{"command":"node","args":[],"env":{}}}}"#,
		)
		.unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
		);
		let probe = DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
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
}
