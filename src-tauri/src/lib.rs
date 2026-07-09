// 文件作用: Tauri Builder 装配入口, 初始化 SQLite 数据库并注册 commands 层
// 创建日期: 2026-07-09

use tauri::Manager;

mod commands;
mod infra;

/// 应用共享状态
pub struct AppState {
	pub db_ok: bool,
}

// 装配并启动 Tauri 应用: 初始化数据库、注册插件与命令处理器
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.setup(|app| {
			// 初始化数据库(路径: 应用数据目录/skillhub.db)
			let dir = app.path().app_data_dir().expect("无法获取应用数据目录");
			std::fs::create_dir_all(&dir).ok();
			let db_ok = infra::store::open_and_migrate(&dir.join("skillhub.db")).is_ok();
			app.manage(AppState { db_ok });
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![commands::health::app_health])
		.run(tauri::generate_context!())
		.expect("运行 Tauri 应用失败");
}
