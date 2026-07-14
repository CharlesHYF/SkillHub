// 文件作用: 首页汇总服务编排层 —— 统计卡片数据组装与最近活动查询, 均为薄封装(直接转调各
//           repo_*), 命令层(commands::dashboard, Task 8)加锁取出 conn 后转调本模块, 呼应
//           services::sync/services::library 既有的分层约定
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::infra::repo_activity::{self, ActivityRespVO};
use crate::infra::repo_agent;
use crate::infra::repo_assoc;
use crate::infra::repo_resource;

/// 首页统计卡片数据: Skill/MCP 数量、Agent 总数与在线数、待同步数
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummaryRespVO {
	pub skill_count: i64,
	pub mcp_count: i64,
	pub agent_count: i64,
	pub online_count: i64,
	pub pending_count: i64,
}

/// 组装首页统计卡片数据。pending_count 取 repo_assoc::count_pending 的口径(resource_agent 里
/// sync_status=0/待同步的行数)作为一个简单可算的参考值, 不逐 Agent 现读配置文件跑一遍
/// reconcile(那需要 home 家目录与逐 Agent 文件 IO, 首页汇总只要一个量级参考, 精确差异请去
/// 同步中心走 sync_diff 逐 Agent 现算); agent_count/online_count 由 agent 表全量行数与其中
/// status=true(在线/可用)的行数直接算出
pub fn summary(conn: &Connection) -> Result<DashboardSummaryRespVO> {
	let (skill_count, mcp_count) = repo_resource::count_by_type(conn)?;
	let agents = repo_agent::list(conn)?;
	let agent_count = agents.len() as i64;
	let online_count = agents.iter().filter(|row| row.status).count() as i64;
	let pending_count = repo_assoc::count_pending(conn)?;

	Ok(DashboardSummaryRespVO {
		skill_count,
		mcp_count,
		agent_count,
		online_count,
		pending_count,
	})
}

/// 查询最近若干条活动记录(薄封装 repo_activity::recent), 供首页"最近变更"列表
pub fn recent_activity(conn: &Connection, limit: i64) -> Result<Vec<ActivityRespVO>> {
	Ok(repo_activity::recent(conn, limit)?)
}

#[cfg(test)]
mod tests {
	use crate::domain::agent::{AgentKind, AgentScope, DetectedAgent};
	use crate::domain::resource::{ResourceType, SourceType};
	use crate::infra::repo_resource::NewResource;

	use super::*;

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	fn seed_resource(conn: &Connection, res_type: ResourceType, name: &str) -> i64 {
		repo_resource::insert(
			conn,
			&NewResource {
				res_type,
				name: name.to_string(),
				display_name: name.to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: "/tmp/unused".to_string(),
				enabled: true,
			},
		)
		.unwrap()
	}

	fn seed_agent(conn: &Connection, config_path: &str, online: bool) -> i64 {
		repo_agent::upsert(
			conn,
			&DetectedAgent {
				kind: AgentKind::ClaudeCode,
				name: "Claude Code".to_string(),
				config_path: config_path.to_string(),
				scope: AgentScope::Global,
				online,
			},
		)
		.unwrap()
	}

	// summary: 应正确组装 skill/mcp 数量、agent 总数与在线数、待同步数五项
	#[test]
	fn summary_assembles_counts_from_repos() {
		let conn = setup_conn();
		let skill_id = seed_resource(&conn, ResourceType::Skill, "demo-skill");
		let mcp_id = seed_resource(&conn, ResourceType::Mcp, "demo-mcp");

		let online_agent = seed_agent(&conn, "/home/demo/.claude.json", true);
		let _offline_agent = seed_agent(&conn, "/home/demo/.cursor/mcp.json", false);

		// online_agent 关联两个资源: 一个待同步(默认 sync_status=0), 一个已同步(sync_status=1)
		repo_assoc::set(&conn, skill_id, online_agent, true).unwrap();
		repo_assoc::set(&conn, mcp_id, online_agent, true).unwrap();
		repo_assoc::set_sync_status(&conn, mcp_id, online_agent, 1).unwrap();

		let got = summary(&conn).unwrap();

		assert_eq!(
			got,
			DashboardSummaryRespVO {
				skill_count: 1,
				mcp_count: 1,
				agent_count: 2,
				online_count: 1,
				pending_count: 1,
			}
		);
	}

	// summary: 全空数据库应五项均为 0, 不报错
	#[test]
	fn summary_reports_all_zero_when_database_empty() {
		let conn = setup_conn();
		let got = summary(&conn).unwrap();
		assert_eq!(
			got,
			DashboardSummaryRespVO {
				skill_count: 0,
				mcp_count: 0,
				agent_count: 0,
				online_count: 0,
				pending_count: 0,
			}
		);
	}

	// recent_activity: 应按插入倒序返回并遵守 limit(薄封装转发, 验证不出岔子即可)
	#[test]
	fn recent_activity_forwards_to_repo() {
		let conn = setup_conn();
		repo_activity::add(&conn, 1, 1, "第一条", "").unwrap();
		repo_activity::add(&conn, 6, 4, "第二条", "").unwrap();

		let rows = recent_activity(&conn, 1).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].title, "第二条");
	}
}
