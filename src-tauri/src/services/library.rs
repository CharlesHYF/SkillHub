// 文件作用: 本地库(Skill/MCP 资源)服务编排层 —— 列表/详情/统计查询的薄封装(直接转调
//           repo_resource), 以及两个真正带业务逻辑的操作: import_local(把用户选择的本地路径
//           登记为资源, 并把内容拷入 SkillHub 存储目录 data_dir)与 delete(删库记录 + 清理其在
//           data_dir 下的内容)。均只接受 &Connection 与 &Path(data_dir), 不摸 AppState/Tauri
//           运行时, 便于单测注入内存库/临时目录; 命令层(commands::library, Task 8)负责加锁
//           取出 conn/data_dir 后转调本模块, 呼应 services::sync 既有的分层约定。
// 创建日期: 2026-07-09

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde::Serialize;

use crate::domain::resource::{Resource, ResourceType, SourceType};
use crate::infra::adapter::skill_target::{copy_dir_recursive, parse_frontmatter_version};
use crate::infra::repo_activity;
use crate::infra::repo_resource::{self, ListFilter, NewResource};

/// 本地库 Skill/MCP 各自数量统计, 供首页/侧栏角标展示
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCounts {
	pub skill: i64,
	pub mcp: i64,
}

/// 按过滤条件查询资源列表(薄封装 repo_resource::list, 无额外业务逻辑)
pub fn list(
	conn: &Connection,
	res_type: Option<ResourceType>,
	keyword: Option<String>,
) -> Result<Vec<Resource>> {
	Ok(repo_resource::list(
		conn,
		&ListFilter { res_type, keyword },
	)?)
}

/// 按主键查询单条资源(薄封装 repo_resource::get)
pub fn get(conn: &Connection, id: i64) -> Result<Option<Resource>> {
	Ok(repo_resource::get(conn, id)?)
}

/// 统计 Skill/MCP 各自数量(薄封装 repo_resource::count_by_type)
pub fn counts(conn: &Connection) -> Result<LibraryCounts> {
	let (skill, mcp) = repo_resource::count_by_type(conn)?;
	Ok(LibraryCounts { skill, mcp })
}

/// 把本地路径登记为一条资源, 并把内容纳入 SkillHub 存储目录(data_dir), 按 `path` 是文件还是
/// 目录分派到 MCP/Skill 两条导入分支(见 import_mcp/import_skill); 既不是文件也不是目录(路径
/// 不存在)时返回 Err。两种分支落库后的 source_type 均为 LocalImport, 并各追加一条"新增"活动
/// 记录(act_type=1, 见 insert_and_record), 最终返回落库后的完整 Resource
/// (含数据库生成的 id/时间戳)
pub fn import_local(conn: &Connection, data_dir: &Path, path: &str) -> Result<Resource> {
	let src = Path::new(path);
	if src.is_dir() {
		import_skill(conn, data_dir, src)
	} else if src.is_file() {
		import_mcp(conn, data_dir, src)
	} else {
		Err(anyhow!("本地路径不存在: {path}"))
	}
}

/// MCP 导入分支: `src` 指向单个服务定义 json 文件, 原样拷到 `data_dir/mcp/<name>.json`,
/// name 取文件名去扩展名(如 `demo-mcp.json` -> `demo-mcp`); MCP 定义本身不带版本概念
/// (呼应 domain::agent::McpServerDef 不含 version 字段), version 落空串
fn import_mcp(conn: &Connection, data_dir: &Path, src: &Path) -> Result<Resource> {
	let name = file_stem_name(src)?;

	let mcp_dir = data_dir.join("mcp");
	fs::create_dir_all(&mcp_dir).with_context(|| format!("创建目录失败: {}", mcp_dir.display()))?;
	let target = mcp_dir.join(format!("{name}.json"));
	fs::copy(src, &target).with_context(|| {
		format!(
			"拷贝 MCP 定义失败: {} -> {}",
			src.display(),
			target.display()
		)
	})?;

	insert_and_record(conn, ResourceType::Mcp, &name, String::new(), &target)
}

