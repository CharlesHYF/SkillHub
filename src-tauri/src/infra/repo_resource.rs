// 文件作用: resource 表仓储 —— insert/list/get/update_meta/set_enabled/delete/count_by_type,
//           显式列名/禁 SELECT */全参数化查询(阿里巴巴泰山版数据库规约)
// 创建日期: 2026-07-09

use rusqlite::{named_params, params, Connection, OptionalExtension, Row};

use crate::domain::resource::{ResourceRespVO, ResourceType, SourceType};

/// 新建资源入参: id/create_time/update_time 由数据库生成, 不在此结构体中
#[derive(Debug, Clone)]
pub struct NewResource {
	pub res_type: ResourceType,
	pub name: String,
	pub display_name: String,
	pub version: String,
	pub source_type: SourceType,
	pub local_path: String,
	pub enabled: bool,
}

/// 列表过滤条件: 字段均可选, None 表示不过滤该维度
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
	pub res_type: Option<ResourceType>,
	pub keyword: Option<String>,
}

/// 可更新的描述性元信息: 不含 res_type/name(构成唯一键)与 enabled(有专用 set_enabled)
#[derive(Debug, Clone)]
pub struct ResourceMetaUpdate {
	pub display_name: String,
	pub version: String,
	pub local_path: String,
}

/// 将一行查询结果映射为 ResourceRespVO 实体
fn row_to_resource(row: &Row) -> rusqlite::Result<ResourceRespVO> {
	Ok(ResourceRespVO {
		id: row.get(0)?,
		res_type: ResourceType::from_i64(row.get(1)?),
		name: row.get(2)?,
		display_name: row.get(3)?,
		version: row.get(4)?,
		source_type: SourceType::from_i64(row.get(5)?),
		local_path: row.get(6)?,
		enabled: row.get(7)?,
		create_time: row.get(8)?,
		update_time: row.get(9)?,
	})
}

/// 新增一条资源, 返回自增主键 id; create_time/update_time 交给列默认值 datetime('now')
pub fn insert(conn: &Connection, item: &NewResource) -> rusqlite::Result<i64> {
	conn.execute(
		"INSERT INTO resource \
		 (res_type, name, display_name, version, source_type, local_path, enabled) \
		 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
		params![
			i64::from(item.res_type),
			item.name,
			item.display_name,
			item.version,
			i64::from(item.source_type),
			item.local_path,
			item.enabled,
		],
	)?;
	Ok(conn.last_insert_rowid())
}

/// 按过滤条件查询资源列表, 按 id 升序; res_type/keyword 均为可选过滤, SQL 文本固定不拼接
pub fn list(conn: &Connection, filter: &ListFilter) -> rusqlite::Result<Vec<ResourceRespVO>> {
	let res_type_param: Option<i64> = filter.res_type.map(i64::from);
	let keyword_param: Option<String> = filter.keyword.as_ref().map(|k| format!("%{k}%"));
	let mut stmt = conn.prepare(
		"SELECT id, res_type, name, display_name, version, source_type, local_path, enabled, \
		 create_time, update_time \
		 FROM resource \
		 WHERE (:res_type IS NULL OR res_type = :res_type) \
		 AND (:keyword IS NULL OR name LIKE :keyword OR display_name LIKE :keyword) \
		 ORDER BY id",
	)?;
	let rows = stmt.query_map(
		named_params! { ":res_type": res_type_param, ":keyword": keyword_param },
		row_to_resource,
	)?;
	rows.collect()
}

/// 按主键查询单条资源, 不存在返回 None(而非 Err)
pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<ResourceRespVO>> {
	conn.query_row(
		"SELECT id, res_type, name, display_name, version, source_type, local_path, enabled, \
		 create_time, update_time \
		 FROM resource WHERE id = ?1",
		params![id],
		row_to_resource,
	)
	.optional()
}

