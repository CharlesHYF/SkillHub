// 文件作用: "从已检测 Agent 导入已装 Skills/MCP 到本地库"服务编排层(M6 Task BE-2) —— 逐个已
//           探测落库的 Agent(repo_agent::list)取其 AgentKind 对应的 AgentAdapter, 用 read_state
//           读出该 Agent 当前的实际态(ActualState{mcp,skills}), 把其中每一条按 (res_type,name)
//           在本地库里去重后落地(复用 services::market 的 MCP 落盘逻辑与 AgentAdapter::
//           export_skill 取 Skill 内容), 并与拥有它的 Agent 建立 desired 关联, 使 Sync Center
//           能显示该 Agent 已装该资源。只接受 &Connection 与 &Path(home/data_dir), 不摸
//           AppState/Tauri 运行时, 呼应 services::sync/services::library 既有的分层约定;
//           命令层(commands::library::library_import_from_agents)负责加锁取出 conn/home/
//           data_dir 后转调本模块。
// 创建日期: 2026-07-11

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::domain::agent::{McpServerDef, SkillRef};
use crate::domain::resource::{ResourceType, SourceType};
use crate::infra::adapter::{all_adapters, AgentAdapter};
use crate::infra::repo_activity;
use crate::infra::repo_agent;
use crate::infra::repo_assoc;
use crate::infra::repo_resource::{self, NewResource};
use crate::services::market::{sanitize_path_segment, write_mcp_def};
use crate::services::sync::{agent_row_to_detected, find_adapter};

/// "从已检测 Agent 导入已装 Skills/MCP 到本地库"一次调用的汇总结果, 供命令层
/// (commands::library::library_import_from_agents)直接返回给前端
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportFromAgentsOutcomeRespVO {
	/// 本次新落地(此前库里没有同类型同名资源)并写入 resource 表的条数
	pub imported: i64,
	/// 本次识别到、但库里已有同类型同名资源、故只复用/未重复落地的条数
	pub skipped: i64,
	/// 本次扫描过的 Agent 数(agent 表当前全量, 不区分该 Agent 是否读取成功)
	pub agents: i64,
}

/// 从已检测 Agent(agent 表全量, 由此前的 services::sync::detect_all 探测落库)反向导入其里
/// "已经装着的"Skill/MCP 到本地库: 逐个 Agent 取其 AgentKind 对应的 AgentAdapter, 用 read_state
/// 读出该 Agent 当前的实际态(ActualState{mcp,skills}), 把其中每一条按 (res_type,name) 在本地库
/// 里去重 —— 已有同类型同名资源则复用其 id(skipped 计数, 不重复落地); 否则落地内容到 data_dir
/// 并 upsert 进 resource 表(source_type 标记为 AgentImport, imported 计数), 再把该资源与拥有它
/// 的 Agent 建立 desired 关联(repo_assoc::set, desired=true), 使 Sync Center 能显示"该 Agent
/// 已装该资源"。同名资源被多个 Agent 拥有时, 关联到每一个(见 import_mcp_item/import_skill_item)。
///
/// 幂等: 重复调用不产生重复 resource 行(按 (res_type,name) 去重复用)、不重复关联(repo_assoc::
/// set 本身按 (resource_id,agent_id) upsert)。
///
/// 关于"不触发同步写回": 本函数只登记库(resource 表)与关联(resource_agent 表), 不改动任何
/// Agent 的配置文件; 落地的内容(MCP 单定义 JSON / Skill 的 SKILL.md 等)均直接取自该 Agent
/// read_state 得到的实际态本身(逐字段照抄/原样复制), 故随后 services::sync::diff_for_agent
/// 重新计算差异时, 期望态(来自刚落库的 resource)与实际态(仍是同一份, 未被改动)理应完全一致
/// (MCP 按 McpServerDef 全字段比较, Skill 按 version 比较, 见 domain::sync::reconcile), 产出
/// 空 DiffPlanRespVO, 不会误触发对 Agent 的写回(见本模块测试
/// `import_from_agents_does_not_trigger_sync_write_back` 的端到端验证)。
///
/// 单 Agent 读取失败(如配置文件本身已损坏)不拖累其它 Agent, 静默跳过该 Agent(呼应 adapter 层
/// "宽松解析, 单项失败不拖累整体"的一贯风格); 单个 Skill 若在其声明的 SkillTarget 落地形态里
/// 其实找不到可导出的内容(边界情形, 如配置与磁盘不一致)也静默跳过该条, 不建立残缺资源行/关联
pub fn import_from_agents(
	conn: &Connection,
	home: &Path,
	data_dir: &Path,
) -> Result<ImportFromAgentsOutcomeRespVO> {
	let agent_rows = repo_agent::list(conn)?;
	let adapters = all_adapters(home);
	let mut imported = 0i64;
	let mut skipped = 0i64;

	for row in &agent_rows {
		let Ok(adapter) = find_adapter(&adapters, row.agent_kind) else {
			continue;
		};
		let detected = agent_row_to_detected(row);
		let Ok(actual) = adapter.read_state(&detected) else {
			continue;
		};

		for def in &actual.mcp {
			let resource_id = import_mcp_item(conn, data_dir, def, &mut imported, &mut skipped)?;
			repo_assoc::set(conn, resource_id, row.id, true)?;
		}

		for skill_ref in &actual.skills {
			let landed = import_skill_item(
				conn,
				data_dir,
				adapter,
				skill_ref,
				&mut imported,
				&mut skipped,
			)?;
			if let Some(resource_id) = landed {
				repo_assoc::set(conn, resource_id, row.id, true)?;
			}
		}
	}

	Ok(ImportFromAgentsOutcomeRespVO {
		imported,
		skipped,
		agents: agent_rows.len() as i64,
	})
}

