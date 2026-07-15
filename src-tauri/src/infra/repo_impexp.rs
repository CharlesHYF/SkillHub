// 文件作用: import_export_log 表仓储 —— 导入导出历史写入与最近记录查询(原型第 6 屏"导入导出历史"
//           表格来源), 显式列名/禁 SELECT */全参数化查询(阿里巴巴泰山版数据库规约)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13

use rusqlite::{params, Connection, Row};
use serde::Serialize;

/// import_export_log 表一行, 字段名与列名逐一对应(create_time 非本仓储关注字段, 不选取)
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImpexpRespVO {
	pub id: i64,
	pub direction: i64,
	pub file_name: String,
	pub file_format: i64,
	pub summary: String,
	pub status: i64,
	pub run_time: String,
}

/// 将一行查询结果映射为 ImpexpRespVO 实体
fn row_to_impexp(row: &Row) -> rusqlite::Result<ImpexpRespVO> {
	Ok(ImpexpRespVO {
		id: row.get(0)?,
		direction: row.get(1)?,
		file_name: row.get(2)?,
		file_format: row.get(3)?,
		summary: row.get(4)?,
		status: row.get(5)?,
		run_time: row.get(6)?,
	})
}

/// 追加一条导入导出历史(direction: 0-导出,1-导入; file_format: 1-zip,2-json,3-tar;
/// status: 0-失败,1-成功,2-部分成功), 返回该行主键 id; run_time 交给列默认值 datetime('now')
pub fn add(
	conn: &Connection,
	direction: i64,
	file_name: &str,
	file_format: i64,
	summary: &str,
	status: i64,
) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO import_export_log (direction, file_name, file_format, summary, status) \
		 VALUES (?1, ?2, ?3, ?4, ?5)",
		params![direction, file_name, file_format, summary, status],
	)?;
	Ok(conn.last_insert_rowid())
}

/// 查询最近若干条导入导出历史, 按 run_time/id 倒序(最新在前)
pub fn recent(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<ImpexpRespVO>> {
	let mut stmt = conn.prepare(
		"SELECT id, direction, file_name, file_format, summary, status, run_time \
		 FROM import_export_log ORDER BY run_time DESC, id DESC LIMIT ?1",
	)?;
	let rows = stmt.query_map(params![limit], row_to_impexp)?;
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

	// add -> recent 应整份还原字段, run_time 由数据库填充
	#[test]
	fn add_then_recent_round_trips_all_fields() {
		let conn = setup_conn();
		let id = add(
			&conn,
			0,
			"skillhub-export-20260710.zip",
			1,
			"3 Skill+2 MCP",
			1,
		)
		.unwrap();

		let rows = recent(&conn, 10).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].id, id);
		assert_eq!(rows[0].direction, 0, "0-导出");
		assert_eq!(rows[0].file_name, "skillhub-export-20260710.zip");
		assert_eq!(rows[0].file_format, 1, "1-zip");
		assert_eq!(rows[0].summary, "3 Skill+2 MCP");
		assert_eq!(rows[0].status, 1, "1-成功");
		assert!(!rows[0].run_time.is_empty());
	}

	// recent 应按插入倒序(最新在前)返回, 且遵守 limit
	#[test]
	fn recent_orders_newest_first_and_respects_limit() {
		let conn = setup_conn();
		let _first = add(&conn, 0, "first.zip", 1, "", 1).unwrap();
		let _second = add(&conn, 1, "second.json", 2, "", 1).unwrap();
		let third = add(&conn, 1, "third.tar", 3, "", 0).unwrap();

		let latest_two = recent(&conn, 2).unwrap();
		assert_eq!(latest_two.len(), 2);
		assert_eq!(latest_two[0].id, third, "最新一条应排最前");
		assert_eq!(latest_two[0].file_name, "third.tar");
		assert_eq!(latest_two[1].file_name, "second.json");
	}

	// recent 在无记录时应返回空列表而非报错
	#[test]
	fn recent_returns_empty_when_no_history() {
		let conn = setup_conn();
		assert_eq!(recent(&conn, 10).unwrap(), Vec::new());
	}
}