/// 按 (res_type, name) 精确查询单条资源(与 resource 表的 uk_resource_type_name 唯一索引同一
/// 维度), 不存在返回 None(而非 Err); name 为精确匹配(非 list 的 keyword 模糊匹配), 供 M6 Task
/// BE-2(services::agent_import, 从已检测 Agent 反向导入已装 Skill/MCP 到本地库)按类型+名称去重
/// 时复用 —— 已有同类型同名资源则复用其 id, 不重复落地/不重复插入
pub fn find_by_type_and_name(
	conn: &Connection,
	res_type: ResourceType,
	name: &str,
) -> rusqlite::Result<Option<ResourceRespVO>> {
	conn.query_row(
		"SELECT id, res_type, name, display_name, version, source_type, local_path, enabled, \
		 create_time, update_time \
		 FROM resource WHERE res_type = ?1 AND name = ?2",
		params![i64::from(res_type), name],
		row_to_resource,
	)
	.optional()
}

/// 覆盖更新描述性元信息(display_name/version/local_path), 返回受影响行数
pub fn update_meta(
	conn: &Connection,
	id: i64,
	meta: &ResourceMetaUpdate,
) -> rusqlite::Result<usize> {
	conn.execute(
		"UPDATE resource \
		 SET display_name = ?1, version = ?2, local_path = ?3, update_time = datetime('now') \
		 WHERE id = ?4",
		params![meta.display_name, meta.version, meta.local_path, id],
	)
}

/// 设置启用/禁用状态, 返回受影响行数
pub fn set_enabled(conn: &Connection, id: i64, enabled: bool) -> rusqlite::Result<usize> {
	conn.execute(
		"UPDATE resource SET enabled = ?1, update_time = datetime('now') WHERE id = ?2",
		params![enabled, id],
	)
}

/// 按主键删除一条资源, 返回受影响行数
pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
	conn.execute("DELETE FROM resource WHERE id = ?1", params![id])
}

