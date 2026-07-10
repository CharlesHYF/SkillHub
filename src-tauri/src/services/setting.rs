// 文件作用: 设置服务编排层 —— 读取 setting 表全量键值对并类型化还原(get_all)、把一份
//           Settings 逐键落库后回读确认(save)。只接受 &Connection, 不摸 AppState/Tauri
//           运行时, 呼应 services::portability/services::market 既有的分层约定(具体命令层
//           加锁与错误转换见 commands::setting)
// 创建日期: 2026-07-10

use anyhow::Result;
use rusqlite::Connection;

use crate::domain::setting::Settings;
use crate::infra::repo_setting;

/// 读取当前设置: 取 setting 表全量行, 按 domain::setting::Settings::from_rows 类型化还原
/// (缺键或值非法均回落默认值, 见其文档); 表为空(如首次运行, migrations 只建表不预置行)时
/// 等价于返回 Settings::default()
pub fn get_all(conn: &Connection) -> Result<Settings> {
	let rows = repo_setting::list_all(conn)?;
	Ok(Settings::from_rows(&rows))
}

/// 保存设置: 把 s 拍平成的 12 个键值对(见 Settings::to_pairs)逐一 upsert 回 setting 表,
/// 再整表回读返回 —— 不直接回传参数 s 本身, 是为了让返回值真实反映落库后的状态(与
/// services::auth 在 store 之后再查一次同一惯例, 见 commands::auth::auth_enter_token 文档);
/// 全程未使用显式事务, 与本仓库其它多步落库操作同一既有取舍(见 services::portability 模块级
/// 说明), 12 次 upsert 均基于 uk_setting_key 唯一索引, 中途某一次失败也不会留下跨键的半套
/// 不一致状态(单键级别的 upsert 本身是原子的)
pub fn save(conn: &Connection, s: &Settings) -> Result<Settings> {
	for (cfg_key, cfg_value) in s.to_pairs() {
		repo_setting::upsert(conn, &cfg_key, &cfg_value)?;
	}
	get_all(conn)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 建一个已迁移好表结构的内存库, 与 infra::repo_setting 测试同一惯例
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	// 空库(未写入任何设置项)读取应等于 Settings::default()
	#[test]
	fn get_all_returns_default_when_table_empty() {
		let conn = setup_conn();
		assert_eq!(get_all(&conn).unwrap(), Settings::default());
	}

	// save 一份每个字段都改成非默认值的 Settings 后, get_all 应与之完全相等(往返幂等)
	#[test]
	fn save_then_get_all_round_trips_non_default_settings() {
		let conn = setup_conn();
		let changed = Settings {
			storage_skill_dir: "/data/skills".to_string(),
			storage_mcp_dir: "/data/mcp".to_string(),
			sync_auto_new_agent: false,
			sync_check_update_on_start: false,
			sync_conflict_prompt: false,
			sync_only_enabled: true,
			net_proxy_mode: 2,
			net_http_proxy: "http://127.0.0.1:7890".to_string(),
			net_https_proxy: "http://127.0.0.1:7891".to_string(),
			net_no_proxy: "localhost,127.0.0.1".to_string(),
			net_timeout_sec: 60,
			update_channel: 1,
		};

		let saved = save(&conn, &changed).unwrap();
		assert_eq!(saved, changed, "save 的返回值应等于回读结果");
		assert_eq!(
			get_all(&conn).unwrap(),
			changed,
			"再次 get_all 应仍与已保存值一致"
		);
	}

	// save 应幂等: 对同一份设置重复保存两次, 不因 upsert 产生重复行(依赖 uk_setting_key
	// 唯一索引 + repo_setting::upsert 本身的幂等性), 结果与单次保存一致
	#[test]
	fn save_twice_is_idempotent() {
		let conn = setup_conn();
		let changed = Settings {
			net_timeout_sec: 45,
			..Settings::default()
		};

		save(&conn, &changed).unwrap();
		let second = save(&conn, &changed).unwrap();
		assert_eq!(second, changed);

		let rows = repo_setting::list_all(&conn).unwrap();
		assert_eq!(rows.len(), 12, "12 个键各只应有一行, 不因重复保存产生多行");
	}
}
