// 文件作用: Codex 适配器 —— 覆盖 Codex CLI 的探测(detect)、MCP+Skill 实际态读取(read_state)
//           与差异计划落地写入(apply)。Codex 的配置文件是 `~/.codex/config.toml`, MCP 服务器
//           登记在 `[mcp_servers.<name>]` 表下(字段 command/args/env), 与其余 6 款 JSON
//           mcpServers 工具的"顶层 JSON 挂字典"形态不同(TOML 语法 + 表名而非 JSON 对象键),
//           故不复用 JsonMcpAdapter, 单独实现(apply 的"整份读入 -> 内存合并 -> 备份 -> 整份
//           写回"策略与 JsonMcpAdapter 一致, 只是数据结构换成 toml::Table/Value)。
//           Skill 的落地读写(read_state.skills/apply 里 Skill 项)已接入 skill_target(见
//           SkillTarget, Task 5/7b), Codex 映射到 InstructionsFile("AGENTS.md")(见
//           adapter::mod::all_adapters)。
// 创建日期: 2026-07-09

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml::{Table, Value};

use crate::domain::agent::{ActualState, AgentKind, AgentScope, DetectedAgent, McpServerDef};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DesiredPayload, DiffAction, DiffItem, DiffPlan, ItemOutcome};

use super::skill_target::SkillTarget;
use super::util::{apply_skill_item, backup_file, err_outcome, ok_outcome};
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

	/// 把本次 plan 里 res_type==Mcp 的若干 DiffItem 合并应用到同一份 config.toml: 先把整份
	/// 文件读入内存(不存在则视为空表), 逐项在内存里对 mcp_servers 表做增/改/删, 全部处理完毕
	/// 后统一备份 + 落盘一次(而非逐项各自备份写盘) —— 策略与 JsonMcpAdapter::apply_mcp_items
	/// 一致, 只是数据结构换成 toml::Table/Value。单项 payload 形状不符(脏数据)不会中断其它项,
	/// 只把该项标记为 ok=false 并记录 err; 读取/解析/备份/落盘这类影响全文件的失败则整体报错
	/// (Err), 因为此时已无法保证任何一项真的落地成功
	fn apply_mcp_items(&self, path: &Path, items: &[&DiffItem]) -> Result<Vec<ItemOutcome>> {
		let mut root: Table = match fs::read_to_string(path) {
			Ok(text) => text
				.parse()
				.with_context(|| format!("解析配置文件 TOML 失败: {}", path.display()))?,
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => Table::new(),
			Err(err) => {
				return Err(err).with_context(|| format!("读取配置文件失败: {}", path.display()))
			}
		};

		let servers = root
			.entry("mcp_servers")
			.or_insert_with(|| Value::Table(Table::new()));
		let Some(servers_table) = servers.as_table_mut() else {
			anyhow::bail!("mcp_servers 不是 TOML 表: {}", path.display());
		};

		let mut outcomes = Vec::with_capacity(items.len());
		let mut changed = false;
		for item in items {
			match (item.action, &item.payload) {
				(DiffAction::Add, Some(DesiredPayload::Mcp(def)))
				| (DiffAction::Update, Some(DesiredPayload::Mcp(def))) => {
					servers_table.insert(item.name.clone(), mcp_def_to_toml(def));
					changed = true;
					outcomes.push(ok_outcome(item));
				}
				(DiffAction::Remove, _) => {
					servers_table.remove(&item.name);
					changed = true;
					outcomes.push(ok_outcome(item));
				}
				(action, payload) => outcomes.push(err_outcome(
					item,
					format!(
						"MCP 项 {} 的 action({action:?})与 payload 形状不符({payload:?})",
						item.name
					),
				)),
			}
		}

		if changed {
			backup_file(path).with_context(|| format!("备份配置文件失败: {}", path.display()))?;
			if let Some(parent) = path.parent() {
				fs::create_dir_all(parent)
					.with_context(|| format!("创建目录失败: {}", parent.display()))?;
			}
			let text = toml::to_string_pretty(&root).context("序列化配置文件失败")?;
			fs::write(path, text)
				.with_context(|| format!("写入配置文件失败: {}", path.display()))?;
		}

		Ok(outcomes)
	}
}

impl AgentAdapter for CodexAdapter {
	/// 本适配器对应的 Agent 种类
	fn kind(&self) -> AgentKind {
		AgentKind::Codex
	}

	/// Codex 可托管 MCP 与 Skill(读取与写入均已接入, 见 read_state/apply)
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

