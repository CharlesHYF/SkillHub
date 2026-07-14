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
//
//           下载安装(Task 9): fetch_install_payload(纯异步, 按 source_type 从给定源列表定位
//           provider 并调用其 fetch_payload, 不碰数据库)与 write_installed(纯同步, 落地文件 +
//           repo_resource::insert + repo_activity::add, 不含 await)同样遵循上述拆分惯例; install
//           是二者的组合, 供不受 Send 约束的调用方使用, 命令层(commands::market::market_install)
//           同样改为三段式(查详情持锁 -> 异步拉取不持锁 -> 落库持锁)调用, 不使用 install 本身。
// 创建日期: 2026-07-10

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use futures::future::join_all;
use rusqlite::Connection;
use serde_json::Value;

use crate::domain::agent::McpServerDef;
use crate::domain::market::{MarketResourceRespVO, Query, SortBy, SourceId};
use crate::domain::resource::{ResourceRespVO, SourceType};
use crate::infra::repo_activity;
use crate::infra::repo_market;
use crate::infra::repo_resource::{self, NewResource};
use crate::infra::source::{AuthKind, FileEntry, InstallPayload, SourceProvider};

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
/// 无意义地携带令牌。http_client 由调用方传入(生产用 commands::market::market_refresh 依当前
/// SettingRespVO 现场构造的 infra::http::build_http_client 结果, 使代理/超时真正生效; 单测传入
/// infra::http::client() 默认客户端即可), 本函数自身不再内部构造, 与 infra::source::mod
/// "client 由调用方传入"的既有约定一致
pub async fn fetch_all(
	sources: &[Box<dyn SourceProvider>],
	github_token: Option<&str>,
	http_client: &reqwest::Client,
) -> Vec<Result<Vec<MarketResourceRespVO>>> {
	let query = full_catalog_query();

	let searches = sources.iter().map(|source| {
		let token = match source.auth_kind() {
			Some(AuthKind::GitHub) => github_token,
			_ => None,
		};
		source.search(http_client, &query, token)
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
	outcomes: Vec<Result<Vec<MarketResourceRespVO>>>,
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
/// 调用 fetch_all/write_refresh_results, 不使用本函数(见文件头注释"关于拆分")。http_client
/// 同 fetch_all, 由调用方传入
pub async fn refresh(
	conn: &Connection,
	sources: &[Box<dyn SourceProvider>],
	github_token: Option<&str>,
	http_client: &reqwest::Client,
) -> Result<usize> {
	let outcomes = fetch_all(sources, github_token, http_client).await;
	write_refresh_results(conn, outcomes)
}

/// 按过滤/排序/分页条件搜索市场缓存, 直接转调 repo_market::query(过滤/排序/分页均已在仓储层
/// 实现, 见其文档), 本层不附加任何业务逻辑。前端原型的"已认证/免费"筛选未在此接入: 每条
/// MarketResourceRespVO 本就携带 auth_required 字段, 可由前端在已取回的当页数据上直接筛选, 暂无需
/// 后端新增查询维度(见本任务报告"疑虑"一节)
pub fn search(conn: &Connection, query: &Query) -> Result<(Vec<MarketResourceRespVO>, i64)> {
	Ok(repo_market::query(conn, query)?)
}

/// 按 (source_type, ext_id) 查询单条市场资源详情, 直接转调 repo_market::get, 不存在返回 None
pub fn detail(
	conn: &Connection,
	source_type: i64,
	ext_id: &str,
) -> Result<Option<MarketResourceRespVO>> {
	Ok(repo_market::get(conn, source_type, ext_id)?)
}

/// 按 source_type 在给定源列表里定位对应 provider 并调用其 fetch_payload, 拉取某条市场资源的
/// 完整安装内容; 纯异步网络调用, 不接触数据库连接(遵循文件头"关于拆分"的既有约定), 供调用方在
/// 拿到 payload 后再单独调用 write_installed 落库。sources 由调用方传入(生产用 infra::source::
/// all_sources(), 单测注入 FakeSource), 与 fetch_all 同一测试性约定, 使本函数可脱离真实网络测试。
/// source_type 在 sources 里找不到匹配 id(理论不会发生: 三源均覆盖 SourceId 全部三个变体, 除非
/// 调用方注入了残缺的源列表)时返回 Err; cached_detail 通常是此前 detail() 查到的完整市场资源
/// 记录, 其 install_manifest 决定 fetch_payload 具体怎么拉。http_client 同 fetch_all, 由调用方
/// 传入(生产用依当前 SettingRespVO 构造的客户端, 使代理/超时真正生效)
pub async fn fetch_install_payload(
	sources: &[Box<dyn SourceProvider>],
	source_type: i64,
	token: Option<&str>,
	cached_detail: &MarketResourceRespVO,
	http_client: &reqwest::Client,
) -> Result<InstallPayload> {
	let target_id = SourceId::from_i64(source_type);
	let provider = sources
		.iter()
		.find(|source| source.id() == target_id)
		.ok_or_else(|| anyhow!("未找到 source_type={source_type} 对应的市场源"))?;
	provider
		.fetch_payload(http_client, cached_detail, token)
		.await
}

/// 把资源名转成安全的文件系统路径片段: 部分来源(如 github_mcp 对 npm 作用域包名的猜测, 见其
/// search 文档)产出的 name 可能内嵌 '/'(如 "@scope/pkg"), 直接拼进单层目录/文件名会被当成多级
/// 路径分隔符, 产出意料之外的嵌套目录甚至写入失败(中间目录未创建); 统一替换为 '_' 后再用于落盘
/// 路径。另外 "."/".."/空 这几个片段作为单层目录名会指向自身/父级, 而 write_skill_files 会对该
/// 目录 remove_dir_all, 一旦命中会清空 skills 根乃至整个 data_dir(含数据库), 故一并回落为安全
/// 占位名。以上转换均不影响数据库里 resource.name 本身(仍原样保留原始名称, 只有构造落盘路径这一步做转换)。
/// 可见性 pub(crate): 供 services::agent_import(M6 Task BE-2, 从已检测 Agent 反向导入已装
/// Skill/MCP 到本地库)落地 MCP 定义文件前复用同一份净化逻辑, 不重复实现
pub(crate) fn sanitize_path_segment(name: &str) -> String {
	let replaced = name.replace(['/', '\\'], "_");
	if replaced.is_empty() || replaced == "." || replaced == ".." {
		"_".to_string()
	} else {
		replaced
	}
}

/// 按来源换算落库用的 SourceType(官方/第三方): mcp_registry 是"官方 MCP Registry"(见
/// domain::market::SourceId 文档字面标注"官方"), 落 Official; github_skills/github_mcp 均是
/// "聚合可配置 GitHub 仓库列表"的机制(RepoRef 列表本身即面向任意第三方仓库设计, 见两源 Default
/// 实现文档"用户可配置追加/替换仓库留待后续任务接入配置持久化"), 即便当前默认值恰好指向
/// Anthropic/MCP 官方仓库, 该聚合"channel"本身面向任意第三方仓库, 故落 ThirdParty。二者边界目前
/// 较主观(任务未给出精确定义), 留待 Charles 审阅确认是否需要调整
fn resource_source_type(source_type: SourceId) -> SourceType {
	match source_type {
		SourceId::McpRegistry => SourceType::Official,
		SourceId::GithubSkills | SourceId::GithubMcp => SourceType::ThirdParty,
	}
}

/// Skill 安装落地: 把 files 逐个写到 data_dir/skills/<safe_name>/ 下(按 rel_path 建目录+写文件);
/// 目标目录若已存在(重复安装同名 Skill)先整体清空再重建, 不做增量合并, 与 services::library::
/// import_skill 的既有惯例一致, 返回该目录完整路径(供落库 local_path 列)
fn write_skill_files(data_dir: &Path, safe_name: &str, files: &[FileEntry]) -> Result<PathBuf> {
	let target = data_dir.join("skills").join(safe_name);
	if target.exists() {
		fs::remove_dir_all(&target)
			.with_context(|| format!("清理旧 Skill 目录失败: {}", target.display()))?;
	}
	fs::create_dir_all(&target).with_context(|| format!("创建目录失败: {}", target.display()))?;
	for file in files {
		let file_path = target.join(&file.rel_path);
		if let Some(parent) = file_path.parent() {
			fs::create_dir_all(parent)
				.with_context(|| format!("创建目录失败: {}", parent.display()))?;
		}
		fs::write(&file_path, &file.content)
			.with_context(|| format!("写入文件失败: {}", file_path.display()))?;
	}
	Ok(target)
}

/// 把 McpServerDef 转为落地到 data_dir/mcp/<name>.json 的 JSON 对象: 不写 name(定义文件内部不
/// 重复携带 name, 由 ResourceRespVO.name 承担, 与 services::sync::resource_to_desired/
/// parse_single_mcp_def 的既有读取约定一致); 有 command 才写 command/args/env, 有 url 才写 url。
/// 与 infra::adapter::json_mcp::mcp_def_to_json 同一惯例, 各自独立维护一份(体量小, 不值得跨层
/// 共享, 呼应该文件"各 provider 模块各自独立维护一份"的既有约定)
fn mcp_def_to_file_json(def: &McpServerDef) -> Value {
	let mut obj = serde_json::Map::new();
	if let Some(command) = &def.command {
		obj.insert("command".to_string(), Value::String(command.clone()));
	}
	if !def.args.is_empty() {
		obj.insert(
			"args".to_string(),
			Value::Array(def.args.iter().cloned().map(Value::String).collect()),
		);
	}
	if !def.env.is_empty() {
		let env_obj: serde_json::Map<String, Value> = def
			.env
			.iter()
			.map(|(k, v)| (k.clone(), Value::String(v.clone())))
			.collect();
		obj.insert("env".to_string(), Value::Object(env_obj));
	}
	if let Some(url) = &def.url {
		obj.insert("url".to_string(), Value::String(url.clone()));
	}
	Value::Object(obj)
}

/// Mcp(含由 McpTemplate 折叠而来)安装落地: 用 env_overrides 覆盖 server_def.env 里的同名键
/// (required_env 已在 fetch_payload 阶段作为空串占位写入该 map, 见 infra::source::github_mcp::
/// build_server_def 文档; 纯 Mcp 资源通常 env_overrides 为空, 覆盖操作对其是空操作, 不必单独
/// 分支处理), 序列化为单定义 JSON(不重复携带 name, 见 mcp_def_to_file_json 文档)写入
/// data_dir/mcp/<safe_name>.json, 返回该文件完整路径(供落库 local_path 列)。
/// 可见性 pub(crate): 供 services::agent_import(M6 Task BE-2)复用同一份 MCP 定义落盘逻辑——
/// 从已检测 Agent 读到的 McpServerDef 同样要落成这种单定义 JSON 文件, 与市场安装的落地形状
/// 完全一致, 不重复实现; 调用时传 env_overrides 为空 BTreeMap 即可(该场景不涉及模板占位覆盖)
pub(crate) fn write_mcp_def(
	data_dir: &Path,
	safe_name: &str,
	mut server_def: McpServerDef,
	env_overrides: &BTreeMap<String, String>,
) -> Result<PathBuf> {
	for (key, value) in env_overrides {
		server_def.env.insert(key.clone(), value.clone());
	}

	let mcp_dir = data_dir.join("mcp");
	fs::create_dir_all(&mcp_dir).with_context(|| format!("创建目录失败: {}", mcp_dir.display()))?;
	let target = mcp_dir.join(format!("{safe_name}.json"));
	let text = serde_json::to_string_pretty(&mcp_def_to_file_json(&server_def))
		.context("序列化 MCP 定义失败")?;
	fs::write(&target, text).with_context(|| format!("写入文件失败: {}", target.display()))?;
	Ok(target)
}

/// 把 fetch_install_payload 拉回的安装内容落地到本地存储目录(data_dir)并登记为一条 ResourceRespVO:
/// Skill 把 payload 携带的全部文件写到 data_dir/skills/<name>/(整树覆盖, 与 services::library::
/// import_skill "重复安装同名 Skill 不做增量合并"的既有惯例一致); Mcp 用 env_overrides 覆盖
/// server_def.env 里的同名键后生成单定义 data_dir/mcp/<name>.json(不重复携带 name, 与
/// services::library::import_mcp 落地的既有文件形状一致, 供 services::sync::resource_to_desired
/// 按同一约定读回); McpTemplate 在 fetch_payload 阶段已折叠进 InstallPayload::Mcp(required_env
/// 已作为空串占位写进 server_def.env, 见 infra::source::github_mcp::build_server_def 文档), 本
/// 函数不再区分, 统一按 Mcp 分支处理。source_type 按来源换算 Official/ThirdParty(见
/// resource_source_type 文档), 落库(repo_resource::insert)后追加一条"下载"活动(act_type=3),
/// 最终回查完整 ResourceRespVO 返回
pub fn write_installed(
	conn: &Connection,
	data_dir: &Path,
	detail: &MarketResourceRespVO,
	payload: InstallPayload,
	env_overrides: &BTreeMap<String, String>,
) -> Result<ResourceRespVO> {
	let safe_name = sanitize_path_segment(&detail.name);
	let local_path = match payload {
		InstallPayload::Skill { files } => write_skill_files(data_dir, &safe_name, &files)?,
		InstallPayload::Mcp { server_def } => {
			write_mcp_def(data_dir, &safe_name, server_def, env_overrides)?
		}
	};

	let resource_id = repo_resource::insert(
		conn,
		&NewResource {
			res_type: detail.res_type,
			name: detail.name.clone(),
			display_name: detail.display_name.clone(),
			version: detail.version.clone(),
			source_type: resource_source_type(detail.source_type),
			local_path: local_path.to_string_lossy().into_owned(),
			enabled: true,
		},
	)?;
	repo_activity::add(
		conn,
		3,
		i64::from(detail.res_type),
		&format!("安装 {}", detail.name),
		&format!("从市场安装(来源 {:?})", detail.source_type),
	)?;

	repo_resource::get(conn, resource_id)?
		.ok_or_else(|| anyhow!("安装后未能查回资源: id={resource_id}"))
}

/// 便捷组合: 先按 (source_type, ext_id) 查市场缓存详情, 再异步拉取安装内容(fetch_install_payload,
/// 不接触数据库), 最后同步落地入库(write_installed), 一次调用完成整个安装流程, 返回落库后的完整
/// ResourceRespVO。供不受 Tauri 命令 Send 约束的调用方使用(本模块测试、未来可能的后台任务);
/// commands::market::market_install 出于 Send 约束(见文件头注释"关于拆分")改为三段式调用(查详情
/// 持锁 -> 异步拉取不持锁 -> 落库持锁), 不使用本函数。ext_id 对应的市场资源不存在时返回 Err。
/// http_client 同 fetch_all, 由调用方传入; 新增该参数后入参数量超过 clippy 默认阈值, 均为
/// 语义独立、不便合并成结构体的平铺参数, 与 infra::repo_sync::add_item 同一豁免惯例
#[allow(clippy::too_many_arguments)]
pub async fn install(
	conn: &Connection,
	data_dir: &Path,
	sources: &[Box<dyn SourceProvider>],
	source_type: i64,
	ext_id: &str,
	token: Option<&str>,
	env_overrides: &BTreeMap<String, String>,
	http_client: &reqwest::Client,
) -> Result<ResourceRespVO> {
	let cached_detail = repo_market::get(conn, source_type, ext_id)?
		.ok_or_else(|| anyhow!("市场资源不存在: source_type={source_type}, ext_id={ext_id}"))?;
	let payload =
		fetch_install_payload(sources, source_type, token, &cached_detail, http_client).await?;
	write_installed(conn, data_dir, &cached_detail, payload, env_overrides)
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use async_trait::async_trait;
	use reqwest::Client;

	use tempfile::tempdir;

	use super::*;
	use crate::domain::market::InstallManifest;
	use crate::domain::resource::ResourceType;
	use crate::infra::http::client;

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	fn sample_resource(source_type: SourceId, ext_id: &str) -> MarketResourceRespVO {
		MarketResourceRespVO {
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
	/// token, 供断言"仅 GitHub 类源转发令牌"这一路由逻辑; fetch_payload 恒返回构造时给定的
	/// payload_result, 并记录本次调用收到的 token(供 fetch_install_payload 相关测试断言令牌
	/// 转发), 未显式配置时(见 bailing_payload 便捷构造)保持"调用即视为测试写错"的旧行为
	struct FakeSource {
		source_id: SourceId,
		auth_kind: Option<AuthKind>,
		result: Result<Vec<MarketResourceRespVO>, String>,
		received_token: Arc<Mutex<Option<Option<String>>>>,
		payload_result: Result<InstallPayload, String>,
		received_fetch_token: Arc<Mutex<Option<Option<String>>>>,
	}

	/// fetch_payload 恒报错占位的固定值, 供本模块内不关心安装内容拉取的 search/refresh 系列测试
	/// 构造 FakeSource 时复用, 避免每处都重复拼一遍同样的 Err 字符串
	fn bailing_payload() -> Result<InstallPayload, String> {
		Err("本测试不应调用 fetch_payload".to_string())
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
		) -> anyhow::Result<Vec<MarketResourceRespVO>> {
			*self.received_token.lock().unwrap() = Some(token.map(str::to_string));
			self.result.clone().map_err(|err| anyhow::anyhow!(err))
		}

		async fn fetch_payload(
			&self,
			_client: &Client,
			_resource: &MarketResourceRespVO,
			token: Option<&str>,
		) -> anyhow::Result<InstallPayload> {
			*self.received_fetch_token.lock().unwrap() = Some(token.map(str::to_string));
			self.payload_result
				.clone()
				.map_err(|err| anyhow::anyhow!(err))
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
			payload_result: bailing_payload(),
			received_fetch_token: Arc::new(Mutex::new(None)),
		});
		let ok_source: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::GithubSkills,
			auth_kind: Some(AuthKind::GitHub),
			result: Ok(vec![sample_resource(SourceId::GithubSkills, "ext-1")]),
			received_token: Arc::new(Mutex::new(None)),
			payload_result: bailing_payload(),
			received_fetch_token: Arc::new(Mutex::new(None)),
		});
		let sources = vec![failing_source, ok_source];

		let count = refresh(&conn, &sources, None, &client()).await.unwrap();

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
			payload_result: bailing_payload(),
			received_fetch_token: Arc::new(Mutex::new(None)),
		});
		let public_source: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::McpRegistry,
			auth_kind: None,
			result: Ok(vec![]),
			received_token: public_token_seen.clone(),
			payload_result: bailing_payload(),
			received_fetch_token: Arc::new(Mutex::new(None)),
		});
		let sources = vec![github_source, public_source];

		refresh(&conn, &sources, Some("gh-secret-token"), &client())
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
		let count = refresh(&conn, &sources, None, &client()).await.unwrap();
		assert_eq!(count, 0);
	}

	// write_refresh_results: 应把成功结果逐条 upsert 落库并按 items.len() 累加返回值, 失败
	// 结果静默跳过不计入(纯同步单测, 直接验证命令层实际调用的落库逻辑本身, 不需要 FakeSource/
	// 异步运行时)
	#[test]
	fn write_refresh_results_sums_successful_items_and_skips_errors() {
		let conn = setup_conn();
		let outcomes: Vec<Result<Vec<MarketResourceRespVO>>> = vec![
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

	// ---------- 下载安装(Task 9): fetch_install_payload / write_installed / install ----------

	fn sample_mcp_server_def(name: &str) -> McpServerDef {
		McpServerDef {
			name: name.to_string(),
			command: Some("npx".to_string()),
			args: vec!["-y".to_string(), name.to_string()],
			env: BTreeMap::new(),
			url: None,
		}
	}

	/// 与 sample_resource 同构, 但产出 Mcp 类资源(res_type/install_manifest 均替换), 供本节
	/// 安装相关测试复用
	fn sample_mcp_resource(source_type: SourceId, ext_id: &str) -> MarketResourceRespVO {
		let mut resource = sample_resource(source_type, ext_id);
		resource.res_type = ResourceType::Mcp;
		resource.install_manifest = InstallManifest::Mcp {
			server_def: sample_mcp_server_def(ext_id),
		};
		resource
	}

	// resource_source_type: mcp_registry 应换算为 Official, github_skills/github_mcp 均换算为
	// ThirdParty(边界含义见该函数文档, 系依据任务说明做出的判断, 留待 Charles 审阅)
	#[test]
	fn resource_source_type_maps_sources_to_official_or_third_party() {
		assert_eq!(
			resource_source_type(SourceId::McpRegistry),
			SourceType::Official
		);
		assert_eq!(
			resource_source_type(SourceId::GithubSkills),
			SourceType::ThirdParty
		);
		assert_eq!(
			resource_source_type(SourceId::GithubMcp),
			SourceType::ThirdParty
		);
	}

	// write_installed: Skill 类 payload 应把全部文件写到 data_dir/skills/<name>/(含嵌套子目录),
	// 落库为对应 ResourceRespVO, 并记一条"下载"活动(act_type=3)
	#[test]
	fn write_installed_persists_skill_files_and_records_resource_and_activity() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let detail = sample_resource(SourceId::GithubSkills, "demo-skill");
		let payload = InstallPayload::Skill {
			files: vec![
				FileEntry {
					rel_path: "SKILL.md".to_string(),
					content: b"skill body".to_vec(),
				},
				FileEntry {
					rel_path: "scripts/run.sh".to_string(),
					content: b"#!/bin/sh\necho hi\n".to_vec(),
				},
			],
		};

		let resource =
			write_installed(&conn, data_dir.path(), &detail, payload, &BTreeMap::new()).unwrap();

		assert_eq!(resource.res_type, ResourceType::Skill);
		assert_eq!(resource.name, "demo-skill");
		assert_eq!(resource.version, detail.version);
		assert_eq!(
			resource.source_type,
			SourceType::ThirdParty,
			"github_skills 落 ThirdParty"
		);
		assert!(resource.enabled);

		let target = data_dir.path().join("skills/demo-skill");
		assert_eq!(resource.local_path, target.to_string_lossy());
		assert_eq!(
			fs::read_to_string(target.join("SKILL.md")).unwrap(),
			"skill body"
		);
		assert_eq!(
			fs::read_to_string(target.join("scripts/run.sh")).unwrap(),
			"#!/bin/sh\necho hi\n"
		);

		assert_eq!(
			repo_resource::get(&conn, resource.id).unwrap(),
			Some(resource)
		);
		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 3, "下载");
		assert_eq!(activities[0].res_type, 1, "Skill");
	}

	// write_installed: 重复安装同名 Skill(目标目录已存在残留旧文件)应整体清空再重建, 不做增量
	// 合并, 与 services::library::import_skill 的既有惯例一致
	#[test]
	fn write_installed_overwrites_existing_skill_directory_without_merging() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let target = data_dir.path().join("skills/demo-skill");
		fs::create_dir_all(&target).unwrap();
		fs::write(target.join("STALE.md"), "旧文件, 应被清理").unwrap();

		let detail = sample_resource(SourceId::GithubSkills, "demo-skill");
		let payload = InstallPayload::Skill {
			files: vec![FileEntry {
				rel_path: "SKILL.md".to_string(),
				content: b"new body".to_vec(),
			}],
		};

		write_installed(&conn, data_dir.path(), &detail, payload, &BTreeMap::new()).unwrap();

		assert!(!target.join("STALE.md").exists(), "旧文件应被清空");
		assert_eq!(
			fs::read_to_string(target.join("SKILL.md")).unwrap(),
			"new body"
		);
	}

	// write_installed: Mcp 类 payload 应生成 data_dir/mcp/<name>.json 单定义文件, 不重复携带
	// name(与 services::library::import_mcp 落地的既有文件形状一致); mcp_registry 来源应换算为
	// SourceType::Official
	#[test]
	fn write_installed_persists_mcp_definition_as_single_file_without_name_field() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let detail = sample_mcp_resource(SourceId::McpRegistry, "demo-mcp");
		let payload = InstallPayload::Mcp {
			server_def: sample_mcp_server_def("demo-mcp"),
		};

		let resource =
			write_installed(&conn, data_dir.path(), &detail, payload, &BTreeMap::new()).unwrap();

		assert_eq!(resource.res_type, ResourceType::Mcp);
		assert_eq!(
			resource.source_type,
			SourceType::Official,
			"mcp_registry 落 Official"
		);

		let target = data_dir.path().join("mcp/demo-mcp.json");
		assert_eq!(resource.local_path, target.to_string_lossy());
		let json: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
		assert_eq!(json["command"], "npx");
		assert_eq!(json["args"][0], "-y");
		assert!(json.get("name").is_none(), "定义文件不应重复携带 name");

		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 3, "下载");
		assert_eq!(activities[0].res_type, 2, "Mcp");
	}

	// write_installed: env_overrides 应覆盖 server_def.env 里的同名键 —— 模拟 McpTemplate 的
	// required_env 在 fetch_payload 阶段已作为空串占位写入 env(见 infra::source::github_mcp::
	// build_server_def 文档), 安装时由用户通过 env_overrides 填充真实值
	#[test]
	fn write_installed_fills_env_overrides_into_mcp_definition() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let detail = sample_mcp_resource(SourceId::GithubMcp, "demo-template-mcp");
		let mut server_def = sample_mcp_server_def("demo-template-mcp");
		server_def.env.insert("API_KEY".to_string(), String::new());
		let payload = InstallPayload::Mcp { server_def };
		let mut overrides = BTreeMap::new();
		overrides.insert("API_KEY".to_string(), "sk-real-value".to_string());

		let resource =
			write_installed(&conn, data_dir.path(), &detail, payload, &overrides).unwrap();

		let json: Value =
			serde_json::from_str(&fs::read_to_string(&resource.local_path).unwrap()).unwrap();
		assert_eq!(json["env"]["API_KEY"], "sk-real-value");
	}

	// write_installed: 名称内嵌 '/'(如 github_mcp 对 npm 作用域包名的猜测 "@scope/pkg", 见其
	// search 文档)时应把 '/' 替换为 '_' 后再落盘, 不应因误当路径分隔符而写入失败或产生意外的
	// 嵌套目录
	#[test]
	fn write_installed_sanitizes_slash_in_name_for_mcp_file_path() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let detail = sample_mcp_resource(SourceId::GithubMcp, "@acme/server-foo");
		let payload = InstallPayload::Mcp {
			server_def: sample_mcp_server_def("@acme/server-foo"),
		};

		let resource =
			write_installed(&conn, data_dir.path(), &detail, payload, &BTreeMap::new()).unwrap();

		let expected = data_dir.path().join("mcp/@acme_server-foo.json");
		assert_eq!(resource.local_path, expected.to_string_lossy());
		assert!(expected.is_file(), "应落在替换后的安全路径, 不产生嵌套目录");
	}

	// sanitize_path_segment: 防目录穿越 —— "."/".."/空 必须回落为安全占位名, 否则 write_skill_files
	// 会对 data_dir/skills/<片段> 执行 remove_dir_all, 片段为 "."/".." 时会清空 skills 根乃至整个
	// data_dir(含数据库)。这是与 services::portability zip-slip 同根因的第二处落盘缺口的回归用例
	#[test]
	fn sanitize_path_segment_neutralizes_dot_dotdot_and_empty() {
		assert_eq!(sanitize_path_segment(".."), "_");
		assert_eq!(sanitize_path_segment("."), "_");
		assert_eq!(sanitize_path_segment(""), "_");
		// 正常名与内嵌分隔符名保持既有行为(仅替换分隔符)
		assert_eq!(sanitize_path_segment("@scope/pkg"), "@scope_pkg");
		assert_eq!(sanitize_path_segment("normal-skill"), "normal-skill");
		// "..." 等非精确匹配是合法文件名, 不应被误伤
		assert_eq!(sanitize_path_segment("..."), "...");
	}

	// fetch_install_payload: 应按 source_type 在 sources 里找到匹配的 provider, 调用其
	// fetch_payload 并原样转发 token, 返回其产出的 payload
	#[tokio::test]
	async fn fetch_install_payload_finds_matching_provider_and_forwards_token() {
		let fetch_token_seen = Arc::new(Mutex::new(None));
		let expected_payload = InstallPayload::Mcp {
			server_def: sample_mcp_server_def("demo-mcp"),
		};
		let fake: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::GithubMcp,
			auth_kind: Some(AuthKind::GitHub),
			result: Ok(vec![]),
			received_token: Arc::new(Mutex::new(None)),
			payload_result: Ok(expected_payload.clone()),
			received_fetch_token: fetch_token_seen.clone(),
		});
		let sources = vec![fake];
		let detail = sample_mcp_resource(SourceId::GithubMcp, "demo-mcp");

		let payload = fetch_install_payload(
			&sources,
			i64::from(SourceId::GithubMcp),
			Some("gh-token"),
			&detail,
			&client(),
		)
		.await
		.unwrap();

		assert_eq!(payload, expected_payload);
		assert_eq!(
			*fetch_token_seen.lock().unwrap(),
			Some(Some("gh-token".to_string()))
		);
	}

	// fetch_install_payload: sources 里找不到匹配 source_type 的 provider 应返回 Err, 不 panic
	#[tokio::test]
	async fn fetch_install_payload_returns_err_when_no_matching_source() {
		let sources: Vec<Box<dyn SourceProvider>> = Vec::new();
		let detail = sample_mcp_resource(SourceId::GithubMcp, "demo-mcp");

		let result = fetch_install_payload(
			&sources,
			i64::from(SourceId::GithubMcp),
			None,
			&detail,
			&client(),
		)
		.await;

		assert!(result.is_err());
	}

	// install: 端到端组合(查详情 -> fetch_install_payload -> write_installed), 用 FakeSource
	// 注入固定 payload 避免真实网络; 应落地文件 + 入库 resource + 记一条"下载"活动
	#[tokio::test]
	async fn install_end_to_end_with_fake_source_persists_resource_and_activity() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let cached = sample_resource(SourceId::GithubSkills, "demo-skill");
		repo_market::upsert_many(&conn, std::slice::from_ref(&cached)).unwrap();

		let fake: Box<dyn SourceProvider> = Box::new(FakeSource {
			source_id: SourceId::GithubSkills,
			auth_kind: Some(AuthKind::GitHub),
			result: Ok(vec![]),
			received_token: Arc::new(Mutex::new(None)),
			payload_result: Ok(InstallPayload::Skill {
				files: vec![FileEntry {
					rel_path: "SKILL.md".to_string(),
					content: b"hello".to_vec(),
				}],
			}),
			received_fetch_token: Arc::new(Mutex::new(None)),
		});
		let sources = vec![fake];

		let resource = install(
			&conn,
			data_dir.path(),
			&sources,
			i64::from(SourceId::GithubSkills),
			"demo-skill",
			None,
			&BTreeMap::new(),
			&client(),
		)
		.await
		.unwrap();

		assert_eq!(resource.name, "demo-skill");
		let target = data_dir.path().join("skills/demo-skill/SKILL.md");
		assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
		assert_eq!(
			repo_resource::get(&conn, resource.id).unwrap(),
			Some(resource)
		);
		let activities = repo_activity::recent(&conn, 10).unwrap();
		assert_eq!(activities.len(), 1);
		assert_eq!(activities[0].act_type, 3, "下载");
	}

	// install: 市场缓存里不存在该 (source_type, ext_id) 时应返回 Err, 不落任何库记录
	#[tokio::test]
	async fn install_returns_err_when_market_resource_missing() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let sources: Vec<Box<dyn SourceProvider>> = Vec::new();

		let result = install(
			&conn,
			data_dir.path(),
			&sources,
			i64::from(SourceId::GithubSkills),
			"nope",
			None,
			&BTreeMap::new(),
			&client(),
		)
		.await;

		assert!(result.is_err());
		assert!(
			repo_resource::list(&conn, &repo_resource::ListFilter::default())
				.unwrap()
				.is_empty()
		);
	}
}
