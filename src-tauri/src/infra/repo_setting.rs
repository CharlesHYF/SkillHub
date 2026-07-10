// 文件作用: setting 表仓储 —— 键值对 upsert 与全量读取, 供导出打包(services::portability::
//           export_bundle 的 include_config 分支)把设置整表落地为 settings.json; 显式列名/
//           禁 SELECT */全参数化查询(阿里巴巴泰山版数据库规约)
// 创建日期: 2026-07-10

use rusqlite::{params, Connection, Row};
use serde::Serialize;

/// setting 表一行(键值对)
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingRow {
	pub cfg_key: String,
	pub cfg_value: String,
}

/// 将一行查询结果映射为 SettingRow 实体
fn row_to_setting(row: &Row) -> rusqlite::Result<SettingRow> {
	Ok(SettingRow {
		cfg_key: row.get(0)?,
		cfg_value: row.get(1)?,
	})
}

/// 按 cfg_key 唯一键(uk_setting_key)插入或冲突更新一条设置, 返回该行主键 id(无论本次是插入
/// 还是更新); 与 infra::repo_agent::upsert/infra::repo_assoc::set 同一 upsert 惯例
pub fn upsert(conn: &Connection, cfg_key: &str, cfg_value: &str) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO setting (cfg_key, cfg_value) VALUES (?1, ?2) \
		 ON CONFLICT(cfg_key) DO UPDATE SET \
		 cfg_value = excluded.cfg_value, update_time = datetime('now')",
		params![cfg_key, cfg_value],
	)?;
	conn.query_row(
		"SELECT id FROM setting WHERE cfg_key = ?1",
		params![cfg_key],
		|row| row.get(0),
	)
}

/// 查询全部设置项, 按 cfg_key 升序(确定性顺序, 便于导出内容/校验和可重现)
pub fn list_all(conn: &Connection) -> rusqlite::Result<Vec<SettingRow>> {
	let mut stmt = conn.prepare("SELECT cfg_key, cfg_value FROM setting ORDER BY cfg_key")?;
	let rows = stmt.query_map([], row_to_setting)?;
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

	// upsert -> list_all 应整份还原键值, 按 cfg_key 升序返回
	#[test]
	fn upsert_then_list_all_round_trips_ordered_by_key() {
		let conn = setup_conn();
		upsert(&conn, "sync.pref", "auto").unwrap();
		upsert(&conn, "net.proxy", "http://127.0.0.1:7890").unwrap();

		let rows = list_all(&conn).unwrap();
		assert_eq!(rows.len(), 2);
		assert_eq!(
			rows[0].cfg_key, "net.proxy",
			"按 cfg_key 升序, net 在 sync 前"
		);
		assert_eq!(rows[0].cfg_value, "http://127.0.0.1:7890");
		assert_eq!(rows[1].cfg_key, "sync.pref");
		assert_eq!(rows[1].cfg_value, "auto");
	}

	// upsert 应幂等: 同 cfg_key 二次调用命中同一行, 用第二次的值覆盖, 不产生第二行
	#[test]
	fn upsert_same_key_twice_is_idempotent_and_overwrites_value() {
		let conn = setup_conn();
		let id1 = upsert(&conn, "net.proxy", "http://old").unwrap();
		let id2 = upsert(&conn, "net.proxy", "http://new").unwrap();

		assert_eq!(id1, id2, "同一 cfg_key 应命中同一行");
		let rows = list_all(&conn).unwrap();
		assert_eq!(rows.len(), 1, "重复 upsert 不应产生多行");
		assert_eq!(rows[0].cfg_value, "http://new");
	}

	// list_all 在无设置项时应返回空列表而非报错
	#[test]
	fn list_all_returns_empty_when_no_settings() {
		let conn = setup_conn();
		assert_eq!(list_all(&conn).unwrap(), Vec::new());
	}
}
