// 文件作用: 同步引擎编排层 —— 串联 domain::sync 的纯函数 reconcile、infra::adapter 的
//           AgentAdapter(detect/read_state/apply)与各仓储(repo_agent/repo_resource/
//           repo_assoc/repo_sync/repo_activity), 提供探测(detect_all)、单 Agent 差异计算
//           (diff_for_agent)、单 Agent 应用(apply_for_agent)三个编排函数。均只接受
//           &Connection 与 &Path(home), 不直接摸 AppState, 便于单测注入内存库/临时目录;
//           命令层(Task 8)负责加锁取出 conn/home 后转调本模块, 本模块不关心 Tauri 运行时。
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::domain::agent::{AgentKind, DetectedAgent, McpServerDef};
use crate::domain::resource::{ResourceRespVO, ResourceType};
use crate::domain::sync::{
	reconcile, DesiredPayload, DesiredResource, DiffAction, DiffItem, DiffPlanRespVO,
};
use crate::infra::adapter::{all_adapters, AgentAdapter};
use crate::infra::repo_activity;
use crate::infra::repo_agent::{self, AgentRespVO};
use crate::infra::repo_assoc;
use crate::infra::repo_resource::{self, ListFilter};
use crate::infra::repo_sync;

/// 一次单 Agent 同步应用(apply_for_agent)的结果汇总, 供命令层(Task 8)直接返回给前端
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummaryRespVO {
	pub success: i64,
	pub failed: i64,
	pub skipped: i64,
}

/// 探测本机全部已知 AI 工具实例并落库: 对 all_adapters(home) 逐个适配器调用 detect(), 探测到
/// 的每个 DetectedAgent 都 upsert 进 agent 表(按 (agent_kind, config_path) 幂等, 见
/// repo_agent::upsert), 最终返回 agent 表当前全量(与 repo_agent::list 语义一致, 供命令层直接
/// 展示; 不止本次探测到的那些行, 历史上探测过但本次未再探测到的行仍会保留, 是否需要据此标记
/// 离线留待后续任务)
pub fn detect_all(conn: &Connection, home: &Path) -> Result<Vec<AgentRespVO>> {
	for adapter in all_adapters(home) {
		for detected in adapter.detect() {
			repo_agent::upsert(conn, &detected)?;
		}
	}
	Ok(repo_agent::list(conn)?)
}

/// 把一条 ResourceRespVO 转成 reconcile 所需的期望资源(DesiredResource): MCP 从 local_path 指向的
/// JSON 文件(单个服务定义对象, 形如 {"command":...,"args":...,"env":...} 或 {"url":...})解析出
/// McpServerDef, name 取 res.name(定义文件内部不重复携带 name, 与
/// infra::adapter::json_mcp::parse_mcp_servers 里"键名即服务器名"的角色对应, 只是这里 name
/// 来自 ResourceRespVO 而非 JSON 键); Skill 直接把 local_path 包成 src_dir, 实际内容(SKILL.md 是否
/// 存在等)留给 apply 阶段的 SkillTarget::write_skill 处理, 此处不预读校验。
/// 读不到 MCP 定义文件、解析失败、或根节点不是 JSON 对象, 都视为该资源本身有问题, 返回 None
/// 让调用方跳过(不让一条坏资源拖垮整次同步的 diff 计算), 呼应 adapter 层"宽松解析, 单条失败
/// 不拖累整体"的一贯风格
fn resource_to_desired(res: &ResourceRespVO) -> Option<DesiredResource> {
	let payload = match res.res_type {
		ResourceType::Mcp => {
			let text = fs::read_to_string(&res.local_path).ok()?;
			let raw: Value = serde_json::from_str(&text).ok()?;
			if !raw.is_object() {
				return None;
			}
			DesiredPayload::Mcp(parse_single_mcp_def(&res.name, &raw))
		}
		ResourceType::Skill => DesiredPayload::Skill {
			src_dir: res.local_path.clone(),
		},
	};
	Some(DesiredResource {
		res_type: res.res_type,
		name: res.name.clone(),
		version: res.version.clone(),
		payload,
	})
}