	/// 把 plan 里的每一项按 res_type 分派应用: Mcp 项合并进 config.toml(见 apply_mcp_items,
	/// 内部只对该文件做一次备份 + 一次写回); Skill 项各自独立调用 skill_target.write_skill/
	/// remove_skill(见 apply_skill_item), 与 Mcp 项的落地位置(AGENTS.md)互不相干。返回的
	/// outcomes 顺序为"先 Mcp 项(按 items 原有顺序), 再 Skill 项(按 items 原有顺序)", 调用方
	/// 应按 name 匹配而非依赖顺序
	fn apply(&self, agent: &DetectedAgent, plan: &DiffPlan) -> Result<Vec<ItemOutcome>> {
		let path = PathBuf::from(&agent.config_path);
		let mut outcomes = Vec::new();

		let mcp_items: Vec<&DiffItem> = plan
			.items
			.iter()
			.filter(|item| item.res_type == ResourceType::Mcp)
			.collect();
		if !mcp_items.is_empty() {
			outcomes.extend(self.apply_mcp_items(&path, &mcp_items)?);
		}

		for item in plan
			.items
			.iter()
			.filter(|item| item.res_type == ResourceType::Skill)
		{
			outcomes.push(apply_skill_item(&self.skill_target, &self.home, item));
		}

		Ok(outcomes)
	}
}

