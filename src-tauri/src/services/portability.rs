// 文件作用: 导入导出服务编排层(导出部分) —— 按 ExportOptions 收集资源元数据与 data_dir 内容、
//           可选设置/关联、算 sha256 校验和、组装 manifest 并按 Zip/Tar/Json 三种格式打包,
//           写入 import_export_log(见 export_bundle)。只接受 &Connection 与 &Path(data_dir),
//           不摸 AppState/Tauri 运行时, 呼应 services::market/services::library 既有的分层约定。
//
//           关于 scope 与 include_skills/include_mcp 的关系: include_skills/include_mcp 是唯一
//           实际生效的类型开关(不论 scope 取何值均恒定生效, 见 collect_resources); scope=All 与
//           scope=ByType 在当前实现下语义等价(ByType 就是"正通过 include_skills/include_mcp 挑
//           类型"这一模式本身, 并不叠加额外过滤), scope=ByTime 因 ExportOptions 未携带任何时间
//           范围字段, 暂等价于 All/ByType(留待后续任务若要真正实现"按时间"过滤, 需先给
//           ExportOptions 增加时间范围字段, 再据 update_time 二次过滤)。
//
//           关于 include_config 同时门控 settings.json 与 agents.json: ExportOptions 未单独
//           提供"是否包含资源-Agent 关联"的开关, brief 要求二者取一种关系并注释——这里选择让
//           agents.json 与 settings.json 共用 include_config 这一开关, 因为 SkillHub 语境下
//           "配置"泛指"本机如何使用这些资源"的状态, 既包含应用级设置(setting 表), 也包含
//           "期望哪些资源装到哪些 Agent"的关联关系(resource_agent 表), 二者都不是"资源内容
//           本身", 用同一开关归类是当前选项形状下最贴切的一种取舍。
// 创建日期: 2026-07-10

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::prelude::*;
use flate2::write::GzEncoder;
use flate2::Compression;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::domain::portability::{BundleFormat, Counts, ExportOptions, Manifest};
use crate::domain::resource::{Resource, ResourceType};
use crate::infra::repo_assoc;
use crate::infra::repo_impexp;
use crate::infra::repo_resource::{self, ListFilter};
use crate::infra::repo_setting;

/// 内存态待打包文件: 已读入内存的归档内相对路径(以 '/' 分隔, 不含 manifest.json 本身, 因为
/// manifest 本身依赖全部其它文件先收集完毕才能算出 checksums, 无法把自身包含进自己的校验和里)
/// 与二进制内容, 供三种归档格式(Zip/Tar/Json)与 sha256 校验和计算共用同一份收集结果, 避免
/// 每种格式各自重新走一遍收集逻辑
struct BundleFile {
	rel_path: String,
	bytes: Vec<u8>,
}

/// agents.json 的一条记录: 资源按名称+类型标识(而非数据库 id), 关联的 Agent 按展示名标识
/// (而非 agent.id) —— 两边的自增主键都只在导出方本机有效, 不跨机器可移植; 供导入方(M3 Task 4)
/// 据此尝试在目标机器上按名称重新建立关联, 具体"如何按名称匹配 Agent"留待该任务设计
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AgentLinkExport {
	resource_name: String,
	res_type: i64,
	agent_name: String,
}

/// Json 格式导出包的顶层结构: manifest 字段与 Zip/Tar 内 manifest.json 的内容完全一致; files
/// 把其余每个文件的相对路径映射到其内容的标准 base64 编码(带 padding), 供没有归档容器可用的
/// 单文件场景内联携带二进制内容, 键与 manifest.checksums 同一套路径, 供导入方按路径核对
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct JsonBundle {
	manifest: Manifest,
	files: BTreeMap<String, String>,
}

/// 按 opts 收集参与导出的资源: include_skills/include_mcp 分别控制两种类型是否纳入(见文件头
/// 注释"关于 scope 与 include_skills/include_mcp 的关系"), 两者都为 false 时返回空列表, 不视为
/// 错误(导出一个空壳, 由调用方/前端决定是否阻止这种操作)。Skill 在前、Mcp 在后, 且各自按
/// repo_resource::list 既有的 id 升序返回, 整体顺序确定, 便于导出内容/manifest 可重现
fn collect_resources(conn: &Connection, opts: &ExportOptions) -> Result<Vec<Resource>> {
	let mut resources = Vec::new();
	if opts.include_skills {
		resources.extend(repo_resource::list(
			conn,
			&ListFilter {
				res_type: Some(ResourceType::Skill),
				keyword: None,
			},
		)?);
	}
	if opts.include_mcp {
		resources.extend(repo_resource::list(
			conn,
			&ListFilter {
				res_type: Some(ResourceType::Mcp),
				keyword: None,
			},
		)?);
	}
	Ok(resources)
}

