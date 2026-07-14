// 文件作用: market_cache 表仓储 —— upsert_many/query/get/etag_for/set_etag, 显式列名/禁
//           SELECT */全参数化查询(阿里巴巴泰山版数据库规约); queryable 列落库供过滤/排序,
//           完整记录整份序列化进 raw_json 供还原(见 migrations/0001_init.sql market_cache
//           表注释)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use rusqlite::{named_params, params, Connection, OptionalExtension, Row};

use crate::domain::market::{MarketResourceRespVO, Query, SortBy};

/// 将一行查询结果还原为 MarketResourceRespVO: 只从 raw_json 反序列化完整记录; queryable 列
/// (source_type 等)只供 SQL 侧过滤/排序使用, 不参与还原, 避免两份真源互相打架
fn row_to_market_resource(row: &Row) -> rusqlite::Result<MarketResourceRespVO> {
	let raw_json: String = row.get(0)?;
	serde_json::from_str(&raw_json).map_err(|err| {
		rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
	})
}

/// 按 (source_type, ext_id) 唯一键(uk_market_cache_src_ext)批量插入或冲突更新。queryable 列
/// (source_type/res_type/ext_id/name/author/stars/category/auth_required)落库供 query 过滤/
/// 排序, 完整记录整份序列化进 raw_json 供 query/get 还原。etag 列不在此维护: MarketResourceRespVO
/// 本身不带 etag 字段(它是一次 HTTP 响应的产物, 不是单条资源的属性), 冲突时保留原值, 新插入行
/// 用列默认空串, 留给未来抓取层收到响应头后单独写入(见 etag_for 的读取侧)。fetch_time 每次
/// 落库都刷新为当前时间, 表示"最近一次在本地缓存写入/刷新的时间"
pub fn upsert_many(conn: &Connection, items: &[MarketResourceRespVO]) -> rusqlite::Result<()> {
	for item in items {
		let raw_json = serde_json::to_string(item)
			.map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
		conn.execute(
			"INSERT INTO market_cache \
			 (source_type, res_type, ext_id, name, author, stars, category, auth_required, raw_json) \
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
			 ON CONFLICT(source_type, ext_id) DO UPDATE SET \
			 res_type = excluded.res_type, name = excluded.name, author = excluded.author, \
			 stars = excluded.stars, category = excluded.category, \
			 auth_required = excluded.auth_required, raw_json = excluded.raw_json, \
			 fetch_time = datetime('now')",
			params![
				i64::from(item.source_type),
				i64::from(item.res_type),
				item.ext_id,
				item.name,
				item.author,
				item.stars,
				item.category,
				item.auth_required,
				raw_json,
			],
		)?;
	}
	Ok(())
}

/// 按关键字(匹配 name 或 author)/res_type/category 过滤 + 排序 + 分页查询市场资源, 返回
/// (本页项, 总数); 总数按同一组过滤条件单独统计(不受分页影响), 供前端渲染分页控件。SQL 文本
/// 固定不拼接: ORDER BY 子句无法参数化(绑定参数只能替换值, 不能替换列名/关键字), 故按 sort
/// 变体各自使用一整条固定 SQL 字面量, 而不是拼接 ORDER BY 片段
pub fn query(
	conn: &Connection,
	query: &Query,
) -> rusqlite::Result<(Vec<MarketResourceRespVO>, i64)> {
	let keyword_param: Option<String> = query.keyword.as_ref().map(|k| format!("%{k}%"));
	let res_type_param: Option<i64> = query.res_type.map(i64::from);
	let category_param: Option<&str> = query.category.as_deref();

	let total: i64 = conn.query_row(
		"SELECT COUNT(id) FROM market_cache \
		 WHERE (:keyword IS NULL OR name LIKE :keyword OR author LIKE :keyword) \
		 AND (:res_type IS NULL OR res_type = :res_type) \
		 AND (:category IS NULL OR category = :category)",
		named_params! {
			":keyword": keyword_param,
			":res_type": res_type_param,
			":category": category_param,
		},
		|row| row.get(0),
	)?;

	let page_size = query.page_size.max(1);
	let offset = (query.page.max(1) - 1) * page_size;

	let sql: &str = match query.sort {
		SortBy::Recommended => {
			"SELECT raw_json FROM market_cache \
			 WHERE (:keyword IS NULL OR name LIKE :keyword OR author LIKE :keyword) \
			 AND (:res_type IS NULL OR res_type = :res_type) \
			 AND (:category IS NULL OR category = :category) \
			 ORDER BY id ASC \
			 LIMIT :limit OFFSET :offset"
		}
		SortBy::Stars => {
			"SELECT raw_json FROM market_cache \
			 WHERE (:keyword IS NULL OR name LIKE :keyword OR author LIKE :keyword) \
			 AND (:res_type IS NULL OR res_type = :res_type) \
			 AND (:category IS NULL OR category = :category) \
			 ORDER BY stars DESC, id ASC \
			 LIMIT :limit OFFSET :offset"
		}
		SortBy::Updated => {
			"SELECT raw_json FROM market_cache \
			 WHERE (:keyword IS NULL OR name LIKE :keyword OR author LIKE :keyword) \
			 AND (:res_type IS NULL OR res_type = :res_type) \
			 AND (:category IS NULL OR category = :category) \
			 ORDER BY fetch_time DESC, id ASC \
			 LIMIT :limit OFFSET :offset"
		}
	};
	let mut stmt = conn.prepare(sql)?;
	let rows = stmt.query_map(
		named_params! {
			":keyword": keyword_param,
			":res_type": res_type_param,
			":category": category_param,
			":limit": page_size,
			":offset": offset,
		},
		row_to_market_resource,
	)?;
	let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
	Ok((items, total))
}

