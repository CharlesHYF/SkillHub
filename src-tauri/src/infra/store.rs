// 文件作用: SQLite 连接管理与数据库迁移执行
// 创建日期: 2026-07-09

use rusqlite::Connection;
use std::path::Path;

/// 迁移脚本表: (版本号, SQL 内容), 按版本升序执行
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../../migrations/0001_init.sql"))];

/// 打开数据库并执行迁移, 返回可用连接
pub fn open_and_migrate(path: &Path) -> rusqlite::Result<Connection> {
	let conn = Connection::open(path)?;
	migrate(&conn)?;
	Ok(conn)
}

/// 按 PRAGMA user_version 增量执行未应用的迁移
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
	let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
	for (ver, sql) in MIGRATIONS {
		if *ver > current {
			conn.execute_batch(sql)?;
			// user_version 不支持参数绑定, 用格式化写入(ver 为常量整数, 无注入风险)
			conn.pragma_update(None, "user_version", *ver)?;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 迁移后 10 张业务表应全部存在
	#[test]
	fn migrate_creates_all_tables() {
		let conn = Connection::open_in_memory().unwrap();
		migrate(&conn).unwrap();
		let n: i64 = conn
			.query_row(
				"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
				 ('resource','agent','resource_agent','sync_run','sync_item','market_cache',\
				  'auth_account','import_export_log','setting','activity_log')",
				[],
				|r| r.get(0),
			)
			.unwrap();
		assert_eq!(n, 10);
	}

	/// 迁移应幂等: 重复执行不报错, 版本停在 1
	#[test]
	fn migrate_is_idempotent() {
		let conn = Connection::open_in_memory().unwrap();
		migrate(&conn).unwrap();
		migrate(&conn).unwrap();
		let v: i64 = conn
			.query_row("PRAGMA user_version", [], |r| r.get(0))
			.unwrap();
		assert_eq!(v, 1);
	}
}
