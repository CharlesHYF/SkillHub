// 文件作用: 设置服务编排层 —— 读取 setting 表全量键值对并类型化还原(get_all)、把一份
//           SettingRespVO 逐键落库后回读确认(save)。只接受 &Connection, 不摸 AppState/Tauri
//           运行时, 呼应 services::portability/services::market 既有的分层约定(具体命令层
//           加锁与错误转换见 commands::setting)
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::domain::setting::SettingRespVO;
use crate::infra::repo_setting;

/// 读取当前设置: 取 setting 表全量行, 按 domain::setting::SettingRespVO::from_rows 类型化还原
/// (缺键或值非法均回落默认值, 见其文档); 表为空(如首次运行, migrations 只建表不预置行)时
/// 等价于返回 SettingRespVO::default()
pub fn get_all(conn: &Connection) -> Result<SettingRespVO> {
	let rows = repo_setting::list_all(conn)?;
	Ok(SettingRespVO::from_rows(&rows))
}

/// 保存设置: 把 s 拍平成的 12 个键值对(见 SettingRespVO::to_pairs)逐一 upsert 回 setting 表,
/// 再整表回读返回 —— 不直接回传参数 s 本身, 是为了让返回值真实反映落库后的状态(与
/// services::auth 在 store 之后再查一次同一惯例, 见 commands::auth::auth_enter_token 文档);
/// 全程未使用显式事务, 与本仓库其它多步落库操作同一既有取舍(见 services::portability 模块级
/// 说明), 12 次 upsert 均基于 uk_setting_key 唯一索引, 中途某一次失败也不会留下跨键的半套
/// 不一致状态(单键级别的 upsert 本身是原子的)
pub fn save(conn: &Connection, s: &SettingRespVO) -> Result<SettingRespVO> {
	for (cfg_key, cfg_value) in s.to_pairs() {
		repo_setting::upsert(conn, &cfg_key, &cfg_value)?;
	}
	get_all(conn)
}

/// 读取设置, 并把"空的存储目录"回填为应用数据目录(data_dir)下的默认位置后返回; 若发生回填
/// (原本为空)则一并持久化, 使设置界面首次进入即展示真实默认目录而非空占位。空串在
/// domain::setting 里语义为"用默认位置", 但用户看不到也无法基于它编辑, 故命令层拿得到 data_dir
/// 时解析成真实 skills/mcp 子目录填好保存(见 Charles 反馈: 目录按默认值填写保存)。幂等: 已填好
/// (非空)后再次调用不再改动/落库; 用户显式设过的目录也不会被覆盖
pub fn get_all_with_default_dirs(conn: &Connection, data_dir: &Path) -> Result<SettingRespVO> {
	let mut settings = get_all(conn)?;
	let mut changed = false;
	if settings.storage_skill_dir.is_empty() {
		settings.storage_skill_dir = data_dir.join("skills").to_string_lossy().into_owned();
		changed = true;
	}
	if settings.storage_mcp_dir.is_empty() {
		settings.storage_mcp_dir = data_dir.join("mcp").to_string_lossy().into_owned();
		changed = true;
	}
	if changed {
		save(conn, &settings)?;
	}
	Ok(settings)
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

	// 空库(未写入任何设置项)读取应等于 SettingRespVO::default()
	#[test]
	fn get_all_returns_default_when_table_empty() {
		let conn = setup_conn();
		assert_eq!(get_all(&conn).unwrap(), SettingRespVO::default());
	}

	// save 一份每个字段都改成非默认值的 SettingRespVO 后, get_all 应与之完全相等(往返幂等)
	#[test]
	fn save_then_get_all_round_trips_non_default_settings() {
		let conn = setup_conn();
		let changed = SettingRespVO {
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
		let changed = SettingRespVO {
			net_timeout_sec: 45,
			..SettingRespVO::default()
		};

		save(&conn, &changed).unwrap();
		let second = save(&conn, &changed).unwrap();
		assert_eq!(second, changed);

		let rows = repo_setting::list_all(&conn).unwrap();
		assert_eq!(rows.len(), 12, "12 个键各只应有一行, 不因重复保存产生多行");
	}

	// get_all_with_default_dirs: 空存储目录应回填为 data_dir/skills、data_dir/mcp 并持久化, 且幂等
	#[test]
	fn get_all_with_default_dirs_fills_empty_dirs_and_persists() {
		let conn = setup_conn();
		let data_dir = Path::new("/tmp/skillhub-data");

		let filled = get_all_with_default_dirs(&conn, data_dir).unwrap();
		assert_eq!(filled.storage_skill_dir, "/tmp/skillhub-data/skills");
		assert_eq!(filled.storage_mcp_dir, "/tmp/skillhub-data/mcp");

		// 已持久化: 直接 get_all 也应读到回填后的值(而非再次回落空串)
		let reloaded = get_all(&conn).unwrap();
		assert_eq!(reloaded.storage_skill_dir, "/tmp/skillhub-data/skills");
		assert_eq!(reloaded.storage_mcp_dir, "/tmp/skillhub-data/mcp");

		// 幂等: 已非空, 换个 data_dir 再调用也不应覆盖已填值
		let again = get_all_with_default_dirs(&conn, Path::new("/other")).unwrap();
		assert_eq!(again.storage_skill_dir, "/tmp/skillhub-data/skills");
		assert_eq!(again.storage_mcp_dir, "/tmp/skillhub-data/mcp");
	}

	// get_all_with_default_dirs: 用户显式设过的目录不被"默认回填"覆盖
	#[test]
	fn get_all_with_default_dirs_keeps_user_set_dirs() {
		let conn = setup_conn();
		let user = SettingRespVO {
			storage_skill_dir: "/my/custom/skills".to_string(),
			storage_mcp_dir: "/my/custom/mcp".to_string(),
			..SettingRespVO::default()
		};
		save(&conn, &user).unwrap();

		let got = get_all_with_default_dirs(&conn, Path::new("/tmp/x")).unwrap();
		assert_eq!(got.storage_skill_dir, "/my/custom/skills");
		assert_eq!(got.storage_mcp_dir, "/my/custom/mcp");
	}
}
