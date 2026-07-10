// 文件作用: 导入导出相关 Tauri 命令(导出部分) —— export_bundle 只负责加锁取出 conn/data_dir
//           与错误类型转换(anyhow::Error -> String, 与仓库其它命令的既有惯例一致), 具体逻辑见
//           services::portability。纯同步命令(无网络 I/O), 全程持锁即可, 不涉及
//           commands::market 那种因 await 产生的 Send 拆分顾虑
// 创建日期: 2026-07-10

use std::path::Path;

use tauri::State;

use crate::domain::portability::{ExportOptions, Manifest};
use crate::services::portability;
use crate::AppState;

/// 导出打包: 按 options 收集资源/配置/关联并打包到 out_path(前端经保存对话框选定的绝对路径,
/// M3 前端可先用固定/输入路径占位, 真实文件对话框留待接入 tauri-plugin-dialog 或等价方案),
/// 返回打包清单(Manifest); 与前端 api 层的契约为 export_bundle{options,outPath}->Manifest
/// (Tauri 自动把 out_path 转成 outPath, 见 progress.md M3 契约记录)
#[tauri::command]
pub fn export_bundle(
	state: State<'_, AppState>,
	options: ExportOptions,
	out_path: String,
) -> Result<Manifest, String> {
	let conn = state.db();
	portability::export_bundle(&conn, &state.data_dir, &options, Path::new(&out_path))
		.map_err(|e| e.to_string())
}
