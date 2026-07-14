// 文件作用: auth_account 表仓储 —— upsert/list/get_by_provider/delete, 显式列名/禁 SELECT */
//           全参数化查询(阿里巴巴泰山版数据库规约); 令牌密文绝不入库, keyring_ref 只是钥匙串
//           条目引用键(见 migrations/0001_init.sql auth_account 表注释)
// 创建日期: 2026-07-09

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::auth::{AuthAccountRespVO, ProviderKind};

/// 将一行查询结果映射为 AuthAccountRespVO 实体; keyring_ref 是纯基建字段, 不进入领域实体, 不在此还原
fn row_to_auth_account(row: &Row) -> rusqlite::Result<AuthAccountRespVO> {
	Ok(AuthAccountRespVO {
		id: row.get(0)?,
		provider: ProviderKind::from_i64(row.get(1)?),
		account: row.get(2)?,
		scope: row.get(3)?,
		status: row.get(4)?,
		connect_time: row.get(5)?,
	})
}

/// 按 (provider, account) 唯一键(uk_auth_account_prov_acc)插入或冲突更新, 返回该行主键 id
/// (无论本次是插入还是更新, 以此为准; item.id 由调用方随意填充, 不参与本次写入)。keyring_ref
/// 单独传参(不在 AuthAccountRespVO 领域实体里), 令牌密文本身绝不经此函数落库, 只落这一引用键
pub fn upsert(
	conn: &Connection,
	item: &AuthAccountRespVO,
	keyring_ref: &str,
) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO auth_account (provider, account, scope, keyring_ref, status, connect_time) \
		 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
		 ON CONFLICT(provider, account) DO UPDATE SET \
		 scope = excluded.scope, keyring_ref = excluded.keyring_ref, status = excluded.status, \
		 connect_time = excluded.connect_time, update_time = datetime('now')",
		params![
			i64::from(item.provider),
			item.account,
			item.scope,
			keyring_ref,
			item.status,
			item.connect_time,
		],
	)?;
	conn.query_row(
		"SELECT id FROM auth_account WHERE provider = ?1 AND account = ?2",
		params![i64::from(item.provider), item.account],
		|row| row.get(0),
	)
}

/// 查询全部已连接账号, 按 id 升序
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<AuthAccountRespVO>> {
	let mut stmt = conn.prepare(
		"SELECT id, provider, account, scope, status, connect_time FROM auth_account ORDER BY id",
	)?;
	let rows = stmt.query_map([], row_to_auth_account)?;
	rows.collect()
}

/// 按 provider 查询账号; 表仅约束 (provider, account) 唯一, 同一 provider 理论上可存在多个
/// 账号, 此处返回最近一条(id 最大, 即最近一次 upsert 命中/新建的账号), 需要全部请用 list 自行
/// 按 provider 过滤。不存在返回 None
pub fn get_by_provider(
	conn: &Connection,
	provider: i64,
) -> rusqlite::Result<Option<AuthAccountRespVO>> {
	conn.query_row(
		"SELECT id, provider, account, scope, status, connect_time FROM auth_account \
		 WHERE provider = ?1 ORDER BY id DESC LIMIT 1",
		params![provider],
		row_to_auth_account,
	)
	.optional()
}

/// 按 provider 删除该提供方下全部账号, 返回受影响行数(密文由调用方负责同步清理系统钥匙串
/// 对应 keyring_ref 条目, 本函数只管数据库这一侧)
pub fn delete(conn: &Connection, provider: i64) -> rusqlite::Result<usize> {
	conn.execute(
		"DELETE FROM auth_account WHERE provider = ?1",
		params![provider],
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

	fn sample_account() -> AuthAccountRespVO {
		AuthAccountRespVO {
			id: 0,
			provider: ProviderKind::GitHub,
			account: "demo@example.com".to_string(),
			scope: "repo,read:org".to_string(),
			status: true,
			connect_time: "2026-07-01T00:00:00Z".to_string(),
		}
	}

	// upsert -> get_by_provider 应还原全部领域字段; keyring_ref 不进入 AuthAccountRespVO, 单独白盒校验落库
	#[test]
	fn upsert_then_get_by_provider_round_trips_fields() {
		let conn = setup_conn();
		let id = upsert(&conn, &sample_account(), "keyring:github:demo").unwrap();

		let got = get_by_provider(&conn, i64::from(ProviderKind::GitHub))
			.unwrap()
			.expect("刚 upsert 的账号应能查到");
		assert_eq!(got.id, id);
		assert_eq!(got.provider, ProviderKind::GitHub);
		assert_eq!(got.account, "demo@example.com");
		assert_eq!(got.scope, "repo,read:org");
		assert!(got.status);
		assert_eq!(got.connect_time, "2026-07-01T00:00:00Z");

		let keyring_ref: String = conn
			.query_row(
				"SELECT keyring_ref FROM auth_account WHERE id = ?1",
				params![id],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(keyring_ref, "keyring:github:demo");
	}

	// upsert 应幂等: 同 (provider, account) 二次调用命中同一行, 用第二次的值覆盖 scope/status/keyring_ref
	#[test]
	fn upsert_same_provider_and_account_is_idempotent_and_overwrites_fields() {
		let conn = setup_conn();
		let first = sample_account();
		let id1 = upsert(&conn, &first, "keyring:v1").unwrap();

		let mut second = first.clone();
		second.scope = "repo".to_string();
		second.status = false;
		let id2 = upsert(&conn, &second, "keyring:v2").unwrap();

		assert_eq!(id1, id2, "同一 (provider, account) 应命中同一行");
		assert_eq!(list(&conn).unwrap().len(), 1, "不应产生多行");

		let got = get_by_provider(&conn, i64::from(ProviderKind::GitHub))
			.unwrap()
			.unwrap();
		assert_eq!(got.scope, "repo");
		assert!(!got.status);
	}

	// list 应按 id 升序返回全部账号(跨多个 provider)
	#[test]
	fn list_returns_all_accounts_ordered_by_id() {
		let conn = setup_conn();
		let github = sample_account();
		upsert(&conn, &github, "keyring:github").unwrap();
		let mut google = sample_account();
		google.provider = ProviderKind::Google;
		google.account = "demo@gmail.com".to_string();
		upsert(&conn, &google, "keyring:google").unwrap();

		let all = list(&conn).unwrap();
		assert_eq!(all.len(), 2);
		assert_eq!(all[0].provider, ProviderKind::GitHub);
		assert_eq!(all[1].provider, ProviderKind::Google);
	}

	// get_by_provider 查无此 provider 应返回 None, 不是 Err
	#[test]
	fn get_by_provider_missing_returns_none() {
		let conn = setup_conn();
		assert_eq!(
			get_by_provider(&conn, i64::from(ProviderKind::GitHub)).unwrap(),
			None
		);
	}

	// delete 应移除该 provider 下全部账号, 不影响其它 provider
	#[test]
	fn delete_removes_only_target_provider_accounts() {
		let conn = setup_conn();
		upsert(&conn, &sample_account(), "keyring:github").unwrap();
		let mut google = sample_account();
		google.provider = ProviderKind::Google;
		google.account = "demo@gmail.com".to_string();
		upsert(&conn, &google, "keyring:google").unwrap();

		let affected = delete(&conn, i64::from(ProviderKind::GitHub)).unwrap();
		assert_eq!(affected, 1);
		assert_eq!(
			get_by_provider(&conn, i64::from(ProviderKind::GitHub)).unwrap(),
			None
		);
		assert!(get_by_provider(&conn, i64::from(ProviderKind::Google))
			.unwrap()
			.is_some());
	}
}
