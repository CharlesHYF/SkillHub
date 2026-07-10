// 文件作用: Tauri Builder 装配入口, 初始化 SQLite 数据库并注册 commands 层
// 创建日期: 2026-07-09

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;
use tauri::Manager;

mod commands;
pub mod domain;
pub mod infra;
pub mod services;

/// 应用共享状态: 持有唯一的 SQLite 连接与应用数据目录
/// rusqlite::Connection 非 Sync, 跨线程共享必须包一层 Mutex
pub struct AppState {
	pub db: Mutex<Connection>,
	pub data_dir: PathBuf,
}

impl AppState {
	/// 加锁获取数据库连接的便捷方法; 锁被污染(poisoned)说明此前某处持锁 panic,
	/// 属严重 bug, 此处快速失败优于静默返回坏状态。
	/// 注意: 对健康检查等需要"优雅报告不可用"而非"崩溃"的场景,
	/// 请直接匹配 `self.db.lock()` 的 Result, 不要用本方法(见 commands::health::app_health)。
	pub fn db(&self) -> MutexGuard<'_, Connection> {
		self.db.lock().expect("数据库连接锁已损坏(poisoned)")
	}
}

// 装配并启动 Tauri 应用: 初始化数据库、注册插件与命令处理器
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.setup(|app| {
			// 初始化数据库(路径: 应用数据目录/skillhub.db), 失败即视为无法启动
			let dir = app.path().app_data_dir().expect("无法获取应用数据目录");
			std::fs::create_dir_all(&dir).ok();
			let conn =
				infra::store::open_and_migrate(&dir.join("skillhub.db")).expect("数据库初始化失败");
			app.manage(AppState {
				db: Mutex::new(conn),
				data_dir: dir,
			});
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![
			commands::health::app_health,
			commands::auth::auth_accounts,
			commands::auth::auth_enter_token,
			commands::auth::auth_login,
			commands::auth::auth_logout,
			commands::library::library_list,
			commands::library::library_get,
			commands::library::library_counts,
			commands::library::resource_import_local,
			commands::library::resource_set_enabled,
			commands::library::resource_delete,
			commands::agent::agent_detect,
			commands::agent::agent_list,
			commands::sync::assoc_set,
			commands::sync::sync_diff,
			commands::sync::sync_apply,
			commands::sync::resource_agent_links,
			commands::dashboard::dashboard_summary,
			commands::dashboard::activity_recent,
			commands::market::market_search,
			commands::market::market_detail,
			commands::market::market_refresh,
			commands::market::market_install,
			commands::portability::export_bundle,
			commands::portability::import_preview,
			commands::portability::import_bundle,
			commands::portability::impexp_history,
		])
		.run(tauri::generate_context!())
		.expect("运行 Tauri 应用失败");
}