/// Skill 导入分支: `src` 指向含 `SKILL.md` 的目录, 整树拷到 `data_dir/skills/<name>/`,
/// name 取目录名, version 从 `SKILL.md` 的 YAML frontmatter 解析(解析不到给空串, 与
/// infra::adapter::skill_target::read_claude_skills_dir 对已装 Skill 的读取口径一致);
/// 目标目录若已存在(重复导入同名 Skill)先整体清空再复制, 不做增量合并(与
/// write_claude_skills_dir 的覆盖式更新惯例一致)
fn import_skill(conn: &Connection, data_dir: &Path, src: &Path) -> Result<Resource> {
	let name = src
		.file_name()
		.map(|s| s.to_string_lossy().into_owned())
		.filter(|s| !s.is_empty())
		.ok_or_else(|| anyhow!("无法从路径解析 Skill 名称: {}", src.display()))?;

	let skill_md_path = src.join("SKILL.md");
	let skill_md = fs::read_to_string(&skill_md_path)
		.with_context(|| format!("读取 SKILL.md 失败: {}", skill_md_path.display()))?;
	let version = parse_frontmatter_version(&skill_md);

	let skills_dir = data_dir.join("skills");
	fs::create_dir_all(&skills_dir)
		.with_context(|| format!("创建目录失败: {}", skills_dir.display()))?;
	let target = skills_dir.join(&name);
	if target.exists() {
		fs::remove_dir_all(&target)
			.with_context(|| format!("清理旧 Skill 目录失败: {}", target.display()))?;
	}
	copy_dir_recursive(src, &target).with_context(|| {
		format!(
			"复制 Skill 内容失败: {} -> {}",
			src.display(),
			target.display()
		)
	})?;

	insert_and_record(conn, ResourceType::Skill, &name, version, &target)
}

/// 从文件路径取"去扩展名"的文件名(如 `/tmp/demo-mcp.json` -> `demo-mcp`), 供 import_mcp 取
/// 资源名; 取不到(路径无文件名部分, 或去扩展名后为空串)视为路径不合法, 返回 Err
fn file_stem_name(path: &Path) -> Result<String> {
	path.file_stem()
		.map(|s| s.to_string_lossy().into_owned())
		.filter(|s| !s.is_empty())
		.ok_or_else(|| anyhow!("无法从路径解析资源名称: {}", path.display()))
}

/// 落库(insert)+ 记一条"新增"活动(act_type=1)+ 回查完整实体, 供 import_mcp/import_skill
/// 共用的收尾步骤; source_type 固定为 LocalImport(两条分支都是本地导入), display_name 与
/// name 相同(导入时没有额外的展示名输入, 与 name 保持一致, 后续可通过 update_meta 单独改)
fn insert_and_record(
	conn: &Connection,
	res_type: ResourceType,
	name: &str,
	version: String,
	target: &Path,
) -> Result<Resource> {
	let resource_id = repo_resource::insert(
		conn,
		&NewResource {
			res_type,
			name: name.to_string(),
			display_name: name.to_string(),
			version,
			source_type: SourceType::LocalImport,
			local_path: target.to_string_lossy().into_owned(),
			enabled: true,
		},
	)?;
	repo_activity::add(
		conn,
		1,
		i64::from(res_type),
		&format!("导入 {name}"),
		"本地导入",
	)?;
	repo_resource::get(conn, resource_id)?
		.ok_or_else(|| anyhow!("导入后未能查回资源: id={resource_id}"))
}

/// 设置资源启用/禁用状态(薄封装 repo_resource::set_enabled)
pub fn set_enabled(conn: &Connection, id: i64, enabled: bool) -> Result<()> {
	repo_resource::set_enabled(conn, id, enabled)?;
	Ok(())
}

