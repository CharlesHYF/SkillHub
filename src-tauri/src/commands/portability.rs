// 文件作用: 导入导出相关 Tauri 命令(导出 + 导入预览部分) —— export_bundle/import_preview 只负责
//           加锁取出 conn/data_dir(如需要)与错误类型转换(anyhow::Error -> String, 与仓库其它
//           命令的既有惯例一致), 具体逻辑见 services::portability。均为纯同步命令(无网络 I/O),
//           全程持锁即可, 不涉及 commands::market 那种因 await 产生的 Send 拆分顾虑
// 创建日期: 2026-07-10

use std::path::Path;

use tauri::State;

use crate::domain::portability::{ExportOptions, ImportPreview, Manifest};
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

/// 导入预览: 解析并校验 path 指向的导入包(见 services::portability::parse_bundle —— 含 schema
/// 版本/条目路径 zip-slip 穿越/条目集合与 manifest 记录一致/大小上限/逐条目 sha256 五类校验,
/// 任一不过即整体失败, 不做任何磁盘落地), 校验通过后换算为前端"将导入内容"面板所需的计数 +
/// schema 兼容性(services::portability::preview); 与前端 api 层的契约为
/// import_preview{path}->ImportPreview(见 progress.md M3 契约记录)。纯文件解析, 不需要数据库
/// 连接或 data_dir(parse_bundle/preview 均不接触 AppState), state 参数只为与本模块/仓库其余命令
/// 的统一签名风格保持一致(见本文件 export_bundle 等既有命令均以 state 打头), 留作后续任务(如
/// 需要在预览阶段核对本机已装同名资源)扩展点, 当前未使用
#[tauri::command]
pub fn import_preview(_state: State<'_, AppState>, path: String) -> Result<ImportPreview, String> {
	let parsed = portability::parse_bundle(Path::new(&path)).map_err(|e| e.to_string())?;
	Ok(portability::preview(&parsed))
}
