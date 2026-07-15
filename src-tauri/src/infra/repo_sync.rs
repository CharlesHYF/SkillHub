// 文件作用: sync_run/sync_item 表仓储 —— 同步运行的起止与明细项记录, 显式列名/
//           禁 SELECT */全参数化查询(阿里巴巴泰山版数据库规约)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use rusqlite::{params, Connection, Row};
use serde::Serialize;

/// sync_run 表一行(一次同步运行的汇总), 字段名与列名逐一对应
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncRunRow {
	pub id: i64,
	pub scope_type: i64,
	pub agent_id: i64,
	pub total_cnt: i64,
	pub success_cnt: i64,
	pub failed_cnt: i64,
	pub skipped_cnt: i64,
	pub status: i64,
	pub run_time: String,
	pub create_time: String,
}

/// sync_item 表一行(一次同步中单个资源项的处理结果), 字段名与列名逐一对应
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncItemRow {
	pub id: i64,
	pub run_id: i64,
	pub resource_id: i64,
	pub agent_id: i64,
	pub action: i64,
	pub local_ver: String,
	pub agent_ver: String,
	pub result: i64,
	pub err_msg: String,
	pub create_time: String,
}

/// 将一行查询结果映射为 SyncRunRow 实体
fn row_to_sync_run(row: &Row) -> rusqlite::Result<SyncRunRow> {
	Ok(SyncRunRow {
		id: row.get(0)?,
		scope_type: row.get(1)?,
		agent_id: row.get(2)?,
		total_cnt: row.get(3)?,
		success_cnt: row.get(4)?,
		failed_cnt: row.get(5)?,
		skipped_cnt: row.get(6)?,
		status: row.get(7)?,
		run_time: row.get(8)?,
		create_time: row.get(9)?,
	})
}

/// 将一行查询结果映射为 SyncItemRow 实体
fn row_to_sync_item(row: &Row) -> rusqlite::Result<SyncItemRow> {
	Ok(SyncItemRow {
		id: row.get(0)?,
		run_id: row.get(1)?,
		resource_id: row.get(2)?,
		agent_id: row.get(3)?,
		action: row.get(4)?,
		local_ver: row.get(5)?,
		agent_ver: row.get(6)?,
		result: row.get(7)?,
		err_msg: row.get(8)?,
		create_time: row.get(9)?,
	})
}

/// 开启一次同步运行(scope_type: 0-全部Agent,1-单Agent,2-选择集; agent_id: 0 表示多个目标),
/// 初始状态固定为"进行中"(status=0, 非入参, 由本方法保证语义, 故直接写常量而非绑定参数),
/// 成功/失败/跳过计数从 0 起算(交给列默认值), 返回该次运行的主键 id(run_id)
pub fn start_run(
	conn: &Connection,
	scope_type: i64,
	agent_id: i64,
	total: i64,
) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO sync_run (scope_type, agent_id, total_cnt, status) VALUES (?1, ?2, ?3, 0)",
		params![scope_type, agent_id, total],
	)?;
	Ok(conn.last_insert_rowid())
}

/// 收尾一次同步运行: 写入最终计数与状态(0-进行中,1-成功,2-部分成功,3-失败),
/// 不改动 total_cnt/scope_type/agent_id, 返回受影响行数
pub fn finish_run(
	conn: &Connection,
	run_id: i64,
	success: i64,
	failed: i64,
	skipped: i64,
	status: i64,
) -> rusqlite::Result<usize> {
	conn.execute(
		"UPDATE sync_run \
		 SET success_cnt = ?1, failed_cnt = ?2, skipped_cnt = ?3, status = ?4 \
		 WHERE id = ?5",
		params![success, failed, skipped, status, run_id],
	)
}

/// 记录一次同步中单个资源项的处理结果(action: 1-新增,2-更新,3-移除;
/// result: 0-待处理,1-成功,2-失败,3-跳过), 返回该行主键 id;
/// 入参逐一对应 sync_item 表列, 按接口约定保持平铺(不额外包装参数结构体), 故豁免 clippy 参数数量检查
#[allow(clippy::too_many_arguments)]
pub fn add_item(
	conn: &Connection,
	run_id: i64,
	resource_id: i64,
	agent_id: i64,
	action: i64,
	local_ver: &str,
	agent_ver: &str,
	result: i64,
	err: &str,
) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO sync_item \
		 (run_id, resource_id, agent_id, action, local_ver, agent_ver, result, err_msg) \
		 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
		params![
			run_id,
			resource_id,
			agent_id,
			action,
			local_ver,
			agent_ver,
			result,
			err
		],
	)?;
	Ok(conn.last_insert_rowid())
}

