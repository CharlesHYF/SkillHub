// 文件作用: 市场聚合服务编排层 —— 并发调用市场三源(github_skills/mcp_registry/github_mcp)的
//           search 拉取全量资源并写入 market_cache 缓存(fetch_all/write_refresh_results/
//           refresh), 以及读缓存的搜索(search, 直接转调 repo_market::query, 过滤/排序/分页
//           均已在仓储层实现)与详情查询(detail)。均只接受 &Connection(refresh 系列另接受
//           源列表/令牌), 不摸 AppState/Tauri 运行时, 呼应 services::sync/services::library
//           既有的分层约定; 源列表由调用方传入(生产用 infra::source::all_sources(), 单测注入
//           假源), 使聚合/容错逻辑本身可脱离真实网络测试。
//
//           关于 fetch_all/write_refresh_results 与 refresh 的拆分: commands::market::
//           market_refresh 是本仓库首个 async 命令, 而 Tauri 要求命令 Future: Send 才能被其
//           异步运行时 spawn(见 tauri::ipc::Invoke::respond_async 的 Send + 'static 约束)。
//           AppState.db 是 std::sync::Mutex<Connection>, 其 MutexGuard 刻意 !Send(见
//           lib.rs); 若在一次 async fn 里先 `state.db()` 取出 guard、再跨一次真正的网络
//           await、又在 await 之后继续用同一个 conn 落库, 则该 guard 必须跨 await 存活,
//           整个命令 Future 会因此退化为 !Send, 无法编译/无法被 spawn。为此把"纯异步网络
//           拉取"(fetch_all, 不碰数据库)与"纯同步落库"(write_refresh_results, 不含 await)
//           拆成两段, 命令层在两段之间才短暂加锁, 临界区内无 await, 从根源规避该问题; refresh
//           则是二者的组合, 供不受此 Send 约束的调用方(本模块测试、未来可能的后台任务)一次
//           调用完成整个刷新流程。
// 创建日期: 2026-07-10

use anyhow::Result;
use futures::future::join_all;
use rusqlite::Connection;

use crate::domain::market::{MarketResource, Query, SortBy};
use crate::infra::http::client;
use crate::infra::repo_market;
use crate::infra::source::{AuthKind, SourceProvider};

/// 供 refresh/fetch_all 内部使用的近乎全量查询参数: 三源(github_skills/mcp_registry/
/// github_mcp)目前均未使用 search 的 query 入参(各自文档均写明"关键字/分类过滤留给聚合层",
/// 恒返回全量), 此处字段取值本身不影响任何一个源的行为, 仅需满足 SourceProvider::search
/// 的签名要求
fn full_catalog_query() -> Query {
	Query {
		keyword: None,
		res_type: None,
		category: None,
		sort: SortBy::Recommended,
		page: 1,
		page_size: i64::MAX,
	}
}

/// 并发调用给定源列表各自的 search 方法拉取全量资源, 按 sources 顺序返回逐源的原始结果
/// (成功或失败)。纯异步网络调用, 不接触数据库连接(见文件头注释"关于拆分"), 供调用方在拿到
/// 全部结果后再单独调用 write_refresh_results 落库。sources 由调用方传入(生产用
/// infra::source::all_sources(), 单测可注入假源), 使并发聚合本身可脱离真实网络测试。
/// github_token 只转发给 auth_kind()==Some(GitHub) 的源(如 github_skills/github_mcp), 公开
/// 只读源(如 mcp_registry)恒收到 None——即便传入的 github_token 非 None, 与这些源自身"恒发
/// 匿名请求"的既有约定一致(见 infra::source::mcp_registry 文档), 不向不需要认证的公开接口
/// 无意义地携带令牌
pub async fn fetch_all(
	sources: &[Box<dyn SourceProvider>],
	github_token: Option<&str>,
) -> Vec<Result<Vec<MarketResource>>> {
	let http_client = client();
	let query = full_catalog_query();

	let searches = sources.iter().map(|source| {
		let token = match source.auth_kind() {
			Some(AuthKind::GitHub) => github_token,
			_ => None,
		};
		source.search(&http_client, &query, token)
	});
	join_all(searches).await
}

