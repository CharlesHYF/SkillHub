// 文件作用: agent 表仓储 —— upsert/list/get, 显式列名/禁 SELECT */全参数化查询
//           (阿里巴巴泰山版数据库规约), 探测结果按 uk_agent_kind_path 冲突更新落库
// 创建日期: 2026-07-09

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::agent::{AgentKind, AgentScope, DetectedAgent};

/// agent 表一行(持久化态); 与 DetectedAgent 的差异在于多了 id/last_sync_time/create_time/update_time
/// 等仅数据库侧维护的字段, 字段名与列名逐一对应
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
	pub id: i64,
	pub agent_kind: AgentKind,
	pub name: String,
	pub config_path: String,
	pub scope: AgentScope,
	pub status: bool,
	pub last_sync_time: String,
	pub create_time: String,
	pub update_time: String,
}

/// 将一行查询结果映射为 AgentRow 实体
fn row_to_agent_row(row: &Row) -> rusqlite::Result<AgentRow> {
	Ok(AgentRow {
		id: row.get(0)?,
		agent_kind: AgentKind::from_code(row.get(1)?),
		name: row.get(2)?,
		config_path: row.get(3)?,
		scope: AgentScope::from_i64(row.get(4)?),
		status: row.get(5)?,
		last_sync_time: row.get(6)?,
		create_time: row.get(7)?,
		update_time: row.get(8)?,
	})
}

/// 探测结果落库: 按 (agent_kind, config_path) 唯一键(uk_agent_kind_path)插入或冲突更新,
/// 冲突时仅覆盖 name/scope/status/update_time(last_sync_time 由同步完成流程单独维护, 此处不动),
/// 返回该行主键 id(无论本次是插入还是更新)
pub fn upsert(conn: &Connection, agent: &DetectedAgent) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO agent (agent_kind, name, config_path, scope, status) \
		 VALUES (?1, ?2, ?3, ?4, ?5) \
		 ON CONFLICT(agent_kind, config_path) DO UPDATE SET \
		 name = excluded.name, scope = excluded.scope, status = excluded.status, \
		 update_time = datetime('now')",
		params![
			agent.kind.code(),
			agent.name,
			agent.config_path,
			i64::from(agent.scope),
			agent.online,
		],
	)?;
	conn.query_row(
		"SELECT id FROM agent WHERE agent_kind = ?1 AND config_path = ?2",
		params![agent.kind.code(), agent.config_path],
		|row| row.get(0),
	)
}

/// 查询全部 Agent, 按 id 升序
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<AgentRow>> {
	let mut stmt = conn.prepare(
		"SELECT id, agent_kind, name, config_path, scope, status, last_sync_time, \
		 create_time, update_time \
		 FROM agent ORDER BY id",
	)?;
	let rows = stmt.query_map([], row_to_agent_row)?;
	rows.collect()
}

/// 按主键查询单个 Agent, 不存在返回 None(而非 Err)
pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<AgentRow>> {
	conn.query_row(
		"SELECT id, agent_kind, name, config_path, scope, status, last_sync_time, \
		 create_time, update_time \
		 FROM agent WHERE id = ?1",
		params![id],
		row_to_agent_row,
	)
	.optional()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 建一个已迁移好 10 张表结构的内存库, 供仓储测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	fn sample_agent() -> DetectedAgent {
		DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: "/home/demo/.claude.json".to_string(),
			scope: AgentScope::Global,
			online: true,
		}
	}

	// upsert 应幂等: 同 (agent_kind, config_path) 二次调用命中同一行, 且用第二次的值覆盖
	// name/scope/status, 不产生第二行
	#[test]
	fn upsert_same_kind_and_path_is_idempotent_and_updates_fields() {
		let conn = setup_conn();
		let first = sample_agent();
		let id1 = upsert(&conn, &first).unwrap();

		let mut second = first.clone();
		second.name = "Claude Code (更新)".to_string();
		second.scope = AgentScope::Project;
		second.online = false;
		let id2 = upsert(&conn, &second).unwrap();

		assert_eq!(id1, id2, "同一 (agent_kind, config_path) 应命中同一行");
		let rows = list(&conn).unwrap();
		assert_eq!(rows.len(), 1, "重复 upsert 不应产生多行");

		let row = get(&conn, id1).unwrap().expect("刚 upsert 的行应能查到");
		assert_eq!(row.name, "Claude Code (更新)");
		assert_eq!(row.scope, AgentScope::Project);
		assert!(!row.status);
		assert_eq!(row.agent_kind, AgentKind::ClaudeCode);
		assert_eq!(row.config_path, "/home/demo/.claude.json");
	}

	// upsert 应支持不同 config_path 的同类 Agent 各自独立成行(如 Claude Code 的多个项目级实例)
	#[test]
	fn upsert_different_paths_creates_separate_rows() {
		let conn = setup_conn();
		let mut a = sample_agent();
		a.config_path = "/home/demo/project-a/.claude.json".to_string();
		let mut b = sample_agent();
		b.config_path = "/home/demo/project-b/.claude.json".to_string();

		upsert(&conn, &a).unwrap();
		upsert(&conn, &b).unwrap();

		assert_eq!(list(&conn).unwrap().len(), 2);
	}

	// get 查询不存在的 id 应返回 None, 不是 Err
	#[test]
	fn get_missing_id_returns_none() {
		let conn = setup_conn();
		assert_eq!(get(&conn, 9999).unwrap(), None);
	}

	// list 应按 id 升序返回全部行, 且 last_sync_time 未同步过应为空串(列默认值),
	// create_time/update_time 应已由数据库填充
	#[test]
	fn list_orders_by_id_and_fills_timestamp_defaults() {
		let conn = setup_conn();
		let id = upsert(&conn, &sample_agent()).unwrap();
		let rows = list(&conn).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].id, id);
		assert_eq!(rows[0].last_sync_time, "", "未同步过应为空串");
		assert!(!rows[0].create_time.is_empty());
		assert!(!rows[0].update_time.is_empty());
	}
}