/// 计算某资源在导出包里的相对根路径(如 "skills/demo-skill" 或 "mcp/demo-mcp.json"): 直接由
/// resource.local_path 相对 data_dir 求得(而非重新用 resource.name 拼一遍), 与磁盘上实际内容
/// 保持 1:1 一致, 天然规避名称内嵌 '/' 等特殊字符被两套不同规则各自处理一遍导致不一致的问题
/// (见 services::market::sanitize_path_segment 文档"部分来源产出的 name 可能内嵌 '/'")。
/// 统一把路径分隔符归一化为 '/', 不论运行平台, 保证打包产物(zip/tar 条目名与 manifest 键)
/// 跨平台一致。local_path 不在 data_dir 内(理论不会发生, 见 services::library::delete 同类
/// 防御性判断)时返回 Err —— 导出场景下宁可整体失败也不悄悄漏掉一部分内容而让用户误以为备份完整
fn bundle_root_rel_path(data_dir: &Path, resource: &Resource) -> Result<String> {
	let full = Path::new(&resource.local_path);
	let rel = full.strip_prefix(data_dir).map_err(|_| {
		anyhow!(
			"资源 {} 的本地路径不在 data_dir 内, 无法导出: {}",
			resource.name,
			resource.local_path
		)
	})?;
	Ok(normalize_rel_path(rel))
}

/// 把 Path 的各个 component 用 '/' 重新拼接成字符串, 不论运行平台原生分隔符是什么(Windows 上
/// PathBuf 内部分隔符是 '\\'), 保证导出产物的路径表示跨平台一致
fn normalize_rel_path(path: &Path) -> String {
	path.components()
		.map(|c| c.as_os_str().to_string_lossy().into_owned())
		.collect::<Vec<_>>()
		.join("/")
}

/// 递归收集 `dir` 下的全部普通文件, 产出的相对路径均以 `/` 分隔、以 `prefix` 开头(如
/// "skills/demo-skill"); 目录项先按文件名排序再递归, 保证同一份内容多次导出产生确定的收集
/// 顺序, 不依赖文件系统本身的目录遍历顺序(有的文件系统按 inode 顺序返回, 不排序会导致同样内容
/// 两次导出产生不同的 checksums 插入顺序, 虽不影响 BTreeMap 的最终键值对, 但会影响 zip/tar
/// 内条目的先后顺序)。`dir` 不存在时按空处理(理论不会发生, 见 bundle_root_rel_path 文档),
/// 不视为错误; 只处理普通文件与子目录, 符号链接等特殊类型目前场景不涉及
fn collect_dir_files(dir: &Path, prefix: &str, out: &mut Vec<BundleFile>) -> Result<()> {
	if !dir.is_dir() {
		return Ok(());
	}
	let mut entries: Vec<_> = fs::read_dir(dir)
		.with_context(|| format!("读取目录失败: {}", dir.display()))?
		.collect::<std::io::Result<Vec<_>>>()
		.with_context(|| format!("读取目录项失败: {}", dir.display()))?;
	entries.sort_by_key(|entry| entry.file_name());

	for entry in entries {
		let path = entry.path();
		let rel = format!("{prefix}/{}", entry.file_name().to_string_lossy());
		if path.is_dir() {
			collect_dir_files(&path, &rel, out)?;
		} else if path.is_file() {
			let bytes =
				fs::read(&path).with_context(|| format!("读取文件失败: {}", path.display()))?;
			out.push(BundleFile {
				rel_path: rel,
				bytes,
			});
		}
	}
	Ok(())
}

/// 读取单个文件(Mcp 资源的定义文件)整体内容, 追加为一条待打包文件
fn collect_single_file(path: &Path, rel_path: String, out: &mut Vec<BundleFile>) -> Result<()> {
	let bytes = fs::read(path).with_context(|| format!("读取文件失败: {}", path.display()))?;
	out.push(BundleFile { rel_path, bytes });
	Ok(())
}