/// 把 fetch_all 拉回的逐源结果落库: 成功的源逐一 upsert 进 market_cache, 返回值为成功写入的
/// 资源条数之和(逐源 items.len() 相加, upsert_many 对同批内每条 item 都精确写一行, 见其
/// 文档); 失败的源(网络错误/解析异常等)静默跳过, 不中断其它源, 该源的缓存保留上次成功刷新
/// 的旧值(不清空/不报错)。纯同步, 不含任何 await, 供命令层加锁取出 conn 后在临界区内直接
/// 调用(见文件头注释"关于拆分")。
///
/// 关于 etag: market_cache.etag 列目前仅由 repo_market::etag_for 读取, 新补的
/// repo_market::set_etag 也尚无调用方——SourceProvider::search 当前的方法签名并未把底层 HTTP
/// 响应携带的 etag 透传给调用方(该值止步于 infra::http::get_json 返回的 HttpResult::Ok.etag,
/// 被 github_skills/mcp_registry/github_mcp 三个实现各自内部丢弃, 且它们发起请求时也恒传
/// None 作为请求侧 etag, 见各自 search 实现), 本函数因此没有 etag 可写。要接入真正基于
/// If-None-Match 的增量刷新, 需要先扩展 SourceProvider::search 的签名以双向传递 etag, 这会
/// 影响三个已交付并测试过的源实现, 超出本任务范围, 留待后续任务评估是否值得做
pub fn write_refresh_results(
	conn: &Connection,
	outcomes: Vec<Result<Vec<MarketResource>>>,
) -> Result<usize> {
	let mut written = 0usize;
	for outcome in outcomes {
		// 单源失败静默跳过, 不中断其它源(见函数文档); 仓库目前未接入任何日志框架
		// (log/tracing 均不在 Cargo.toml 依赖内), 暂不记录具体错误原因, 留待后续任务按需补充
		let Ok(items) = outcome else {
			continue;
		};
		if items.is_empty() {
			continue;
		}
		repo_market::upsert_many(conn, &items)?;
		written += items.len();
	}
	Ok(written)
}

/// 便捷组合: 先并发拉取(fetch_all)再落库(write_refresh_results), 一次调用完成整个市场缓存
/// 刷新流程, 返回本次成功写入的资源条数之和。供不受 Tauri 命令 Send 约束的调用方使用(本模块
/// 测试、未来可能的后台定时任务); commands::market::market_refresh 出于 Send 约束改为分两步
/// 调用 fetch_all/write_refresh_results, 不使用本函数(见文件头注释"关于拆分")
pub async fn refresh(
	conn: &Connection,
	sources: &[Box<dyn SourceProvider>],
	github_token: Option<&str>,
) -> Result<usize> {
	let outcomes = fetch_all(sources, github_token).await;
	write_refresh_results(conn, outcomes)
}

/// 按过滤/排序/分页条件搜索市场缓存, 直接转调 repo_market::query(过滤/排序/分页均已在仓储层
/// 实现, 见其文档), 本层不附加任何业务逻辑。前端原型的"已认证/免费"筛选未在此接入: 每条
/// MarketResource 本就携带 auth_required 字段, 可由前端在已取回的当页数据上直接筛选, 暂无需
/// 后端新增查询维度(见本任务报告"疑虑"一节)
pub fn search(conn: &Connection, query: &Query) -> Result<(Vec<MarketResource>, i64)> {
	Ok(repo_market::query(conn, query)?)
}

