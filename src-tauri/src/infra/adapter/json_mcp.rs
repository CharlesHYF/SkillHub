// 文件作用: 通用 JSON mcpServers 适配器 —— 覆盖 ClaudeCode/ClaudeDesktop/Cursor/Windsurf/Cline/
//           GeminiCli 六款工具的探测(detect)、MCP+Skill 实际态读取(read_state)与差异计划落地
//           写入(apply)。这六款工具的配置文件形态一致: 顶层 JSON 对象下挂一个 mcpServers 字典,
//           差异只在文件路径; 故用同一结构体 + 候选路径表覆盖全部六款, 具体路径表见 adapter::mod
//           的 json_mcp_agent_configs。Skill 的落地读写(read_state.skills/apply 里 Skill 项)
//           已接入 skill_target(见 SkillTarget, Task 5/7b); MCP 的落地写入(apply 里 Mcp 项)
//           采取"整份读入 -> 内存合并 -> 备份 -> 整份写回"策略, 务必保留配置文件里用户自己的
//           其它服务器与其它顶层键。本任务(新增 CodeBuddy/WorkBuddy)追加复用本适配器接入这两款
//           工具(同为 JSON mcpServers 形态), 其中 CodeBuddy 官方文档未提供 Skill 落地约定,
//           构造时传入 SkillTarget::None 占位, supports() 据此把 Skill 能力如实汇报为 false
//           (见下方 supports 实现), 不再对"是否支持 Skill"一律写死为 true。
// 创建日期: 2026-07-09

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::domain::agent::{ActualState, AgentKind, AgentScope, DetectedAgent, McpServerDef};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DesiredPayload, DiffAction, DiffItem, DiffPlan, ItemOutcome};

use super::skill_target::SkillTarget;
use super::util::{apply_skill_item, backup_file, err_outcome, ok_outcome};
use super::AgentAdapter;

/// 通用 JSON mcpServers 适配器: `rel_candidates` 为相对 `home` 的候选配置文件路径(工具版本/
/// 操作系统可能导致路径漂移, 按声明顺序取第一个真实存在的); `servers_key` 为该工具配置文件里
/// 挂 MCP 服务器字典的键名(六款工具目前均为 "mcpServers", 保留为参数以防未来漂移); `skill_target`
/// 为该工具的 Skill 落地形态(见 SkillTarget), 由调用方按工具种类传入。
pub struct JsonMcpAdapter {
	kind: AgentKind,
	home: PathBuf,
	rel_candidates: Vec<PathBuf>,
	servers_key: &'static str,
	skill_target: SkillTarget,
}

impl JsonMcpAdapter {
	/// 构造一个适配器实例; `home` 生产环境通常取自 `dirs::home_dir()`, 测试时注入临时目录,
	/// 避免探测逻辑触碰真实机器配置
	pub fn new(
		kind: AgentKind,
		home: PathBuf,
		rel_candidates: Vec<PathBuf>,
		servers_key: &'static str,
		skill_target: SkillTarget,
	) -> Self {
		Self {
			kind,
			home,
			rel_candidates,
			servers_key,
			skill_target,
		}
	}

	/// 按声明顺序在候选相对路径里找第一个真实存在的文件, 都不存在返回 None
	fn existing_config_path(&self) -> Option<PathBuf> {
		self.rel_candidates
			.iter()
			.map(|rel| self.home.join(rel))
			.find(|abs| abs.is_file())
	}

	/// 把本次 plan 里 res_type==Mcp 的若干 DiffItem 合并应用到同一份配置文件: 先把整份文件读入
	/// 内存(不存在则视为空对象 {}), 逐项在内存里对 servers_key 对象做增/改/删, 全部处理完毕后
	/// 统一备份 + 落盘一次(而非逐项各自备份写盘) —— 这样一次 apply 调用只留一份"应用前"快照,
	/// 也避免同名字段被中途状态污染。单项 payload 形状不符(脏数据)不会中断其它项, 只把该项
	/// 标记为 ok=false 并记录 err; 读取/解析/备份/落盘这类影响全文件的失败则整体报错(Err),
	/// 因为此时已无法保证任何一项真的落地成功
	fn apply_mcp_items(&self, path: &Path, items: &[&DiffItem]) -> Result<Vec<ItemOutcome>> {
		let mut root: Value = match fs::read_to_string(path) {
			Ok(text) => serde_json::from_str(&text)
				.with_context(|| format!("解析配置文件 JSON 失败: {}", path.display()))?,
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
				Value::Object(serde_json::Map::new())
			}
			Err(err) => {
				return Err(err).with_context(|| format!("读取配置文件失败: {}", path.display()))
			}
		};