/// 查询最近若干次同步运行, 按 id 倒序(最新在前)
pub fn recent_runs(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<SyncRunRow>> {
	let mut stmt = conn.prepare(
		"SELECT id, scope_type, agent_id, total_cnt, success_cnt, failed_cnt, skipped_cnt, \
		 status, run_time, create_time \
		 FROM sync_run ORDER BY id DESC LIMIT ?1",
	)?;
	let rows = stmt.query_map(params![limit], row_to_sync_run)?;
	rows.collect()
}

/// 查询某次同步运行的全部明细项, 按 id 升序(处理顺序)
pub fn items_for_run(conn: &Connection, run_id: i64) -> rusqlite::Result<Vec<SyncItemRow>> {
	let mut stmt = conn.prepare(
		"SELECT id, run_id, resource_id, agent_id, action, local_ver, agent_ver, result, \
		 err_msg, create_time \
		 FROM sync_item WHERE run_id = ?1 ORDER BY id",
	)?;
	let rows = stmt.query_map(params![run_id], row_to_sync_item)?;
	rows.collect()
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

	// start_run 应以 status=0(进行中)、给定 total_cnt 起步, 其余计数为 0, 时间戳列非空
	#[test]
	fn start_run_initializes_in_progress_with_total_count() {
		let conn = setup_conn();
		let run_id = start_run(&conn, 1, 20, 3).unwrap();

		let runs = recent_runs(&conn, 10).unwrap();
		assert_eq!(runs.len(), 1);
		let run = &runs[0];
		assert_eq!(run.id, run_id);
		assert_eq!(run.scope_type, 1);
		assert_eq!(run.agent_id, 20);
		assert_eq!(run.total_cnt, 3);
		assert_eq!(run.success_cnt, 0);
		assert_eq!(run.failed_cnt, 0);
		assert_eq!(run.skipped_cnt, 0);
		assert_eq!(run.status, 0, "初始状态应为进行中");
		assert!(!run.run_time.is_empty());
		assert!(!run.create_time.is_empty());
	}

	// finish_run 应写入最终计数与状态, 不影响 total_cnt/scope_type/agent_id
	#[test]
	fn finish_run_updates_counts_and_status_only() {
		let conn = setup_conn();
		let run_id = start_run(&conn, 0, 0, 3).unwrap();

		let affected = finish_run(&conn, run_id, 2, 1, 0, 2).unwrap();
		assert_eq!(affected, 1);

		let run = recent_runs(&conn, 10).unwrap().into_iter().next().unwrap();
		assert_eq!(run.success_cnt, 2);
		assert_eq!(run.failed_cnt, 1);
		assert_eq!(run.skipped_cnt, 0);
		assert_eq!(run.status, 2, "部分成功");
		assert_eq!(run.total_cnt, 3, "finish_run 不应改动 total_cnt");
	}

	// add_item -> items_for_run 应按插入顺序(id 升序)整份还原全部字段
	#[test]
	fn add_item_then_items_for_run_round_trips_all_fields() {
		let conn = setup_conn();
		let run_id = start_run(&conn, 0, 0, 2).unwrap();

		add_item(&conn, run_id, 100, 20, 1, "", "1.0.0", 1, "").unwrap();
		add_item(&conn, run_id, 101, 20, 3, "1.0.0", "", 2, "目标不可写").unwrap();

		let items = items_for_run(&conn, run_id).unwrap();
		assert_eq!(items.len(), 2);
		assert_eq!(items[0].resource_id, 100);
		assert_eq!(items[0].action, 1, "新增");
		assert_eq!(items[0].agent_ver, "1.0.0");
		assert_eq!(items[0].result, 1, "成功");
		assert_eq!(items[0].err_msg, "");
		assert_eq!(items[1].resource_id, 101);
		assert_eq!(items[1].action, 3, "移除");
		assert_eq!(items[1].result, 2, "失败");
		assert_eq!(items[1].err_msg, "目标不可写");
		assert!(items.iter().all(|it| it.run_id == run_id));
	}

	// items_for_run 应只返回该 run 的明细, 不与其它 run 的明细混淆
	#[test]
	fn items_for_run_does_not_leak_across_runs() {
		let conn = setup_conn();
		let run1 = start_run(&conn, 0, 0, 1).unwrap();
		let run2 = start_run(&conn, 0, 0, 1).unwrap();
		add_item(&conn, run1, 1, 1, 1, "", "1.0.0", 1, "").unwrap();
		add_item(&conn, run2, 2, 1, 1, "", "1.0.0", 1, "").unwrap();

		assert_eq!(items_for_run(&conn, run1).unwrap().len(), 1);
		assert_eq!(items_for_run(&conn, run1).unwrap()[0].resource_id, 1);
		assert_eq!(items_for_run(&conn, run2).unwrap()[0].resource_id, 2);
	}

	// recent_runs 应按 id 倒序返回, 且遵守 limit
	#[test]
	fn recent_runs_orders_descending_and_respects_limit() {
		let conn = setup_conn();
		let r1 = start_run(&conn, 0, 0, 1).unwrap();
		let r2 = start_run(&conn, 0, 0, 1).unwrap();
		let r3 = start_run(&conn, 0, 0, 1).unwrap();

		let latest_two = recent_runs(&conn, 2).unwrap();
		assert_eq!(latest_two.len(), 2);
		assert_eq!(latest_two[0].id, r3);
		assert_eq!(latest_two[1].id, r2);
		assert!(
			!latest_two.iter().any(|r| r.id == r1),
			"limit=2 不应包含最早一条"
		);
	}
}