/// 计算字节内容的 sha256, 输出为小写十六进制字符串(与 git/sha256sum 等常见工具的展示格式一致),
/// 供 manifest.checksums 使用
fn sha256_hex(bytes: &[u8]) -> String {
	let digest = Sha256::digest(bytes);
	digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 把 setting 表整表落地为 settings.json 的一条待打包文件(键值对拍平成一个 JSON 对象, 键按
/// cfg_key 升序, 见 repo_setting::list_all), 返回该文件与写入的设置项数(供 Counts.config)
fn collect_settings_file(conn: &Connection) -> Result<(BundleFile, i64)> {
	let rows = repo_setting::list_all(conn)?;
	let count = rows.len() as i64;
	let map: BTreeMap<String, String> = rows
		.into_iter()
		.map(|row| (row.cfg_key, row.cfg_value))
		.collect();
	let bytes = serde_json::to_vec_pretty(&map).context("序列化 settings.json 失败")?;
	Ok((
		BundleFile {
			rel_path: "settings.json".to_string(),
			bytes,
		},
		count,
	))
}

/// 把本次导出涉及资源(resources)的期望态关联(resource_agent, 仅 desired=1, 见
/// repo_assoc::list_all_links)落地为 agents.json 的一条待打包文件, 只保留关联双方里资源确实
/// 在本次导出集合内的记录(不引用未导出的资源), 返回该文件与写入的关联条数(供 Counts.agent)
fn collect_agents_file(conn: &Connection, resources: &[Resource]) -> Result<(BundleFile, i64)> {
	let resource_by_id: BTreeMap<i64, &Resource> = resources.iter().map(|r| (r.id, r)).collect();

	let links: Vec<AgentLinkExport> = repo_assoc::list_all_links(conn)?
		.into_iter()
		.filter_map(|link| {
			resource_by_id
				.get(&link.resource_id)
				.map(|resource| AgentLinkExport {
					resource_name: resource.name.clone(),
					res_type: i64::from(resource.res_type),
					agent_name: link.agent_name,
				})
		})
		.collect();

	let count = links.len() as i64;
	let bytes = serde_json::to_vec_pretty(&links).context("序列化 agents.json 失败")?;
	Ok((
		BundleFile {
			rel_path: "agents.json".to_string(),
			bytes,
		},
		count,
	))
}

/// 按 opts 收集资源/配置/关联并打包到 out_path, 返回打包清单(Manifest); 写一条导出方向
/// (direction=0)的 import_export_log 记录。纯同步, 不含网络 I/O, 全程持锁调用亦可(无 await,
/// 不涉及 commands::market 那种 Send 拆分顾虑, 见 commands::portability::export_bundle)
pub fn export_bundle(
	conn: &Connection,
	data_dir: &Path,
	opts: &ExportOptions,
	out_path: &Path,
) -> Result<Manifest> {
	let resources = collect_resources(conn, opts)?;

	let mut files: Vec<BundleFile> = Vec::new();
	let mut versions: BTreeMap<String, String> = BTreeMap::new();
	let mut skill_count = 0i64;
	let mut mcp_count = 0i64;

	for resource in &resources {
		let root = bundle_root_rel_path(data_dir, resource)?;
		let full_path = Path::new(&resource.local_path);
		match resource.res_type {
			ResourceType::Skill => {
				skill_count += 1;
				collect_dir_files(full_path, &root, &mut files)?;
			}
			ResourceType::Mcp => {
				mcp_count += 1;
				collect_single_file(full_path, root.clone(), &mut files)?;
			}
		}
		if opts.include_version_lock {
			versions.insert(root, resource.version.clone());
		}
	}

	// settings.json 与 agents.json 共用 include_config 开关, 见文件头注释
	let mut config_count = 0i64;
	let mut agent_count = 0i64;
	if opts.include_config {
		let (settings_file, settings_count) = collect_settings_file(conn)?;
		config_count = settings_count;
		files.push(settings_file);

		let (agents_file, links_count) = collect_agents_file(conn, &resources)?;
		agent_count = links_count;
		files.push(agents_file);
	}

	let mut checksums = BTreeMap::new();
	for file in &files {
		checksums.insert(file.rel_path.clone(), sha256_hex(&file.bytes));
	}

	let manifest = Manifest {
		schema_version: 1,
		exported_at: rfc3339_now(),
		counts: Counts {
			skill: skill_count,
			mcp: mcp_count,
			config: config_count,
			agent: agent_count,
		},
		versions,
		checksums,
	};

	match opts.format {
		BundleFormat::Zip => write_zip(out_path, &manifest, &files)?,
		BundleFormat::Tar => write_tar_gz(out_path, &manifest, &files)?,
		BundleFormat::Json => write_json_inline(out_path, &manifest, &files)?,
	}

	let file_name = out_path
		.file_name()
		.map(|s| s.to_string_lossy().into_owned())
		.unwrap_or_default();
	let summary = format!("{skill_count} Skill+{mcp_count} MCP");
	repo_impexp::add(conn, 0, &file_name, i64::from(opts.format), &summary, 1)?;

	Ok(manifest)
}

/// 打包为 Zip: manifest.json 在先, 其余文件按收集顺序(Skill 资源在前、Mcp 在后, 各自内部见
/// collect_dir_files 的排序说明)依次写入, 压缩方式用 Deflate(比 Stored 体积更小, 且已在
/// Cargo.toml 只开启 deflate 特性, 不引入 aes/zstd/bzip2 等未用到的能力)
fn write_zip(out_path: &Path, manifest: &Manifest, files: &[BundleFile]) -> Result<()> {
	let file = fs::File::create(out_path)
		.with_context(|| format!("创建文件失败: {}", out_path.display()))?;
	let mut zip = ZipWriter::new(file);
	let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

	let manifest_bytes = serde_json::to_vec_pretty(manifest).context("序列化 manifest 失败")?;
	zip.start_file("manifest.json", options)
		.context("写入 manifest.json 条目失败")?;
	zip.write_all(&manifest_bytes)
		.context("写入 manifest.json 内容失败")?;

	for entry in files {
		zip.start_file(entry.rel_path.as_str(), options)
			.with_context(|| format!("写入 {} 条目失败", entry.rel_path))?;
		zip.write_all(&entry.bytes)
			.with_context(|| format!("写入 {} 内容失败", entry.rel_path))?;
	}

	zip.finish().context("关闭 zip 归档失败")?;
	Ok(())
}

/// 把一段字节内容作为一个条目追加进 tar 归档: size/mode 需在调用 append_data 前手动设置(tar
/// crate 不会替调用方猜测), path/cksum 由 append_data 内部处理
fn append_tar_entry(
	builder: &mut tar::Builder<GzEncoder<fs::File>>,
	name: &str,
	bytes: &[u8],
) -> Result<()> {
	let mut header = tar::Header::new_gnu();
	header.set_size(bytes.len() as u64);
	header.set_mode(0o644);
	builder
		.append_data(&mut header, name, bytes)
		.with_context(|| format!("写入 tar 条目失败: {name}"))?;
	Ok(())
}

/// 打包为 Tar.gz: tar 只管归档结构本身, 压缩交给外层包一层 GzEncoder(默认压缩级别), 与
/// Tar.gz 的常规组合方式一致; manifest.json 在先, 其余文件按收集顺序依次写入
fn write_tar_gz(out_path: &Path, manifest: &Manifest, files: &[BundleFile]) -> Result<()> {
	let file = fs::File::create(out_path)
		.with_context(|| format!("创建文件失败: {}", out_path.display()))?;
	let encoder = GzEncoder::new(file, Compression::default());
	let mut builder = tar::Builder::new(encoder);

	let manifest_bytes = serde_json::to_vec_pretty(manifest).context("序列化 manifest 失败")?;
	append_tar_entry(&mut builder, "manifest.json", &manifest_bytes)?;
	for entry in files {
		append_tar_entry(&mut builder, &entry.rel_path, &entry.bytes)?;
	}

	let encoder = builder.into_inner().context("关闭 tar 归档失败")?;
	encoder.finish().context("关闭 gzip 流失败")?;
	Ok(())
}

/// 打包为单文件 Json: manifest 字段与 zip/tar 内 manifest.json 完全一致的结构; 其余每个文件的
/// 二进制内容用标准 base64(带 padding)内联进 files 字段, 供没有归档容器可用的场景(如需要把
/// 整个导出内容粘贴进纯文本渠道)使用, 代价是体积比二进制归档大(base64 约膨胀 1/3, 且没有压缩)
fn write_json_inline(out_path: &Path, manifest: &Manifest, files: &[BundleFile]) -> Result<()> {
	let encoded: BTreeMap<String, String> = files
		.iter()
		.map(|entry| (entry.rel_path.clone(), BASE64_STANDARD.encode(&entry.bytes)))
		.collect();
	let bundle = JsonBundle {
		manifest: manifest.clone(),
		files: encoded,
	};
	let text = serde_json::to_string_pretty(&bundle).context("序列化 Json 格式导出包失败")?;
	fs::write(out_path, text).with_context(|| format!("写入文件失败: {}", out_path.display()))?;
	Ok(())
}

/// 取当前 UTC 时间拼一个 RFC3339 字符串(如 "2026-07-10T12:34:56Z"), 供 Manifest.exported_at;
/// 不引入 chrono/time 等日期时间 crate, 呼应 services::auth::sqlite_now 文档"不引入日期时间
/// crate, 与全库时间戳保持同一权威时间源"的既有取舍, 只精确到秒(足够展示粒度)
fn rfc3339_now() -> String {
	let secs = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| d.as_secs())
		.unwrap_or(0);
	format_rfc3339(secs)
}

/// 把 unix 秒数格式化为 UTC RFC3339 字符串
fn format_rfc3339(unix_secs: u64) -> String {
	let days = (unix_secs / 86400) as i64;
	let secs_of_day = unix_secs % 86400;
	let (year, month, day) = civil_from_days(days);
	let hour = secs_of_day / 3600;
	let minute = (secs_of_day % 3600) / 60;
	let second = secs_of_day % 60;
	format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant 的 civil_from_days 算法(公开算法, 见
/// https://howardhinnant.github.io/date_algorithms.html#civil_from_days): 把"自 1970-01-01
/// 起的天数"转换为(年, 月, 日), 在公历(proleptic Gregorian calendar)极宽的范围内精确成立,
/// 不需要任何日期时间 crate; 已用已知 unix 秒数(0/1700000000 等)交叉验证, 见本文件测试
fn civil_from_days(z: i64) -> (i64, u32, u32) {
	let z = z + 719468;
	let era = if z >= 0 { z } else { z - 146096 } / 146097;
	let doe = z - era * 146097; // [0, 146096]
	let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
	let y = yoe + era * 400;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
	let mp = (5 * doy + 2) / 153; // [0, 11]
	let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
	let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
	let year = if m <= 2 { y + 1 } else { y };
	(year, m, d)
}

#[cfg(test)]
mod tests {
	use std::io::Read as _;

	use tempfile::tempdir;

	use super::*;
	use crate::domain::agent::{AgentKind, AgentScope, DetectedAgent};
	use crate::domain::portability::Scope;
	use crate::domain::resource::SourceType;
	use crate::infra::{repo_agent, repo_setting};

	/// 建一个已迁移好 10 张表结构的内存库, 供本模块测试复用(migrate 为 pub(crate), 见 infra::store)
	fn setup_conn() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		crate::infra::store::migrate(&mut conn).unwrap();
		conn
	}

	/// 造一份最小 data_dir: 1 个 Skill(demo-skill, 带 frontmatter version + 嵌套子目录文件,
	/// 用来验证递归收集)+ 1 个 Mcp(demo-mcp.json), 并各自登记为一条 resource(local_path 精确
	/// 指向刚落地的路径), 返回 (TempDir 句柄, skill resource_id, mcp resource_id)
	fn seed_data_dir_and_resources(conn: &Connection) -> (tempfile::TempDir, i64, i64) {
		let data_dir = tempdir().unwrap();
		let skill_dir = data_dir.path().join("skills/demo-skill");
		fs::create_dir_all(skill_dir.join("scripts")).unwrap();
		fs::write(
			skill_dir.join("SKILL.md"),
			"---\nversion: 1.2.0\n---\n正文\n",
		)
		.unwrap();
		fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/sh\necho hi\n").unwrap();

		let mcp_dir = data_dir.path().join("mcp");
		fs::create_dir_all(&mcp_dir).unwrap();
		let mcp_path = mcp_dir.join("demo-mcp.json");
		fs::write(&mcp_path, r#"{"command":"node","args":["index.js"]}"#).unwrap();

		let skill_id = repo_resource::insert(
			conn,
			&repo_resource::NewResource {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				display_name: "Demo Skill".to_string(),
				version: "1.2.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: skill_dir.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();

		let mcp_id = repo_resource::insert(
			conn,
			&repo_resource::NewResource {
				res_type: ResourceType::Mcp,
				name: "demo-mcp".to_string(),
				display_name: "Demo MCP".to_string(),
				version: String::new(),
				source_type: SourceType::LocalImport,
				local_path: mcp_path.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();

		(data_dir, skill_id, mcp_id)
	}

	fn full_options(format: BundleFormat) -> ExportOptions {
		ExportOptions {
			include_skills: true,
			include_mcp: true,
			scope: Scope::All,
			format,
			include_config: false,
			include_version_lock: false,
		}
	}

	// export_bundle(Zip): 应产出可解归档的 zip 文件, 含 manifest.json + 两个资源的原始内容;
	// manifest.counts/checksums 均正确; 并应写入一条 import_export_log 导出记录
	#[test]
	fn export_bundle_zip_produces_extractable_archive_with_correct_manifest() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.zip");

		let manifest = export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Zip),
			&out_path,
		)
		.unwrap();

		assert!(out_path.is_file());
		assert_eq!(
			manifest.counts,
			Counts {
				skill: 1,
				mcp: 1,
				config: 0,
				agent: 0
			}
		);
		assert_eq!(
			manifest.checksums.len(),
			3,
			"SKILL.md + run.sh + demo-mcp.json"
		);
		assert!(manifest.versions.is_empty(), "未开启 include_version_lock");

		let skill_md_bytes = fs::read(data_dir.path().join("skills/demo-skill/SKILL.md")).unwrap();
		assert_eq!(
			manifest.checksums["skills/demo-skill/SKILL.md"],
			sha256_hex(&skill_md_bytes)
		);
		let mcp_bytes = fs::read(data_dir.path().join("mcp/demo-mcp.json")).unwrap();
		assert_eq!(
			manifest.checksums["mcp/demo-mcp.json"],
			sha256_hex(&mcp_bytes)
		);

		let file = fs::File::open(&out_path).unwrap();
		let mut archive = zip::ZipArchive::new(file).unwrap();
		let names: Vec<String> = (0..archive.len())
			.map(|i| archive.by_index(i).unwrap().name().to_string())
			.collect();
		assert!(names.contains(&"manifest.json".to_string()));
		assert!(names.contains(&"skills/demo-skill/SKILL.md".to_string()));
		assert!(names.contains(&"skills/demo-skill/scripts/run.sh".to_string()));
		assert!(names.contains(&"mcp/demo-mcp.json".to_string()));

		let mut manifest_text = String::new();
		archive
			.by_name("manifest.json")
			.unwrap()
			.read_to_string(&mut manifest_text)
			.unwrap();
		let manifest_in_zip: Manifest = serde_json::from_str(&manifest_text).unwrap();
		assert_eq!(
			manifest_in_zip, manifest,
			"归档内 manifest.json 应与返回值一致"
		);

		let mut skill_md_text = String::new();
		archive
			.by_name("skills/demo-skill/SKILL.md")
			.unwrap()
			.read_to_string(&mut skill_md_text)
			.unwrap();
		assert_eq!(skill_md_text, "---\nversion: 1.2.0\n---\n正文\n");

		// 历史记录: 应写入一条 direction=0(导出) 的成功记录
		let history = repo_impexp::recent(&conn, 10).unwrap();
		assert_eq!(history.len(), 1);
		assert_eq!(history[0].direction, 0, "0-导出");
		assert_eq!(history[0].file_name, "out.zip");
		assert_eq!(history[0].file_format, 1, "1-zip");
		assert_eq!(history[0].summary, "1 Skill+1 MCP");
		assert_eq!(history[0].status, 1, "1-成功");
	}

	// export_bundle(Tar): 应产出可用 flate2+tar 解出的 tar.gz, 内容与 zip 场景一致
	#[test]
	fn export_bundle_tar_produces_extractable_gzip_tar_with_same_contents() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.tar.gz");

		let manifest = export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Tar),
			&out_path,
		)
		.unwrap();

		assert!(out_path.is_file());

		let file = fs::File::open(&out_path).unwrap();
		let decoder = flate2::read::GzDecoder::new(file);
		let mut tar_archive = tar::Archive::new(decoder);

		let mut seen: BTreeMap<String, Vec<u8>> = BTreeMap::new();
		for entry in tar_archive.entries().unwrap() {
			let mut entry = entry.unwrap();
			let path = entry.path().unwrap().to_string_lossy().into_owned();
			let mut content = Vec::new();
			entry.read_to_end(&mut content).unwrap();
			seen.insert(path, content);
		}

		assert!(seen.contains_key("manifest.json"));
		assert!(seen.contains_key("skills/demo-skill/SKILL.md"));
		assert!(seen.contains_key("skills/demo-skill/scripts/run.sh"));
		assert!(seen.contains_key("mcp/demo-mcp.json"));

		let manifest_in_tar: Manifest = serde_json::from_slice(&seen["manifest.json"]).unwrap();
		assert_eq!(manifest_in_tar, manifest);
		assert_eq!(
			seen["skills/demo-skill/SKILL.md"],
			"---\nversion: 1.2.0\n---\n正文\n".as_bytes().to_vec()
		);
		assert_eq!(
			seen["mcp/demo-mcp.json"],
			br#"{"command":"node","args":["index.js"]}"#.to_vec()
		);
	}

	// export_bundle(Json): 单文件 JSON 应内联 base64 内容, 解出后应能精确还原为源文件字节
	#[test]
	fn export_bundle_json_inlines_base64_content_recoverable_to_original_bytes() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.json");

		let manifest = export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Json),
			&out_path,
		)
		.unwrap();

		let text = fs::read_to_string(&out_path).unwrap();
		let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();

		let manifest_in_json: Manifest =
			serde_json::from_value(parsed["manifest"].clone()).unwrap();
		assert_eq!(manifest_in_json, manifest);

		let skill_md_b64 = parsed["files"]["skills/demo-skill/SKILL.md"]
			.as_str()
			.unwrap();
		let decoded = BASE64_STANDARD.decode(skill_md_b64).unwrap();
		assert_eq!(
			decoded,
			"---\nversion: 1.2.0\n---\n正文\n".as_bytes().to_vec()
		);

		let mcp_b64 = parsed["files"]["mcp/demo-mcp.json"].as_str().unwrap();
		let decoded_mcp = BASE64_STANDARD.decode(mcp_b64).unwrap();
		assert_eq!(
			decoded_mcp,
			br#"{"command":"node","args":["index.js"]}"#.to_vec()
		);
	}

	// export_bundle: include_mcp=false 时应只导出 Skill, Mcp 内容/计数均应被排除
	#[test]
	fn export_bundle_respects_include_flags_and_excludes_other_type() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.zip");

		let opts = ExportOptions {
			include_skills: true,
			include_mcp: false,
			scope: Scope::ByType,
			format: BundleFormat::Zip,
			include_config: false,
			include_version_lock: false,
		};
		let manifest = export_bundle(&conn, data_dir.path(), &opts, &out_path).unwrap();

		assert_eq!(manifest.counts.skill, 1);
		assert_eq!(manifest.counts.mcp, 0, "include_mcp=false 不应统计 Mcp");
		assert!(
			!manifest.checksums.contains_key("mcp/demo-mcp.json"),
			"不应包含被排除类型的内容"
		);
		assert!(manifest
			.checksums
			.contains_key("skills/demo-skill/SKILL.md"));
	}

	/// 在给定 conn 里登记一个 Agent, 返回其 agent_id, 供 include_config 相关测试构造真实可
	/// JOIN 的关联行
	fn seed_agent(conn: &Connection, name: &str) -> i64 {
		repo_agent::upsert(
			conn,
			&DetectedAgent {
				kind: AgentKind::ClaudeCode,
				name: name.to_string(),
				config_path: format!("/tmp/{name}.json"),
				scope: AgentScope::Global,
				online: true,
			},
		)
		.unwrap()
	}

	// export_bundle: include_config=true 时应附带 settings.json(setting 表整表)与
	// agents.json(仅本次导出资源的期望态关联), 且 Counts.config/agent 应对应写入的条数
	#[test]
	fn export_bundle_include_config_true_writes_settings_and_agents_json() {
		let conn = setup_conn();
		let (data_dir, skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		repo_setting::upsert(&conn, "net.proxy", "http://127.0.0.1:7890").unwrap();
		repo_setting::upsert(&conn, "sync.pref", "auto").unwrap();
		let agent_id = seed_agent(&conn, "Claude Code");
		repo_assoc::set(&conn, skill_id, agent_id, true).unwrap();

		let opts = ExportOptions {
			include_skills: true,
			include_mcp: true,
			scope: Scope::All,
			format: BundleFormat::Zip,
			include_config: true,
			include_version_lock: false,
		};
		let out_path = data_dir.path().join("out.zip");
		let manifest = export_bundle(&conn, data_dir.path(), &opts, &out_path).unwrap();

		assert_eq!(manifest.counts.config, 2, "两条 setting 记录");
		assert_eq!(manifest.counts.agent, 1, "一条期望态关联");
		assert!(manifest.checksums.contains_key("settings.json"));
		assert!(manifest.checksums.contains_key("agents.json"));

		let file = fs::File::open(&out_path).unwrap();
		let mut archive = zip::ZipArchive::new(file).unwrap();

		let mut settings_text = String::new();
		archive
			.by_name("settings.json")
			.unwrap()
			.read_to_string(&mut settings_text)
			.unwrap();
		let settings: BTreeMap<String, String> = serde_json::from_str(&settings_text).unwrap();
		assert_eq!(settings["net.proxy"], "http://127.0.0.1:7890");
		assert_eq!(settings["sync.pref"], "auto");

		let mut agents_text = String::new();
		archive
			.by_name("agents.json")
			.unwrap()
			.read_to_string(&mut agents_text)
			.unwrap();
		let links: Vec<serde_json::Value> = serde_json::from_str(&agents_text).unwrap();
		assert_eq!(links.len(), 1);
		assert_eq!(links[0]["resourceName"], "demo-skill");
		assert_eq!(links[0]["resType"], 1, "1-Skill");
		assert_eq!(links[0]["agentName"], "Claude Code");
	}

	// export_bundle: include_config=false 时不应附带 settings.json/agents.json, 即便库里
	// 确有设置项与关联存在
	#[test]
	fn export_bundle_include_config_false_omits_settings_and_agents_json() {
		let conn = setup_conn();
		let (data_dir, skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		repo_setting::upsert(&conn, "net.proxy", "http://127.0.0.1:7890").unwrap();
		let agent_id = seed_agent(&conn, "Claude Code");
		repo_assoc::set(&conn, skill_id, agent_id, true).unwrap();

		let out_path = data_dir.path().join("out.zip");
		let manifest = export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Zip),
			&out_path,
		)
		.unwrap();

		assert_eq!(manifest.counts.config, 0);
		assert_eq!(manifest.counts.agent, 0);
		assert!(!manifest.checksums.contains_key("settings.json"));
		assert!(!manifest.checksums.contains_key("agents.json"));
	}

	// export_bundle: include_version_lock=true 时 manifest.versions 应记录各资源的精确版本
	// (键为该资源在包内的相对根路径), 为 false 时应为空 map
	#[test]
	fn export_bundle_include_version_lock_controls_versions_map() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);

		let mut opts = full_options(BundleFormat::Zip);
		opts.include_version_lock = true;
		let out_path = data_dir.path().join("out.zip");
		let manifest = export_bundle(&conn, data_dir.path(), &opts, &out_path).unwrap();

		assert_eq!(manifest.versions.len(), 2);
		assert_eq!(manifest.versions["skills/demo-skill"], "1.2.0");
		assert_eq!(manifest.versions["mcp/demo-mcp.json"], "");
	}

	// export_bundle: 资源的 local_path 不在 data_dir 内(理论不应发生的脏数据)时应整体返回 Err,
	// 不应悄悄跳过导致用户误以为备份完整; 也不应产生任何历史记录
	#[test]
	fn export_bundle_errors_when_resource_local_path_outside_data_dir() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let outside = tempdir().unwrap();
		let stray_path = outside.path().join("stray.json");
		fs::write(&stray_path, "{}").unwrap();

		repo_resource::insert(
			&conn,
			&repo_resource::NewResource {
				res_type: ResourceType::Mcp,
				name: "stray-mcp".to_string(),
				display_name: "Stray MCP".to_string(),
				version: String::new(),
				source_type: SourceType::LocalImport,
				local_path: stray_path.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();

		let out_path = data_dir.path().join("out.zip");
		let result = export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Zip),
			&out_path,
		);

		assert!(result.is_err());
		assert!(repo_impexp::recent(&conn, 10).unwrap().is_empty());
	}

	// sha256_hex: 应匹配已知测试向量(空串/"abc", 经 shasum -a 256 交叉核实)
	#[test]
	fn sha256_hex_matches_known_test_vectors() {
		assert_eq!(
			sha256_hex(b""),
			"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
		);
		assert_eq!(
			sha256_hex(b"abc"),
			"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
		);
	}

	// format_rfc3339: 应匹配已知 unix 秒数对应的 UTC 时间(经 `date -u -r <secs>` 交叉核实)
	#[test]
	fn format_rfc3339_matches_known_unix_seconds() {
		assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
		assert_eq!(format_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
		assert_eq!(format_rfc3339(1_789_000_000), "2026-09-10T00:26:40Z");
	}
}