/// 处理单条来自某 Agent 实际态的 MCP 服务定义: 按 (Mcp, name) 在本地库里去重, 已存在同类型
/// 同名资源则直接复用其 id(skipped 计数, 不重复落地); 否则落地(复用 services::market::
/// write_mcp_def 生成单定义 JSON 文件, 与市场安装同一套落盘逻辑, 不重复实现; env_overrides 传
/// 空表, 本场景不涉及模板占位覆盖)+ upsert 进 resource 表(source_type=AgentImport)+ 记一条
/// "新增"活动(imported 计数), 返回资源 id(供调用方建立 desired 关联)。MCP 定义本身来自
/// read_state, 恒有完整内容, 不存在"内容缺失"分支, 故返回值是 i64 而非 Option<i64>(与
/// import_skill_item 的签名形状有意不同)
fn import_mcp_item(
	conn: &Connection,
	data_dir: &Path,
	def: &McpServerDef,
	imported: &mut i64,
	skipped: &mut i64,
) -> Result<i64> {
	if let Some(existing) =
		repo_resource::find_by_type_and_name(conn, ResourceType::Mcp, &def.name)?
	{
		*skipped += 1;
		return Ok(existing.id);
	}

	let safe_name = sanitize_path_segment(&def.name);
	let target = write_mcp_def(data_dir, &safe_name, def.clone(), &BTreeMap::new())?;

	let resource_id = repo_resource::insert(
		conn,
		&NewResource {
			res_type: ResourceType::Mcp,
			name: def.name.clone(),
			display_name: def.name.clone(),
			version: String::new(),
			source_type: SourceType::AgentImport,
			local_path: target.to_string_lossy().into_owned(),
			enabled: true,
		},
	)?;
	repo_activity::add(
		conn,
		1,
		i64::from(ResourceType::Mcp),
		&format!("从 Agent 导入 {}", def.name),
		"检测导入",
	)?;
	*imported += 1;
	Ok(resource_id)
}

