// 文件作用: SQLite 连接管理与数据库迁移执行
// 创建日期: 2026-07-09

use rusqlite::Connection;
use std::path::Path;

/// 迁移脚本表: (版本号, SQL 内容), 按版本升序执行
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../../migrations/0001_init.sql"))];

/// 打开数据库并执行迁移, 返回可用连接
pub fn open_and_migrate(path: &Path) -> rusqlite::Result<Connection> {
	let mut conn = Connection::open(path)?;
	migrate(&mut conn)?;
	Ok(conn)
}

/// 按 PRAGMA user_version 增量执行内置迁移集
/// 可见性为 pub(crate): 供 infra::repo_resource 等仓储层测试在内存库上复用建表逻辑
pub(crate) fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
	apply_migrations(conn, MIGRATIONS)
}

/// 执行给定迁移集: 每条迁移的 DDL 与版本号写入包在同一事务里原子提交,
/// 中途失败则整体回滚, 不会留下"表已建但 user_version 未推进"的半损坏库。
fn apply_migrations(conn: &mut Connection, migrations: &[(i64, &str)]) -> rusqlite::Result<()> {
	let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
	for (ver, sql) in migrations {
		if *ver > current {
			let tx = conn.transaction()?;
			tx.execute_batch(sql)?;
			// user_version 参与事务, 随 COMMIT 原子提交(ver 为常量整数, 无注入风险)
			tx.pragma_update(None, "user_version", *ver)?;
			tx.commit()?;
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
		let mut conn = Connection::open_in_memory().unwrap();
		migrate(&mut conn).unwrap();
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
		let mut conn = Connection::open_in_memory().unwrap();
		migrate(&mut conn).unwrap();
		migrate(&mut conn).unwrap();
		let v: i64 = conn
			.query_row("PRAGMA user_version", [], |r| r.get(0))
			.unwrap();
		assert_eq!(v, 1);
	}

	/// 迁移应原子: 中途失败则整体回滚, 不推进版本、不留半建的表
	#[test]
	fn migration_is_atomic_on_failure() {
		let mut conn = Connection::open_in_memory().unwrap();
		// 构造一条会中途失败的迁移: 先建合法表, 再执行非法 SQL
		let bad: &[(i64, &str)] = &[(
			1,
			"CREATE TABLE t_partial (id INTEGER PRIMARY KEY);\nCREATE TABLE ;",
		)];
		let res = apply_migrations(&mut conn, bad);
		assert!(res.is_err(), "非法迁移应返回 Err");

		let tables: i64 = conn
			.query_row(
				"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 't_partial'",
				[],
				|r| r.get(0),
			)
			.unwrap();
		assert_eq!(tables, 0, "失败迁移建的表应被回滚");

		let v: i64 = conn
			.query_row("PRAGMA user_version", [], |r| r.get(0))
			.unwrap();
		assert_eq!(v, 0, "失败迁移不应推进 user_version");
	}
}
