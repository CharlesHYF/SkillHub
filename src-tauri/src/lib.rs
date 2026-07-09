// 文件作用: Tauri Builder 装配入口(脚手架基线), 后续任务会在此接入 SQLite 与 commands
// 创建日期: 2026-07-09

mod infra;

// 示例命令: 演示前端经 invoke() 调用 Rust 的链路是否打通, 后续任务会替换为真实业务命令
// 了解更多: https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
	format!("Hello, {}! You've been greeted from Rust!", name)
}

// 装配并启动 Tauri 应用: 注册插件与命令处理器
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.invoke_handler(tauri::generate_handler![greet])
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