/// 按 (source_type, ext_id) 精确查询单条市场资源(uk_market_cache_src_ext), 不存在返回 None
pub fn get(
	conn: &Connection,
	source_type: i64,
	ext_id: &str,
) -> rusqlite::Result<Option<MarketResourceRespVO>> {
	conn.query_row(
		"SELECT raw_json FROM market_cache WHERE source_type = ?1 AND ext_id = ?2",
		params![source_type, ext_id],
		row_to_market_resource,
	)
	.optional()
}

/// 查询某来源当前的 etag(供未来增量刷新发起 If-None-Match), 取该来源下 fetch_time 最新的一行;
/// 该来源尚无任何缓存行时返回 None。etag 列不由 upsert_many 维护(见其文档), 此处只负责读
pub fn etag_for(conn: &Connection, source_type: i64) -> rusqlite::Result<Option<String>> {
	conn.query_row(
		"SELECT etag FROM market_cache WHERE source_type = ?1 \
		 ORDER BY fetch_time DESC, id DESC LIMIT 1",
		params![source_type],
		|row| row.get(0),
	)
	.optional()
}

/// 写入某来源当前的 etag(供未来增量刷新发起 If-None-Match), 更新该来源下 fetch_time 最新的
/// 一行(与 etag_for 的读取口径一致, 见其文档), 返回受影响行数; 该来源尚无任何缓存行时为空
/// 操作(找不到可更新的行, 不报错, 返回 0)。当前尚无调用方: SourceProvider::search 还没有把
/// HTTP 响应携带的 etag 透传给调用方(见 services::market::refresh 文件头注释"关于 etag"
/// 一节), 打通这条链路超出 M2 Task 6 的范围; 本函数是为后续任务预留的最小落库原语, 与
/// etag_for 对称
pub fn set_etag(conn: &Connection, source_type: i64, etag: &str) -> rusqlite::Result<usize> {
	conn.execute(
		"UPDATE market_cache SET etag = ?1 WHERE id = ( \
		 SELECT id FROM market_cache WHERE source_type = ?2 \
		 ORDER BY fetch_time DESC, id DESC LIMIT 1 \
		 )",
		params![etag, source_type],
	)
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use super::*;
	use crate::domain::agent::McpServerDef;
	use crate::domain::market::{InstallManifest, SourceId};
	use crate::domain::resource::ResourceType;

	/// 建一个已迁移好 10 张表结构的内存库, 供仓储测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	fn sample_market_resource(ext_id: &str) -> MarketResourceRespVO {
		MarketResourceRespVO {
			source_type: SourceId::GithubSkills,
			res_type: ResourceType::Skill,
			ext_id: ext_id.to_string(),
			name: "demo-skill".to_string(),
			display_name: "Demo Skill".to_string(),
			description: "示例描述".to_string(),
			author: "acme".to_string(),
			version: "1.0.0".to_string(),
			stars: 10,
			category: "productivity".to_string(),
			tags: vec!["demo".to_string()],
			auth_required: false,
			install_manifest: InstallManifest::Skill {
				repo: "acme/skills".to_string(),
				path: "skills/demo".to_string(),
				git_ref: "main".to_string(),
			},
			updated_at: "2026-07-01T00:00:00Z".to_string(),
		}
	}

	fn sample_query(sort: SortBy, page: i64, page_size: i64) -> Query {
		Query {
			keyword: None,
			res_type: None,
			category: None,
			sort,
			page,
			page_size,
		}
	}

	// upsert_many -> get 应完整还原 MarketResourceRespVO(raw_json 往返不丢字段, 含嵌套 InstallManifest)
	#[test]
	fn upsert_many_then_get_round_trips_full_record() {
		let conn = setup_conn();
		let item = sample_market_resource("ext-1");
		upsert_many(&conn, std::slice::from_ref(&item)).unwrap();

		let got = get(&conn, i64::from(SourceId::GithubSkills), "ext-1")
			.unwrap()
			.expect("刚 upsert 的记录应能查到");
		assert_eq!(got, item);
	}

	// upsert_many 按 (source_type, ext_id) 冲突更新, 不产生第二行, 且新值覆盖旧值
	#[test]
	fn upsert_many_on_conflict_updates_in_place_without_duplicating() {
		let conn = setup_conn();
		let mut item = sample_market_resource("ext-1");
		upsert_many(&conn, std::slice::from_ref(&item)).unwrap();

		item.stars = 999;
		item.category = "updated-category".to_string();
		upsert_many(&conn, &[item]).unwrap();

		let got = get(&conn, i64::from(SourceId::GithubSkills), "ext-1")
			.unwrap()
			.unwrap();
		assert_eq!(got.stars, 999);
		assert_eq!(got.category, "updated-category");

		let (_, total) = query(&conn, &sample_query(SortBy::Recommended, 1, 10)).unwrap();
		assert_eq!(total, 1, "冲突更新不应产生第二行");
	}

	// get 查不存在的 (source_type, ext_id) 应返回 None, 不是 Err
	#[test]
	fn get_missing_returns_none() {
		let conn = setup_conn();
		assert_eq!(
			get(&conn, i64::from(SourceId::GithubSkills), "nope").unwrap(),
			None
		);
	}

	// query 应支持关键字过滤(匹配 name 或 author), 并返回匹配的总数
	#[test]
	fn query_filters_by_keyword_across_name_and_author() {
		let conn = setup_conn();
		let mut a = sample_market_resource("ext-a");
		a.name = "code-review-helper".to_string();
		a.author = "alice".to_string();
		let mut b = sample_market_resource("ext-b");
		b.name = "unrelated-tool".to_string();
		b.author = "review-bot".to_string();
		let mut c = sample_market_resource("ext-c");
		c.name = "totally-different".to_string();
		c.author = "carol".to_string();
		upsert_many(&conn, &[a, b, c]).unwrap();

		let mut q = sample_query(SortBy::Recommended, 1, 10);
		q.keyword = Some("review".to_string());
		let (items, total) = query(&conn, &q).unwrap();
		assert_eq!(total, 2, "name 或 author 命中 review 的应有两条");
		let ext_ids: Vec<_> = items.iter().map(|i| i.ext_id.clone()).collect();
		assert!(ext_ids.contains(&"ext-a".to_string()));
		assert!(ext_ids.contains(&"ext-b".to_string()));
	}

	// query 应支持按 res_type 精确过滤
	#[test]
	fn query_filters_by_res_type() {
		let conn = setup_conn();
		let skill = sample_market_resource("ext-skill");
		let mut mcp = sample_market_resource("ext-mcp");
		mcp.res_type = ResourceType::Mcp;
		mcp.install_manifest = InstallManifest::Mcp {
			server_def: McpServerDef {
				name: "srv".to_string(),
				command: Some("npx".to_string()),
				args: vec![],
				env: BTreeMap::new(),
				url: None,
			},
		};
		upsert_many(&conn, &[skill, mcp]).unwrap();

		let mut q = sample_query(SortBy::Recommended, 1, 10);
		q.res_type = Some(ResourceType::Mcp);
		let (items, total) = query(&conn, &q).unwrap();
		assert_eq!(total, 1);
		assert_eq!(items[0].ext_id, "ext-mcp");
	}

	// query 应支持按 category 精确过滤
	#[test]
	fn query_filters_by_category_exact_match() {
		let conn = setup_conn();
		let mut a = sample_market_resource("ext-a");
		a.category = "productivity".to_string();
		let mut b = sample_market_resource("ext-b");
		b.category = "devtools".to_string();
		upsert_many(&conn, &[a, b]).unwrap();

		let mut q = sample_query(SortBy::Recommended, 1, 10);
		q.category = Some("devtools".to_string());
		let (items, total) = query(&conn, &q).unwrap();
		assert_eq!(total, 1);
		assert_eq!(items[0].ext_id, "ext-b");
	}

	// query 按 SortBy::Stars 应以 stars 降序返回
	#[test]
	fn query_sorts_by_stars_descending() {
		let conn = setup_conn();
		let mut low = sample_market_resource("ext-low");
		low.stars = 5;
		let mut high = sample_market_resource("ext-high");
		high.stars = 100;
		let mut mid = sample_market_resource("ext-mid");
		mid.stars = 50;
		upsert_many(&conn, &[low, high, mid]).unwrap();

		let (items, total) = query(&conn, &sample_query(SortBy::Stars, 1, 10)).unwrap();
		assert_eq!(total, 3);
		assert_eq!(
			items.iter().map(|i| i.ext_id.clone()).collect::<Vec<_>>(),
			vec!["ext-high", "ext-mid", "ext-low"]
		);
	}

	// query 按 SortBy::Updated 应以 fetch_time 降序返回(手工拉开 fetch_time 差距制造确定性顺序,
	// 避免同一秒内 upsert 的行 datetime('now') 打平)
	#[test]
	fn query_sorts_by_updated_uses_fetch_time_descending() {
		let conn = setup_conn();
		upsert_many(
			&conn,
			&[
				sample_market_resource("ext-a"),
				sample_market_resource("ext-b"),
				sample_market_resource("ext-c"),
			],
		)
		.unwrap();
		conn.execute(
			"UPDATE market_cache SET fetch_time = '2026-01-01 00:00:00' WHERE ext_id = 'ext-a'",
			[],
		)
		.unwrap();
		conn.execute(
			"UPDATE market_cache SET fetch_time = '2026-03-01 00:00:00' WHERE ext_id = 'ext-b'",
			[],
		)
		.unwrap();
		conn.execute(
			"UPDATE market_cache SET fetch_time = '2026-02-01 00:00:00' WHERE ext_id = 'ext-c'",
			[],
		)
		.unwrap();

		let (items, _) = query(&conn, &sample_query(SortBy::Updated, 1, 10)).unwrap();
		assert_eq!(
			items.iter().map(|i| i.ext_id.clone()).collect::<Vec<_>>(),
			vec!["ext-b", "ext-c", "ext-a"]
		);
	}

	// query 应支持分页, 且总数不受分页影响(按 Recommended=id 自然顺序验证跨页边界)
	#[test]
	fn query_paginates_and_returns_total_count() {
		let conn = setup_conn();
		let items: Vec<MarketResourceRespVO> = (1..=5)
			.map(|i| sample_market_resource(&format!("ext-{i}")))
			.collect();
		upsert_many(&conn, &items).unwrap();

		let (page1, total1) = query(&conn, &sample_query(SortBy::Recommended, 1, 2)).unwrap();
		assert_eq!(total1, 5);
		assert_eq!(
			page1.iter().map(|i| i.ext_id.clone()).collect::<Vec<_>>(),
			vec!["ext-1", "ext-2"]
		);

		let (page3, total3) = query(&conn, &sample_query(SortBy::Recommended, 3, 2)).unwrap();
		assert_eq!(total3, 5);
		assert_eq!(
			page3.iter().map(|i| i.ext_id.clone()).collect::<Vec<_>>(),
			vec!["ext-5"]
		);
	}

	// etag_for: 该来源尚无任何缓存行时应返回 None
	#[test]
	fn etag_for_returns_none_when_source_has_no_rows() {
		let conn = setup_conn();
		assert_eq!(
			etag_for(&conn, i64::from(SourceId::GithubSkills)).unwrap(),
			None
		);
	}

	// etag_for: upsert_many 不维护 etag 列(MarketResourceRespVO 不带 etag 字段), 落库后默认应为空串;
	// 该列一旦被直接写入(模拟未来抓取层收到 HTTP 响应头后落库), etag_for 应能读到最新值
	#[test]
	fn etag_for_reads_current_column_value() {
		let conn = setup_conn();
		upsert_many(&conn, &[sample_market_resource("ext-1")]).unwrap();

		assert_eq!(
			etag_for(&conn, i64::from(SourceId::GithubSkills)).unwrap(),
			Some(String::new()),
			"upsert_many 不维护 etag 列, 默认应为空串"
		);

		conn.execute(
			"UPDATE market_cache SET etag = ?1 WHERE ext_id = ?2",
			params!["W/\"abc123\"", "ext-1"],
		)
		.unwrap();

		assert_eq!(
			etag_for(&conn, i64::from(SourceId::GithubSkills)).unwrap(),
			Some("W/\"abc123\"".to_string())
		);
	}

	// set_etag: 应更新该来源下 fetch_time 最新一行的 etag 列, etag_for 应能读到新值
	#[test]
	fn set_etag_then_etag_for_round_trips_value() {
		let conn = setup_conn();
		upsert_many(&conn, &[sample_market_resource("ext-1")]).unwrap();

		let affected = set_etag(&conn, i64::from(SourceId::GithubSkills), "W/\"abc123\"").unwrap();

		assert_eq!(affected, 1);
		assert_eq!(
			etag_for(&conn, i64::from(SourceId::GithubSkills)).unwrap(),
			Some("W/\"abc123\"".to_string())
		);
	}

	// set_etag: 该来源尚无任何缓存行时应为空操作(受影响行数 0), 不报错
	#[test]
	fn set_etag_is_noop_when_source_has_no_rows() {
		let conn = setup_conn();
		let affected = set_etag(&conn, i64::from(SourceId::GithubSkills), "W/\"abc123\"").unwrap();
		assert_eq!(affected, 0);
	}
}