/// 删除一条资源: 先删库记录, 再尽力清理其在 SkillHub 存储目录(data_dir)下的内容(只清理
/// local_path 位于 data_dir 内的部分, 防误删——尽管 import_local 落地的 local_path 理论上
/// 总在 data_dir 内, 仍加一道防御性判断), 最后记一条"卸载"活动(act_type=7)。
/// 资源本就不存在时视为已达成目的, 直接返回 Ok, 不产生活动记录; 磁盘内容清理失败(如权限问题)
/// 不阻断本次删除——库记录已被移除才是"卸载"对用户可见的主要效果, 残留文件不算致命错误
pub fn delete(conn: &Connection, data_dir: &Path, id: i64) -> Result<()> {
	let Some(resource) = repo_resource::get(conn, id)? else {
		return Ok(());
	};

	repo_resource::delete(conn, id)?;

	let target = Path::new(&resource.local_path);
	if target.starts_with(data_dir) {
		if target.is_dir() {
			let _ = fs::remove_dir_all(target);
		} else if target.is_file() {
			let _ = fs::remove_file(target);
		}
	}

	repo_activity::add(
		conn,
		7,
		i64::from(resource.res_type),
		&format!("卸载 {}", resource.name),
		"",
	)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use tempfile::tempdir;

	use super::*;

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	// import_local(MCP 分支): 传入一个单定义 json 文件, 应落库为 Mcp 资源、内容拷入
	// data_dir/mcp/<name>.json、并记一条"新增"活动
	#[test]
	fn import_local_registers_mcp_definition_file() {
		let workspace = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		let src = workspace.path().join("demo-mcp.json");
		fs::write(&src, r#"{"command":"node","args":["index.js"]}"#).unwrap();

		let resource = import_local(&conn, data_dir.path(), &src.to_string_lossy()).unwrap();

		assert_eq!(resource.res_type, ResourceType::Mcp);
		assert_eq!(resource.name, "demo-mcp");
		assert_eq!(resource.display_name, "demo-mcp");
		assert_eq!(resource.version, "");
		assert_eq!(resource.source_type, SourceType::LocalImport);
		assert!(resource.enabled);

		let target = data_dir.path().join("mcp/demo-mcp.json");
		assert_eq!(resource.local_path, target.to_string_lossy());
		assert_eq!(
			fs::read_to_string(&target).unwrap(),
			r#"{"command":"node","args":["index.js"]}"#
		);

		assert_eq!(
			repo_resource::get(&conn, resource.id).unwrap(),
			Some(resource)
		);
		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 1, "新增");
		assert_eq!(activities[0].res_type, 2, "Mcp");
		assert_eq!(activities[0].title, "导入 demo-mcp");
	}

	// import_local(Skill 分支): 传入一个含 SKILL.md(带 frontmatter version)的目录, 应整树
	// 拷入 data_dir/skills/<name>/、version 从 frontmatter 解析、并记一条"新增"活动
	#[test]
	fn import_local_registers_skill_directory_and_parses_version() {
		let workspace = tempdir().unwrap();
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		let src = workspace.path().join("demo-skill");
		fs::create_dir_all(src.join("scripts")).unwrap();
		fs::write(
			src.join("SKILL.md"),
			"---\nname: demo-skill\nversion: 1.2.0\n---\n\n正文\n",
		)
		.unwrap();
		fs::write(src.join("scripts/run.sh"), "#!/bin/sh\necho hi\n").unwrap();

		let resource = import_local(&conn, data_dir.path(), &src.to_string_lossy()).unwrap();

		assert_eq!(resource.res_type, ResourceType::Skill);
		assert_eq!(resource.name, "demo-skill");
		assert_eq!(resource.version, "1.2.0");
		assert_eq!(resource.source_type, SourceType::LocalImport);

		let target = data_dir.path().join("skills/demo-skill");
		assert_eq!(resource.local_path, target.to_string_lossy());
		assert!(target.join("SKILL.md").exists());
		assert_eq!(
			fs::read_to_string(target.join("scripts/run.sh")).unwrap(),
			"#!/bin/sh\necho hi\n"
		);

		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 1, "新增");
		assert_eq!(activities[0].res_type, 1, "Skill");
	}

	// import_local: 路径既不是文件也不是目录(不存在)时应返回 Err, 不落任何库记录/活动
	#[test]
	fn import_local_fails_when_path_missing() {
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		let result = import_local(&conn, data_dir.path(), "/does/not/exist");

		assert!(result.is_err());
		assert!(repo_resource::list(&conn, &ListFilter::default())
			.unwrap()
			.is_empty());
		assert!(repo_activity::recent(&conn, 10).unwrap().is_empty());
	}

	// counts: 分别插入一条 Skill/MCP 资源后应各计 1
	#[test]
	fn counts_reflects_inserted_resources() {
		let conn = setup_conn();
		repo_resource::insert(
			&conn,
			&NewResource {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				display_name: "Demo Skill".to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: "/tmp/demo-skill".to_string(),
				enabled: true,
			},
		)
		.unwrap();
		repo_resource::insert(
			&conn,
			&NewResource {
				res_type: ResourceType::Mcp,
				name: "demo-mcp".to_string(),
				display_name: "Demo MCP".to_string(),
				version: String::new(),
				source_type: SourceType::LocalImport,
				local_path: "/tmp/demo-mcp.json".to_string(),
				enabled: true,
			},
		)
		.unwrap();

		let got = counts(&conn).unwrap();
		assert_eq!(got, LibraryCounts { skill: 1, mcp: 1 });
	}

	// set_enabled: 应精确更新 enabled 列(薄封装转发, 验证不出岔子即可)
	#[test]
	fn set_enabled_updates_flag() {
		let conn = setup_conn();
		let id = repo_resource::insert(
			&conn,
			&NewResource {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				display_name: "Demo Skill".to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: "/tmp/demo-skill".to_string(),
				enabled: true,
			},
		)
		.unwrap();

		set_enabled(&conn, id, false).unwrap();

		assert!(!get(&conn, id).unwrap().unwrap().enabled);
	}

	// delete: 应删库记录、清理 data_dir 内容、并记一条"卸载"活动
	#[test]
	fn delete_removes_resource_row_and_data_dir_content() {
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		let target = data_dir.path().join("skills/demo-skill");
		fs::create_dir_all(&target).unwrap();
		fs::write(target.join("SKILL.md"), "---\nversion: 1.0.0\n---\n").unwrap();

		let id = repo_resource::insert(
			&conn,
			&NewResource {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				display_name: "Demo Skill".to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: target.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();

		delete(&conn, data_dir.path(), id).unwrap();

		assert_eq!(get(&conn, id).unwrap(), None);
		assert!(!target.exists(), "data_dir 下的内容也应被清理");

		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 7, "卸载");
		assert_eq!(activities[0].res_type, 1, "Skill");
	}

	// delete: 资源不存在时应是 no-op(Ok, 不报错), 也不产生活动记录
	#[test]
	fn delete_is_noop_when_resource_missing() {
		let data_dir = tempdir().unwrap();
		let conn = setup_conn();

		delete(&conn, data_dir.path(), 9999).unwrap();

		assert!(repo_activity::recent(&conn, 10).unwrap().is_empty());
	}

	// delete: local_path 不在 data_dir 内(防御性场景, 理论上 import_local 不会产生这种脏数据)
	// 时不应删除该外部路径, 只删库记录
	#[test]
	fn delete_does_not_touch_content_outside_data_dir() {
		let data_dir = tempdir().unwrap();
		let outside = tempdir().unwrap();
		let conn = setup_conn();

		let outside_file = outside.path().join("user-owned.json");
		fs::write(&outside_file, "{}").unwrap();

		let id = repo_resource::insert(
			&conn,
			&NewResource {
				res_type: ResourceType::Mcp,
				name: "demo-mcp".to_string(),
				display_name: "Demo MCP".to_string(),
				version: String::new(),
				source_type: SourceType::LocalImport,
				local_path: outside_file.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();

		delete(&conn, data_dir.path(), id).unwrap();

		assert_eq!(get(&conn, id).unwrap(), None, "库记录仍应被删除");
		assert!(outside_file.exists(), "data_dir 之外的内容不应被触碰");
	}

	// list: 应支持透传 res_type/keyword 过滤(薄封装转发, 与 repo_resource::list 的过滤逻辑
	// 已有专门测试, 这里只验证服务层确实原样转发)
	#[test]
	fn list_forwards_filter_to_repo() {
		let conn = setup_conn();
		repo_resource::insert(
			&conn,
			&NewResource {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				display_name: "Demo Skill".to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: "/tmp/demo-skill".to_string(),
				enabled: true,
			},
		)
		.unwrap();

		let all = list(&conn, None, None).unwrap();
		assert_eq!(all.len(), 1);

		let filtered = list(&conn, Some(ResourceType::Mcp), None).unwrap();
		assert!(filtered.is_empty());
	}
}
