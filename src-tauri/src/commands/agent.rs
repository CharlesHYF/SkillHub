// 文件作用: Agent(本机 AI 工具实例)相关 Tauri 命令 —— 探测并落库、查询全量列表;
//           探测转调 services::sync::detect_all, 列表是对 repo_agent::list 的直接透传
//           (无额外业务逻辑, 命令层不再绕经 services 一层)
// 创建日期: 2026-07-09

use tauri::State;

use crate::infra::repo_agent::{self, AgentRespVO};
use crate::services::sync;
use crate::AppState;

use super::home_dir;

/// 探测本机全部已知 AI 工具实例并落库, 返回 Agent 表当前全量(见 services::sync::detect_all)
#[tauri::command]
pub fn agent_detect(state: State<'_, AppState>) -> Result<Vec<AgentRespVO>, String> {
	let conn = state.db();
	let home = home_dir()?;
	sync::detect_all(&conn, &home).map_err(|e| e.to_string())
}

/// 查询 Agent 表当前全量, 不触发探测(纯读取, 供列表页刷新使用)
#[tauri::command]
pub fn agent_list(state: State<'_, AppState>) -> Result<Vec<AgentRespVO>, String> {
	let conn = state.db();
	repo_agent::list(&conn).map_err(|e| e.to_string())
}
