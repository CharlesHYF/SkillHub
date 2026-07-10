// 文件作用: 桌面应用二进制入口, 委托给 skillhub_lib::run() 启动 Tauri
// 创建日期: 2026-07-09

// 阻止 Windows release 构建下弹出额外的控制台窗口, 不要删除该行
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 程序入口: 调用 lib crate 的 run() 装配并启动 Tauri 应用
fn main() {
	skillhub_lib::run()
}