/// 从单个 MCP 服务定义 JSON 对象(形如 {"command":...,"args":...,"env":...} 或 {"url":...})
/// 提取字段构造 McpServerDef, name 由调用方给定(取 ResourceRespVO.name, 定义文件内部不重复携带
/// name)。字段逐个宽松提取, 与 infra::adapter::json_mcp::parse_mcp_servers 对单条服务器的
/// 解析策略一致: 字段缺失或类型不符都退回默认值, 不因单个字段异常整体失败
fn parse_single_mcp_def(name: &str, raw: &Value) -> McpServerDef {
	McpServerDef {
		name: name.to_string(),
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
	}
}

/// 计算某 Agent 的期望态(desired)与其配置文件实际态(actual)之间的差异计划: 读该 Agent 当前
/// 期望关联的资源(resource_agent.desired=1)与它配置文件里的实际内容, 交给 domain::sync::
/// reconcile 纯函数产出 DiffPlanRespVO; managed 边界取该 Agent 在 resource_agent 里全部登记过的
/// 资源(不论当前 desired 取值, 见 repo_assoc::managed_keys_for_agent), 使"曾期望但现已取消
/// 关联"的历史项能被正确判定为 Remove(reconcile 的安全边界语义见该函数文档)
pub fn diff_for_agent(conn: &Connection, home: &Path, agent_id: i64) -> Result<DiffPlanRespVO> {
	let row = repo_agent::get(conn, agent_id)?
		.ok_or_else(|| anyhow::anyhow!("Agent 不存在: id={agent_id}"))?;
	let detected = agent_row_to_detected(&row);

	let adapters = all_adapters(home);
	let adapter = find_adapter(&adapters, row.agent_kind)?;
	let actual = adapter.read_state(&detected)?;

	let mut desired: Vec<DesiredResource> = Vec::new();
	for resource_id in repo_assoc::desired_for_agent(conn, agent_id)? {
		if let Some(res) = repo_resource::get(conn, resource_id)? {
			if let Some(item) = resource_to_desired(&res) {
				desired.push(item);
			}
		}
	}

	let managed = repo_assoc::managed_keys_for_agent(conn, agent_id)?;

	Ok(reconcile(&desired, &actual, &managed))
}

/// 把 AgentRespVO(数据库持久化态)还原为 DetectedAgent(AgentAdapter::read_state/apply 所需的探测
/// 态入参); 字段逐一对应, online 取自 status 列。
/// 可见性 pub(crate): 供 services::agent_import(M6 Task BE-2, 从已检测 Agent 反向导入已装
/// Skill/MCP 到本地库)复用同一份换算逻辑, 与 diff_for_agent 取 adapter/组装探测态的方式一致
pub(crate) fn agent_row_to_detected(row: &AgentRespVO) -> DetectedAgent {
	DetectedAgent {
		kind: row.agent_kind,
		name: row.name.clone(),
		config_path: row.config_path.clone(),
		scope: row.scope,
		online: row.status,
	}
}

/// 在全量适配器表里按 kind 找到对应的适配器; 找不到说明 all_adapters 未覆盖该 AgentKind
/// (编程错误, 理论不会发生), 返回 Err 而非 panic。
/// 可见性 pub(crate): 供 services::agent_import(M6 Task BE-2)复用同一份查找逻辑
pub(crate) fn find_adapter(
	adapters: &[Box<dyn AgentAdapter>],
	kind: AgentKind,
) -> Result<&dyn AgentAdapter> {
	adapters
		.iter()
		.find(|adapter| adapter.kind() == kind)
		.map(Box::as_ref)
		.ok_or_else(|| anyhow::anyhow!("未找到 {kind:?} 对应的适配器"))
}

