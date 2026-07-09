// 文件作用: resource_agent 关联表仓储 —— 期望态(desired) upsert 与双向查询,
//           应用指纹(applied_hash)/同步状态(sync_status)维护, 显式列名/禁 SELECT */
//           全参数化查询(阿里巴巴泰山版数据库规约)
// 创建日期: 2026-07-09

use rusqlite::{params, Connection};

/// 期望态 upsert: 按 (resource_id, agent_id) 唯一键(uk_resource_agent_rid_aid)插入或冲突更新,
/// 冲突时仅覆盖 desired/update_time(applied_hash/sync_status 由各自专用方法维护, 此处不动),
/// 返回该行主键 id(无论本次是插入还是更新)
pub fn set(
	conn: &Connection,
	resource_id: i64,
	agent_id: i64,
	desired: bool,
) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO resource_agent (resource_id, agent_id, desired) \
		 VALUES (?1, ?2, ?3) \
		 ON CONFLICT(resource_id, agent_id) DO UPDATE SET \
		 desired = excluded.desired, update_time = datetime('now')",
		params![resource_id, agent_id, desired],
	)?;
	conn.query_row(
		"SELECT id FROM resource_agent WHERE resource_id = ?1 AND agent_id = ?2",
		params![resource_id, agent_id],
		|row| row.get(0),
	)
}

/// 查询某 Agent 期望装(desired=1)的资源 id 列表, 按 resource_id 升序
pub fn desired_for_agent(conn: &Connection, agent_id: i64) -> rusqlite::Result<Vec<i64>> {
	let mut stmt = conn.prepare(
		"SELECT resource_id FROM resource_agent \
		 WHERE agent_id = ?1 AND desired = 1 \
		 ORDER BY resource_id",
	)?;
	let rows = stmt.query_map(params![agent_id], |row| row.get(0))?;
	rows.collect()
}

/// 查询期望装某资源(desired=1)的 Agent id 列表, 按 agent_id 升序;
/// 与 desired_for_agent 互为镜像查询, 同样只看"期望存在"的关联边
pub fn agents_for_resource(conn: &Connection, resource_id: i64) -> rusqlite::Result<Vec<i64>> {
	let mut stmt = conn.prepare(
		"SELECT agent_id FROM resource_agent \
		 WHERE resource_id = ?1 AND desired = 1 \
		 ORDER BY agent_id",
	)?;
	let rows = stmt.query_map(params![resource_id], |row| row.get(0))?;
	rows.collect()
}

/// 记录一次成功应用后的内容指纹(供漂移检测), 按 (resource_id, agent_id) 定位, 返回受影响行数
pub fn set_applied_hash(
	conn: &Connection,
	resource_id: i64,
	agent_id: i64,
	hash: &str,
) -> rusqlite::Result<usize> {
	conn.execute(
		"UPDATE resource_agent SET applied_hash = ?1, update_time = datetime('now') \
		 WHERE resource_id = ?2 AND agent_id = ?3",
		params![hash, resource_id, agent_id],
	)
}

/// 更新同步状态(0-待同步,1-已同步,2-本地修改,3-同步失败,4-已禁用),
/// 按 (resource_id, agent_id) 定位, 返回受影响行数
pub fn set_sync_status(
	conn: &Connection,
	resource_id: i64,
	agent_id: i64,
	status: i64,
) -> rusqlite::Result<usize> {
	conn.execute(
		"UPDATE resource_agent SET sync_status = ?1, update_time = datetime('now') \
		 WHERE resource_id = ?2 AND agent_id = ?3",
		params![status, resource_id, agent_id],
	)
}