		let Some(obj) = root.as_object_mut() else {
			anyhow::bail!("配置文件根节点不是 JSON 对象: {}", path.display());
		};
		let servers = obj
			.entry(self.servers_key)
			.or_insert_with(|| Value::Object(serde_json::Map::new()));
		let Some(servers_obj) = servers.as_object_mut() else {
			anyhow::bail!("{} 不是 JSON 对象: {}", self.servers_key, path.display());
		};

		let mut outcomes = Vec::with_capacity(items.len());
		let mut changed = false;
		for item in items {
			match (item.action, &item.payload) {
				(DiffAction::Add, Some(DesiredPayload::Mcp(def)))
				| (DiffAction::Update, Some(DesiredPayload::Mcp(def))) => {
					servers_obj.insert(item.name.clone(), mcp_def_to_json(def));
					changed = true;
					outcomes.push(ok_outcome(item));
				}
				(DiffAction::Remove, _) => {
					servers_obj.remove(&item.name);
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
			let text = serde_json::to_string_pretty(&root).context("序列化配置文件失败")?;
			fs::write(path, text)
				.with_context(|| format!("写入配置文件失败: {}", path.display()))?;
		}

		Ok(outcomes)
	}
}

impl AgentAdapter for JsonMcpAdapter {
	/// 本适配器对应的 Agent 种类(构造时确定)
	fn kind(&self) -> AgentKind {
		self.kind
	}

	/// 均可托管 MCP; Skill 能力取决于构造时传入的 skill_target 是否为真正落地形态 ——
	/// SkillTarget::None(目前仅 CodeBuddy)代表该工具官方文档未提供任何 Skill 落地约定,
	/// 如实汇报 supports(Skill)=false, 其余 skill_target(ClaudeSkillsDir/RulesDir/
	/// InstructionsFile)均视为已接入 Skill 读写, 汇报 true
	fn supports(&self, ty: ResourceType) -> bool {
		match ty {
			ResourceType::Mcp => true,
			ResourceType::Skill => !matches!(self.skill_target, SkillTarget::None),
		}
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

	/// 读取 `agent.config_path` 指向的配置文件, 解析出 mcpServers 字典, 并按 skill_target
	/// 读出 Skill 清单一并装进 ActualState。mcp 配置文件不存在视为该维度"实际态为空"(工具装了
	/// 但还没配任何 MCP, 不算错误, 且不影响 Skill 照常读取, 二者落地位置本就互相独立); 文件
	/// 存在但 JSON 解析失败才报错, 因为那属于配置文件本身损坏, 不应被静默吞掉
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
		let root: Value = serde_json::from_str(&text)
			.with_context(|| format!("解析配置文件 JSON 失败: {path}"))?;
		Ok(ActualState {
			mcp: parse_mcp_servers(&root, self.servers_key),
			skills: self.skill_target.read_skills(&self.home),
		})
	}

	/// 把 plan 里的每一项按 res_type 分派应用: Mcp 项合并进配置文件(见 apply_mcp_items,
	/// 内部只对该文件做一次备份 + 一次写回); Skill 项各自独立调用 skill_target.write_skill/
	/// remove_skill(见 apply_skill_item), 与 Mcp 项的落地位置互不相干。返回的 outcomes 顺序
	/// 为"先 Mcp 项(按 items 原有顺序), 再 Skill 项(按 items 原有顺序)", 调用方应按 name 匹配
	/// 而非依赖顺序
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