/// 对某 Agent 执行一次完整同步: 计算差异计划 -> 交给适配器落地写入配置文件 -> 把每一项的
/// 执行结果记入 sync_run/sync_item, 并据此维护 resource_agent 的 applied_hash/sync_status ->
/// 收尾运行汇总 -> 记一条活动日志, 最终返回给命令层的汇总结果(SyncSummaryRespVO)
pub fn apply_for_agent(conn: &Connection, home: &Path, agent_id: i64) -> Result<SyncSummaryRespVO> {
	let plan = diff_for_agent(conn, home, agent_id)?;

	let row = repo_agent::get(conn, agent_id)?
		.ok_or_else(|| anyhow::anyhow!("Agent 不存在: id={agent_id}"))?;
	let detected = agent_row_to_detected(&row);
	let adapters = all_adapters(home);
	let adapter = find_adapter(&adapters, row.agent_kind)?;

	let run_id = repo_sync::start_run(conn, 1, agent_id, plan.items.len() as i64)?;
	let outcomes = adapter.apply(&detected, &plan)?;

	// (res_type, name) -> resource.id 的索引, 供按 DiffItem 反查资源主键写 sync_item/
	// resource_agent(DiffItem/ItemOutcome 本身均不携带 resource_id, 只有 res_type+name)
	let resource_index: BTreeMap<(ResourceType, String), i64> =
		repo_resource::list(conn, &ListFilter::default())?
			.into_iter()
			.map(|res| ((res.res_type, res.name), res.id))
			.collect();

	let mut success = 0i64;
	let mut failed = 0i64;
	let skipped = 0i64; // 当前 AgentAdapter::apply 契约里每项都会产出 ok/err 结果, 无跳过语义

	for outcome in &outcomes {
		let Some(item) = plan
			.items
			.iter()
			.find(|item| item.name == outcome.name && item.action == outcome.action)
		else {
			continue;
		};
		let resource_id = resource_index
			.get(&(item.res_type, item.name.clone()))
			.copied()
			.unwrap_or(0);

		repo_sync::add_item(
			conn,
			run_id,
			resource_id,
			agent_id,
			diff_action_code(item.action),
			&item.local_ver,
			&item.agent_ver,
			if outcome.ok { 1 } else { 2 },
			&outcome.err,
		)?;

		if outcome.ok {
			success += 1;
		} else {
			failed += 1;
		}

		match (item.action, outcome.ok) {
			(DiffAction::Add, true) | (DiffAction::Update, true) => {
				let hash = content_fingerprint(item);
				repo_assoc::set_applied_hash(conn, resource_id, agent_id, &hash)?;
				repo_assoc::set_sync_status(conn, resource_id, agent_id, 1)?;
			}
			// Remove 成功: 按约定可不留 applied_hash, 也不改 sync_status(该关联行的 desired
			// 在被判定为 Remove 之前就已经是 0, 其去留由资源关联管理界面另行维护)
			(DiffAction::Remove, true) => {}
			(_, false) => {
				repo_assoc::set_sync_status(conn, resource_id, agent_id, 3)?;
			}
		}
	}

	let status = if failed == 0 {
		1
	} else if success == 0 {
		3
	} else {
		2
	};
	repo_sync::finish_run(conn, run_id, success, failed, skipped, status)?;
	// 只要对该 Agent 走完一次应用流程就回写 last_sync_time(不论各项是否全部成功, 见
	// repo_agent::touch_last_sync 文档注释), 使 Sync Center 的"最后同步时间"列有值
	repo_agent::touch_last_sync(conn, agent_id)?;
	repo_activity::add(
		conn,
		6,
		4,
		&format!("同步 {}", row.name),
		&format!("成功 {success} 失败 {failed} 跳过 {skipped}"),
	)?;

	Ok(SyncSummaryRespVO {
		success,
		failed,
		skipped,
	})
}

/// DiffAction 转 sync_item.action 列编码(1-新增,2-更新,3-移除), 与 repo_sync::add_item 文档
/// 注释的枚举语义一一对应
fn diff_action_code(action: DiffAction) -> i64 {
	match action {
		DiffAction::Add => 1,
		DiffAction::Update => 2,
		DiffAction::Remove => 3,
	}
}

/// 为一次成功应用的 Add/Update 项计算一个内容指纹, 存入 resource_agent.applied_hash 供后续
/// 漂移检测比对: 取 local_ver 与 payload 序列化文本的简单校验和拼接, 只需达到"内容变了没有"
/// 这一粒度即可, 不追求密码学强度, 引入额外哈希算法依赖属过度设计
fn content_fingerprint(item: &DiffItem) -> String {
	let payload_json = item
		.payload
		.as_ref()
		.and_then(|payload| serde_json::to_string(payload).ok())
		.unwrap_or_default();
	let checksum = payload_json.bytes().fold(0u64, |acc, byte| {
		acc.wrapping_mul(31).wrapping_add(u64::from(byte))
	});
	format!("{}:{checksum:x}", item.local_ver)
}

