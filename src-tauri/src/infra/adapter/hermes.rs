// 文件作用: Hermes Agent 适配器 —— 覆盖 Hermes(Nous Research 自托管 AI 桌面 agent)的探测
//           (detect)、MCP+Skill 实际态读取(read_state)与差异计划落地写入(apply)。Hermes 的
//           配置文件是 `~/.hermes/config.yaml`(YAML), MCP 服务器登记在顶层 `mcp_servers:`
//           映射下, 每项形如 stdio 型 { command, args, env } 或 http 型 { url, headers }
//           (另可有 enabled/timeout 等键), 与其余工具的 JSON/TOML 形态不同(YAML 语法), 故不
//           复用 JsonMcpAdapter/CodexAdapter, 单独实现(apply 的"整份读入 -> 内存合并 -> 备份
//           -> 整份写回"策略与二者一致, 只是数据结构换成 serde_yaml_ng::Mapping/Value)。
//           http 型服务器的 headers 字段目前无处落地 —— McpServerDef 本身没有 headers 字段
//           (与 JsonMcpAdapter/CodexAdapter 对 url 型服务器的既有简化处理一致), 读取时丢弃、
//           写入时也不产出, 后续若需透传 headers 需扩展 McpServerDef。
//           Skill 的落地读写(read_state.skills/apply 里 Skill 项)直接复用 SkillTarget::
//           ClaudeSkillsDir(".hermes/skills"), 与 Claude 家族同一套目录形态(构造实参见
//           adapter::mod::all_adapters 内 Hermes 条目)。
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_yaml_ng::{Mapping, Value};

use crate::domain::agent::{ActualState, AgentKind, AgentScope, DetectedAgent, McpServerDef};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DesiredPayload, DiffAction, DiffItem, DiffPlanRespVO, ItemOutcome};

use super::skill_target::SkillTarget;
use super::util::{apply_skill_item, backup_file, err_outcome, ok_outcome};
use super::AgentAdapter;

/// Hermes 配置文件相对家目录的固定路径; 官方文档目前只有这一种已知形态(不像其余工具存在多
/// 操作系统路径漂移), 故不设候选表, 与 CodexAdapter 的惯例一致
const CONFIG_REL_PATH: &str = ".hermes/config.yaml";

/// mcp_servers 顶层键名; YAML 映射的键统一用 `Value::String` 表达, 抽成常量避免多处手写字面量
const MCP_SERVERS_KEY: &str = "mcp_servers";

/// Hermes 适配器: 读取 `~/.hermes/config.yaml` 的顶层 `mcp_servers` 映射, 并按 skill_target
/// 读取 Skill 落地清单
pub struct HermesAdapter {
	home: PathBuf,
	skill_target: SkillTarget,
}

impl HermesAdapter {
	/// 构造一个适配器实例; `home` 生产环境通常取自 `dirs::home_dir()`, 测试时注入临时目录,
	/// 避免探测逻辑触碰真实机器配置; `skill_target` 声明 Hermes 的 Skill 落地形态(见 SkillTarget,
	/// 目前固定为 ClaudeSkillsDir(".hermes/skills"), 由调用方在 all_adapters 里传入)
	pub fn new(home: PathBuf, skill_target: SkillTarget) -> Self {
		Self { home, skill_target }
	}

	/// Hermes 配置文件的绝对路径(`home` 拼接固定相对路径)
	fn config_path(&self) -> PathBuf {
		self.home.join(CONFIG_REL_PATH)
	}