/// 按类型统计数量, 返回 (skill 数量, mcp 数量); 用 COUNT(id) 而非 COUNT(*) 保持列名显式
pub fn count_by_type(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
	let skill: i64 = conn.query_row(
		"SELECT COUNT(id) FROM resource WHERE res_type = ?1",
		params![i64::from(ResourceType::Skill)],
		|row| row.get(0),
	)?;
	let mcp: i64 = conn.query_row(
		"SELECT COUNT(id) FROM resource WHERE res_type = ?1",
		params![i64::from(ResourceType::Mcp)],
		|row| row.get(0),
	)?;
	Ok((skill, mcp))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 建一个已迁移好 10 张表结构的内存库, 供仓储测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	fn sample_new_resource() -> NewResource {
		NewResource {
			res_type: ResourceType::Skill,
			name: "demo-skill".to_string(),
			display_name: "Demo Skill".to_string(),
			version: "1.0.0".to_string(),
			source_type: SourceType::LocalImport,
			local_path: "/tmp/demo-skill".to_string(),
			enabled: true,
		}
	}

	// insert -> get 应还原全部字段(枚举列/布尔列/时间戳列均正确映射)
	#[test]
	fn insert_then_get_round_trips_all_fields() {
		let conn = setup_conn();
		let id = insert(&conn, &sample_new_resource()).unwrap();
		let got = get(&conn, id).unwrap().expect("刚插入的资源应能查到");
		assert_eq!(got.id, id);
		assert_eq!(got.res_type, ResourceType::Skill);
		assert_eq!(got.name, "demo-skill");
		assert_eq!(got.display_name, "Demo Skill");
		assert_eq!(got.version, "1.0.0");
		assert_eq!(got.source_type, SourceType::LocalImport);
		assert_eq!(got.local_path, "/tmp/demo-skill");
		assert!(got.enabled);
		assert!(!got.create_time.is_empty());
		assert!(!got.update_time.is_empty());
	}

	// get 查不存在的 id 应返回 None, 不是 Err
	#[test]
	fn get_missing_id_returns_none() {
		let conn = setup_conn();
		assert_eq!(get(&conn, 9999).unwrap(), None);
	}

	// list 应支持按 res_type 与 keyword(name/display_name 模糊匹配)分别过滤及组合过滤
	#[test]
	fn list_filters_by_res_type_and_keyword() {
		let conn = setup_conn();
		insert(&conn, &sample_new_resource()).unwrap();
		let mut mcp = sample_new_resource();
		mcp.res_type = ResourceType::Mcp;
		mcp.name = "demo-mcp".to_string();
		mcp.display_name = "Demo MCP".to_string();
		insert(&conn, &mcp).unwrap();

		let all = list(&conn, &ListFilter::default()).unwrap();
		assert_eq!(all.len(), 2);

		let only_skill = list(
			&conn,
			&ListFilter {
				res_type: Some(ResourceType::Skill),
				keyword: None,
			},
		)
		.unwrap();
		assert_eq!(only_skill.len(), 1);
		assert_eq!(only_skill[0].name, "demo-skill");

		let by_keyword = list(
			&conn,
			&ListFilter {
				res_type: None,
				keyword: Some("mcp".to_string()),
			},
		)
		.unwrap();
		assert_eq!(by_keyword.len(), 1);
		assert_eq!(by_keyword[0].name, "demo-mcp");
	}

	// set_enabled 应精确更新 enabled 列, 不影响其它字段
	#[test]
	fn set_enabled_updates_flag() {
		let conn = setup_conn();
		let id = insert(&conn, &sample_new_resource()).unwrap();
		let affected = set_enabled(&conn, id, false).unwrap();
		assert_eq!(affected, 1);
		let got = get(&conn, id).unwrap().unwrap();
		assert!(!got.enabled);
	}

	// update_meta 应整份覆盖 display_name/version/local_path, 不影响 res_type/name/enabled
	#[test]
	fn update_meta_overwrites_descriptive_fields_only() {
		let conn = setup_conn();
		let id = insert(&conn, &sample_new_resource()).unwrap();
		let affected = update_meta(
			&conn,
			id,
			&ResourceMetaUpdate {
				display_name: "Demo Skill v2".to_string(),
				version: "2.0.0".to_string(),
				local_path: "/tmp/demo-skill-v2".to_string(),
			},
		)
		.unwrap();
		assert_eq!(affected, 1);
		let got = get(&conn, id).unwrap().unwrap();
		assert_eq!(got.display_name, "Demo Skill v2");
		assert_eq!(got.version, "2.0.0");
		assert_eq!(got.local_path, "/tmp/demo-skill-v2");
		assert_eq!(got.res_type, ResourceType::Skill);
		assert_eq!(got.name, "demo-skill");
		assert!(got.enabled);
	}

	// count_by_type 应分别统计 skill 与 mcp 数量
	#[test]
	fn count_by_type_counts_skill_and_mcp_separately() {
		let conn = setup_conn();
		insert(&conn, &sample_new_resource()).unwrap();
		let mut mcp = sample_new_resource();
		mcp.res_type = ResourceType::Mcp;
		mcp.name = "demo-mcp".to_string();
		insert(&conn, &mcp).unwrap();

		let (skill_count, mcp_count) = count_by_type(&conn).unwrap();
		assert_eq!(skill_count, 1);
		assert_eq!(mcp_count, 1);
	}

	// delete 应移除该行, 之后 get 应返回 None
	#[test]
	fn delete_removes_row() {
		let conn = setup_conn();
		let id = insert(&conn, &sample_new_resource()).unwrap();
		let affected = delete(&conn, id).unwrap();
		assert_eq!(affected, 1);
		assert_eq!(get(&conn, id).unwrap(), None);
	}

	// find_by_type_and_name: 命中时应返回完整资源; 类型相符但名称不同、名称相符但类型不同,
	// 均不应误命中(精确匹配 (res_type,name) 这一对, 与 uk_resource_type_name 同一维度)
	#[test]
	fn find_by_type_and_name_matches_exact_type_and_name_only() {
		let conn = setup_conn();
		let id = insert(&conn, &sample_new_resource()).unwrap();

		let found = find_by_type_and_name(&conn, ResourceType::Skill, "demo-skill").unwrap();
		assert_eq!(found.map(|r| r.id), Some(id));

		assert_eq!(
			find_by_type_and_name(&conn, ResourceType::Skill, "other-name").unwrap(),
			None,
			"名称不同不应命中"
		);
		assert_eq!(
			find_by_type_and_name(&conn, ResourceType::Mcp, "demo-skill").unwrap(),
			None,
			"类型不同(同名)不应命中"
		);
	}

	// find_by_type_and_name: 查不存在的 (res_type,name) 应返回 None, 不报错
	#[test]
	fn find_by_type_and_name_returns_none_when_absent() {
		let conn = setup_conn();
		assert_eq!(
			find_by_type_and_name(&conn, ResourceType::Skill, "nope").unwrap(),
			None
		);
	}
}