/// 把 McpServerDef 转为写回 config.toml 用的 TOML 表: 有 command 才写 command/args/env,
/// 有 url 才写 url; args/env 为空时省略(保持配置文件简洁, 与 JsonMcpAdapter::mcp_def_to_json
/// 的简洁策略一致)
fn mcp_def_to_toml(def: &McpServerDef) -> Value {
	let mut table = Table::new();
	if let Some(command) = &def.command {
		table.insert("command".to_string(), Value::String(command.clone()));
	}
	if !def.args.is_empty() {
		table.insert(
			"args".to_string(),
			Value::Array(def.args.iter().cloned().map(Value::String).collect()),
		);
	}
	if !def.env.is_empty() {
		let mut env_table = Table::new();
		for (key, val) in &def.env {
			env_table.insert(key.clone(), Value::String(val.clone()));
		}
		table.insert("env".to_string(), Value::Table(env_table));
	}
	if let Some(url) = &def.url {
		table.insert("url".to_string(), Value::String(url.clone()));
	}
	Value::Table(table)
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

	/// 构造一个 command 型 McpServerDef, env 固定为空, 供 apply 系列测试复用
	fn mcp_def(name: &str, command: &str, args: &[&str]) -> McpServerDef {
		McpServerDef {
			name: name.to_string(),
			command: Some(command.to_string()),
			args: args.iter().map(|a| a.to_string()).collect(),
			env: BTreeMap::new(),
			url: None,
		}
	}

	/// 构造一个 Add/Update 用的 Mcp 型 DiffItem
	fn mcp_diff_item(action: DiffAction, name: &str, command: &str, args: &[&str]) -> DiffItem {
		DiffItem {
			res_type: ResourceType::Mcp,
			name: name.to_string(),
			action,
			local_ver: String::new(),
			agent_ver: String::new(),
			payload: Some(DesiredPayload::Mcp(mcp_def(name, command, args))),
		}
	}

	/// 构造一个 Remove 用的 Mcp 型 DiffItem(payload 恒为 None, 与 reconcile 产出的 Remove
	/// 项形状一致)
	fn mcp_remove_item(name: &str) -> DiffItem {
		DiffItem {
			res_type: ResourceType::Mcp,
			name: name.to_string(),
			action: DiffAction::Remove,
			local_ver: String::new(),
			agent_ver: String::new(),
			payload: None,
		}
	}

	fn probe_at(config_path: &std::path::Path) -> DetectedAgent {
		DetectedAgent {
			kind: AgentKind::Codex,
			name: "Codex".to_string(),
			config_path: config_path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		}
	}

	// apply: Add 一个新 MCP 服务器应写入配置文件, 且保留文件里用户自己的其它服务器
	// (userSrv)与其它顶层键(model); 写入前应生成一份时间戳备份
	#[test]
	fn apply_add_writes_new_server_while_preserving_existing_content_and_backs_up() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(
			&config_path,
			"model = \"gpt-5\"\n\n[mcp_servers.userSrv]\ncommand = \"python\"\nargs = [\"server.py\"]\n",
		)
		.unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = probe_at(&config_path);
		let plan = DiffPlan {
			items: vec![mcp_diff_item(
				DiffAction::Add,
				"newSrv",
				"node",
				&["index.js"],
			)],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();

		assert_eq!(outcomes.len(), 1);
		assert!(outcomes[0].ok, "err = {}", outcomes[0].err);
		assert_eq!(outcomes[0].name, "newSrv");
		assert_eq!(outcomes[0].action, DiffAction::Add);

		let root: Table = fs::read_to_string(&config_path).unwrap().parse().unwrap();
		assert_eq!(
			root["mcp_servers"]["newSrv"]["command"].as_str(),
			Some("node")
		);
		assert_eq!(
			root["mcp_servers"]["userSrv"]["command"].as_str(),
			Some("python")
		);
		assert_eq!(root["model"].as_str(), Some("gpt-5"));

		let backups: Vec<_> = fs::read_dir(config_path.parent().unwrap())
			.unwrap()
			.filter_map(Result::ok)
			.filter(|entry| entry.file_name().to_string_lossy().contains("skillhub-bak"))
			.collect();
		assert_eq!(backups.len(), 1, "写入前应生成一份备份");
	}

	// apply: Update 应把已存在的服务器内容改成新的 command/args, 且不影响其它服务器
	#[test]
	fn apply_update_changes_existing_server_content() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(
			&config_path,
			"[mcp_servers.userSrv]\ncommand = \"python\"\n\n[mcp_servers.target]\ncommand = \"node\"\nargs = [\"old.js\"]\n",
		)
		.unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = probe_at(&config_path);
		let plan = DiffPlan {
			items: vec![mcp_diff_item(
				DiffAction::Update,
				"target",
				"node",
				&["new.js"],
			)],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok);

		let root: Table = fs::read_to_string(&config_path).unwrap().parse().unwrap();
		assert_eq!(
			root["mcp_servers"]["target"]["args"][0].as_str(),
			Some("new.js")
		);
		assert_eq!(
			root["mcp_servers"]["userSrv"]["command"].as_str(),
			Some("python")
		);
	}

	// apply: Remove 应删掉目标服务器, 但保留用户自己的其它服务器(userSrv)
	#[test]
	fn apply_remove_deletes_target_server_keeping_others() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(
			&config_path,
			"[mcp_servers.userSrv]\ncommand = \"python\"\n\n[mcp_servers.toRemove]\ncommand = \"node\"\n",
		)
		.unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = probe_at(&config_path);
		let plan = DiffPlan {
			items: vec![mcp_remove_item("toRemove")],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok);
		assert_eq!(outcomes[0].action, DiffAction::Remove);

		let root: Table = fs::read_to_string(&config_path).unwrap().parse().unwrap();
		assert!(root["mcp_servers"].get("toRemove").is_none());
		assert_eq!(
			root["mcp_servers"]["userSrv"]["command"].as_str(),
			Some("python")
		);
	}

	// apply: 配置文件不存在(全新安装, 尚未生成过配置)时应能创建文件与所需目录, Add 正常生效
	#[test]
	fn apply_add_creates_config_file_when_missing() {
		let dir = tempdir().unwrap();
		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let config_path = dir.path().join(".codex/config.toml");
		let probe = probe_at(&config_path);
		let plan = DiffPlan {
			items: vec![mcp_diff_item(
				DiffAction::Add,
				"newSrv",
				"node",
				&["index.js"],
			)],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok, "err = {}", outcomes[0].err);

		let root: Table = fs::read_to_string(&config_path).unwrap().parse().unwrap();
		assert_eq!(
			root["mcp_servers"]["newSrv"]["command"].as_str(),
			Some("node")
		);
	}

	// apply: 单项失败不应中断其它项 —— 一个 payload 形状不符的坏项(Add 却没带 Mcp payload)
	// 与一个正常好项同批应用, 好项应正常生效, 坏项应产出 ok=false 且带非空 err, 不影响好项
	#[test]
	fn apply_bad_item_does_not_block_good_item_in_same_plan() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, "").unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = probe_at(&config_path);
		let bad_item = DiffItem {
			res_type: ResourceType::Mcp,
			name: "badSrv".to_string(),
			action: DiffAction::Add,
			local_ver: String::new(),
			agent_ver: String::new(),
			payload: None, // Add 却没带 payload, 属脏数据
		};
		let good_item = mcp_diff_item(DiffAction::Add, "goodSrv", "node", &["index.js"]);
		let plan = DiffPlan {
			items: vec![bad_item, good_item],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert_eq!(outcomes.len(), 2);

		let bad_outcome = outcomes.iter().find(|o| o.name == "badSrv").unwrap();
		assert!(!bad_outcome.ok);
		assert!(!bad_outcome.err.is_empty());

		let good_outcome = outcomes.iter().find(|o| o.name == "goodSrv").unwrap();
		assert!(good_outcome.ok, "err = {}", good_outcome.err);

		let root: Table = fs::read_to_string(&config_path).unwrap().parse().unwrap();
		assert_eq!(
			root["mcp_servers"]["goodSrv"]["command"].as_str(),
			Some("node")
		);
		assert!(root["mcp_servers"].get("badSrv").is_none());
	}

	// apply: res_type==Skill 的项应分派给 skill_target.write_skill(InstructionsFile 形态),
	// 与 MCP 项各自独立生效
	#[test]
	fn apply_skill_item_delegates_to_skill_target_write_skill() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".codex/config.toml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, "").unwrap();

		let src_dir = dir.path().join("src-demo-skill");
		fs::create_dir_all(&src_dir).unwrap();
		fs::write(src_dir.join("SKILL.md"), "内容").unwrap();

		let adapter = CodexAdapter::new(
			dir.path().to_path_buf(),
			SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md")),
		);
		let probe = probe_at(&config_path);
		let plan = DiffPlan {
			items: vec![DiffItem {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				action: DiffAction::Add,
				local_ver: "1.0.0".to_string(),
				agent_ver: String::new(),
				payload: Some(DesiredPayload::Skill {
					src_dir: src_dir.to_string_lossy().into_owned(),
				}),
			}],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert_eq!(outcomes.len(), 1);
		assert!(outcomes[0].ok, "err = {}", outcomes[0].err);

		let agents_md = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
		assert!(agents_md.contains("<!-- skillhub:start:demo-skill@1.0.0 -->"));
		assert!(agents_md.contains("内容"));
	}
}
