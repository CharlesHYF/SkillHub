// 文件作用: 设置相关 Tauri 命令 —— 读取当前设置(settings_get)、保存整份设置并回读确认
//           (settings_save)、读取应用版本号供"关于"区展示(app_version)。均为纯同步命令
//           (无网络 I/O), 只负责加锁取出 conn 与错误类型转换(anyhow::Error -> String, 与
//           commands::portability 等既有命令同一惯例), 具体逻辑见 services::setting
// 创建日期: 2026-07-10

use tauri::State;

use crate::domain::setting::SettingRespVO;
use crate::services::setting;
use crate::AppState;

/// 读取当前设置; 空的存储目录会被回填为应用数据目录下的默认位置(skills/mcp)并持久化, 使
/// 设置界面首次进入即展示真实目录而非空占位; 见 services::setting::get_all_with_default_dirs
#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<SettingRespVO, String> {
	let conn = state.db();
	setting::get_all_with_default_dirs(&conn, &state.data_dir).map_err(|e| e.to_string())
}

/// 保存整份设置并回读确认; 见 services::setting::save
#[tauri::command]
pub fn settings_save(
	state: State<'_, AppState>,
	settings: SettingRespVO,
) -> Result<SettingRespVO, String> {
	let conn = state.db();
	setting::save(&conn, &settings).map_err(|e| e.to_string())
}

/// 应用版本号, 供前端"关于"区展示; 直接读构建期注入的 Cargo 包版本(与
/// commands::health::app_health 内联读取同一常量的既有做法一致), 单独暴露一个专属命令,
/// 使"设置"屏无需拉取整份健康检查信息也能取到版本号。返回 Result 是为了与本模块其它命令
/// 同一签名风格保持一致, 便于前端 api 层统一处理, 本命令实际恒返回 Ok
#[tauri::command]
pub fn app_version() -> Result<String, String> {
	Ok(env!("CARGO_PKG_VERSION").to_string())
}
