// 文件作用: 首页汇总相关 Tauri 命令 —— 统计卡片数据与最近活动列表, 均转调 services::dashboard
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use tauri::State;

use crate::infra::repo_activity::ActivityRespVO;
use crate::services::dashboard::{self, DashboardSummaryRespVO};
use crate::AppState;

/// 查询首页统计卡片数据(Skill/MCP 数量、Agent 总数与在线数、待同步数)
#[tauri::command]
pub fn dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummaryRespVO, String> {
	let conn = state.db();
	dashboard::summary(&conn).map_err(|e| e.to_string())
}

/// 查询最近若干条活动记录, 供首页"最近变更"列表
#[tauri::command]
pub fn activity_recent(
	state: State<'_, AppState>,
	limit: i64,
) -> Result<Vec<ActivityRespVO>, String> {
	let conn = state.db();
	dashboard::recent_activity(&conn, limit).map_err(|e| e.to_string())
}