/// 按 (source_type, ext_id) 查询单条市场资源详情, 直接转调 repo_market::get, 不存在返回 None
pub fn detail(conn: &Connection, source_type: i64, ext_id: &str) -> Result<Option<MarketResource>> {
	Ok(repo_market::get(conn, source_type, ext_id)?)
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use async_trait::async_trait;
	use reqwest::Client;

	use super::*;
	use crate::domain::market::{InstallManifest, SourceId};
	use crate::domain::resource::ResourceType;
	use crate::infra::source::InstallPayload;

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	fn sample_resource(source_type: SourceId, ext_id: &str) -> MarketResource {
		MarketResource {
			source_type,
			res_type: ResourceType::Skill,
			ext_id: ext_id.to_string(),
			name: ext_id.to_string(),
			display_name: ext_id.to_string(),
			description: String::new(),
			author: "acme".to_string(),
			version: "1.0.0".to_string(),
			stars: 0,
			category: String::new(),
			tags: Vec::new(),
			auth_required: false,
			install_manifest: InstallManifest::Skill {
				repo: "acme/demo".to_string(),
				path: "skills/demo".to_string(),
				git_ref: "main".to_string(),
			},
			updated_at: String::new(),
		}
	}

	fn sample_query() -> Query {
		Query {
			keyword: None,
			res_type: None,
			category: None,
			sort: SortBy::Recommended,
			page: 1,
			page_size: 10,
		}
	}

	/// 测试用假源: search 恒返回构造时给定的固定结果(Ok(items) 或 Err), 并记录本次调用收到的
	/// token, 供断言"仅 GitHub 类源转发令牌"这一路由逻辑; 本模块测试不会调用 fetch_payload,
	/// 恒返回错误占位(调用即视为测试写错)
	struct FakeSource {
		source_id: SourceId,
		auth_kind: Option<AuthKind>,
		result: Result<Vec<MarketResource>, String>,
		received_token: Arc<Mutex<Option<Option<String>>>>,
	}

	#[async_trait]
	impl SourceProvider for FakeSource {
		fn id(&self) -> SourceId {
			self.source_id
		}

		async fn search(
			&self,
			_client: &Client,
			_query: &Query,
			token: Option<&str>,
		) -> anyhow::Result<Vec<MarketResource>> {
			*self.received_token.lock().unwrap() = Some(token.map(str::to_string));
			self.result.clone().map_err(|err| anyhow::anyhow!(err))
		}

		async fn fetch_payload(
			&self,
			_client: &Client,
			_resource: &MarketResource,
			_token: Option<&str>,
		) -> anyhow::Result<InstallPayload> {
			anyhow::bail!("本测试不应调用 fetch_payload")
		}

		fn auth_kind(&self) -> Option<AuthKind> {
			self.auth_kind
		}
	}

	// refresh: 应把各源成功返回的结果 upsert 进 market_cache, 返回值为写入条数之和; 某源失败
	// 不应中断其它源(该源贡献 0, 不报错), 其余源仍正常写入
	#[tokio::test]
	async fn refresh_upserts_successful_sources_and_skips_failing_source() {
		let conn = setup_conn();
		let failing_source: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::McpRegistry,
			auth_kind: None,
			result: Err("模拟网络错误".to_string()),
			received_token: Arc::new(Mutex::new(None)),
		});
		let ok_source: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::GithubSkills,
			auth_kind: Some(AuthKind::GitHub),
			result: Ok(vec![sample_resource(SourceId::GithubSkills, "ext-1")]),
			received_token: Arc::new(Mutex::new(None)),
		});
		let sources = vec![failing_source, ok_source];

		let count = refresh(&conn, &sources, None).await.unwrap();

		assert_eq!(count, 1, "失败源应贡献 0, 只有成功源的 1 条应计入");
		let (items, total) = repo_market::query(&conn, &sample_query()).unwrap();
		assert_eq!(total, 1);
		assert_eq!(items[0].ext_id, "ext-1");
	}

	// refresh: github_token 应只转发给 auth_kind()==Some(GitHub) 的源, 公开源恒收到 None,
	// 即便传入的 github_token 非 None
	#[tokio::test]
	async fn refresh_forwards_token_only_to_github_auth_sources() {
		let conn = setup_conn();
		let github_token_seen = Arc::new(Mutex::new(None));
		let public_token_seen = Arc::new(Mutex::new(None));

		let github_source: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::GithubSkills,
			auth_kind: Some(AuthKind::GitHub),
			result: Ok(vec![]),
			received_token: github_token_seen.clone(),
		});
		let public_source: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::McpRegistry,
			auth_kind: None,
			result: Ok(vec![]),
			received_token: public_token_seen.clone(),
		});
		let sources = vec![github_source, public_source];

		refresh(&conn, &sources, Some("gh-secret-token"))
			.await
			.unwrap();

		assert_eq!(
			*github_token_seen.lock().unwrap(),
			Some(Some("gh-secret-token".to_string())),
			"GitHub 类源应收到令牌"
		);
		assert_eq!(
			*public_token_seen.lock().unwrap(),
			Some(None),
			"公开源应恒收到 None, 即便传入了令牌"
		);
	}

	// refresh: 空源列表应直接返回 0, 不报错
	#[tokio::test]
	async fn refresh_returns_zero_for_empty_source_list() {
		let conn = setup_conn();
		let sources: Vec<Box<dyn SourceProvider>> = Vec::new();
		let count = refresh(&conn, &sources, None).await.unwrap();
		assert_eq!(count, 0);
	}

	// write_refresh_results: 应把成功结果逐条 upsert 落库并按 items.len() 累加返回值, 失败
	// 结果静默跳过不计入(纯同步单测, 直接验证命令层实际调用的落库逻辑本身, 不需要 FakeSource/
	// 异步运行时)
	#[test]
	fn write_refresh_results_sums_successful_items_and_skips_errors() {
		let conn = setup_conn();
		let outcomes: Vec<Result<Vec<MarketResource>>> = vec![
			Ok(vec![sample_resource(SourceId::GithubSkills, "ext-1")]),
			Err(anyhow::anyhow!("模拟来源失败")),
			Ok(vec![
				sample_resource(SourceId::McpRegistry, "ext-2"),
				sample_resource(SourceId::McpRegistry, "ext-3"),
			]),
		];

		let count = write_refresh_results(&conn, outcomes).unwrap();

		assert_eq!(count, 3, "失败项应贡献 0, 两个成功项共 1+2=3 条");
		let (_, total) = repo_market::query(&conn, &sample_query()).unwrap();
		assert_eq!(total, 3);
	}

	// search: 应直接转发给 repo_market::query, 过滤/总数均应正确返回(repo_market::query 自身
	// 的详尽边界测试见 infra::repo_market, 此处只验证服务层确实原样转发)
	#[test]
	fn search_forwards_to_repo_and_returns_filtered_items_and_total() {
		let conn = setup_conn();
		repo_market::upsert_many(
			&conn,
			&[
				sample_resource(SourceId::GithubSkills, "ext-a"),
				sample_resource(SourceId::GithubSkills, "ext-b"),
			],
		)
		.unwrap();

		let mut query = sample_query();
		query.keyword = Some("ext-a".to_string());
		let (items, total) = search(&conn, &query).unwrap();

		assert_eq!(total, 1);
		assert_eq!(items[0].ext_id, "ext-a");
	}

	// detail: 应转发给 repo_market::get, 命中时返回完整记录
	#[test]
	fn detail_returns_resource_when_present() {
		let conn = setup_conn();
		let item = sample_resource(SourceId::GithubSkills, "ext-1");
		repo_market::upsert_many(&conn, std::slice::from_ref(&item)).unwrap();

		let got = detail(&conn, i64::from(SourceId::GithubSkills), "ext-1").unwrap();
		assert_eq!(got, Some(item));
	}

	// detail: 查不存在的 (source_type, ext_id) 应返回 None, 不是 Err
	#[test]
	fn detail_returns_none_when_missing() {
		let conn = setup_conn();
		assert_eq!(
			detail(&conn, i64::from(SourceId::GithubSkills), "nope").unwrap(),
			None
		);
	}
}