	/// 转发给 self.skill_target.export_skill(见 SkillTarget::export_skill, M6 Task BE-2 从已
	/// 检测 Agent 反向导入已装 Skill 到本地库所需的"读回可落地内容"), 不重复实现
	fn export_skill(&self, name: &str, dest_dir: &Path) -> Result<bool> {
		self.skill_target.export_skill(&self.home, name, dest_dir)
	}
}

/// 把 McpServerDef 转为写回配置文件用的 JSON 对象: 有 command 才写 command/args/env,
/// 有 url 才写 url; args/env 为空时省略(保持配置文件简洁, 与手写风格一致), 与下方
/// parse_mcp_servers 的读取方向正好相反
fn mcp_def_to_json(def: &McpServerDef) -> Value {
	let mut obj = serde_json::Map::new();
	if let Some(command) = &def.command {
		obj.insert("command".to_string(), Value::String(command.clone()));
	}
	if !def.args.is_empty() {
		obj.insert(
			"args".to_string(),
			Value::Array(def.args.iter().cloned().map(Value::String).collect()),
		);
	}
	if !def.env.is_empty() {
		let env_obj: serde_json::Map<String, Value> = def
			.env
			.iter()
			.map(|(k, v)| (k.clone(), Value::String(v.clone())))
			.collect();
		obj.insert("env".to_string(), Value::Object(env_obj));
	}
	if let Some(url) = &def.url {
		obj.insert("url".to_string(), Value::String(url.clone()));
	}
	Value::Object(obj)
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
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);

		let detected = adapter.detect();
		assert_eq!(detected.len(), 1);
		assert_eq!(detected[0].kind, AgentKind::ClaudeCode);
		assert!(detected[0].online);
		assert_eq!(detected[0].scope, AgentScope::Global);
		assert_eq!(detected[0].config_path, abs.to_string_lossy());

		let state = adapter.read_state(&detected[0]).unwrap();
		assert_fixture_mcp(&state.mcp);
		assert!(
			state.skills.is_empty(),
			"本测试未落地任何 .claude/skills fixture, 应为空"
		);
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
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
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
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
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
			SkillTarget::RulesDir {
				dir: PathBuf::from(".cursor/rules"),
				ext: "mdc".to_string(),
			},
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
			SkillTarget::InstructionsFile(PathBuf::from("GEMINI.md")),
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
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
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
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
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