/// 统计期望装(desired=1)某资源的 Agent 数, 供"已安装"界面显示; 用 COUNT(id) 保持列名显式
pub fn count_agents_for_resource(conn: &Connection, resource_id: i64) -> rusqlite::Result<i64> {
	conn.query_row(
		"SELECT COUNT(id) FROM resource_agent WHERE resource_id = ?1 AND desired = 1",
		params![resource_id],
		|row| row.get(0),
	)
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

	/// 直接查询 applied_hash/sync_status 两列, 供测试校验落库结果
	/// (这两列均无对应的公开 getter, 属于白盒校验, 不暴露为仓储 API)
	fn fetch_hash_and_status(conn: &Connection, resource_id: i64, agent_id: i64) -> (String, i64) {
		conn.query_row(
			"SELECT applied_hash, sync_status FROM resource_agent \
			 WHERE resource_id = ?1 AND agent_id = ?2",
			params![resource_id, agent_id],
			|row| Ok((row.get(0)?, row.get(1)?)),
		)
		.unwrap()
	}

	// set(desired=true) 后应能在 desired_for_agent 与 agents_for_resource 两个方向都查到
	#[test]
	fn set_desired_true_is_visible_from_both_directions() {
		let conn = setup_conn();
		set(&conn, 10, 20, true).unwrap();

		assert_eq!(desired_for_agent(&conn, 20).unwrap(), vec![10]);
		assert_eq!(agents_for_resource(&conn, 10).unwrap(), vec![20]);
	}

	// set 应幂等 upsert: 同 (resource_id, agent_id) 二次调用命中同一行, 用第二次的 desired 覆盖第一次
	#[test]
	fn set_same_pair_twice_is_idempotent_and_overwrites_desired() {
		let conn = setup_conn();
		let id1 = set(&conn, 10, 20, true).unwrap();
		let id2 = set(&conn, 10, 20, false).unwrap();

		assert_eq!(id1, id2, "同一 (resource_id, agent_id) 应命中同一行");
		assert_eq!(desired_for_agent(&conn, 20).unwrap(), Vec::<i64>::new());
		assert_eq!(agents_for_resource(&conn, 10).unwrap(), Vec::<i64>::new());

		let total: i64 = conn
			.query_row("SELECT COUNT(id) FROM resource_agent", [], |row| row.get(0))
			.unwrap();
		assert_eq!(total, 1, "重复 set 不应产生多行");
	}

	// desired_for_agent 应只返回 desired=1 的行, 按 resource_id 升序
	#[test]
	fn desired_for_agent_filters_non_desired_and_orders_ascending() {
		let conn = setup_conn();
		set(&conn, 30, 1, true).unwrap();
		set(&conn, 10, 1, true).unwrap();
		set(&conn, 20, 1, false).unwrap();

		assert_eq!(desired_for_agent(&conn, 1).unwrap(), vec![10, 30]);
	}

	// set_applied_hash/set_sync_status 应分别精确更新对应列, 不影响 desired
	#[test]
	fn set_applied_hash_and_sync_status_update_their_own_columns() {
		let conn = setup_conn();
		set(&conn, 10, 20, true).unwrap();

		let affected_hash = set_applied_hash(&conn, 10, 20, "sha256:abc").unwrap();
		let affected_status = set_sync_status(&conn, 10, 20, 3).unwrap();
		assert_eq!(affected_hash, 1);
		assert_eq!(affected_status, 1);

		let (hash, status) = fetch_hash_and_status(&conn, 10, 20);
		assert_eq!(hash, "sha256:abc");
		assert_eq!(status, 3, "3-同步失败");
		assert_eq!(
			desired_for_agent(&conn, 20).unwrap(),
			vec![10],
			"不应影响 desired"
		);
	}

	// count_agents_for_resource 应只统计 desired=1 的关联行
	#[test]
	fn count_agents_for_resource_counts_only_desired_rows() {
		let conn = setup_conn();
		set(&conn, 100, 1, true).unwrap();
		set(&conn, 100, 2, true).unwrap();
		set(&conn, 100, 3, false).unwrap();

		assert_eq!(count_agents_for_resource(&conn, 100).unwrap(), 2);
	}
}