#[cfg(test)]
mod tests {
	use rusqlite::params;
	use tempfile::tempdir;

	use super::*;
	use crate::domain::agent::AgentScope;
	use crate::domain::resource::SourceType;
	use crate::infra::repo_resource::NewResource;

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	/// 把一个 ClaudeCode 实例登记进 agent 表(config_path 指向 home/.claude.json), 返回其
	/// agent_id; 覆盖大多数测试共用的"已探测到一个 Agent"前置状态
	fn seed_claude_code_agent(conn: &Connection, home: &Path) -> i64 {
		repo_agent::upsert(
			conn,
			&DetectedAgent {
				kind: AgentKind::ClaudeCode,
				name: "Claude Code".to_string(),
				config_path: home.join(".claude.json").to_string_lossy().into_owned(),
				scope: AgentScope::Global,
				online: true,
			},
		)
		.unwrap()
	}

	/// 插入一条资源并返回其 resource_id; 覆盖 MCP/Skill 两种类型测试共用的最小字段集
	fn seed_resource(
		conn: &Connection,
		res_type: ResourceType,
		name: &str,
		local_path: &str,
	) -> i64 {
		repo_resource::insert(
			conn,
			&NewResource {
				res_type,
				name: name.to_string(),
				display_name: name.to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: local_path.to_string(),
				enabled: true,
			},
		)
		.unwrap()
	}

	/// 直接查询 applied_hash/sync_status 两列(白盒校验, 与 repo_assoc.rs 自身测试同一手法)
	fn fetch_hash_and_status(conn: &Connection, resource_id: i64, agent_id: i64) -> (String, i64) {
		conn.query_row(
			"SELECT applied_hash, sync_status FROM resource_agent \
			 WHERE resource_id = ?1 AND agent_id = ?2",
			params![resource_id, agent_id],
			|row| Ok((row.get(0)?, row.get(1)?)),
		)
		.unwrap()
	}

