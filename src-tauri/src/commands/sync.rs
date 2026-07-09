// 文件作用: 同步相关 Tauri 命令 —— 关联期望态设置、单 Agent 差异计算、批量应用(带进度事件)。
//           assoc_set 是对 repo_assoc::set 的直接透传, sync_diff 转调 services::sync::
//           diff_for_agent; sync_apply 逐 Agent 调 services::sync::apply_for_agent 并在每个
//           Agent 处理前后向前端推送一次进度事件, 这部分"多 Agent 编排 + Tauri 事件"逻辑刻意
//           留在命令层, 不下沉进 services::sync(该模块按其文件头注释约定不摸 AppHandle/Tauri
//           运行时, 只认 &Connection 与 &Path, 便于单测)
// 创建日期: 2026-07-09

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::domain::sync::DiffPlan;
use crate::infra::repo_agent;
use crate::infra::repo_assoc::{self, ResourceAgentLink};
use crate::services::sync::{self, SyncSummary};
use crate::AppState;

use super::home_dir;

/// 同步进度事件负载, 经 "sync://progress" 频道推送给前端; current_name 为当前正在处理的
/// Agent 展示名, 供前端进度提示(如"正在同步 Claude Code (2/5)")
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
	agent_id: i64,
	done: i64,
	total: i64,
	current_name: String,
}

/// 设置某资源相对某 Agent 的期望态(desired: 应存在/不应存在), 对 repo_assoc::set 的直接透传
/// (无额外业务逻辑)
#[tauri::command]
pub fn assoc_set(
	state: State<'_, AppState>,
	resource_id: i64,
	agent_id: i64,
	desired: bool,
) -> Result<(), String> {
	let conn = state.db();
	repo_assoc::set(&conn, resource_id, agent_id, desired).map_err(|e| e.to_string())?;
	Ok(())
}

/// 一次性查询全部资源的关联 Agent 展示行(仅期望态 desired=1, 附 Agent 展示名), 供"已安装"
/// 界面统计"已关联 Agent 数"列与详情面板"已关联 Agent"列表复用同一份数据, 避免逐资源 N+1
/// 查询(见 repo_assoc::list_all_links); 是对仓储的直接透传, 无额外业务逻辑, 故不下沉 services 层
#[tauri::command]
pub fn resource_agent_links(state: State<'_, AppState>) -> Result<Vec<ResourceAgentLink>, String> {
	let conn = state.db();
	repo_assoc::list_all_links(&conn).map_err(|e| e.to_string())
}

/// 计算某 Agent 的期望态与其配置文件实际态之间的差异计划(见 services::sync::diff_for_agent)
#[tauri::command]
pub fn sync_diff(state: State<'_, AppState>, agent_id: i64) -> Result<DiffPlan, String> {
	let conn = state.db();
	let home = home_dir()?;
	sync::diff_for_agent(&conn, &home, agent_id).map_err(|e| e.to_string())
}

/// 对给定 Agent 列表逐一应用同步(见 services::sync::apply_for_agent), 每处理完一个 Agent
/// (处理前后各一次)向前端 "sync://progress" 频道推送一次进度, 最终返回各 Agent 结果相加后的
/// 总 SyncSummary。逐 Agent 顺序处理(非并发): 都共享同一个 state.db 连接, 顺序处理也更符合
/// "进度条"这个使用场景的直觉; 若某个 Agent 应用过程本身出错(如 agent_id 已失效这类结构性
/// 错误, 区别于"该 Agent 内某几项同步失败"——那种情况已被计入 SyncSummary.failed, 不会
/// 走到这里报错), 整批立即中止并把错误信息返回给前端, 不再继续处理剩余 Agent
#[tauri::command]
pub fn sync_apply(
	app: AppHandle,
	state: State<'_, AppState>,
	agent_ids: Vec<i64>,
) -> Result<SyncSummary, String> {
	let home = home_dir()?;
	let total = agent_ids.len() as i64;
	let mut summary = SyncSummary {
		success: 0,
		failed: 0,
		skipped: 0,
	};

	for (idx, agent_id) in agent_ids.into_iter().enumerate() {
		let done = idx as i64;
		let name = agent_display_name(&state, agent_id);
		emit_progress(&app, agent_id, done, total, &name);

		let outcome = {
			let conn = state.db();
			sync::apply_for_agent(&conn, &home, agent_id).map_err(|e| e.to_string())?
		};
		summary.success += outcome.success;
		summary.failed += outcome.failed;
		summary.skipped += outcome.skipped;

		emit_progress(&app, agent_id, done + 1, total, &name);
	}

	Ok(summary)
}

/// 查该 Agent 的展示名, 供进度事件的 current_name 字段; 查不到(极端情况下 id 已失效)兜底为
/// 空串, 不因此中断整次同步(名字只是给前端展示用, 不影响同步本身)
fn agent_display_name(state: &State<'_, AppState>, agent_id: i64) -> String {
	let conn = state.db();
	repo_agent::get(&conn, agent_id)
		.ok()
		.flatten()
		.map(|row| row.name)
		.unwrap_or_default()
}

/// 向前端 "sync://progress" 频道推送一次同步进度; 推送失败(极端情况, 如窗口已关闭)只忽略,
/// 不应因为事件推送失败而中断同步本身
fn emit_progress(app: &AppHandle, agent_id: i64, done: i64, total: i64, current_name: &str) {
	let _ = app.emit(
		"sync://progress",
		ProgressEvent {
			agent_id,
			done,
			total,
			current_name: current_name.to_string(),
		},
	);
}
