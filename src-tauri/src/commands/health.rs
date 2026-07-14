// 文件作用: 应用健康检查命令(M0 用于打通前后端调用链路; M1 起基于真实连接探活而非静态标记)
// 创建日期: 2026-07-09

use rusqlite::Connection;
use serde::Serialize;

/// 健康信息: 应用版本 + 数据库是否就绪
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppHealthRespVO {
	pub version: String,
	pub db_ok: bool,
}

/// 纯逻辑: 组装健康信息(便于单测, 与 Tauri 运行时解耦)
pub fn build_health(version: &str, db_ok: bool) -> AppHealthRespVO {
	AppHealthRespVO {
		version: version.to_string(),
		db_ok,
	}
}

/// 对给定连接跑一句最小查询探活, 能正常返回结果即认为数据库可用
fn probe_db_ok(conn: &Connection) -> bool {
	conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
		.is_ok()
}

/// Tauri 命令: 返回应用健康信息
/// db_ok 由"加锁是否成功 + 能否跑通 SELECT 1"共同推导; 故意不用 AppState::db()(会在锁污染时 panic),
/// 健康检查应优雅报告不可用, 而不是让整个命令崩溃。
#[tauri::command]
pub fn app_health(state: tauri::State<'_, crate::AppState>) -> AppHealthRespVO {
	let db_ok = match state.db.lock() {
		Ok(conn) => probe_db_ok(&conn),
		Err(_) => false,
	};
	build_health(env!("CARGO_PKG_VERSION"), db_ok)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn build_health_maps_fields() {
		let h = build_health("0.1.0", true);
		assert_eq!(h.version, "0.1.0");
		assert!(h.db_ok);
	}

	#[test]
	fn build_health_reflects_false_db_ok() {
		let h = build_health("0.1.0", false);
		assert!(!h.db_ok);
	}

	#[test]
	fn probe_db_ok_true_for_live_connection() {
		let conn = Connection::open_in_memory().unwrap();
		assert!(probe_db_ok(&conn));
	}
}