	/// 把本次 plan 里 res_type==Mcp 的若干 DiffItem 合并应用到同一份 config.yaml: 先把整份
	/// 文件读入内存(不存在则视为空映射), 逐项在内存里对 mcp_servers 映射做增/改/删, 全部处理
	/// 完毕后统一备份 + 落盘一次(而非逐项各自备份写盘) —— 策略与 JsonMcpAdapter/CodexAdapter::
	/// apply_mcp_items 一致, 只是数据结构换成 serde_yaml_ng::Mapping/Value, 借此保留 config.yaml
	/// 里除 mcp_servers 外的其它顶层键(如 model)不丢失。单项 payload 形状不符(脏数据)不会
	/// 中断其它项, 只把该项标记为 ok=false 并记录 err; 读取/解析/备份/落盘这类影响全文件的失败
	/// 则整体报错(Err), 因为此时已无法保证任何一项真的落地成功
	fn apply_mcp_items(&self, path: &Path, items: &[&DiffItem]) -> Result<Vec<ItemOutcome>> {
		let mut root: Mapping = match fs::read_to_string(path) {
			Ok(text) => serde_yaml_ng::from_str(&text)
				.with_context(|| format!("解析配置文件 YAML 失败: {}", path.display()))?,
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => Mapping::new(),
			Err(err) => {
				return Err(err).with_context(|| format!("读取配置文件失败: {}", path.display()))
			}
		};

		let servers = root
			.entry(Value::String(MCP_SERVERS_KEY.to_string()))
			.or_insert_with(|| Value::Mapping(Mapping::new()));
		let Some(servers_table) = servers.as_mapping_mut() else {
			anyhow::bail!("{MCP_SERVERS_KEY} 不是 YAML 映射: {}", path.display());
		};

		let mut outcomes = Vec::with_capacity(items.len());
		let mut changed = false;
		for item in items {
			match (item.action, &item.payload) {
				(DiffAction::Add, Some(DesiredPayload::Mcp(def)))
				| (DiffAction::Update, Some(DesiredPayload::Mcp(def))) => {
					servers_table.insert(Value::String(item.name.clone()), mcp_def_to_yaml(def));
					changed = true;
					outcomes.push(ok_outcome(item));
				}
				(DiffAction::Remove, _) => {
					servers_table.remove(item.name.as_str());
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
			let text = serde_yaml_ng::to_string(&root).context("序列化配置文件失败")?;
			fs::write(path, text)
				.with_context(|| format!("写入配置文件失败: {}", path.display()))?;
		}

		Ok(outcomes)
	}
}

impl AgentAdapter for HermesAdapter {
	/// 本适配器对应的 Agent 种类
	fn kind(&self) -> AgentKind {
		AgentKind::Hermes
	}

	/// Hermes 可托管 MCP 与 Skill(读取与写入均已接入, 见 read_state/apply)
	fn supports(&self, ty: ResourceType) -> bool {
		matches!(ty, ResourceType::Skill | ResourceType::Mcp)
	}

	/// 配置文件存在即视为该工具已安装; 不存在返回空表(未安装/未配置)
	fn detect(&self) -> Vec<DetectedAgent> {
		let path = self.config_path();
		if path.is_file() {
			vec![DetectedAgent {
				kind: AgentKind::Hermes,
				name: AgentKind::Hermes.label().to_string(),
				config_path: path.to_string_lossy().into_owned(),
				scope: AgentScope::Global,
				online: true,
			}]
		} else {
			Vec::new()
		}
	}

	/// 读取 `agent.config_path` 指向的 YAML 配置文件, 解析出 mcp_servers 映射, 并按 skill_target
	/// 读出 Skill 清单一并装进 ActualState。mcp 配置文件不存在视为该维度"实际态为空"(工具装了
	/// 但还没配任何 MCP, 不算错误, 且不影响 Skill 照常读取, 二者落地位置本就互相独立); 文件
	/// 存在但 YAML 解析失败才报错, 因为那属于配置文件本身损坏, 不应被静默吞掉
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
		let root: Mapping = serde_yaml_ng::from_str(&text)
			.with_context(|| format!("解析配置文件 YAML 失败: {path}"))?;
		Ok(ActualState {
			mcp: parse_mcp_servers(&root),
			skills: self.skill_target.read_skills(&self.home),
		})
	}

	/// 把 plan 里的每一项按 res_type 分派应用: Mcp 项合并进 config.yaml(见 apply_mcp_items,
	/// 内部只对该文件做一次备份 + 一次写回); Skill 项各自独立调用 skill_target.write_skill/
	/// remove_skill(见 apply_skill_item), 与 Mcp 项的落地位置(.hermes/skills)互不相干。返回的
	/// outcomes 顺序为"先 Mcp 项(按 items 原有顺序), 再 Skill 项(按 items 原有顺序)", 调用方
	/// 应按 name 匹配而非依赖顺序
	fn apply(&self, agent: &DetectedAgent, plan: &DiffPlanRespVO) -> Result<Vec<ItemOutcome>> {
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

	/// 转发给 self.skill_target.export_skill(见 SkillTarget::export_skill, M6 Task BE-2 从已
	/// 检测 Agent 反向导入已装 Skill 到本地库所需的"读回可落地内容"), 不重复实现
	fn export_skill(&self, name: &str, dest_dir: &Path) -> Result<bool> {
		self.skill_target.export_skill(&self.home, name, dest_dir)
	}
}

/// 把 McpServerDef 转为写回 config.yaml 用的 YAML 映射: 有 command 才写 command/args/env,
/// 有 url 才写 url; args/env 为空时省略(保持配置文件简洁, 与 JsonMcpAdapter::mcp_def_to_json/
/// CodexAdapter::mcp_def_to_toml 的简洁策略一致)
fn mcp_def_to_yaml(def: &McpServerDef) -> Value {
	let mut map = Mapping::new();
	if let Some(command) = &def.command {
		map.insert(
			Value::String("command".to_string()),
			Value::String(command.clone()),
		);
	}
	if !def.args.is_empty() {
		map.insert(
			Value::String("args".to_string()),
			Value::Sequence(def.args.iter().cloned().map(Value::String).collect()),
		);
	}
	if !def.env.is_empty() {
		let mut env_map = Mapping::new();
		for (key, val) in &def.env {
			env_map.insert(Value::String(key.clone()), Value::String(val.clone()));
		}
		map.insert(Value::String("env".to_string()), Value::Mapping(env_map));
	}
	if let Some(url) = &def.url {
		map.insert(Value::String("url".to_string()), Value::String(url.clone()));
	}
	Value::Mapping(map)
}

/// 从配置文件根 YAML 映射取出 `mcp_servers` 映射, 逐条转为 McpServerDef; 键即服务器名(非字符串
/// 键的异常场景兜底为空串, 不 panic)。字段逐个宽松提取: 字段缺失或类型不符都退回默认值, 不让
/// 单条脏数据拖垮整份读取(与 JsonMcpAdapter::parse_mcp_servers/CodexAdapter::parse_mcp_servers
/// 的宽松策略保持一致)。http 型服务器的 headers 字段暂不读取(见文件头注释, McpServerDef 无处
/// 落地)
fn parse_mcp_servers(root: &Mapping) -> Vec<McpServerDef> {
	let Some(servers) = root.get(MCP_SERVERS_KEY).and_then(Value::as_mapping) else {
		return Vec::new();
	};
	servers
		.iter()
		.map(|(name, raw)| McpServerDef {
			name: name.as_str().unwrap_or_default().to_string(),
			command: raw
				.get("command")
				.and_then(Value::as_str)
				.map(str::to_string),
			args: raw
				.get("args")
				.and_then(Value::as_sequence)
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
				.and_then(Value::as_mapping)
				.map(|obj| {
					obj.iter()
						.filter_map(|(k, v)| {
							Some((k.as_str()?.to_string(), v.as_str()?.to_string()))
						})
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

	/// fixture: stdio 型服务器 foo(含 args/env) + http 型服务器 bar(url, 附带本适配器不落地的
	/// headers 字段), 覆盖 brief 要求的 stdio+http 各一
	const FIXTURE_YAML: &str = "mcp_servers:\n  foo:\n    command: node\n    args:\n      - x\n    env:\n      K: V\n  bar:\n    url: http://localhost:1\n    headers:\n      Authorization: Bearer xxx\n";

	fn hermes_skill_target() -> SkillTarget {
		SkillTarget::ClaudeSkillsDir(PathBuf::from(".hermes/skills"))
	}

	// detect + read_state: 配置文件存在时应命中并解析出 foo(stdio 型)与 bar(http 型)两条
	#[test]
	fn detect_and_read_state_parse_stdio_and_http_servers() {
		let dir = tempdir().unwrap();
		let abs = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(abs.parent().unwrap()).unwrap();
		fs::write(&abs, FIXTURE_YAML).unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1);
		assert_eq!(detected[0].kind, AgentKind::Hermes);
		assert!(detected[0].online);
		assert_eq!(detected[0].scope, AgentScope::Global);
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_eq!(state.mcp.len(), 2);

		let foo = state
			.mcp
			.iter()
			.find(|s| s.name == "foo")
			.expect("应含 foo");
		assert_eq!(foo.command, Some("node".to_string()));
		assert_eq!(foo.args, vec!["x".to_string()]);
		let mut expected_env = BTreeMap::new();
		expected_env.insert("K".to_string(), "V".to_string());
		assert_eq!(foo.env, expected_env);
		assert_eq!(foo.url, None);

		let bar = state
			.mcp
			.iter()
			.find(|s| s.name == "bar")
			.expect("应含 bar");
		assert_eq!(bar.command, None);
		assert!(bar.args.is_empty());
		assert!(
			bar.env.is_empty(),
			"headers 不映射进 env, McpServerDef 无处落地"
		);
		assert_eq!(bar.url, Some("http://localhost:1".to_string()));

		assert!(
			state.skills.is_empty(),
			"本测试未落地任何 .hermes/skills fixture, 应为空"
		);
	}

	// detect 应在配置文件不存在时返回空表(Hermes 未安装/未配置), 不 panic 不报错
	#[test]
	fn detect_returns_empty_when_config_file_missing() {
		let dir = tempdir().unwrap();
		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		assert!(adapter.detect().is_empty());
	}

	// read_state: 配置文件不存在时视为"实际态为空"而非错误(工具已装但尚未配置任何 MCP)
	#[test]
	fn read_state_returns_empty_actual_state_when_config_file_missing() {
		let dir = tempdir().unwrap();
		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = DetectedAgent {
			kind: AgentKind::Hermes,
			name: "Hermes".to_string(),
			config_path: dir
				.path()
				.join(".hermes/config.yaml")
				.to_string_lossy()
				.into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
		assert!(state.skills.is_empty());
	}

	// read_state: 文件存在但内容不是合法 YAML, 属配置文件本身损坏, 应报错而不是静默兜底成空态
	#[test]
	fn read_state_returns_err_on_malformed_yaml() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(&path, "mcp_servers: [this, is, not: a valid, mapping").unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = DetectedAgent {
			kind: AgentKind::Hermes,
			name: "Hermes".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		assert!(adapter.read_state(&probe).is_err());
	}

	// read_state: YAML 合法但没有 mcp_servers 键(如刚安装还没写任何 MCP 配置段),
	// mcp 应为空表而非报错
	#[test]
	fn read_state_returns_empty_mcp_when_mcp_servers_key_absent() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(&path, "model: some-model\n").unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = DetectedAgent {
			kind: AgentKind::Hermes,
			name: "Hermes".to_string(),
			config_path: path.to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		};

		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
	}

	// read_state: 单条 mcp_servers 条目格式异常(值不是映射)应被当作"全默认"兜底, 不拖累
	// 其余条目与整体读取(呼应"宽松解析"要求, 对齐 JsonMcpAdapter/CodexAdapter 同类测试)
	#[test]
	fn read_state_tolerates_malformed_single_server_entry() {
		let dir = tempdir().unwrap();
		let path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(
			&path,
			"mcp_servers:\n  broken: not-a-mapping\n  ok:\n    command: node\n",
		)
		.unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = DetectedAgent {
			kind: AgentKind::Hermes,
			name: "Hermes".to_string(),
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

	// read_state: 应同时返回 mcp(mcp_servers 映射)与 skills(.hermes/skills/<name>/SKILL.md),
	// 二者落地位置互不相干, 应各自独立解析并一并装进 ActualState
	#[test]
	fn read_state_combines_mcp_and_hermes_skills_dir_skills() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, FIXTURE_YAML).unwrap();

		let skill_dir = dir.path().join(".hermes/skills/demo-skill");
		fs::create_dir_all(&skill_dir).unwrap();
		fs::write(
			skill_dir.join("SKILL.md"),
			"---\nname: demo-skill\nversion: 1.2.0\n---\n",
		)
		.unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());

		let detected = adapter.detect();
		let state = adapter.read_state(&detected[0]).unwrap();

		assert_eq!(state.mcp.len(), 2);
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, "demo-skill");
		assert_eq!(state.skills[0].version, "1.2.0");
	}

	// read_state: Hermes 配置文件缺失只代表"还没配任何 MCP", 不应连带影响 Skill 的读取
	// (二者落地位置独立, 缺一不代表另一个也该为空)
	#[test]
	fn read_state_reads_skills_even_when_config_file_missing() {
		let dir = tempdir().unwrap();
		let skill_dir = dir.path().join(".hermes/skills/demo-skill");
		fs::create_dir_all(&skill_dir).unwrap();
		fs::write(skill_dir.join("SKILL.md"), "---\nversion: 0.1.0\n---\n").unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = DetectedAgent {
			kind: AgentKind::Hermes,
			name: "Hermes".to_string(),
			config_path: dir
				.path()
				.join(".hermes/config.yaml")
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

	/// 构造一个 stdio 型 McpServerDef, env 固定为空, 供 apply 系列测试复用
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
			kind: AgentKind::Hermes,
			name: "Hermes".to_string(),
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
		let config_path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(
			&config_path,
			"model: some-model\nmcp_servers:\n  userSrv:\n    command: python\n    args:\n      - server.py\n",
		)
		.unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = probe_at(&config_path);
		let plan = DiffPlanRespVO {
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

		let root: Mapping =
			serde_yaml_ng::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert_eq!(
			root["mcp_servers"]["newSrv"]["command"].as_str(),
			Some("node")
		);
		assert_eq!(
			root["mcp_servers"]["userSrv"]["command"].as_str(),
			Some("python")
		);
		assert_eq!(
			root["model"].as_str(),
			Some("some-model"),
			"其它顶层键不应被吞掉"
		);

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
		let config_path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(
			&config_path,
			"mcp_servers:\n  userSrv:\n    command: python\n  target:\n    command: node\n    args:\n      - old.js\n",
		)
		.unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = probe_at(&config_path);
		let plan = DiffPlanRespVO {
			items: vec![mcp_diff_item(
				DiffAction::Update,
				"target",
				"node",
				&["new.js"],
			)],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok);

		let root: Mapping =
			serde_yaml_ng::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
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
		let config_path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(
			&config_path,
			"mcp_servers:\n  userSrv:\n    command: python\n  toRemove:\n    command: node\n",
		)
		.unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = probe_at(&config_path);
		let plan = DiffPlanRespVO {
			items: vec![mcp_remove_item("toRemove")],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok);
		assert_eq!(outcomes[0].action, DiffAction::Remove);

		let root: Mapping =
			serde_yaml_ng::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
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
		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let config_path = dir.path().join(".hermes/config.yaml");
		let probe = probe_at(&config_path);
		let plan = DiffPlanRespVO {
			items: vec![mcp_diff_item(
				DiffAction::Add,
				"newSrv",
				"node",
				&["index.js"],
			)],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok, "err = {}", outcomes[0].err);

		let root: Mapping =
			serde_yaml_ng::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
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
		let config_path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, "").unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
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
		let plan = DiffPlanRespVO {
			items: vec![bad_item, good_item],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert_eq!(outcomes.len(), 2);

		let bad_outcome = outcomes.iter().find(|o| o.name == "badSrv").unwrap();
		assert!(!bad_outcome.ok);
		assert!(!bad_outcome.err.is_empty());

		let good_outcome = outcomes.iter().find(|o| o.name == "goodSrv").unwrap();
		assert!(good_outcome.ok, "err = {}", good_outcome.err);

		let root: Mapping =
			serde_yaml_ng::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert_eq!(
			root["mcp_servers"]["goodSrv"]["command"].as_str(),
			Some("node")
		);
		assert!(root["mcp_servers"].get("badSrv").is_none());
	}

	// apply: res_type==Skill 的项应分派给 skill_target.write_skill(ClaudeSkillsDir 形态),
	// 与 MCP 项各自独立生效
	#[test]
	fn apply_skill_item_delegates_to_skill_target_write_skill() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".hermes/config.yaml");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, "").unwrap();

		let src_dir = dir.path().join("src-demo-skill");
		fs::create_dir_all(&src_dir).unwrap();
		fs::write(src_dir.join("SKILL.md"), "---\nversion: 1.0.0\n---\n内容\n").unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());
		let probe = probe_at(&config_path);
		let plan = DiffPlanRespVO {
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

		let installed = dir.path().join(".hermes/skills/demo-skill/SKILL.md");
		assert!(installed.exists());
	}

	// export_skill: 应转发给 skill_target.export_skill(ClaudeSkillsDir 形态, .hermes/skills),
	// 供 M6 Task BE-2 从已检测 Agent 反向导入使用; 名称不存在应返回 Ok(false)
	#[test]
	fn export_skill_delegates_to_skill_target() {
		let dir = tempdir().unwrap();
		let skill_dir = dir.path().join(".hermes/skills/demo-skill");
		fs::create_dir_all(&skill_dir).unwrap();
		fs::write(
			skill_dir.join("SKILL.md"),
			"---\nversion: 1.2.0\n---\n内容\n",
		)
		.unwrap();

		let adapter = HermesAdapter::new(dir.path().to_path_buf(), hermes_skill_target());

		let dest = dir.path().join("exported/demo-skill");
		let ok = adapter.export_skill("demo-skill", &dest).unwrap();
		assert!(ok);
		assert_eq!(
			fs::read_to_string(dest.join("SKILL.md")).unwrap(),
			"---\nversion: 1.2.0\n---\n内容\n"
		);

		assert!(!adapter
			.export_skill("no-such-skill", &dir.path().join("exported/nope"))
			.unwrap());
	}
}