	// read_state: 应同时返回 mcp(配置文件里的 mcpServers)与 skills(skill_target 描述的
	// .claude/skills 目录), 二者落地位置互不相干, 应各自独立解析并一并装进 ActualState
	#[test]
	fn read_state_combines_mcp_and_claude_skills_dir_skills() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".claude.json");
		fs::write(&config_path, FIXTURE_JSON).unwrap();

		let skill_dir = dir.path().join(".claude/skills/demo-skill");
		fs::create_dir_all(&skill_dir).unwrap();
		fs::write(
			skill_dir.join("SKILL.md"),
			"---\nname: demo-skill\nversion: 1.2.0\n---\n",
		)
		.unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);

		let detected = adapter.detect();
		let state = adapter.read_state(&detected[0]).unwrap();

		assert_fixture_mcp(&state.mcp);
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, "demo-skill");
		assert_eq!(state.skills[0].version, "1.2.0");
	}

	// read_state: skill_target 换成 RulesDir 形态(Cursor/Windsurf/Cline/VsCode 的落地方式)
	// 时也应正确读出, 证明该字段与具体 SkillTarget 变体无关, 由外部注入决定
	#[test]
	fn read_state_combines_mcp_and_rules_dir_skills() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".cursor/mcp.json");
		fs::create_dir_all(config_path.parent().unwrap()).unwrap();
		fs::write(&config_path, FIXTURE_JSON).unwrap();
		fs::create_dir_all(dir.path().join(".cursor/rules")).unwrap();
		fs::write(dir.path().join(".cursor/rules/demo-skill.mdc"), "规则内容").unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::Cursor,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".cursor/mcp.json")],
			"mcpServers",
			SkillTarget::RulesDir {
				dir: PathBuf::from(".cursor/rules"),
				ext: "mdc".to_string(),
			},
		);

		let detected = adapter.detect();
		let state = adapter.read_state(&detected[0]).unwrap();

		assert_fixture_mcp(&state.mcp);
		assert_eq!(state.skills.len(), 1);
		assert_eq!(state.skills[0].name, "demo-skill");
		assert_eq!(state.skills[0].version, "");
	}

	// read_state: mcp 配置文件缺失只代表"还没配任何 MCP"(见 read_state 文档注释), 不应连带
	// 影响 Skill 的读取 —— 二者落地位置独立, 缺一不代表另一个也该为空
	#[test]
	fn read_state_reads_skills_even_when_mcp_config_file_missing() {
		let dir = tempdir().unwrap();
		let skill_dir = dir.path().join(".claude/skills/demo-skill");
		fs::create_dir_all(&skill_dir).unwrap();
		fs::write(skill_dir.join("SKILL.md"), "---\nversion: 0.1.0\n---\n").unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);
		let probe = DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: dir
				.path()
				.join(".claude.json")
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

	/// 构造一个 command 型 McpServerDef, env 固定为空(测试里不需要覆盖 env 分支时用这个
	/// 更简洁), 供 apply 系列测试复用
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

	fn probe_for(dir: &std::path::Path, config_rel: &str) -> DetectedAgent {
		DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: dir.join(config_rel).to_string_lossy().into_owned(),
			scope: AgentScope::Global,
			online: true,
		}
	}

	// apply: Add 一个新 MCP 服务器应写入配置文件, 且保留文件里用户自己的其它服务器
	// (userSrv)与其它顶层键(otherTopLevelKey); 写入前应生成一份时间戳备份
	#[test]
	fn apply_add_writes_new_server_while_preserving_existing_content_and_backs_up() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".claude.json");
		fs::write(
			&config_path,
			r#"{"mcpServers":{"userSrv":{"command":"python","args":["server.py"]}},"otherTopLevelKey":"保留我"}"#,
		)
		.unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);
		let probe = probe_for(dir.path(), ".claude.json");
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

		let root: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert_eq!(root["mcpServers"]["newSrv"]["command"], "node");
		assert_eq!(root["mcpServers"]["userSrv"]["command"], "python");
		assert_eq!(root["otherTopLevelKey"], "保留我");

		let backups: Vec<_> = fs::read_dir(dir.path())
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
		let config_path = dir.path().join(".claude.json");
		fs::write(
			&config_path,
			r#"{"mcpServers":{"userSrv":{"command":"python","args":["server.py"]},"target":{"command":"node","args":["old.js"]}}}"#,
		)
		.unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);
		let probe = probe_for(dir.path(), ".claude.json");
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

		let root: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert_eq!(root["mcpServers"]["target"]["args"][0], "new.js");
		assert_eq!(root["mcpServers"]["userSrv"]["command"], "python");
	}

	// apply: Remove 应删掉目标服务器, 但保留用户自己的其它服务器(userSrv)
	#[test]
	fn apply_remove_deletes_target_server_keeping_others() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".claude.json");
		fs::write(
			&config_path,
			r#"{"mcpServers":{"userSrv":{"command":"python"},"toRemove":{"command":"node"}}}"#,
		)
		.unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);
		let probe = probe_for(dir.path(), ".claude.json");
		let plan = DiffPlan {
			items: vec![mcp_remove_item("toRemove")],
		};

		let outcomes = adapter.apply(&probe, &plan).unwrap();
		assert!(outcomes[0].ok);
		assert_eq!(outcomes[0].action, DiffAction::Remove);

		let root: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert!(root["mcpServers"].get("toRemove").is_none());
		assert_eq!(root["mcpServers"]["userSrv"]["command"], "python");
	}

	// apply: 配置文件不存在(全新工具, 尚未生成过配置)时应能创建文件与所需目录, Add 正常生效
	#[test]
	fn apply_add_creates_config_file_when_missing() {
		let dir = tempdir().unwrap();
		let adapter = JsonMcpAdapter::new(
			AgentKind::Cursor,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".cursor/mcp.json")],
			"mcpServers",
			SkillTarget::RulesDir {
				dir: PathBuf::from(".cursor/rules"),
				ext: "mdc".to_string(),
			},
		);
		let probe = probe_for(dir.path(), ".cursor/mcp.json");
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

		let root: Value =
			serde_json::from_str(&fs::read_to_string(dir.path().join(".cursor/mcp.json")).unwrap())
				.unwrap();
		assert_eq!(root["mcpServers"]["newSrv"]["command"], "node");
	}

	// apply: 单项失败不应中断其它项 —— 一个 payload 形状不符的坏项(Add 却没带 Mcp payload)
	// 与一个正常好项同批应用, 好项应正常生效, 坏项应产出 ok=false 且带非空 err, 不影响好项
	#[test]
	fn apply_bad_item_does_not_block_good_item_in_same_plan() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".claude.json");
		fs::write(&config_path, r#"{"mcpServers":{}}"#).unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);
		let probe = probe_for(dir.path(), ".claude.json");
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

		let root: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert_eq!(root["mcpServers"]["goodSrv"]["command"], "node");
		assert!(root["mcpServers"].get("badSrv").is_none());
	}

	// apply: res_type==Skill 的项应分派给 skill_target.write_skill, 与 MCP 项各自独立生效
	#[test]
	fn apply_skill_item_delegates_to_skill_target_write_skill() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".claude.json");
		fs::write(&config_path, r#"{"mcpServers":{}}"#).unwrap();

		let src_dir = dir.path().join("src-demo-skill");
		fs::create_dir_all(&src_dir).unwrap();
		fs::write(src_dir.join("SKILL.md"), "---\nversion: 1.0.0\n---\n内容\n").unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);
		let probe = probe_for(dir.path(), ".claude.json");
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

		let installed = dir.path().join(".claude/skills/demo-skill/SKILL.md");
		assert!(installed.exists());
	}

	// export_skill: 应转发给 skill_target.export_skill, 把已装 Skill 内容导出到指定目录
	// (供 M6 Task BE-2 从已检测 Agent 反向导入使用); 名称不存在应返回 Ok(false)
	#[test]
	fn export_skill_delegates_to_skill_target() {
		let dir = tempdir().unwrap();
		let skill_dir = dir.path().join(".claude/skills/demo-skill");
		fs::create_dir_all(&skill_dir).unwrap();
		fs::write(
			skill_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\n内容\n",
		)
		.unwrap();

		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
		);

		let dest = dir.path().join("exported/demo-skill");
		let ok = adapter.export_skill("demo-skill", &dest).unwrap();
		assert!(ok);
		assert_eq!(
			fs::read_to_string(dest.join("SKILL.md")).unwrap(),
			"---\nversion: 1.0.0\n---\n内容\n"
		);

		assert!(!adapter
			.export_skill("no-such-skill", &dir.path().join("exported/nope"))
			.unwrap());
	}

	// supports: skill_target 为 SkillTarget::None(CodeBuddy 纯 MCP 场景)时应如实汇报
	// supports(Skill)=false, 但 supports(Mcp) 不受影响仍为 true
	#[test]
	fn supports_reports_no_skill_when_skill_target_is_none() {
		let dir = tempdir().unwrap();
		let adapter = JsonMcpAdapter::new(
			AgentKind::ClaudeCode,
			dir.path().to_path_buf(),
			vec![PathBuf::from(".claude.json")],
			"mcpServers",
			SkillTarget::None,
		);

		assert!(adapter.supports(ResourceType::Mcp));
		assert!(!adapter.supports(ResourceType::Skill));
	}

	// supports: 其余三种真正落地形态(ClaudeSkillsDir/RulesDir/InstructionsFile)均应继续
	// 汇报 supports(Skill)=true, 与新增的 None 分支互不影响(回归既有 6+VSCode 款工具的行为)
	#[test]
	fn supports_reports_skill_true_for_real_skill_targets() {
		let dir = tempdir().unwrap();
		let real_targets = vec![
			SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills")),
			SkillTarget::RulesDir {
				dir: PathBuf::from(".cursor/rules"),
				ext: "mdc".to_string(),
			},
			SkillTarget::InstructionsFile(PathBuf::from("GEMINI.md")),
		];
		for target in real_targets {
			let adapter = JsonMcpAdapter::new(
				AgentKind::ClaudeCode,
				dir.path().to_path_buf(),
				vec![PathBuf::from(".claude.json")],
				"mcpServers",
				target,
			);
			assert!(adapter.supports(ResourceType::Mcp));
			assert!(adapter.supports(ResourceType::Skill));
		}
	}
}