	// detect_all: tempdir 里落地一份 ClaudeCode 配置 fixture 后, detect_all 应把它 upsert 进
	// agent 表并在返回值里体现
	#[test]
	fn detect_all_upserts_detected_agent_into_repo() {
		let dir = tempdir().unwrap();
		fs::write(dir.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();

		let rows = detect_all(&conn, dir.path()).unwrap();

		assert_eq!(rows.len(), 1, "本 fixture 只应命中 ClaudeCode 一款工具");
		assert_eq!(rows[0].agent_kind, AgentKind::ClaudeCode);
		assert_eq!(
			rows[0].config_path,
			dir.path().join(".claude.json").to_string_lossy()
		);
		assert_eq!(repo_agent::list(&conn).unwrap().len(), 1, "应已落库");
	}

	// diff_for_agent: Agent 配置为空 + 关联一个 desired 的 MCP 资源(local_path 指向单定义
	// json) -> 应得 1 条 Add, payload 携带解析出的 McpServerDef
	#[test]
	fn diff_for_agent_reports_add_for_desired_mcp_when_agent_config_empty() {
		let dir = tempdir().unwrap();
		fs::write(dir.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let def_path = dir.path().join("demo-mcp.json");
		fs::write(&def_path, r#"{"command":"node","args":["index.js"]}"#).unwrap();
		let resource_id = seed_resource(
			&conn,
			ResourceType::Mcp,
			"demo-mcp",
			&def_path.to_string_lossy(),
		);
		repo_assoc::set(&conn, resource_id, agent_id, true).unwrap();

		let plan = diff_for_agent(&conn, dir.path(), agent_id).unwrap();

		assert_eq!(plan.items.len(), 1);
		let item = &plan.items[0];
		assert_eq!(item.res_type, ResourceType::Mcp);
		assert_eq!(item.name, "demo-mcp");
		assert_eq!(item.action, DiffAction::Add);
		assert_eq!(
			item.payload,
			Some(DesiredPayload::Mcp(McpServerDef {
				name: "demo-mcp".to_string(),
				command: Some("node".to_string()),
				args: vec!["index.js".to_string()],
				env: BTreeMap::new(),
				url: None,
			}))
		);
	}

	// diff_for_agent: 一条 MCP 资源的定义文件读不到(local_path 指向不存在的文件)不应拖累
	// 整次 diff —— 该条被静默跳过, 同批里的另一条正常资源应照常产出 Add
	#[test]
	fn diff_for_agent_skips_unreadable_mcp_resource_without_failing_whole_diff() {
		let dir = tempdir().unwrap();
		fs::write(dir.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let bad_id = seed_resource(
			&conn,
			ResourceType::Mcp,
			"bad-mcp",
			&dir.path().join("missing.json").to_string_lossy(),
		);
		repo_assoc::set(&conn, bad_id, agent_id, true).unwrap();

		let good_def_path = dir.path().join("good-mcp.json");
		fs::write(&good_def_path, r#"{"command":"node","args":[]}"#).unwrap();
		let good_id = seed_resource(
			&conn,
			ResourceType::Mcp,
			"good-mcp",
			&good_def_path.to_string_lossy(),
		);
		repo_assoc::set(&conn, good_id, agent_id, true).unwrap();

		let plan = diff_for_agent(&conn, dir.path(), agent_id).unwrap();

		assert_eq!(plan.items.len(), 1, "坏资源应被跳过, 只剩好资源这一条");
		assert_eq!(plan.items[0].name, "good-mcp");
	}

	// diff_for_agent: desired 的 Skill 资源应产出 Add, payload 为 Skill{src_dir: local_path},
	// 不要求 local_path 指向的目录此刻真实存在(留给 apply 阶段处理)
	#[test]
	fn diff_for_agent_reports_add_for_desired_skill_resource() {
		let dir = tempdir().unwrap();
		fs::write(dir.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let src_dir = dir.path().join("src-demo-skill");
		let resource_id = seed_resource(
			&conn,
			ResourceType::Skill,
			"demo-skill",
			&src_dir.to_string_lossy(),
		);
		repo_assoc::set(&conn, resource_id, agent_id, true).unwrap();

		let plan = diff_for_agent(&conn, dir.path(), agent_id).unwrap();

		assert_eq!(plan.items.len(), 1);
		let item = &plan.items[0];
		assert_eq!(item.res_type, ResourceType::Skill);
		assert_eq!(item.action, DiffAction::Add);
		assert_eq!(
			item.payload,
			Some(DesiredPayload::Skill {
				src_dir: src_dir.to_string_lossy().into_owned(),
			})
		);
	}

	// diff_for_agent: 曾经关联(managed)但现已取消期望(desired=0)的资源, 若其在 Agent 实际
	// 配置里仍然存在, 应判定为 Remove
	#[test]
	fn diff_for_agent_reports_remove_for_deassociated_but_managed_resource() {
		let dir = tempdir().unwrap();
		fs::write(
			dir.path().join(".claude.json"),
			r#"{"mcpServers":{"old-mcp":{"command":"python","args":["server.py"]}}}"#,
		)
		.unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let resource_id = seed_resource(&conn, ResourceType::Mcp, "old-mcp", "/tmp/unused.json");
		repo_assoc::set(&conn, resource_id, agent_id, true).unwrap();
		repo_assoc::set(&conn, resource_id, agent_id, false).unwrap();

		let plan = diff_for_agent(&conn, dir.path(), agent_id).unwrap();

		assert_eq!(plan.items.len(), 1);
		let item = &plan.items[0];
		assert_eq!(item.res_type, ResourceType::Mcp);
		assert_eq!(item.name, "old-mcp");
		assert_eq!(item.action, DiffAction::Remove);
		assert_eq!(item.payload, None);
	}

	// apply_for_agent: 接上"desired 的 MCP 资源"场景, apply 后配置文件应真的写入该 MCP,
	// sync_run/sync_item 应落库, resource_agent 的 applied_hash/sync_status 应更新为已同步,
	// SyncSummaryRespVO 应报告 success=1
	#[test]
	fn apply_for_agent_writes_config_and_persists_sync_history() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join(".claude.json");
		fs::write(&config_path, r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let def_path = dir.path().join("demo-mcp.json");
		fs::write(&def_path, r#"{"command":"node","args":["index.js"]}"#).unwrap();
		let resource_id = seed_resource(
			&conn,
			ResourceType::Mcp,
			"demo-mcp",
			&def_path.to_string_lossy(),
		);
		repo_assoc::set(&conn, resource_id, agent_id, true).unwrap();

		let summary = apply_for_agent(&conn, dir.path(), agent_id).unwrap();

		assert_eq!(
			summary,
			SyncSummaryRespVO {
				success: 1,
				failed: 0,
				skipped: 0,
			}
		);

		let root: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
		assert_eq!(root["mcpServers"]["demo-mcp"]["command"], "node");

		let runs = repo_sync::recent_runs(&conn, 10).unwrap();
		assert_eq!(runs.len(), 1);
		assert_eq!(runs[0].total_cnt, 1);
		assert_eq!(runs[0].success_cnt, 1);
		assert_eq!(runs[0].failed_cnt, 0);
		assert_eq!(runs[0].status, 1, "全成");

		let items = repo_sync::items_for_run(&conn, runs[0].id).unwrap();
		assert_eq!(items.len(), 1);
		assert_eq!(items[0].resource_id, resource_id);
		assert_eq!(items[0].agent_id, agent_id);
		assert_eq!(items[0].action, 1, "新增");
		assert_eq!(items[0].result, 1, "成功");

		let (hash, status) = fetch_hash_and_status(&conn, resource_id, agent_id);
		assert!(!hash.is_empty(), "成功应用后应写入内容指纹");
		assert_eq!(status, 1, "已同步");

		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 6, "同步");
	}

	// apply_for_agent: Skill 资源的 src_dir 指向不存在的目录, 应用应失败(而非整体报错),
	// SyncSummaryRespVO 应报告 failed=1, sync_run 状态应为全败, resource_agent.sync_status 应置为 3
	#[test]
	fn apply_for_agent_marks_failed_when_skill_source_dir_missing() {
		let dir = tempdir().unwrap();
		fs::write(dir.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let missing_src = dir.path().join("does-not-exist");
		let resource_id = seed_resource(
			&conn,
			ResourceType::Skill,
			"broken-skill",
			&missing_src.to_string_lossy(),
		);
		repo_assoc::set(&conn, resource_id, agent_id, true).unwrap();

		let summary = apply_for_agent(&conn, dir.path(), agent_id).unwrap();

		assert_eq!(summary.success, 0);
		assert_eq!(summary.failed, 1);
		assert_eq!(summary.skipped, 0);

		let runs = repo_sync::recent_runs(&conn, 10).unwrap();
		assert_eq!(runs[0].status, 3, "全败");

		let items = repo_sync::items_for_run(&conn, runs[0].id).unwrap();
		assert_eq!(items.len(), 1);
		assert_eq!(items[0].result, 2, "失败");
		assert!(!items[0].err_msg.is_empty());

		let (_, status) = fetch_hash_and_status(&conn, resource_id, agent_id);
		assert_eq!(status, 3, "同步失败");
	}

	// apply_for_agent: 应用完成后应回写该 Agent 的 last_sync_time(不论本次应用是否有失败项),
	// 使 Sync Center 的"最后同步时间"列有值; 复用"Skill 源目录缺失导致失败"场景, 确保就算全败
	// 也会 touch, 而不是只有全成功才 touch(呼应原型里"同步失败"状态的 Agent 仍展示非空的
	// "最后同步时间")
	#[test]
	fn apply_for_agent_touches_last_sync_time_even_when_failed() {
		let dir = tempdir().unwrap();
		fs::write(dir.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();
		let conn = setup_conn();
		let agent_id = seed_claude_code_agent(&conn, dir.path());

		let missing_src = dir.path().join("does-not-exist");
		let resource_id = seed_resource(
			&conn,
			ResourceType::Skill,
			"broken-skill",
			&missing_src.to_string_lossy(),
		);
		repo_assoc::set(&conn, resource_id, agent_id, true).unwrap();

		assert_eq!(
			repo_agent::get(&conn, agent_id)
				.unwrap()
				.unwrap()
				.last_sync_time,
			"",
			"同步前应为空串"
		);

		let summary = apply_for_agent(&conn, dir.path(), agent_id).unwrap();
		assert_eq!(summary.failed, 1, "本次应用应全败");

		let row = repo_agent::get(&conn, agent_id).unwrap().unwrap();
		assert!(
			!row.last_sync_time.is_empty(),
			"即使本次应用全败, 也应回写 last_sync_time"
		);
	}
}