/// 处理单条来自某 Agent 实际态的 Skill 引用(SkillRef 只带 name/version 两个元数据字段, 不含
/// 可落地内容): 按 (Skill, name) 在本地库里去重, 已存在同类型同名资源则直接复用其 id(skipped
/// 计数, 不重复落地/不重复导出); 否则通过 adapter.export_skill(见 AgentAdapter::export_skill)
/// 按该 Agent 的 SkillTarget 落地形态取回真正的内容, 写到 data_dir/skills/<safe_name>/ 下 +
/// upsert 进 resource 表(source_type=AgentImport, version 取该 Agent 上的当前版本)+ 记一条
/// "新增"活动(imported 计数)。若该 Skill 名在其声明的落地形态里其实找不到内容(边界情形, 如
/// 配置与磁盘不一致)返回 Ok(None), 不落任何资源行, 也不计入 imported/skipped(调用方据此跳过
/// 建立关联, 不产生指向不存在内容的空关联)
fn import_skill_item(
	conn: &Connection,
	data_dir: &Path,
	adapter: &dyn AgentAdapter,
	skill_ref: &SkillRef,
	imported: &mut i64,
	skipped: &mut i64,
) -> Result<Option<i64>> {
	if let Some(existing) =
		repo_resource::find_by_type_and_name(conn, ResourceType::Skill, &skill_ref.name)?
	{
		*skipped += 1;
		return Ok(Some(existing.id));
	}

	let safe_name = sanitize_path_segment(&skill_ref.name);
	let target = data_dir.join("skills").join(&safe_name);
	if !adapter.export_skill(&skill_ref.name, &target)? {
		return Ok(None);
	}

	let resource_id = repo_resource::insert(
		conn,
		&NewResource {
			res_type: ResourceType::Skill,
			name: skill_ref.name.clone(),
			display_name: skill_ref.name.clone(),
			version: skill_ref.version.clone(),
			source_type: SourceType::AgentImport,
			local_path: target.to_string_lossy().into_owned(),
			enabled: true,
		},
	)?;
	repo_activity::add(
		conn,
		1,
		i64::from(ResourceType::Skill),
		&format!("从 Agent 导入 {}", skill_ref.name),
		"检测导入",
	)?;
	*imported += 1;
	Ok(Some(resource_id))
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::*;
	use crate::domain::agent::{AgentKind, AgentScope, DetectedAgent};
	use crate::services::sync;

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	/// 注册一个 Agent 行, config_path 按 home 拼出(与真正 detect() 会产出的路径一致, 使
	/// read_state 里对 mcp 配置文件的读取真正生效), 返回其 agent_id
	fn seed_agent(conn: &Connection, home: &Path, kind: AgentKind, config_rel: &str) -> i64 {
		repo_agent::upsert(
			conn,
			&DetectedAgent {
				kind,
				name: kind.label().to_string(),
				config_path: home.join(config_rel).to_string_lossy().into_owned(),
				scope: AgentScope::Global,
				online: true,
			},
		)
		.unwrap()
	}

	/// 造两个假 Agent 的 fixture(共享同一 home, 与真实机器"多款工具装在同一用户家目录下"的
	/// 形态一致): ClaudeCode(.claude.json 里一条 mcp + .claude/skills 下一个 Skill, 覆盖
	/// ClaudeSkillsDir 形态)与 Cursor(.cursor/mcp.json 里一条 mcp + .cursor/rules 下一个
	/// Skill, 覆盖 RulesDir 形态); 返回 (claude_id, cursor_id)
	fn seed_claude_and_cursor(conn: &Connection, home: &Path) -> (i64, i64) {
		fs::write(
			home.join(".claude.json"),
			r#"{"mcpServers":{"demo-mcp":{"command":"node","args":["index.js"],"env":{"K":"V"}}}}"#,
		)
		.unwrap();
		let claude_skill_dir = home.join(".claude/skills/demo-skill");
		fs::create_dir_all(claude_skill_dir.join("scripts")).unwrap();
		fs::write(
			claude_skill_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\n# Demo Skill\n正文\n",
		)
		.unwrap();
		fs::write(
			claude_skill_dir.join("scripts/run.sh"),
			"#!/bin/sh\necho hi\n",
		)
		.unwrap();
		let claude_id = seed_agent(conn, home, AgentKind::ClaudeCode, ".claude.json");

		fs::create_dir_all(home.join(".cursor")).unwrap();
		fs::write(
			home.join(".cursor/mcp.json"),
			r#"{"mcpServers":{"cursor-mcp":{"url":"http://localhost:9999"}}}"#,
		)
		.unwrap();
		fs::create_dir_all(home.join(".cursor/rules")).unwrap();
		fs::write(
			home.join(".cursor/rules/cursor-skill.mdc"),
			"# Cursor Skill 规则内容\n",
		)
		.unwrap();
		let cursor_id = seed_agent(conn, home, AgentKind::Cursor, ".cursor/mcp.json");

		(claude_id, cursor_id)
	}

	// import_from_agents: 端到端 —— 2 个 Agent(ClaudeCode+Cursor), 各带 1 条 mcp + 1 个 Skill,
	// 均为库中此前不存在的新资源: 应各自落地内容到 data_dir、写入 resource 表(source=AgentImport)、
	// 与各自 Agent 建立 desired 关联; ImportFromAgentsOutcomeRespVO 应报告 imported=4/skipped=0/agents=2
	#[test]
	fn import_from_agents_lands_new_resources_and_associates_owning_agents() {
		let home = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();
		let (claude_id, cursor_id) = seed_claude_and_cursor(&conn, home.path());

		let outcome = import_from_agents(&conn, home.path(), data_dir.path()).unwrap();

		assert_eq!(
			outcome,
			ImportFromAgentsOutcomeRespVO {
				imported: 4,
				skipped: 0,
				agents: 2,
			}
		);

		let mcp_resource =
			repo_resource::find_by_type_and_name(&conn, ResourceType::Mcp, "demo-mcp")
				.unwrap()
				.expect("demo-mcp 应已落库");
		assert_eq!(mcp_resource.source_type, SourceType::AgentImport);
		let mcp_json: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(&mcp_resource.local_path).unwrap()).unwrap();
		assert_eq!(mcp_json["command"], "node");
		assert_eq!(mcp_json["args"][0], "index.js");
		assert_eq!(mcp_json["env"]["K"], "V");

		let skill_resource =
			repo_resource::find_by_type_and_name(&conn, ResourceType::Skill, "demo-skill")
				.unwrap()
				.expect("demo-skill 应已落库");
		assert_eq!(skill_resource.version, "1.0.0");
		assert_eq!(skill_resource.source_type, SourceType::AgentImport);
		assert_eq!(
			fs::read_to_string(Path::new(&skill_resource.local_path).join("SKILL.md")).unwrap(),
			"---\nversion: 1.0.0\n---\n# Demo Skill\n正文\n"
		);
		assert_eq!(
			fs::read_to_string(Path::new(&skill_resource.local_path).join("scripts/run.sh"))
				.unwrap(),
			"#!/bin/sh\necho hi\n"
		);

		let cursor_mcp =
			repo_resource::find_by_type_and_name(&conn, ResourceType::Mcp, "cursor-mcp")
				.unwrap()
				.expect("cursor-mcp 应已落库");
		let cursor_skill =
			repo_resource::find_by_type_and_name(&conn, ResourceType::Skill, "cursor-skill")
				.unwrap()
				.expect("cursor-skill 应已落库");
		assert_eq!(cursor_skill.version, "", "RulesDir 形态恒无版本");
		assert_eq!(
			fs::read_to_string(Path::new(&cursor_skill.local_path).join("SKILL.md")).unwrap(),
			"# Cursor Skill 规则内容\n"
		);

		// 关联: demo-mcp/demo-skill 应关联到 claude_id, cursor-mcp/cursor-skill 应关联到 cursor_id
		assert_eq!(
			repo_assoc::agents_for_resource(&conn, mcp_resource.id).unwrap(),
			vec![claude_id]
		);
		assert_eq!(
			repo_assoc::agents_for_resource(&conn, skill_resource.id).unwrap(),
			vec![claude_id]
		);
		assert_eq!(
			repo_assoc::agents_for_resource(&conn, cursor_mcp.id).unwrap(),
			vec![cursor_id]
		);
		assert_eq!(
			repo_assoc::agents_for_resource(&conn, cursor_skill.id).unwrap(),
			vec![cursor_id]
		);
	}

	// import_from_agents: 二次调用应幂等 —— 不新增 resource 行、不新增关联行, imported=0,
	// skipped=全部(=4), agents 计数不变
	#[test]
	fn import_from_agents_is_idempotent_on_repeated_calls() {
		let home = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();
		seed_claude_and_cursor(&conn, home.path());

		let first = import_from_agents(&conn, home.path(), data_dir.path()).unwrap();
		assert_eq!(first.imported, 4);

		let resource_count_after_first: i64 = conn
			.query_row("SELECT COUNT(id) FROM resource", [], |row| row.get(0))
			.unwrap();
		let assoc_count_after_first: i64 = conn
			.query_row("SELECT COUNT(id) FROM resource_agent", [], |row| row.get(0))
			.unwrap();

		let second = import_from_agents(&conn, home.path(), data_dir.path()).unwrap();
		assert_eq!(
			second,
			ImportFromAgentsOutcomeRespVO {
				imported: 0,
				skipped: 4,
				agents: 2,
			}
		);

		let resource_count_after_second: i64 = conn
			.query_row("SELECT COUNT(id) FROM resource", [], |row| row.get(0))
			.unwrap();
		let assoc_count_after_second: i64 = conn
			.query_row("SELECT COUNT(id) FROM resource_agent", [], |row| row.get(0))
			.unwrap();
		assert_eq!(
			resource_count_after_first, resource_count_after_second,
			"不应新增 resource 行"
		);
		assert_eq!(
			assoc_count_after_first, assoc_count_after_second,
			"不应新增关联行"
		);
	}

	// import_from_agents: 多个 Agent 拥有同名 Skill 时应去重为一条 resource, 但关联到每一个
	// 拥有它的 Agent(ClaudeCode 与 Hermes 均声明了名为 shared-skill 的 Skill, 版本相同)
	#[test]
	fn import_from_agents_deduplicates_same_named_skill_across_agents_but_links_both() {
		let home = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		fs::write(home.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let claude_skill_dir = home.path().join(".claude/skills/shared-skill");
		fs::create_dir_all(&claude_skill_dir).unwrap();
		fs::write(
			claude_skill_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\nClaude 侧内容\n",
		)
		.unwrap();
		let claude_id = seed_agent(&conn, home.path(), AgentKind::ClaudeCode, ".claude.json");

		fs::create_dir_all(home.path().join(".hermes")).unwrap();
		fs::write(home.path().join(".hermes/config.yaml"), "mcp_servers: {}\n").unwrap();
		let hermes_skill_dir = home.path().join(".hermes/skills/shared-skill");
		fs::create_dir_all(&hermes_skill_dir).unwrap();
		fs::write(
			hermes_skill_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\nHermes 侧内容(与 Claude 同名同版本)\n",
		)
		.unwrap();
		let hermes_id = seed_agent(&conn, home.path(), AgentKind::Hermes, ".hermes/config.yaml");

		let outcome = import_from_agents(&conn, home.path(), data_dir.path()).unwrap();

		assert_eq!(
			outcome,
			ImportFromAgentsOutcomeRespVO {
				imported: 1,
				skipped: 1,
				agents: 2,
			}
		);

		let resources = repo_resource::list(&conn, &repo_resource::ListFilter::default()).unwrap();
		let shared: Vec<_> = resources
			.iter()
			.filter(|r| r.name == "shared-skill")
			.collect();
		assert_eq!(shared.len(), 1, "应去重为一条 resource");

		let mut linked_agents = repo_assoc::agents_for_resource(&conn, shared[0].id).unwrap();
		linked_agents.sort_unstable();
		let mut expected = vec![claude_id, hermes_id];
		expected.sort_unstable();
		assert_eq!(linked_agents, expected, "应关联到两个 Agent");
	}

	// import_from_agents: 落库的内容与关联均取自 Agent 的实际态本身(逐字段照抄), 不应改动任何
	// Agent 的配置文件 —— 导入后针对该 Agent 重新计算差异(services::sync::diff_for_agent), 应
	// 产出空 DiffPlanRespVO(desired 与 actual 完全一致, 无 Add/Update/Remove), 证明"导入不触发同步写回"
	#[test]
	fn import_from_agents_does_not_trigger_sync_write_back() {
		let home = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();
		let (claude_id, cursor_id) = seed_claude_and_cursor(&conn, home.path());

		import_from_agents(&conn, home.path(), data_dir.path()).unwrap();

		let claude_plan = sync::diff_for_agent(&conn, home.path(), claude_id).unwrap();
		assert!(
			claude_plan.items.is_empty(),
			"导入后不应产生任何待同步差异(ClaudeCode): {:?}",
			claude_plan.items
		);
		let cursor_plan = sync::diff_for_agent(&conn, home.path(), cursor_id).unwrap();
		assert!(
			cursor_plan.items.is_empty(),
			"导入后不应产生任何待同步差异(Cursor): {:?}",
			cursor_plan.items
		);

		// 配置文件本身应原封不动(导入过程未曾写回过 Agent 的配置文件)
		assert_eq!(
			fs::read_to_string(home.path().join(".claude.json")).unwrap(),
			r#"{"mcpServers":{"demo-mcp":{"command":"node","args":["index.js"],"env":{"K":"V"}}}}"#
		);
	}

	// import_from_agents: 没有任何已注册 Agent 时应返回全零结果, 不报错
	#[test]
	fn import_from_agents_returns_zeroes_when_no_agents_registered() {
		let home = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		let outcome = import_from_agents(&conn, home.path(), data_dir.path()).unwrap();

		assert_eq!(
			outcome,
			ImportFromAgentsOutcomeRespVO {
				imported: 0,
				skipped: 0,
				agents: 0,
			}
		);
	}

	// ImportFromAgentsOutcomeRespVO: 序列化应使用 camelCase 字段名, 与前端契约一致
	#[test]
	fn import_from_agents_outcome_serializes_as_camel_case() {
		let outcome = ImportFromAgentsOutcomeRespVO {
			imported: 1,
			skipped: 2,
			agents: 3,
		};
		let json = serde_json::to_value(&outcome).unwrap();
		assert_eq!(json["imported"], 1);
		assert_eq!(json["skipped"], 2);
		assert_eq!(json["agents"], 3);
	}
}
