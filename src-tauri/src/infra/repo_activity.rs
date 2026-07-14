// 文件作用: activity_log 表仓储 —— 活动流写入与最近记录查询(首页"最近变更"来源),
//           显式列名/禁 SELECT */全参数化查询(阿里巴巴泰山版数据库规约)
// 创建日期: 2026-07-09

use rusqlite::{params, Connection, Row};
use serde::Serialize;

/// activity_log 表一行, 字段名与列名逐一对应
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRespVO {
	pub id: i64,
	pub act_type: i64,
	pub res_type: i64,
	pub title: String,
	pub detail: String,
	pub create_time: String,
}

/// 将一行查询结果映射为 ActivityRespVO 实体
fn row_to_activity(row: &Row) -> rusqlite::Result<ActivityRespVO> {
	Ok(ActivityRespVO {
		id: row.get(0)?,
		act_type: row.get(1)?,
		res_type: row.get(2)?,
		title: row.get(3)?,
		detail: row.get(4)?,
		create_time: row.get(5)?,
	})
}

/// 追加一条活动记录(act_type: 1-新增,2-更新,3-下载,4-导入,5-导出,6-同步,7-卸载;
/// res_type: 0-无,1-Skill,2-MCP,3-配置,4-Agent), 返回该行主键 id
pub fn add(
	conn: &Connection,
	act_type: i64,
	res_type: i64,
	title: &str,
	detail: &str,
) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO activity_log (act_type, res_type, title, detail) VALUES (?1, ?2, ?3, ?4)",
		params![act_type, res_type, title, detail],
	)?;
	Ok(conn.last_insert_rowid())
}

/// 查询最近若干条活动, 按 create_time/id 倒序(最新在前)
pub fn recent(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<ActivityRespVO>> {
	let mut stmt = conn.prepare(
		"SELECT id, act_type, res_type, title, detail, create_time \
		 FROM activity_log ORDER BY create_time DESC, id DESC LIMIT ?1",
	)?;
	let rows = stmt.query_map(params![limit], row_to_activity)?;
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

	// add -> recent 应整份还原字段, create_time 由数据库填充
	#[test]
	fn add_then_recent_round_trips_all_fields() {
		let conn = setup_conn();
		let id = add(&conn, 1, 1, "安装 charles-coding", "从官方仓库安装").unwrap();

		let rows = recent(&conn, 10).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].id, id);
		assert_eq!(rows[0].act_type, 1, "1-新增");
		assert_eq!(rows[0].res_type, 1, "1-Skill");
		assert_eq!(rows[0].title, "安装 charles-coding");
		assert_eq!(rows[0].detail, "从官方仓库安装");
		assert!(!rows[0].create_time.is_empty());
	}

	// recent 应按插入倒序(最新在前)返回, 且遵守 limit
	#[test]
	fn recent_orders_newest_first_and_respects_limit() {
		let conn = setup_conn();
		let _first = add(&conn, 1, 1, "第一条", "").unwrap();
		let _second = add(&conn, 2, 1, "第二条", "").unwrap();
		let third = add(&conn, 6, 4, "第三条", "").unwrap();

		let latest_two = recent(&conn, 2).unwrap();
		assert_eq!(latest_two.len(), 2);
		assert_eq!(latest_two[0].id, third, "最新一条应排最前");
		assert_eq!(latest_two[0].title, "第三条");
		assert_eq!(latest_two[1].title, "第二条");
	}

	// recent 在无记录时应返回空列表而非报错
	#[test]
	fn recent_returns_empty_when_no_activity() {
		let conn = setup_conn();
		assert_eq!(recent(&conn, 10).unwrap(), Vec::new());
	}
}
