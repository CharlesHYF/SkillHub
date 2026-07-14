// 文件作用: 导入导出相关 Tauri 命令(导出 + 导入预览 + 导入应用) —— export_bundle/import_preview/
//           import_bundle 只负责加锁取出 conn/data_dir(如需要)与错误类型转换(anyhow::Error ->
//           String, 与仓库其它命令的既有惯例一致), 具体逻辑见 services::portability。均为纯
//           同步命令(无网络 I/O): apply_for_agent(import_bundle 的 auto_sync 分支所依赖)全程
//           无 await, 不涉及 commands::market 那种因网络 I/O 产生的 !Send 拆分顾虑, 全程一次
//           加锁即可
// 创建日期: 2026-07-10

use std::path::Path;

use tauri::State;

use crate::domain::portability::{
	ConflictStrategy, ExportReqVO, ImportOutcomeRespVO, ImportPreviewRespVO, ManifestRespVO,
};
use crate::infra::repo_impexp::{self, ImpexpRespVO};
use crate::services::portability;
use crate::AppState;

use super::home_dir;

/// 导出打包: 按 options 收集资源/配置/关联并打包到 out_path(前端经保存对话框选定的绝对路径,
/// M3 前端可先用固定/输入路径占位, 真实文件对话框留待接入 tauri-plugin-dialog 或等价方案),
/// 返回打包清单(ManifestRespVO); 与前端 api 层的契约为 export_bundle{options,outPath}->ManifestRespVO
/// (Tauri 自动把 out_path 转成 outPath, 见 progress.md M3 契约记录)
#[tauri::command]
pub fn export_bundle(
	state: State<'_, AppState>,
	options: ExportReqVO,
	out_path: String,
) -> Result<ManifestRespVO, String> {
	let conn = state.db();
	portability::export_bundle(&conn, &state.data_dir, &options, Path::new(&out_path))
		.map_err(|e| e.to_string())
}

/// 导入预览: 解析并校验 path 指向的导入包(见 services::portability::parse_bundle —— 含 schema
/// 版本/条目路径 zip-slip 穿越/条目集合与 manifest 记录一致/大小上限/逐条目 sha256 五类校验,
/// 任一不过即整体失败, 不做任何磁盘落地), 校验通过后换算为前端"将导入内容"面板所需的计数 +
/// schema 兼容性(services::portability::preview); 与前端 api 层的契约为
/// import_preview{path}->ImportPreviewRespVO(见 progress.md M3 契约记录)。纯文件解析, 不需要数据库
/// 连接或 data_dir(parse_bundle/preview 均不接触 AppState), state 参数只为与本模块/仓库其余命令
/// 的统一签名风格保持一致(见本文件 export_bundle 等既有命令均以 state 打头), 留作后续任务(如
/// 需要在预览阶段核对本机已装同名资源)扩展点, 当前未使用
#[tauri::command]
pub fn import_preview(
	_state: State<'_, AppState>,
	path: String,
) -> Result<ImportPreviewRespVO, String> {
	let parsed = portability::parse_bundle(Path::new(&path)).map_err(|e| e.to_string())?;
	Ok(portability::preview(&parsed))
}

/// 导入应用: 解析并校验 path 指向的导入包(见 services::portability::parse_bundle), 按
/// strategy(0-覆盖/1-跳过/2-保留两者, 见 domain::portability::ConflictStrategy)落地到本地库
/// 与 data_dir(见 services::portability::import_bundle); auto_sync=true 时额外对本机全部
/// 在线 Agent 触发一次同步应用(见 services::portability::sync_online_agents)。与前端 api 层
/// 的契约为 import_bundle{path,strategy,autoSync}->ImportOutcomeRespVO(见 progress.md M3 契约
/// 记录)。同步是导入完成后锦上添花的附加动作, 其结果不影响本命令的返回值 —— 即便 auto_sync
/// 过程中有 Agent 应用失败, 只要导入本身成功就照常返回 ImportOutcomeRespVO, 不因同步的问题回退已经
/// 落地的导入结果(sync_online_agents 内部也已对单 Agent 失败做了静默跳过, 见其文档)
#[tauri::command]
pub fn import_bundle(
	state: State<'_, AppState>,
	path: String,
	strategy: i64,
	auto_sync: bool,
) -> Result<ImportOutcomeRespVO, String> {
	let parsed = portability::parse_bundle(Path::new(&path)).map_err(|e| e.to_string())?;

	let conn = state.db();
	let outcome = portability::import_bundle(
		&conn,
		&state.data_dir,
		parsed,
		ConflictStrategy::from_i64(strategy),
	)
	.map_err(|e| e.to_string())?;

	if auto_sync {
		let home = home_dir()?;
		let _ = portability::sync_online_agents(&conn, &home);
	}

	Ok(outcome)
}

/// 导入导出历史: 返回最近 limit 条导入/导出记录(按时间倒序, 见 infra::repo_impexp::recent),
/// 供前端"导入导出历史"表渲染。与前端 api 层的契约为 impexp_history{limit}->ImpexpRespVO[]
#[tauri::command]
pub fn impexp_history(state: State<'_, AppState>, limit: i64) -> Result<Vec<ImpexpRespVO>, String> {
	let conn = state.db();
	repo_impexp::recent(&conn, limit).map_err(|e| e.to_string())
}
