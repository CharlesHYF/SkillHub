// 文件作用: 本地库(Skill/MCP 资源)相关 Tauri 命令 —— 列表/详情/统计/本地导入/启停/删除/从已
//           检测 Agent 导入; 命令层只负责加锁取出 conn/data_dir 与转换错误类型, 具体逻辑见
//           services::library(本地路径导入)与 services::agent_import(M6 Task BE-2, 从已检测
//           Agent 反向导入)
// 创建日期: 2026-07-09

use tauri::State;

use crate::domain::resource::{Resource, ResourceType};
use crate::services::agent_import::{self, ImportFromAgentsOutcome};
use crate::services::library::{self, LibraryCounts};
use crate::AppState;

use super::home_dir;

/// 按类型/关键字查询本地库资源列表; res_type 为 None 表示不按类型过滤(前端传原始整数编码,
/// 由本命令转换为 ResourceType), keyword 为 None 表示不按关键字过滤
#[tauri::command]
pub fn library_list(
	state: State<'_, AppState>,
	res_type: Option<i64>,
	keyword: Option<String>,
) -> Result<Vec<Resource>, String> {
	let conn = state.db();
	library::list(&conn, res_type.map(ResourceType::from_i64), keyword).map_err(|e| e.to_string())
}

/// 按主键查询单条资源, 不存在返回 None
#[tauri::command]
pub fn library_get(state: State<'_, AppState>, id: i64) -> Result<Option<Resource>, String> {
	let conn = state.db();
	library::get(&conn, id).map_err(|e| e.to_string())
}

/// 统计本地库 Skill/MCP 各自数量, 供首页/侧栏角标展示
#[tauri::command]
pub fn library_counts(state: State<'_, AppState>) -> Result<LibraryCounts, String> {
	let conn = state.db();
	library::counts(&conn).map_err(|e| e.to_string())
}

/// 把本地路径(MCP 单定义 json 文件或含 SKILL.md 的 Skill 目录)导入为一条资源: 内容拷入
/// SkillHub 存储目录并落库, 详见 services::library::import_local
#[tauri::command]
pub fn resource_import_local(state: State<'_, AppState>, path: String) -> Result<Resource, String> {
	let conn = state.db();
	library::import_local(&conn, &state.data_dir, &path).map_err(|e| e.to_string())
}

/// 设置资源启用/禁用状态
#[tauri::command]
pub fn resource_set_enabled(
	state: State<'_, AppState>,
	id: i64,
	enabled: bool,
) -> Result<(), String> {
	let conn = state.db();
	library::set_enabled(&conn, id, enabled).map_err(|e| e.to_string())
}

/// 删除一条资源: 删库记录 + 清理其在 SkillHub 存储目录下的内容 + 记一条卸载活动
#[tauri::command]
pub fn resource_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
	let conn = state.db();
	library::delete(&conn, &state.data_dir, id).map_err(|e| e.to_string())
}

/// 从已探测到的全部 Agent(agent 表全量)反向导入其里已经装着的 Skill/MCP 到本地库: 按
/// (类型,名称)去重落地 + 与拥有它的 Agent 建立关联, 详见 services::agent_import::
/// import_from_agents(M6 Task BE-2)
#[tauri::command]
pub fn library_import_from_agents(
	state: State<'_, AppState>,
) -> Result<ImportFromAgentsOutcome, String> {
	let conn = state.db();
	let home = home_dir()?;
	agent_import::import_from_agents(&conn, &home, &state.data_dir).map_err(|e| e.to_string())
}
