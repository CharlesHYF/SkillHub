// 文件作用: 导入导出服务编排层(导出部分) —— 按 ExportReqVO 收集资源元数据与 data_dir 内容、
//           可选设置/关联、算 sha256 校验和、组装 manifest 并按 Zip/Tar/Json 三种格式打包,
//           写入 import_export_log(见 export_bundle)。只接受 &Connection 与 &Path(data_dir),
//           不摸 AppState/Tauri 运行时, 呼应 services::market/services::library 既有的分层约定。
//
//           关于 scope 与 include_skills/include_mcp 的关系: include_skills/include_mcp 是唯一
//           实际生效的类型开关(不论 scope 取何值均恒定生效, 见 collect_resources); scope=All 与
//           scope=ByType 在当前实现下语义等价(ByType 就是"正通过 include_skills/include_mcp 挑
//           类型"这一模式本身, 并不叠加额外过滤), scope=ByTime 因 ExportReqVO 未携带任何时间
//           范围字段, 暂等价于 All/ByType(留待后续任务若要真正实现"按时间"过滤, 需先给
//           ExportReqVO 增加时间范围字段, 再据 update_time 二次过滤)。
//
//           关于 include_config 同时门控 settings.json 与 agents.json: ExportReqVO 未单独
//           提供"是否包含资源-Agent 关联"的开关, brief 要求二者取一种关系并注释——这里选择让
//           agents.json 与 settings.json 共用 include_config 这一开关, 因为 SkillHub 语境下
//           "配置"泛指"本机如何使用这些资源"的状态, 既包含应用级设置(setting 表), 也包含
//           "期望哪些资源装到哪些 Agent"的关联关系(resource_agent 表), 二者都不是"资源内容
//           本身", 用同一开关归类是当前选项形状下最贴切的一种取舍。
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::prelude::*;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::domain::portability::{
	BundleFormat, ConflictStrategy, Counts, ExportReqVO, ImportOutcomeRespVO, ImportPreviewRespVO,
	ManifestRespVO,
};
use crate::domain::resource::{ResourceRespVO, ResourceType, SourceType};
use crate::infra::adapter::skill_target::parse_frontmatter_version;
use crate::infra::repo_agent;
use crate::infra::repo_assoc;
use crate::infra::repo_impexp;
use crate::infra::repo_resource::{self, ListFilter};
use crate::infra::repo_setting;
use crate::services::sync;

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
	manifest: ManifestRespVO,
	files: BTreeMap<String, String>,
}

/// 按 opts 收集参与导出的资源: include_skills/include_mcp 分别控制两种类型是否纳入(见文件头
/// 注释"关于 scope 与 include_skills/include_mcp 的关系"), 两者都为 false 时返回空列表, 不视为
/// 错误(导出一个空壳, 由调用方/前端决定是否阻止这种操作)。Skill 在前、Mcp 在后, 且各自按
/// repo_resource::list 既有的 id 升序返回, 整体顺序确定, 便于导出内容/manifest 可重现
fn collect_resources(conn: &Connection, opts: &ExportReqVO) -> Result<Vec<ResourceRespVO>> {
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
fn bundle_root_rel_path(data_dir: &Path, resource: &ResourceRespVO) -> Result<String> {
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
fn collect_agents_file(
	conn: &Connection,
	resources: &[ResourceRespVO],
) -> Result<(BundleFile, i64)> {
	let resource_by_id: BTreeMap<i64, &ResourceRespVO> =
		resources.iter().map(|r| (r.id, r)).collect();

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

/// 按 opts 收集资源/配置/关联并打包到 out_path, 返回打包清单(ManifestRespVO); 写一条导出方向
/// (direction=0)的 import_export_log 记录。纯同步, 不含网络 I/O, 全程持锁调用亦可(无 await,
/// 不涉及 commands::market 那种 Send 拆分顾虑, 见 commands::portability::export_bundle)
pub fn export_bundle(
	conn: &Connection,
	data_dir: &Path,
	opts: &ExportReqVO,
	out_path: &Path,
) -> Result<ManifestRespVO> {
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

	let manifest = ManifestRespVO {
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
fn write_zip(out_path: &Path, manifest: &ManifestRespVO, files: &[BundleFile]) -> Result<()> {
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
fn write_tar_gz(out_path: &Path, manifest: &ManifestRespVO, files: &[BundleFile]) -> Result<()> {
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
fn write_json_inline(
	out_path: &Path,
	manifest: &ManifestRespVO,
	files: &[BundleFile],
) -> Result<()> {
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

/// 取当前 UTC 时间拼一个 RFC3339 字符串(如 "2026-07-10T12:34:56Z"), 供 ManifestRespVO.exported_at;
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

// ============================================================================
// 导入解析与校验(M3 Task 3): 按魔数/扩展名识别 zip/tar.gz/json 三种归档格式其一, 解出
// manifest.json + 各条目内容后依次校验 schema 版本兼容、条目路径不发生 zip-slip 穿越、条目集合
// 与 manifest 记录一一对应、单文件/总量不超上限、逐条目 sha256 与 manifest.checksums 一致 ——
// 全程只读进内存, 不做任何磁盘落地; 是否、如何真正落地由 M3 Task 4 的 import_bundle 决定,
// 本节只负责"读进来 + 校验通过"这一步
// ============================================================================

/// 当前实现能理解的最高 schema_version: 与 export_bundle 恒定写出的版本一致(见本文件 export_bundle
/// 内 `schema_version: 1`)。导入包携带的版本一旦超过这个上限, 说明它出自更新版本的 SkillHub,
/// 可能带有本版本尚不认识的字段/语义, 宁可直接拒绝也不要硬着头皮按旧逻辑解析出一个可能残缺或
/// 错误的结果
const MAX_SUPPORTED_SCHEMA_VERSION: i64 = 1;

/// 单个条目允许的最大字节数: 远超真实 Skill/MCP 定义文件的体量(纯文本配置 + 少量脚本/资源,
/// 通常几 KB 到几 MB), 只用来挡下明显异常的超大条目(如刻意构造的解压炸弹), 不应卡到任何正常用量
const MAX_ENTRY_BYTES: u64 = 100 * 1024 * 1024;

/// 整个导入包解压后允许的总字节数上限, 同上给正常场景(几十个 Skill/MCP 打包在一起)预留充分余量
const MAX_TOTAL_BUNDLE_BYTES: u64 = 300 * 1024 * 1024;

/// 已解析且通过全部安全校验的导入包: 只在内存里持有 manifest 与各条目内容(键为包内相对路径,
/// 与 manifest.checksums 同一路径体系, 不含 manifest.json 本身), 未做任何磁盘落地 —— 是否、如何
/// 落地由 M3 Task 4 的 import_bundle 决定, 本类型只承载"已读进来 + 已验证安全/完整"这一状态
pub struct ParsedBundle {
	pub manifest: ManifestRespVO,
	pub entries: BTreeMap<String, Vec<u8>>,
	/// 导入包原始文件名(取自路径的 file_name, 取不到则为空串), 供 import_bundle(M3 Task 4)落地
	/// 时写入 import_export_log.file_name, 与 export_bundle 写入该列的既有惯例对称(见其文档)
	pub source_file_name: String,
	/// 导入包识别出的归档格式, 供 import_bundle(M3 Task 4)落地时写入 import_export_log.file_format
	pub source_format: BundleFormat,
}

/// 导入包三种受支持的归档格式, 对应 domain::portability::BundleFormat 的三个变体。单独建这个
/// 内部枚举而不直接复用 BundleFormat, 是因为识别阶段关心的是"这段字节该怎么解出 manifest+条目"
/// 这一纯技术判断, 与 BundleFormat 承载的"用户在导出面板选的格式"语义上是两回事
enum DetectedFormat {
	Zip,
	Tar,
	Json,
}

/// 识别导入包格式: 优先按文件内容魔数判断(zip 固定以 "PK\x03\x04" 开头, gzip 固定以 0x1f 0x8b
/// 开头), 不依赖调用方有没有把文件改成误导性的扩展名, 也更难被"伪装成别的格式"蒙混过关; 内容判断
/// 不出结果时(如空文件等极端情况)才退回按文件名扩展名兜底判断(大小写不敏感)。三种格式都判断不出
/// 时返回 Err, 而不是猜一个默认值硬解析
fn detect_bundle_format(path: &Path, bytes: &[u8]) -> Result<DetectedFormat> {
	if bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
		return Ok(DetectedFormat::Zip);
	}
	if bytes.starts_with(&[0x1f, 0x8b]) {
		return Ok(DetectedFormat::Tar);
	}
	if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{') {
		return Ok(DetectedFormat::Json);
	}

	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default()
		.to_lowercase();
	if file_name.ends_with(".zip") {
		return Ok(DetectedFormat::Zip);
	}
	if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
		return Ok(DetectedFormat::Tar);
	}
	if file_name.ends_with(".json") {
		return Ok(DetectedFormat::Json);
	}

	Err(anyhow!("无法识别的导入包格式: {}", path.display()))
}

/// zip-slip 防护核心判定: entry_path 是归档条目的原始名称(未经任何处理的不可信输入), 判定它
/// 是否可安全用作"目标根下的相对落地路径"。改用纯字符串运算, 不借助 std::path::Path 的
/// join/component 语义(那套语义随编译目标平台变化, 比如 Windows 盘符前缀只有编译到 windows 目标
/// 才会被识别成 Prefix 分量, 校验逻辑不应因运行平台不同而结论不同), 不触碰文件系统, 不要求路径
/// 真实存在, 可安全用于校验阶段:
///
/// 1) 显式前缀判定 —— 以 '/'、'\\' 开头, 或形如 "C:" 的盘符前缀, 一律视为绝对路径直接拒绝;
/// 2) ".." 零容忍 —— 按 '/' 与 '\\' 双分隔符切分后, 只要出现任何一个 ".." 分量即拒绝。
///    本应用自身导出的合法包(见 export_bundle 产出的 skills/<name>/..、mcp/<name>.json 等)绝不
///    含 "..", 故零容忍不会误杀合法条目; 而此前"能与前一个普通 segment 抵消就放行"的词法规范化
///    模型, 与下游 group_import_items 按 "skills/<name>/" 前缀二段式拆分再落地的真实消费方式不
///    匹配 —— 例如 "skills/../pwned/x" 规范化后看似仍在根内(旧逻辑放行), 但拆分出的 <name> 会是
///    "..", 拼成 data_dir/skills/.. 即指向 data_dir 自身, 触发 land_skill_content 的 remove_dir_all
///    清空整个数据目录(含 SQLite 库文件), 属灾难级不可恢复的数据丢失。故对 ".." 从校验入口即
///    零容忍堵死(另有 is_safe_landed_name 在落地前做纵深防御第二道)
fn is_entry_path_safe(entry_path: &str) -> bool {
	if entry_path.is_empty() {
		return false;
	}
	if entry_path.starts_with('/') || entry_path.starts_with('\\') {
		return false;
	}
	let bytes = entry_path.as_bytes();
	if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
		return false;
	}

	// 任何 ".." 分量一律拒绝(不再做"能抵消就放行"的规范化); 空/"." 分量无害, 忽略即可
	for segment in entry_path.split(['/', '\\']) {
		if segment == ".." {
			return false;
		}
	}
	true
}

/// 从 reader 里最多读取 cap+1 字节: 不信任归档头部自称的"解压后大小"字段(该字段本身可以被恶意
/// 构造的归档伪造, 真正起限制作用的必须是"实际读到多少字节"), 用 Read::take 硬性限制单次读取
/// 上限, 读满 cap+1 字节仍未到流末尾即视为超限, 直接返回 Err, 而不是把一整个解压炸弹先吞进内存
/// 再回头检查长度
fn read_capped<R: Read>(reader: R, cap: u64, what: &str) -> Result<Vec<u8>> {
	let mut limited = reader.take(cap + 1);
	let mut buf = Vec::new();
	limited
		.read_to_end(&mut buf)
		.with_context(|| format!("读取内容失败: {what}"))?;
	if buf.len() as u64 > cap {
		return Err(anyhow!("{what} 大小超过单文件上限({cap} 字节)"));
	}
	Ok(buf)
}

/// 校验条目总大小: 逐条目检查单文件上限, 同时累加已读总量并检查总量上限, 一旦命中任一上限立即
/// 短路返回 Err。上限以参数形式传入(而不是直接读全局常量), 便于测试用很小的自定义上限而不必真的
/// 构造出上百 MB 的内容
fn check_size_limits(
	entries: &BTreeMap<String, Vec<u8>>,
	max_entry_bytes: u64,
	max_total_bytes: u64,
) -> Result<()> {
	let mut total: u64 = 0;
	for (rel_path, content) in entries {
		let size = content.len() as u64;
		if size > max_entry_bytes {
			return Err(anyhow!(
				"导入包条目 {rel_path} 大小 {size} 字节超过单文件上限 {max_entry_bytes} 字节"
			));
		}
		total += size;
		if total > max_total_bytes {
			return Err(anyhow!("导入包总大小超过上限 {max_total_bytes} 字节"));
		}
	}
	Ok(())
}

/// 解析 Zip 格式导入包: 逐条目读取(目录条目跳过), 名为 "manifest.json" 的条目单独解析为
/// ManifestRespVO, 其余按原始条目名收进 entries; 单条目读取经 read_capped 限制在 MAX_ENTRY_BYTES 内,
/// 边读边累加总量, 一旦超过 MAX_TOTAL_BUNDLE_BYTES 立即失败, 不等全部读完再检查
fn parse_zip_bundle(bytes: &[u8]) -> Result<(ManifestRespVO, BTreeMap<String, Vec<u8>>)> {
	let cursor = Cursor::new(bytes);
	let mut archive = ZipArchive::new(cursor).context("zip 归档格式无效")?;

	let mut manifest: Option<ManifestRespVO> = None;
	let mut entries: BTreeMap<String, Vec<u8>> = BTreeMap::new();
	let mut total_size: u64 = 0;

	for i in 0..archive.len() {
		let mut file = archive.by_index(i).context("读取 zip 条目失败")?;
		if file.is_dir() {
			continue;
		}
		let name = file.name().to_string();
		let content = read_capped(&mut file, MAX_ENTRY_BYTES, &name)?;
		total_size += content.len() as u64;
		if total_size > MAX_TOTAL_BUNDLE_BYTES {
			return Err(anyhow!(
				"导入包总大小超过上限({MAX_TOTAL_BUNDLE_BYTES} 字节)"
			));
		}
		if name == "manifest.json" {
			manifest = Some(serde_json::from_slice(&content).context("解析 manifest.json 失败")?);
		} else {
			entries.insert(name, content);
		}
	}

	let manifest = manifest.ok_or_else(|| anyhow!("导入包缺少 manifest.json"))?;
	Ok((manifest, entries))
}

/// 解析 Tar.gz 格式导入包: 逐条目读取(目录条目跳过), 其余处理方式与 parse_zip_bundle 一致
fn parse_tar_gz_bundle(bytes: &[u8]) -> Result<(ManifestRespVO, BTreeMap<String, Vec<u8>>)> {
	let cursor = Cursor::new(bytes);
	let decoder = GzDecoder::new(cursor);
	let mut archive = tar::Archive::new(decoder);

	let mut manifest: Option<ManifestRespVO> = None;
	let mut entries: BTreeMap<String, Vec<u8>> = BTreeMap::new();
	let mut total_size: u64 = 0;

	for entry in archive.entries().context("读取 tar 归档失败")? {
		let mut entry = entry.context("读取 tar 条目失败")?;
		if entry.header().entry_type().is_dir() {
			continue;
		}
		let name = entry
			.path()
			.context("读取 tar 条目路径失败")?
			.to_string_lossy()
			.into_owned();
		let content = read_capped(&mut entry, MAX_ENTRY_BYTES, &name)?;
		total_size += content.len() as u64;
		if total_size > MAX_TOTAL_BUNDLE_BYTES {
			return Err(anyhow!(
				"导入包总大小超过上限({MAX_TOTAL_BUNDLE_BYTES} 字节)"
			));
		}
		if name == "manifest.json" {
			manifest = Some(serde_json::from_slice(&content).context("解析 manifest.json 失败")?);
		} else {
			entries.insert(name, content);
		}
	}

	let manifest = manifest.ok_or_else(|| anyhow!("导入包缺少 manifest.json"))?;
	Ok((manifest, entries))
}

/// 解析单文件 Json 格式导入包: 结构与 write_json_inline 写出的 JsonBundle 完全对称, files 里
/// 每项内容先 base64 解码, 再套用与 zip/tar 一致的大小上限检查
fn parse_json_bundle(bytes: &[u8]) -> Result<(ManifestRespVO, BTreeMap<String, Vec<u8>>)> {
	let bundle: JsonBundle = serde_json::from_slice(bytes).context("解析 Json 格式导入包失败")?;

	let mut entries: BTreeMap<String, Vec<u8>> = BTreeMap::new();
	let mut total_size: u64 = 0;
	for (rel_path, encoded) in bundle.files {
		let content = BASE64_STANDARD
			.decode(&encoded)
			.with_context(|| format!("解码条目 {rel_path} 的 base64 内容失败"))?;
		if content.len() as u64 > MAX_ENTRY_BYTES {
			return Err(anyhow!(
				"导入包条目 {rel_path} 大小超过单文件上限({MAX_ENTRY_BYTES} 字节)"
			));
		}
		total_size += content.len() as u64;
		if total_size > MAX_TOTAL_BUNDLE_BYTES {
			return Err(anyhow!(
				"导入包总大小超过上限({MAX_TOTAL_BUNDLE_BYTES} 字节)"
			));
		}
		entries.insert(rel_path, content);
	}

	Ok((bundle.manifest, entries))
}

/// 对已解出的 manifest + 条目做完整安全校验, 四类检查独立进行、任一失败即整体 Err(不做"部分通过"
/// 这种含糊状态), 按"访问代价从低到高"排序尽快短路失败: schema 版本(单一字段比较)→ 逐条目路径
/// 穿越(纯字符串运算)→ 条目数与 manifest 记录是否一一对应(集合大小比较, 顺带堵住"归档里夹带
/// manifest 未记录的额外文件"这个 manifest.checksums 逐项比对本身覆盖不到的缺口)→ 大小上限 →
/// 逐条目 sha256(需要对每个条目内容整体算一遍摘要, 最慢, 放最后)
fn validate_bundle(manifest: &ManifestRespVO, entries: &BTreeMap<String, Vec<u8>>) -> Result<()> {
	if manifest.schema_version > MAX_SUPPORTED_SCHEMA_VERSION {
		return Err(anyhow!(
			"导入包 schema_version={} 高于当前支持的最高版本 {MAX_SUPPORTED_SCHEMA_VERSION}, 请升级 SkillHub 后重试",
			manifest.schema_version
		));
	}

	for rel_path in entries.keys() {
		if !is_entry_path_safe(rel_path) {
			return Err(anyhow!(
				"导入包条目路径不安全, 疑似路径穿越攻击: {rel_path}"
			));
		}
	}

	if entries.len() != manifest.checksums.len() {
		return Err(anyhow!(
			"导入包内容条目数({})与 manifest 记录的条目数({})不一致, 疑似夹带未受控内容或缺失内容",
			entries.len(),
			manifest.checksums.len()
		));
	}

	check_size_limits(entries, MAX_ENTRY_BYTES, MAX_TOTAL_BUNDLE_BYTES)?;

	for (rel_path, expected_sha256) in &manifest.checksums {
		let content = entries
			.get(rel_path)
			.ok_or_else(|| anyhow!("导入包缺少 manifest 记录的条目: {rel_path}"))?;
		let actual_sha256 = sha256_hex(content);
		if &actual_sha256 != expected_sha256 {
			return Err(anyhow!(
				"导入包条目 {rel_path} 校验和不匹配, 疑似内容被篡改"
			));
		}
	}

	Ok(())
}

/// 解析并校验一个导入包(见文件本节开头说明): 按扩展名/魔数识别 zip/tar.gz/json 三种格式其一,
/// 解出 manifest.json + 其余各条目内容(全程只读进内存, 不落地到任何磁盘目录), 随后依次校验
/// schema 版本兼容、条目路径不发生 zip-slip 穿越、条目集合与 manifest 记录一致、大小不超上限、
/// 逐条目 sha256 与 manifest.checksums 一致 —— 任一校验不过即整体返回 Err 且不产生任何副作用
/// 文件(本函数与其调用的各解析/校验函数均只做内存读取与纯计算, 没有任何 fs::write/
/// fs::create_dir 之类的落地调用)
pub fn parse_bundle(path: &Path) -> Result<ParsedBundle> {
	let bytes = fs::read(path).with_context(|| format!("读取导入包失败: {}", path.display()))?;
	let format = detect_bundle_format(path, &bytes)?;

	let (manifest, entries) = match format {
		DetectedFormat::Zip => parse_zip_bundle(&bytes)?,
		DetectedFormat::Tar => parse_tar_gz_bundle(&bytes)?,
		DetectedFormat::Json => parse_json_bundle(&bytes)?,
	};

	validate_bundle(&manifest, &entries)?;

	let source_file_name = path
		.file_name()
		.map(|s| s.to_string_lossy().into_owned())
		.unwrap_or_default();
	let source_format = match format {
		DetectedFormat::Zip => BundleFormat::Zip,
		DetectedFormat::Tar => BundleFormat::Tar,
		DetectedFormat::Json => BundleFormat::Json,
	};

	Ok(ParsedBundle {
		manifest,
		entries,
		source_file_name,
		source_format,
	})
}

/// 由已通过校验的 ParsedBundle 生成"将导入内容"面板所需的预览: counts 直接取 manifest 已有统计
/// (导出时就已按类型计好, 见 export_bundle), schema_ok 独立重新判定一次 schema_version 是否仍在
/// 当前支持范围内 —— 理论上调用到这里时必然为 true(不兼容的包在 parse_bundle 内 validate_bundle
/// 阶段已经 Err, 根本不会走到这一步构造出 ParsedBundle), 之所以仍实打实算一遍而不直接硬编码
/// true, 是为了这个字段本身保持自解释, 不因未来 validate_bundle 校验策略调整(如放宽为警告而非
/// 硬错误)而变成一个名不副实的死字段
pub fn preview(parsed: &ParsedBundle) -> ImportPreviewRespVO {
	ImportPreviewRespVO {
		skill: parsed.manifest.counts.skill,
		mcp: parsed.manifest.counts.mcp,
		config: parsed.manifest.counts.config,
		agent: parsed.manifest.counts.agent,
		schema_ok: parsed.manifest.schema_version <= MAX_SUPPORTED_SCHEMA_VERSION,
	}
}

// ============================================================================
// 导入落地(M3 Task 4): 按 ConflictStrategy 把已解析且通过校验的 ParsedBundle 落地到 data_dir
// 与数据库 —— 内容路径分组(group_import_items, export_bundle 产出路径形状的精确逆过程)→
// 逐资源按策略落地(Overwrite 覆盖同名既有记录+内容; Skip 命中已存在同名资源则整体不动;
// KeepBoth 命中冲突则改名 `<name>-imported`/`-imported-2`… 后另起一行落地)→ 可选恢复
// settings.json/agents.json → 写 import_export_log(direction=1)。均不使用显式事务(与本仓库
// 其它多步落库操作同一既有取舍, 见 services::market::write_installed/services::library::
// import_local 均未包一层事务), 中途硬错误(如磁盘写入失败)会整体返回 Err 且不写历史记录,
// 但此前已落地的部分内容/数据库变更不会被回滚。
// ============================================================================

/// 分组后的一个待落地资源: 由 group_import_items 从 entries 的路径形状反推得到, name/res_type
/// 均为导入包记录的原始名称(改名与否留给 import_bundle 主循环按策略决定, 本类型不关心)
struct ImportItem {
	res_type: ResourceType,
	name: String,
	/// Skill: 该资源内部各文件相对其自身根目录的路径(如 "SKILL.md"/"scripts/run.sh") -> 内容,
	/// 可能多个; Mcp: 恒只有一个元素, 相对路径固定为空串占位(用不上, 见 land_mcp_content 只
	/// 取内容本身)
	files: Vec<(String, Vec<u8>)>,
}

/// 落地目录/文件名安全校验(zip-slip 纵深防御第二道): 即便 is_entry_path_safe 已在校验阶段对含
/// ".." 的条目零容忍, 落地前仍对"从条目路径按 skills/<name>/ 前缀二段式拆分出的资源名 <name>"
/// 独立再校验一次 —— 拒绝空串、"."、".." 以及内嵌 '/'、'\\' 分隔符者。杜绝 data_dir/skills/<name>
/// (或 data_dir/mcp/<name>.json)因 <name> 为 "."/".."/空而指向 data_dir 自身或其父级/子目录根,
/// 进而被 land_skill_content 的 remove_dir_all 误删的可能, 与校验入口(is_entry_path_safe)形成双保险
fn is_safe_landed_name(name: &str) -> bool {
	!name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

/// 从已解析导入包的 entries 里按 "skills/<name>/..." 与 "mcp/<name>.json" 两种既定路径形状,
/// 重新分组出每个资源各自的文件集合, 是 export_bundle 产出路径(bundle_root_rel_path/
/// collect_dir_files/collect_single_file)的精确逆过程; settings.json/agents.json 两个根级
/// 元数据文件不属于任何资源, 调用方应在调用本函数前先从 entries 里取出(见 import_bundle),
/// 本函数只按路径前缀匹配, 不识别的路径(理论不会出现, entries 已在 validate_bundle 阶段与
/// manifest.checksums 逐一核对过)静默忽略, 不视为错误。返回顺序按资源名升序(源 BTreeMap 迭代
/// 顺序天然有序), Skill 排在 Mcp 之前, 与 export_bundle 收集顺序一致, 便于结果确定可预期
fn group_import_items(entries: BTreeMap<String, Vec<u8>>) -> Vec<ImportItem> {
	let mut skills: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
	let mut mcps: BTreeMap<String, Vec<u8>> = BTreeMap::new();

	for (path, bytes) in entries {
		if let Some(rest) = path.strip_prefix("skills/") {
			if let Some((name, inner_rel)) = rest.split_once('/') {
				if is_safe_landed_name(name) {
					skills
						.entry(name.to_string())
						.or_default()
						.push((inner_rel.to_string(), bytes));
				}
			}
		} else if let Some(rest) = path.strip_prefix("mcp/") {
			if let Some(name) = rest.strip_suffix(".json") {
				// is_safe_landed_name 已涵盖"含分隔符"判定(原 !name.contains('/') 的超集)
				if is_safe_landed_name(name) {
					mcps.insert(name.to_string(), bytes);
				}
			}
		}
	}

	let mut items: Vec<ImportItem> = Vec::new();
	for (name, files) in skills {
		items.push(ImportItem {
			res_type: ResourceType::Skill,
			name,
			files,
		});
	}
	for (name, bytes) in mcps {
		items.push(ImportItem {
			res_type: ResourceType::Mcp,
			name,
			files: vec![(String::new(), bytes)],
		});
	}
	items
}

/// 按 (res_type, name) 精确查找既有资源(区别于 repo_resource::ListFilter.keyword 的模糊匹配):
/// 直接复用 repo_resource::list 按类型过滤后在内存里找精确同名项, 不新增仓储层方法 —— 依赖
/// uk_resource_type_name 唯一索引, 至多命中一条
fn find_existing_resource(
	conn: &Connection,
	res_type: ResourceType,
	name: &str,
) -> Result<Option<ResourceRespVO>> {
	Ok(repo_resource::list(
		conn,
		&ListFilter {
			res_type: Some(res_type),
			keyword: None,
		},
	)?
	.into_iter()
	.find(|r| r.name == name))
}

/// 为 KeepBoth 策略计算一个不冲突的新名称: 先试 `<name>-imported`, 仍冲突则依次尝试
/// `<name>-imported-2`、`<name>-imported-3`……直到找到本机尚不存在同类型同名资源的一个
fn next_available_name(conn: &Connection, res_type: ResourceType, name: &str) -> Result<String> {
	let first = format!("{name}-imported");
	if find_existing_resource(conn, res_type, &first)?.is_none() {
		return Ok(first);
	}
	let mut suffix = 2i64;
	loop {
		let candidate = format!("{name}-imported-{suffix}");
		if find_existing_resource(conn, res_type, &candidate)?.is_none() {
			return Ok(candidate);
		}
		suffix += 1;
	}
}

/// 落地一个 Skill 资源的内容到 data_dir/skills/<landed_name>/: 若该目录已存在残留内容(不论是否
/// 曾注册为资源)先整体清空再重建, 不做增量合并(与 services::market::write_skill_files/
/// services::library::import_skill 同一"整树覆盖"既有惯例), 再逐文件写入(files 的相对路径
/// 已在 group_import_items 阶段剥离 "skills/<name>/" 前缀); 返回落地后的完整目录路径
fn land_skill_content(
	data_dir: &Path,
	landed_name: &str,
	files: &[(String, Vec<u8>)],
) -> Result<PathBuf> {
	let target = data_dir.join("skills").join(landed_name);
	if target.exists() {
		fs::remove_dir_all(&target)
			.with_context(|| format!("清理旧 Skill 目录失败: {}", target.display()))?;
	}
	fs::create_dir_all(&target).with_context(|| format!("创建目录失败: {}", target.display()))?;
	for (rel_path, bytes) in files {
		let file_path = target.join(rel_path);
		if let Some(parent) = file_path.parent() {
			fs::create_dir_all(parent)
				.with_context(|| format!("创建目录失败: {}", parent.display()))?;
		}
		fs::write(&file_path, bytes)
			.with_context(|| format!("写入文件失败: {}", file_path.display()))?;
	}
	Ok(target)
}

/// 落地一个 Mcp 资源的内容到 data_dir/mcp/<landed_name>.json: 单文件整体覆盖写入(fs::write
/// 本就整体替换旧内容, 不需要 Skill 分支那样先删再建的额外步骤), 返回落地后的完整文件路径
fn land_mcp_content(data_dir: &Path, landed_name: &str, bytes: &[u8]) -> Result<PathBuf> {
	let mcp_dir = data_dir.join("mcp");
	fs::create_dir_all(&mcp_dir).with_context(|| format!("创建目录失败: {}", mcp_dir.display()))?;
	let target = mcp_dir.join(format!("{landed_name}.json"));
	fs::write(&target, bytes).with_context(|| format!("写入文件失败: {}", target.display()))?;
	Ok(target)
}

/// 解析某待落地资源应采用的版本号: 优先取 manifest.versions 里按其"导出时原始名称"记录的精确
/// 锁定值(仅 ExportReqVO.include_version_lock=true 时非空, 键为 "skills/<name>"/
/// "mcp/<name>.json", 与 export_bundle 写入该 map 时的键格式一致, 见其文档); 取不到时退回按
/// 内容自行解析 —— Skill 从其 SKILL.md 内容解析 frontmatter version(与本地导入
/// services::library::import_skill 同一逻辑, 见 parse_frontmatter_version), Mcp 恒为空串
/// (定义文件本身不携带版本概念, 与本地导入 services::library::import_mcp 同一惯例)。
/// 注意用的是资源"导出时的原始名称"(item.name, KeepBoth 改名前), 因为 manifest.versions 的键
/// 在导出那一刻就已固定, 与导入这一刻是否需要改名无关
fn resolve_version(manifest: &ManifestRespVO, item: &ImportItem) -> String {
	let root_key = match item.res_type {
		ResourceType::Skill => format!("skills/{}", item.name),
		ResourceType::Mcp => format!("mcp/{}.json", item.name),
	};
	if let Some(locked) = manifest.versions.get(&root_key) {
		return locked.clone();
	}
	match item.res_type {
		ResourceType::Skill => item
			.files
			.iter()
			.find(|(rel, _)| rel == "SKILL.md")
			.map(|(_, bytes)| parse_frontmatter_version(&String::from_utf8_lossy(bytes)))
			.unwrap_or_default(),
		ResourceType::Mcp => String::new(),
	}
}

/// 落地一个资源项(写 data_dir 内容 + upsert/insert resource 记录), 供 import_bundle 主循环
/// 的三种冲突策略分支共用: `landed_name` 为实际落地使用的名称(KeepBoth 命中冲突时是改名后的
/// 新名称, 其余场景恒等于 item.name); `existing` 非 None 时表示 Overwrite 策略命中了同名既有
/// 资源 —— 此时保留其 id 与 display_name 只更新 version/local_path(update_meta, 不删除重建,
/// 使已有的 resource_agent 关联/同步状态不因 id 变化而失联); 否则视为全新资源插入
/// (display_name 取 landed_name, 与 services::library::import_local"导入时没有额外展示名
/// 输入"的既有惯例一致)。source_type 统一落 LocalImport(内容来自本地导入包, 而非网络市场安装),
/// 返回落地后的资源 id
fn land_item(
	conn: &Connection,
	data_dir: &Path,
	manifest: &ManifestRespVO,
	item: &ImportItem,
	landed_name: &str,
	existing: Option<&ResourceRespVO>,
) -> Result<i64> {
	let version = resolve_version(manifest, item);
	let local_path = match item.res_type {
		ResourceType::Skill => land_skill_content(data_dir, landed_name, &item.files)?,
		ResourceType::Mcp => {
			let bytes = &item
				.files
				.first()
				.ok_or_else(|| anyhow!("Mcp 资源 {} 缺少内容", item.name))?
				.1;
			land_mcp_content(data_dir, landed_name, bytes)?
		}
	};
	let local_path = local_path.to_string_lossy().into_owned();

	match existing {
		Some(existing) => {
			repo_resource::update_meta(
				conn,
				existing.id,
				&repo_resource::ResourceMetaUpdate {
					display_name: existing.display_name.clone(),
					version,
					local_path,
				},
			)?;
			Ok(existing.id)
		}
		None => Ok(repo_resource::insert(
			conn,
			&repo_resource::NewResource {
				res_type: item.res_type,
				name: landed_name.to_string(),
				display_name: landed_name.to_string(),
				version,
				source_type: SourceType::LocalImport,
				local_path,
				enabled: true,
			},
		)?),
	}
}

/// 把 settings.json(内容为 cfg_key -> cfg_value 的扁平 JSON 对象, 与 collect_settings_file
/// 写出的形状一致)逐条 upsert 回 setting 表; 内容本身解析失败(结构不是预期的扁平对象)视为
/// 硬错误整体返回 Err —— 已通过 sha256 校验和验证的内容仍解析失败, 说明这是一个内容格式本身
/// 有问题的包, 不应放任, 参照 export_bundle 对"资源路径不在 data_dir 内"这类结构性问题的处理
/// 态度(整体失败, 不悄悄放过)
fn restore_settings(conn: &Connection, bytes: &[u8]) -> Result<()> {
	let map: BTreeMap<String, String> =
		serde_json::from_slice(bytes).context("解析 settings.json 失败")?;
	for (key, value) in map {
		repo_setting::upsert(conn, &key, &value)?;
	}
	Ok(())
}

/// 按名称在 agent 表里查找 Agent id: 供 restore_agent_links 按 agents.json 记录的展示名匹配
/// 本机 Agent(数据库自增 id 不跨机器可移植, 只能按名称尽力匹配, 见 export_bundle 内
/// AgentLinkExport 的文档), 找不到返回 None(不视为错误)
fn find_agent_by_name(conn: &Connection, name: &str) -> Result<Option<i64>> {
	Ok(repo_agent::list(conn)?
		.into_iter()
		.find(|agent| agent.name == name)
		.map(|agent| agent.id))
}

/// 恢复 agents.json 记录的资源-Agent 关联(resource_agent.desired=true): 记录里的资源按
/// (res_type, resource_name) 通过 `landed` 映射(本次导入实际落地的资源 id, 键为落地前的原始
/// 名称, 见 import_bundle 主循环)查到目标 resource_id —— 查不到说明该资源本次并未真正落地
/// (如 Skip 策略下被跳过的资源, "不改库"包含不为其恢复关联, 见 domain::portability::
/// ConflictStrategy 文档), 静默跳过且不计入"部分成功"判定(这是策略选择的预期结果, 不是一次
/// 尽力而未达成的失败); Agent 按 agent_name 在本机 agent 表按展示名查找, 找不到才计入"部分
/// 成功"(导出方与导入方是两台不同机器, 本机确实可能从未探测/登记过这个名字的 Agent, 属于
/// "尽力恢复, 找不到就跳过"的既有约定)。返回值为"是否全部记录的 Agent 都成功匹配", 供调用方
/// 据此决定 import_export_log 的 status
fn restore_agent_links(
	conn: &Connection,
	bytes: &[u8],
	landed: &BTreeMap<(ResourceType, String), i64>,
) -> Result<bool> {
	let links: Vec<AgentLinkExport> =
		serde_json::from_slice(bytes).context("解析 agents.json 失败")?;

	let mut all_agents_matched = true;
	for link in links {
		let res_type = ResourceType::from_i64(link.res_type);
		let Some(&resource_id) = landed.get(&(res_type, link.resource_name.clone())) else {
			continue;
		};
		match find_agent_by_name(conn, &link.agent_name)? {
			Some(agent_id) => {
				repo_assoc::set(conn, resource_id, agent_id, true)?;
			}
			None => all_agents_matched = false,
		}
	}
	Ok(all_agents_matched)
}

/// 按 strategy 把已解析且通过校验的导入包(parsed)落地到 data_dir 与数据库, 返回落地统计
/// (ImportOutcomeRespVO, 见其文档)。落地前先重新校验一遍(validate_bundle, 与 parse_bundle 内部的
/// 校验重复, 但 ParsedBundle 的字段均为 pub, 调用方理论上可绕开 parse_bundle 直接手工构造,
/// 这里多一道防线, 呼应 brief"先 validate_bundle 再落地"的顺序要求)。三种 ConflictStrategy
/// 的处理规则见 domain::portability::ConflictStrategy 文档与本函数内各分支注释; 落地后可选
/// 恢复 settings.json(整表 upsert)/agents.json(尽力恢复关联, 见 restore_agent_links), 最终写
/// 一条 direction=1(导入)的 import_export_log 记录。全程不使用显式事务, 见本节开头的模块级
/// 说明
pub fn import_bundle(
	conn: &Connection,
	data_dir: &Path,
	parsed: ParsedBundle,
	strategy: ConflictStrategy,
) -> Result<ImportOutcomeRespVO> {
	validate_bundle(&parsed.manifest, &parsed.entries)?;

	let ParsedBundle {
		manifest,
		mut entries,
		source_file_name,
		source_format,
	} = parsed;
	let settings_bytes = entries.remove("settings.json");
	let agents_bytes = entries.remove("agents.json");
	let items = group_import_items(entries);

	let mut imported = 0i64;
	let mut skipped = 0i64;
	let mut renamed = 0i64;
	// 原始名称(改名前) -> 落地后的实际资源 id, 供 restore_agent_links 定位关联目标;
	// Skip 策略下被跳过的资源不登记进这里(见 restore_agent_links 文档"不改库"的解读)
	let mut landed: BTreeMap<(ResourceType, String), i64> = BTreeMap::new();

	for item in &items {
		let original_key = (item.res_type, item.name.clone());
		match strategy {
			// Overwrite: 命中同名既有资源则原地更新(保留 id), 否则等同全新插入
			ConflictStrategy::Overwrite => {
				let existing = find_existing_resource(conn, item.res_type, &item.name)?;
				let resource_id = land_item(
					conn,
					data_dir,
					&manifest,
					item,
					&item.name,
					existing.as_ref(),
				)?;
				landed.insert(original_key, resource_id);
				imported += 1;
			}
			// Skip: 命中同名既有资源则整体不动(不写内容/不改库), 否则等同全新插入
			ConflictStrategy::Skip => {
				if find_existing_resource(conn, item.res_type, &item.name)?.is_some() {
					skipped += 1;
					continue;
				}
				let resource_id = land_item(conn, data_dir, &manifest, item, &item.name, None)?;
				landed.insert(original_key, resource_id);
				imported += 1;
			}
			// KeepBoth: 命中同名既有资源则改名后另起一行落地(原资源原样保留不动),
			// 否则等同全新插入
			ConflictStrategy::KeepBoth => {
				if find_existing_resource(conn, item.res_type, &item.name)?.is_some() {
					let unique_name = next_available_name(conn, item.res_type, &item.name)?;
					let resource_id =
						land_item(conn, data_dir, &manifest, item, &unique_name, None)?;
					landed.insert(original_key, resource_id);
					renamed += 1;
				} else {
					let resource_id = land_item(conn, data_dir, &manifest, item, &item.name, None)?;
					landed.insert(original_key, resource_id);
					imported += 1;
				}
			}
		}
	}

	if let Some(bytes) = settings_bytes {
		restore_settings(conn, &bytes)?;
	}

	let mut partial = false;
	if let Some(bytes) = agents_bytes {
		if !restore_agent_links(conn, &bytes, &landed)? {
			partial = true;
		}
	}

	let status = if partial { 2 } else { 1 };
	let summary = format!("{imported} 项导入, {skipped} 项跳过, {renamed} 项重命名");
	repo_impexp::add(
		conn,
		1,
		&source_file_name,
		i64::from(source_format),
		&summary,
		status,
	)?;

	Ok(ImportOutcomeRespVO {
		imported,
		skipped,
		renamed,
		status,
	})
}

/// 对本机全部"在线"(agent.status=true)的 Agent 逐一触发一次完整同步应用(见 services::sync::
/// apply_for_agent), 供 commands::portability::import_bundle 命令层 auto_sync=true 分支复用,
/// 也让这一步能脱离真实 tauri::State 直接单测(本仓库 commands 层普遍约定, 见
/// commands::market 文件头"均只接受 &Connection/&Path"的既有分层思路)。单个 Agent 应用失败
/// (理论只在 agent_id 已失效等结构性错误时发生, 常规的"某几项同步失败"已被 apply_for_agent
/// 内部吸收进其 SyncSummaryRespVO.failed, 不会以 Err 形式冒出来)不应拖累其它 Agent, 静默跳过继续
/// 处理下一个; 返回值为各在线 Agent(成功应用的那些)各自的 SyncSummaryRespVO, 供调用方按需查看/丢弃
pub fn sync_online_agents(conn: &Connection, home: &Path) -> Result<Vec<sync::SyncSummaryRespVO>> {
	let mut summaries = Vec::new();
	for agent in repo_agent::list(conn)? {
		if !agent.status {
			continue;
		}
		if let Ok(summary) = sync::apply_for_agent(conn, home, agent.id) {
			summaries.push(summary);
		}
	}
	Ok(summaries)
}

#[cfg(test)]
mod tests {
	use std::io::Read as _;

	use tempfile::tempdir;

	use super::*;
	use crate::domain::agent::{AgentKind, AgentScope, DetectedAgent};
	use crate::domain::portability::Scope;
	use crate::domain::resource::SourceType;
	use crate::infra::{repo_agent, repo_setting, repo_sync};

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

	fn full_options(format: BundleFormat) -> ExportReqVO {
		ExportReqVO {
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
		let manifest_in_zip: ManifestRespVO = serde_json::from_str(&manifest_text).unwrap();
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

		let manifest_in_tar: ManifestRespVO =
			serde_json::from_slice(&seen["manifest.json"]).unwrap();
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

		let manifest_in_json: ManifestRespVO =
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

		let opts = ExportReqVO {
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

		let opts = ExportReqVO {
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

	// ========================================================================
	// 导入解析与校验(M3 Task 3): is_entry_path_safe / read_capped / check_size_limits /
	// validate_bundle 的纯逻辑单测在前(不需要构造任何真实归档, 快且能精确覆盖每个判定分支);
	// parse_bundle 端到端测试在后(正常包三格式往返 + 篡改校验和 + 恶意包 zip-slip + schema 过高,
	// 均需要真实归档字节, 见 build_raw_zip_bytes/build_malicious_tar_gz_bytes 两个测试专用夹具
	// 构造函数及其文档 —— zip/tar 官方写入 API 均会主动拒绝/规范化穿越路径, 无法用于构造恶意包,
	// 只能绕开各自的高层 API 直接拼裸字节)
	// ========================================================================

	// 造一份内容计数与 versions/checksums 均为空的最小 ManifestRespVO, 只有 schema_version 可指定,
	// 供只关心 schema_version/checksums 这两个维度的校验类测试复用, 不必每次手写全部字段
	fn empty_manifest(schema_version: i64) -> ManifestRespVO {
		ManifestRespVO {
			schema_version,
			exported_at: "2026-07-10T00:00:00Z".to_string(),
			counts: Counts {
				skill: 0,
				mcp: 0,
				config: 0,
				agent: 0,
			},
			versions: BTreeMap::new(),
			checksums: BTreeMap::new(),
		}
	}

	// 序列化一份 schema_version=1、内容为空的最小合法 manifest.json 字节, 供恶意 zip/tar 测试
	// 夹具当作 "manifest.json" 条目内容使用(这些测试关心的是恶意条目本身能否被拒绝, 不关心
	// manifest 内容, 用最小合法值即可)
	fn minimal_manifest_json_bytes() -> Vec<u8> {
		serde_json::to_vec(&empty_manifest(1)).unwrap()
	}

	// is_entry_path_safe: 正常相对路径(合法包实际出现的形状)应放行
	#[test]
	fn is_entry_path_safe_accepts_normal_relative_paths() {
		assert!(is_entry_path_safe("skills/demo-skill/SKILL.md"));
		assert!(is_entry_path_safe("mcp/demo-mcp.json"));
		assert!(is_entry_path_safe("manifest.json"));
		assert!(is_entry_path_safe("a/b/c/d.txt"));
	}

	// is_entry_path_safe: 对 ".." 零容忍 —— 即便"抵消后净路径仍在根内"的中段 ".." 也必须拒绝。
	// 这类路径正是能骗过旧规范化模型、经 group_import_items 拆出 <name>=".." 从而 remove_dir_all
	// 清空整个 data_dir 的攻击载荷(见 is_entry_path_safe 文档与 land_skill_content), 是本条防线的
	// 核心回归用例(此前旧实现把第一条断言判为 safe, 即漏洞根因)
	#[test]
	fn is_entry_path_safe_rejects_any_dotdot_segment_even_if_net_in_root() {
		assert!(!is_entry_path_safe("skills/../pwned/SKILL.md"));
		assert!(!is_entry_path_safe("skills/demo/../other/SKILL.md"));
		assert!(!is_entry_path_safe("skills/.."));
		assert!(!is_entry_path_safe("skills/..\\pwned/x"));
	}

	// is_entry_path_safe: Unix 绝对路径(以 '/' 开头)应拒绝, 对应 brief 的 "/etc/evil" 用例
	#[test]
	fn is_entry_path_safe_rejects_absolute_unix_path() {
		assert!(!is_entry_path_safe("/etc/evil"));
	}

	// is_entry_path_safe: 起手即向上逃逸应拒绝, 对应 brief 的 "../../evil.txt" 用例
	#[test]
	fn is_entry_path_safe_rejects_leading_parent_dir_escape() {
		assert!(!is_entry_path_safe("../../evil.txt"));
	}

	// is_entry_path_safe: 前面先有正常 segment、随后仍向上跳出根的 .. 也应拒绝(不能只看开头)
	#[test]
	fn is_entry_path_safe_rejects_parent_dir_escape_after_normal_segments() {
		assert!(!is_entry_path_safe("skills/../../evil.txt"));
	}

	// is_entry_path_safe: Windows 盘符前缀(无论后面接反斜杠还是相对该盘符)均视为绝对路径拒绝
	#[test]
	fn is_entry_path_safe_rejects_windows_drive_letter_prefix() {
		assert!(!is_entry_path_safe("C:\\evil.txt"));
		assert!(!is_entry_path_safe("C:evil.txt"));
	}

	// is_entry_path_safe: 反斜杠打头(Windows 风格的根)也应视为绝对路径拒绝
	#[test]
	fn is_entry_path_safe_rejects_leading_backslash() {
		assert!(!is_entry_path_safe("\\evil.txt"));
	}

	// is_entry_path_safe: 空路径没有意义, 应拒绝
	#[test]
	fn is_entry_path_safe_rejects_empty_path() {
		assert!(!is_entry_path_safe(""));
	}

	// group_import_items: 纵深防御 —— 即便不安全的 <name>(".."/"."/空/含分隔符)侥幸到达分组阶段,
	// 也应被 is_safe_landed_name 跳过, 绝不产出以其为落地名的 ImportItem(否则 land_skill_content
	// 会对 data_dir/skills/<name> 执行 remove_dir_all, <name> 为 "."/".." 时即清空 skills 根乃至
	// 整个 data_dir); 同时保证合法项不被误伤
	#[test]
	fn group_import_items_skips_unsafe_landed_names() {
		let mut entries: BTreeMap<String, Vec<u8>> = BTreeMap::new();
		entries.insert("skills/../evil/SKILL.md".to_string(), b"x".to_vec());
		entries.insert("skills/./evil2/SKILL.md".to_string(), b"x".to_vec());
		entries.insert("skills//empty-name.md".to_string(), b"x".to_vec());
		entries.insert("mcp/../evil.json".to_string(), b"x".to_vec());
		// 合法对照项, 确保过滤未误伤正常路径
		entries.insert("skills/good/SKILL.md".to_string(), b"x".to_vec());

		let items = group_import_items(entries);

		assert!(
			items
				.iter()
				.all(|it| it.name != ".." && it.name != "." && !it.name.is_empty()),
			"不安全的落地名不应产出任何 ImportItem"
		);
		assert!(
			items.iter().any(|it| it.name == "good"),
			"合法项 good 应正常保留"
		);
	}

	// read_capped: 内容恰好等于上限应放行(边界值不应被误杀)
	#[test]
	fn read_capped_ok_when_content_exactly_at_cap() {
		let data = vec![7u8; 10];
		let result = read_capped(Cursor::new(data), 10, "test").unwrap();
		assert_eq!(result.len(), 10);
	}

	// read_capped: 内容超过上限应报错, 而不是静默截断
	#[test]
	fn read_capped_errs_when_content_exceeds_cap() {
		let data = vec![7u8; 11];
		let result = read_capped(Cursor::new(data), 10, "test");
		assert!(result.is_err());
	}

	// check_size_limits: 单个条目超过单文件上限应报错
	#[test]
	fn check_size_limits_errs_when_single_entry_exceeds_cap() {
		let mut entries = BTreeMap::new();
		entries.insert("big.bin".to_string(), vec![0u8; 11]);
		assert!(check_size_limits(&entries, 10, 1_000).is_err());
	}

	// check_size_limits: 单个条目都不超限, 但总量超过总上限也应报错
	#[test]
	fn check_size_limits_errs_when_total_exceeds_cap_even_if_each_entry_is_within_single_limit() {
		let mut entries = BTreeMap::new();
		entries.insert("a.bin".to_string(), vec![0u8; 6]);
		entries.insert("b.bin".to_string(), vec![0u8; 6]);
		assert!(check_size_limits(&entries, 10, 10).is_err());
	}

	// check_size_limits: 恰好等于两个上限均应放行(边界值不应被误杀)
	#[test]
	fn check_size_limits_ok_when_exactly_at_both_caps() {
		let mut entries = BTreeMap::new();
		entries.insert("a.bin".to_string(), vec![0u8; 10]);
		assert!(check_size_limits(&entries, 10, 10).is_ok());
	}

	// validate_bundle: schema_version 高于当前支持上限应报错
	#[test]
	fn validate_bundle_errs_when_schema_version_exceeds_supported_max() {
		let manifest = empty_manifest(MAX_SUPPORTED_SCHEMA_VERSION + 1);
		let entries = BTreeMap::new();
		assert!(validate_bundle(&manifest, &entries).is_err());
	}

	// validate_bundle: 条目内容与 manifest.checksums 记录的摘要不符(篡改)应报错
	#[test]
	fn validate_bundle_errs_when_checksum_does_not_match_entry_content() {
		let mut manifest = empty_manifest(1);
		manifest
			.checksums
			.insert("skills/demo/SKILL.md".to_string(), "wrong-hash".to_string());
		let mut entries = BTreeMap::new();
		entries.insert("skills/demo/SKILL.md".to_string(), b"real content".to_vec());
		assert!(validate_bundle(&manifest, &entries).is_err());
	}

	// validate_bundle: manifest.checksums 记录的条目在 entries 里缺失应报错, 不能悄悄放行
	#[test]
	fn validate_bundle_errs_when_entry_referenced_by_manifest_is_missing() {
		let mut manifest = empty_manifest(1);
		manifest
			.checksums
			.insert("skills/demo/SKILL.md".to_string(), sha256_hex(b"content"));
		let entries = BTreeMap::new();
		assert!(validate_bundle(&manifest, &entries).is_err());
	}

	// validate_bundle: entries 里存在 manifest.checksums 未记录的额外文件应报错(即便该额外文件
	// 本身路径安全、大小正常), 堵住"逐项比对 checksums"本身覆盖不到的夹带缺口
	#[test]
	fn validate_bundle_errs_when_entries_contain_extra_file_not_covered_by_manifest_checksums() {
		let manifest = empty_manifest(1);
		let mut entries = BTreeMap::new();
		entries.insert("sneaky.txt".to_string(), b"whatever".to_vec());
		assert!(validate_bundle(&manifest, &entries).is_err());
	}

	// validate_bundle: entries 里含路径不安全的条目(即便凑巧不在 checksums 里)应报错
	#[test]
	fn validate_bundle_errs_when_an_entry_path_is_unsafe() {
		let mut manifest = empty_manifest(1);
		manifest
			.checksums
			.insert("../../evil.txt".to_string(), sha256_hex(b"pwned"));
		let mut entries = BTreeMap::new();
		entries.insert("../../evil.txt".to_string(), b"pwned".to_vec());
		assert!(validate_bundle(&manifest, &entries).is_err());
	}

	// validate_bundle: schema/路径/条目集合/校验和均一致时应整体放行
	#[test]
	fn validate_bundle_ok_when_everything_matches() {
		let mut manifest = empty_manifest(1);
		manifest.checksums.insert(
			"skills/demo/SKILL.md".to_string(),
			sha256_hex(b"real content"),
		);
		let mut entries = BTreeMap::new();
		entries.insert("skills/demo/SKILL.md".to_string(), b"real content".to_vec());
		assert!(validate_bundle(&manifest, &entries).is_ok());
	}

	// preview: 应原样映射 manifest.counts 四个计数字段, 且 schema_ok 应为 true(未超支持上限)
	#[test]
	fn preview_maps_manifest_counts_and_schema_ok_flag() {
		let parsed = ParsedBundle {
			manifest: ManifestRespVO {
				counts: Counts {
					skill: 2,
					mcp: 1,
					config: 1,
					agent: 3,
				},
				..empty_manifest(1)
			},
			entries: BTreeMap::new(),
			source_file_name: String::new(),
			source_format: BundleFormat::Zip,
		};
		let result = preview(&parsed);
		assert_eq!(result.skill, 2);
		assert_eq!(result.mcp, 1);
		assert_eq!(result.config, 1);
		assert_eq!(result.agent, 3);
		assert!(result.schema_ok);
	}

	// crc32: 计算标准 CRC-32(IEEE 802.3, zip/gzip 通用多项式 0xEDB88320): 逐字节逐位计算,
	// 不追求性能(只在测试里为手工拼装的裸 zip 字节计算正确 crc, 数据量都是几十字节级别), 换来
	// 不必新增一个仅供测试使用的 crc32 三方库依赖
	fn crc32(data: &[u8]) -> u32 {
		let mut crc: u32 = 0xFFFF_FFFF;
		for &byte in data {
			crc ^= byte as u32;
			for _ in 0..8 {
				let mask = 0u32.wrapping_sub(crc & 1);
				crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
			}
		}
		!crc
	}

	// crc32: 用标准检验值交叉核实手写实现是否正确("123456789" -> 0xCBF43926 是 CRC-32/ISO-HDLC
	// 的公开标准检验值, 空串为 0)
	#[test]
	fn crc32_matches_known_check_value() {
		assert_eq!(crc32(b""), 0);
		assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
	}

	/// 手工按 ZIP 文件格式(Stored/不压缩, 无 zip64)拼裸字节: 不经过 zip crate 的 ZipWriter,
	/// 因为其 start_file 会主动规范化/过滤掉 ".." 与盘符前缀(见 zip::write::ZipWriter::
	/// start_file 文档 "ignores any '..' or Windows drive letter that would produce a path
	/// outside the ZIP file's"), 想在测试里构造一个"条目名真的带逃逸路径"的恶意包, 只能绕开这层
	/// 过滤自己拼字节。依次写: 每个条目的本地文件头+数据, 再写中央目录, 最后写目录结束记录,
	/// 均为小端序, 字段顺序/长度均严格对照 ZIP 格式规范(PKWARE APPNOTE 4.3.7/4.3.12/4.3.16);
	/// crc32 用上面手写的实现算真实值, 保证这份测试夹具本身是一份完全合规、能被任何标准 zip
	/// 实现打开的归档, 不必依赖"读取到一半就失败"这种更脆弱的前提
	fn build_raw_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
		let mut out = Vec::new();
		let mut offsets = Vec::new();

		for (name, data) in entries {
			offsets.push(out.len() as u32);
			let name_bytes = name.as_bytes();
			out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
			out.extend_from_slice(&20u16.to_le_bytes());
			out.extend_from_slice(&0u16.to_le_bytes());
			out.extend_from_slice(&0u16.to_le_bytes());
			out.extend_from_slice(&0u16.to_le_bytes());
			out.extend_from_slice(&0u16.to_le_bytes());
			out.extend_from_slice(&crc32(data).to_le_bytes());
			out.extend_from_slice(&(data.len() as u32).to_le_bytes());
			out.extend_from_slice(&(data.len() as u32).to_le_bytes());
			out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
			out.extend_from_slice(&0u16.to_le_bytes());
			out.extend_from_slice(name_bytes);
			out.extend_from_slice(data);
		}

		let central_start = out.len() as u32;
		let mut central = Vec::new();
		for ((name, data), offset) in entries.iter().zip(offsets.iter()) {
			let name_bytes = name.as_bytes();
			central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
			central.extend_from_slice(&20u16.to_le_bytes());
			central.extend_from_slice(&20u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&crc32(data).to_le_bytes());
			central.extend_from_slice(&(data.len() as u32).to_le_bytes());
			central.extend_from_slice(&(data.len() as u32).to_le_bytes());
			central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u16.to_le_bytes());
			central.extend_from_slice(&0u32.to_le_bytes());
			central.extend_from_slice(&offset.to_le_bytes());
			central.extend_from_slice(name_bytes);
		}
		out.extend_from_slice(&central);

		out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
		out.extend_from_slice(&0u16.to_le_bytes());
		out.extend_from_slice(&0u16.to_le_bytes());
		out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
		out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
		out.extend_from_slice(&(central.len() as u32).to_le_bytes());
		out.extend_from_slice(&central_start.to_le_bytes());
		out.extend_from_slice(&0u16.to_le_bytes());

		out
	}

	/// 手工拼一个含恶意条目的 tar.gz: manifest.json 走正常 append_data(路径本身安全, 不需要
	/// 绕过校验); 恶意条目则绕开 tar::Header::set_path —— 该方法与 Builder::append_data 会拒绝
	/// 含 ParentDir/RootDir 分量的路径(见 tar crate header.rs 里 copy_path_into 的文档 "an
	/// invalid path component is encountered (e.g. a root path or parent dir)"), 没法通过公开
	/// 写入 API 拼出真正带穿越路径的条目, 只能直接把裸字节写进 header 的 name 字段(该字段是
	/// pub [u8;100], 见 tar::GnuHeader), 再用 Builder::append(不像 append_data 会先调用
	/// prepare_header_path 校验/重写路径)按裸 header 写入
	fn build_malicious_tar_gz_bytes(
		manifest_bytes: &[u8],
		malicious_name: &str,
		malicious_data: &[u8],
	) -> Vec<u8> {
		let buf: Vec<u8> = Vec::new();
		let encoder = GzEncoder::new(buf, Compression::default());
		let mut builder = tar::Builder::new(encoder);

		let mut manifest_header = tar::Header::new_gnu();
		manifest_header.set_size(manifest_bytes.len() as u64);
		manifest_header.set_mode(0o644);
		builder
			.append_data(&mut manifest_header, "manifest.json", manifest_bytes)
			.unwrap();

		let mut evil_header = tar::Header::new_gnu();
		evil_header.set_size(malicious_data.len() as u64);
		evil_header.set_mode(0o644);
		{
			let name_field = &mut evil_header
				.as_gnu_mut()
				.expect("new_gnu 产出的头一定是 gnu 格式")
				.name;
			*name_field = [0u8; 100];
			let raw = malicious_name.as_bytes();
			name_field[..raw.len()].copy_from_slice(raw);
		}
		evil_header.set_cksum();
		builder.append(&evil_header, malicious_data).unwrap();

		let encoder = builder.into_inner().unwrap();
		encoder.finish().unwrap()
	}

	// parse_bundle(Zip): 用 Task 2 的 export_bundle 产出一份真实 zip 再 parse, 预览计数应与
	// export 时的 manifest.counts 一致, schema_ok 应为 true
	#[test]
	fn parse_bundle_zip_round_trips_export_output_with_correct_preview() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.zip");
		export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Zip),
			&out_path,
		)
		.unwrap();

		let parsed = parse_bundle(&out_path).unwrap();
		let result = preview(&parsed);
		assert_eq!(result.skill, 1);
		assert_eq!(result.mcp, 1);
		assert_eq!(result.config, 0);
		assert_eq!(result.agent, 0);
		assert!(result.schema_ok);
	}

	// parse_bundle(Tar): 同上, 换 Tar 格式往返
	#[test]
	fn parse_bundle_tar_round_trips_export_output_with_correct_preview() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.tar.gz");
		export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Tar),
			&out_path,
		)
		.unwrap();

		let parsed = parse_bundle(&out_path).unwrap();
		let result = preview(&parsed);
		assert_eq!(result.skill, 1);
		assert_eq!(result.mcp, 1);
		assert!(result.schema_ok);
	}

	// parse_bundle(Json): 同上, 换单文件 Json 格式往返
	#[test]
	fn parse_bundle_json_round_trips_export_output_with_correct_preview() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.json");
		export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Json),
			&out_path,
		)
		.unwrap();

		let parsed = parse_bundle(&out_path).unwrap();
		let result = preview(&parsed);
		assert_eq!(result.skill, 1);
		assert_eq!(result.mcp, 1);
		assert!(result.schema_ok);
	}

	// parse_bundle(Json): 篡改一个文件内容但不同步更新 manifest.checksums(模拟包在传输/存储中
	// 被篡改), 应被 sha256 比对识破而报错; 用 Json 格式篡改最直接(单文件文本, 改一个 base64 字段
	// 即可), 不需要在压缩二进制里做手术
	#[test]
	fn parse_bundle_errs_when_a_file_content_is_tampered_but_manifest_checksum_unchanged() {
		let conn = setup_conn();
		let (data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&conn);
		let out_path = data_dir.path().join("out.json");
		export_bundle(
			&conn,
			data_dir.path(),
			&full_options(BundleFormat::Json),
			&out_path,
		)
		.unwrap();

		let text = fs::read_to_string(&out_path).unwrap();
		let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
		let tampered = BASE64_STANDARD.encode(b"tampered content");
		value["files"]["skills/demo-skill/SKILL.md"] = serde_json::Value::String(tampered);
		fs::write(&out_path, serde_json::to_string(&value).unwrap()).unwrap();

		let result = parse_bundle(&out_path);
		assert!(result.is_err(), "篡改内容后应因校验和不符而报错");
	}

	// parse_bundle(Zip 恶意包): 条目路径为 "../../evil.txt"(brief 明确给出的用例)应被拒绝,
	// 且恶意包所在目录、以及一个完全无关的"目标根"临时目录均不应出现任何新文件 —— parse_bundle
	// 本身不接收任何落地目标路径, 全程只读进内存, 这里额外断言一个模拟的目标根目录仍为空, 是对
	// "不产生任何副作用文件"这一约束的双重确认
	#[test]
	fn parse_bundle_zip_rejects_parent_dir_escape_entry_and_writes_nothing() {
		let manifest_bytes = minimal_manifest_json_bytes();
		let zip_bytes = build_raw_zip_bytes(&[
			("manifest.json", &manifest_bytes),
			("../../evil.txt", b"pwned"),
		]);
		let bundle_dir = tempdir().unwrap();
		let bundle_path = bundle_dir.path().join("evil.zip");
		fs::write(&bundle_path, &zip_bytes).unwrap();
		let target_root = tempdir().unwrap();

		let result = parse_bundle(&bundle_path);

		assert!(result.is_err(), "含 ../../evil.txt 逃逸路径的包应被拒绝");
		assert_eq!(
			fs::read_dir(bundle_dir.path()).unwrap().count(),
			1,
			"恶意包所在目录不应被写入任何额外文件"
		);
		assert_eq!(
			fs::read_dir(target_root.path()).unwrap().count(),
			0,
			"目标根不应被写入任何逃逸文件"
		);
	}

	// parse_bundle(Zip 恶意包): 条目路径为绝对路径 "/etc/evil"(brief 明确给出的另一用例)应被
	// 拒绝, 断言方式同上
	#[test]
	fn parse_bundle_zip_rejects_absolute_path_entry_and_writes_nothing() {
		let manifest_bytes = minimal_manifest_json_bytes();
		let zip_bytes =
			build_raw_zip_bytes(&[("manifest.json", &manifest_bytes), ("/etc/evil", b"pwned")]);
		let bundle_dir = tempdir().unwrap();
		let bundle_path = bundle_dir.path().join("evil.zip");
		fs::write(&bundle_path, &zip_bytes).unwrap();
		let target_root = tempdir().unwrap();

		let result = parse_bundle(&bundle_path);

		assert!(result.is_err(), "含绝对路径 /etc/evil 的包应被拒绝");
		assert_eq!(
			fs::read_dir(bundle_dir.path()).unwrap().count(),
			1,
			"恶意包所在目录不应被写入任何额外文件"
		);
		assert_eq!(
			fs::read_dir(target_root.path()).unwrap().count(),
			0,
			"目标根不应被写入任何逃逸文件"
		);
	}

	// parse_bundle(Tar 恶意包): 证明 zip-slip 防护同样适用于 tar.gz 格式, 不是只在 zip 分支
	// 生效; 用例同样取 brief 给出的 "../../evil.txt"
	#[test]
	fn parse_bundle_tar_rejects_parent_dir_escape_entry_and_writes_nothing() {
		let manifest_bytes = minimal_manifest_json_bytes();
		let tar_bytes = build_malicious_tar_gz_bytes(&manifest_bytes, "../../evil.txt", b"pwned");
		let bundle_dir = tempdir().unwrap();
		let bundle_path = bundle_dir.path().join("evil.tar.gz");
		fs::write(&bundle_path, &tar_bytes).unwrap();
		let target_root = tempdir().unwrap();

		let result = parse_bundle(&bundle_path);

		assert!(result.is_err(), "含 ../../evil.txt 逃逸路径的包应被拒绝");
		assert_eq!(
			fs::read_dir(bundle_dir.path()).unwrap().count(),
			1,
			"恶意包所在目录不应被写入任何额外文件"
		);
		assert_eq!(
			fs::read_dir(target_root.path()).unwrap().count(),
			0,
			"目标根不应被写入任何逃逸文件"
		);
	}

	// parse_bundle(Json 恶意包): 证明 zip-slip 防护同样适用于 Json 格式(files 的 key 本身就是
	// 条目路径, 无需任何归档格式配合即可直接夹带), 不是只在二进制归档格式生效
	#[test]
	fn parse_bundle_json_rejects_absolute_path_key_and_writes_nothing() {
		let mut files = BTreeMap::new();
		files.insert("/etc/evil".to_string(), BASE64_STANDARD.encode(b"pwned"));
		let bundle = JsonBundle {
			manifest: empty_manifest(1),
			files,
		};
		let text = serde_json::to_string(&bundle).unwrap();

		let bundle_dir = tempdir().unwrap();
		let bundle_path = bundle_dir.path().join("evil.json");
		fs::write(&bundle_path, &text).unwrap();
		let target_root = tempdir().unwrap();

		let result = parse_bundle(&bundle_path);

		assert!(result.is_err(), "含绝对路径 key 的包应被拒绝");
		assert_eq!(
			fs::read_dir(bundle_dir.path()).unwrap().count(),
			1,
			"恶意包所在目录不应被写入任何额外文件"
		);
		assert_eq!(
			fs::read_dir(target_root.path()).unwrap().count(),
			0,
			"目标根不应被写入任何逃逸文件"
		);
	}

	// parse_bundle: schema_version 高于当前支持上限的包应被拒绝(用 Json 格式手工构造, 不依赖
	// 归档格式本身)
	#[test]
	fn parse_bundle_errs_when_schema_version_exceeds_supported_max() {
		let bundle = JsonBundle {
			manifest: empty_manifest(MAX_SUPPORTED_SCHEMA_VERSION + 1),
			files: BTreeMap::new(),
		};
		let text = serde_json::to_string(&bundle).unwrap();
		let dir = tempdir().unwrap();
		let path = dir.path().join("future.json");
		fs::write(&path, text).unwrap();

		let result = parse_bundle(&path);
		assert!(result.is_err());
	}

	// ========================================================================
	// 导入落地(M3 Task 4): build_parsed_bundle 手工构造 ParsedBundle(不必每次都真的走一遍
	// zip 打包/解析), 覆盖三种 ConflictStrategy 的冲突/无冲突分支、agents.json/settings.json
	// 恢复、以及 auto_sync 依赖的 sync_online_agents; 最后用一次真正经 export_bundle ->
	// parse_bundle 的完整往返测试收尾
	// ========================================================================

	/// 手工拼一份 ParsedBundle, 供只关心 import_bundle 落地逻辑本身(而非真实归档格式)的测试
	/// 快速构造输入: entries 直接给定, checksums 按 entries 内容精确计算(保证 validate_bundle
	/// 能通过); manifest.counts 本身不影响 import_bundle 的任何行为(只是 ImportPreviewRespVO 展示用
	/// 的统计, 见其文档), 固定填 0 占位即可; source_file_name/source_format 同样给固定占位值
	fn build_parsed_bundle(files: &[(&str, &[u8])]) -> ParsedBundle {
		let entries: BTreeMap<String, Vec<u8>> = files
			.iter()
			.map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
			.collect();
		let checksums = entries
			.iter()
			.map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
			.collect();
		ParsedBundle {
			manifest: ManifestRespVO {
				schema_version: 1,
				exported_at: "2026-07-10T00:00:00Z".to_string(),
				counts: Counts {
					skill: 0,
					mcp: 0,
					config: 0,
					agent: 0,
				},
				versions: BTreeMap::new(),
				checksums,
			},
			entries,
			source_file_name: "bundle.zip".to_string(),
			source_format: BundleFormat::Zip,
		}
	}

	/// 插入一条最小资源, 供冲突场景测试构造"库中已存在同名资源"的前置状态; 返回其 id 与落地
	/// 目录/文件路径(与 group_import_items 反推出的路径形状一致, 供测试直接写入初始内容)
	fn seed_existing_skill(conn: &Connection, data_dir: &Path, name: &str, version: &str) -> i64 {
		let target = data_dir.join("skills").join(name);
		repo_resource::insert(
			conn,
			&repo_resource::NewResource {
				res_type: ResourceType::Skill,
				name: name.to_string(),
				display_name: name.to_string(),
				version: version.to_string(),
				source_type: SourceType::LocalImport,
				local_path: target.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap()
	}

	// import_bundle: 空库导入(无同名冲突)应把全部资源计入 imported, 内容应精确落地到
	// data_dir/skills/<name>/ 与 data_dir/mcp/<name>.json, 并写入一条成功的导入历史记录
	#[test]
	fn import_bundle_into_empty_library_lands_all_as_imported() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let parsed = build_parsed_bundle(&[
			(
				"skills/demo-skill/SKILL.md",
				"---\nversion: 1.2.0\n---\n正文\n".as_bytes(),
			),
			("skills/demo-skill/scripts/run.sh", b"#!/bin/sh\necho hi\n"),
			(
				"mcp/demo-mcp.json",
				br#"{"command":"node","args":["index.js"]}"#,
			),
		]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Overwrite).unwrap();

		assert_eq!(
			outcome,
			ImportOutcomeRespVO {
				imported: 2,
				skipped: 0,
				renamed: 0,
				status: 1,
			}
		);

		let resources = repo_resource::list(&conn, &ListFilter::default()).unwrap();
		assert_eq!(resources.len(), 2);
		let skill = resources
			.iter()
			.find(|r| r.name == "demo-skill")
			.expect("应含 demo-skill");
		assert_eq!(skill.res_type, ResourceType::Skill);
		assert_eq!(skill.version, "1.2.0", "应从 SKILL.md frontmatter 解析");
		assert_eq!(skill.display_name, "demo-skill");
		assert_eq!(skill.source_type, SourceType::LocalImport);
		let mcp = resources
			.iter()
			.find(|r| r.name == "demo-mcp")
			.expect("应含 demo-mcp");
		assert_eq!(mcp.res_type, ResourceType::Mcp);
		assert_eq!(mcp.version, "");

		assert_eq!(
			fs::read_to_string(data_dir.path().join("skills/demo-skill/SKILL.md")).unwrap(),
			"---\nversion: 1.2.0\n---\n正文\n"
		);
		assert_eq!(
			fs::read_to_string(data_dir.path().join("skills/demo-skill/scripts/run.sh")).unwrap(),
			"#!/bin/sh\necho hi\n"
		);
		assert_eq!(
			fs::read_to_string(data_dir.path().join("mcp/demo-mcp.json")).unwrap(),
			r#"{"command":"node","args":["index.js"]}"#
		);

		let history = repo_impexp::recent(&conn, 10).unwrap();
		assert_eq!(history.len(), 1);
		assert_eq!(history[0].direction, 1, "1-导入");
		assert_eq!(history[0].status, 1, "1-成功");
	}

	// import_bundle(Overwrite): 命中同名既有资源应原地覆盖 —— 保留原资源 id, 内容与
	// version/local_path 更新为导入包内容, 库里仍只有一行; display_name 保留原值不变
	#[test]
	fn import_bundle_overwrite_replaces_existing_resource_content_in_place() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();

		let old_dir = data_dir.path().join("skills/demo-skill");
		fs::create_dir_all(&old_dir).unwrap();
		fs::write(
			old_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\n旧内容\n",
		)
		.unwrap();
		fs::write(old_dir.join("STALE.md"), "旧版本独有文件").unwrap();
		let existing_id = repo_resource::insert(
			&conn,
			&repo_resource::NewResource {
				res_type: ResourceType::Skill,
				name: "demo-skill".to_string(),
				display_name: "我的 Demo Skill".to_string(),
				version: "1.0.0".to_string(),
				source_type: SourceType::LocalImport,
				local_path: old_dir.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();

		let parsed = build_parsed_bundle(&[(
			"skills/demo-skill/SKILL.md",
			"---\nversion: 2.0.0\n---\n新内容\n".as_bytes(),
		)]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Overwrite).unwrap();

		assert_eq!(outcome.imported, 1);
		assert_eq!(outcome.skipped, 0);
		assert_eq!(outcome.renamed, 0);

		let resources = repo_resource::list(&conn, &ListFilter::default()).unwrap();
		assert_eq!(resources.len(), 1, "覆盖不应新增行");
		let got = &resources[0];
		assert_eq!(got.id, existing_id, "应保留原 id");
		assert_eq!(
			got.display_name, "我的 Demo Skill",
			"display_name 不应被重置"
		);
		assert_eq!(got.version, "2.0.0");

		assert!(!old_dir.join("STALE.md").exists(), "旧版本独有文件应被清理");
		assert_eq!(
			fs::read_to_string(old_dir.join("SKILL.md")).unwrap(),
			"---\nversion: 2.0.0\n---\n新内容\n"
		);
	}

	// import_bundle(Skip): 命中同名既有资源应整体不动 —— 不写内容、不改库, 计入 skipped
	#[test]
	fn import_bundle_skip_leaves_existing_resource_and_content_untouched() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();

		let old_dir = data_dir.path().join("skills/demo-skill");
		fs::create_dir_all(&old_dir).unwrap();
		fs::write(
			old_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\n旧内容\n",
		)
		.unwrap();
		let existing_id = seed_existing_skill(&conn, data_dir.path(), "demo-skill", "1.0.0");

		let parsed = build_parsed_bundle(&[(
			"skills/demo-skill/SKILL.md",
			"---\nversion: 2.0.0\n---\n新内容\n".as_bytes(),
		)]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Skip).unwrap();

		assert_eq!(
			outcome,
			ImportOutcomeRespVO {
				imported: 0,
				skipped: 1,
				renamed: 0,
				status: 1,
			}
		);

		let resources = repo_resource::list(&conn, &ListFilter::default()).unwrap();
		assert_eq!(resources.len(), 1);
		assert_eq!(resources[0].id, existing_id);
		assert_eq!(resources[0].version, "1.0.0", "不应被改动");

		assert_eq!(
			fs::read_to_string(old_dir.join("SKILL.md")).unwrap(),
			"---\nversion: 1.0.0\n---\n旧内容\n",
			"内容不应被改动"
		);
	}

	// import_bundle(KeepBoth): 命中同名既有资源应改名为 `<name>-imported` 后另起一行落地,
	// 原资源原样保留不动, 计入 renamed
	#[test]
	fn import_bundle_keep_both_renames_and_lands_alongside_existing() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();

		let old_dir = data_dir.path().join("skills/demo-skill");
		fs::create_dir_all(&old_dir).unwrap();
		fs::write(
			old_dir.join("SKILL.md"),
			"---\nversion: 1.0.0\n---\n旧内容\n",
		)
		.unwrap();
		let existing_id = seed_existing_skill(&conn, data_dir.path(), "demo-skill", "1.0.0");

		let parsed = build_parsed_bundle(&[(
			"skills/demo-skill/SKILL.md",
			"---\nversion: 2.0.0\n---\n新内容\n".as_bytes(),
		)]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::KeepBoth).unwrap();

		assert_eq!(
			outcome,
			ImportOutcomeRespVO {
				imported: 0,
				skipped: 0,
				renamed: 1,
				status: 1,
			}
		);

		let resources = repo_resource::list(&conn, &ListFilter::default()).unwrap();
		assert_eq!(resources.len(), 2);
		let original = resources.iter().find(|r| r.id == existing_id).unwrap();
		assert_eq!(original.name, "demo-skill");
		assert_eq!(original.version, "1.0.0", "原资源应原样保留");
		let renamed_res = resources
			.iter()
			.find(|r| r.name == "demo-skill-imported")
			.expect("应新增改名后的资源");
		assert_eq!(renamed_res.version, "2.0.0");
		assert_eq!(renamed_res.display_name, "demo-skill-imported");

		assert_eq!(
			fs::read_to_string(old_dir.join("SKILL.md")).unwrap(),
			"---\nversion: 1.0.0\n---\n旧内容\n",
			"原内容不应被改动"
		);
		assert_eq!(
			fs::read_to_string(data_dir.path().join("skills/demo-skill-imported/SKILL.md"))
				.unwrap(),
			"---\nversion: 2.0.0\n---\n新内容\n"
		);
	}

	// import_bundle(KeepBoth): `<name>-imported` 也已被占用时应继续尝试 `<name>-imported-2`
	#[test]
	fn import_bundle_keep_both_numbers_second_conflict() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();

		seed_existing_skill(&conn, data_dir.path(), "demo-skill", "1.0.0");
		seed_existing_skill(&conn, data_dir.path(), "demo-skill-imported", "1.0.0");

		let parsed = build_parsed_bundle(&[(
			"skills/demo-skill/SKILL.md",
			"---\nversion: 2.0.0\n---\n新内容\n".as_bytes(),
		)]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::KeepBoth).unwrap();

		assert_eq!(outcome.renamed, 1);
		let resources = repo_resource::list(&conn, &ListFilter::default()).unwrap();
		assert_eq!(resources.len(), 3);
		assert!(resources.iter().any(|r| r.name == "demo-skill-imported-2"));
	}

	// import_bundle: agents.json 记录的关联在本机能找到同名 Agent 时应恢复 resource_agent
	// 期望态, status 应为 1(全部匹配成功)
	#[test]
	fn import_bundle_restores_agent_link_when_agent_found_by_name() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let agent_id = seed_agent(&conn, "Claude Code");

		let agents_json = serde_json::json!([
			{ "resourceName": "demo-skill", "resType": 1, "agentName": "Claude Code" }
		])
		.to_string();
		let parsed = build_parsed_bundle(&[
			(
				"skills/demo-skill/SKILL.md",
				"---\nversion: 1.0.0\n---\n正文\n".as_bytes(),
			),
			("agents.json", agents_json.as_bytes()),
		]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Overwrite).unwrap();

		assert_eq!(outcome.status, 1, "应全部匹配成功");
		let resource_id = repo_resource::list(&conn, &ListFilter::default())
			.unwrap()
			.into_iter()
			.find(|r| r.name == "demo-skill")
			.unwrap()
			.id;
		assert_eq!(
			repo_assoc::agents_for_resource(&conn, resource_id).unwrap(),
			vec![agent_id]
		);
	}

	// import_bundle: agents.json 记录的关联在本机找不到同名 Agent 时应跳过该关联(不报错),
	// 但应整体标记为 status=2(部分成功); 资源本身仍应正常导入
	#[test]
	fn import_bundle_marks_partial_status_when_agent_not_found_by_name() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();

		let agents_json = serde_json::json!([
			{ "resourceName": "demo-skill", "resType": 1, "agentName": "不存在的 Agent" }
		])
		.to_string();
		let parsed = build_parsed_bundle(&[
			(
				"skills/demo-skill/SKILL.md",
				"---\nversion: 1.0.0\n---\n正文\n".as_bytes(),
			),
			("agents.json", agents_json.as_bytes()),
		]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Overwrite).unwrap();

		assert_eq!(outcome.imported, 1, "资源本身应正常导入");
		assert_eq!(outcome.status, 2, "关联未能全部恢复应标记部分成功");

		let resource_id = repo_resource::list(&conn, &ListFilter::default())
			.unwrap()
			.into_iter()
			.find(|r| r.name == "demo-skill")
			.unwrap()
			.id;
		assert!(repo_assoc::agents_for_resource(&conn, resource_id)
			.unwrap()
			.is_empty());

		let history = repo_impexp::recent(&conn, 10).unwrap();
		assert_eq!(history[0].status, 2, "2-部分成功");
	}

	// import_bundle: Skip 策略跳过的资源不应恢复其 agents.json 关联(呼应 Skip"不改库"的
	// 既有约定), 即便本机确实存在同名 Agent; 但这属于策略选择的预期结果, 不应计入部分成功
	#[test]
	fn import_bundle_skip_does_not_restore_agent_link_for_skipped_resource() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let _agent_id = seed_agent(&conn, "Claude Code");
		seed_existing_skill(&conn, data_dir.path(), "demo-skill", "1.0.0");

		let agents_json = serde_json::json!([
			{ "resourceName": "demo-skill", "resType": 1, "agentName": "Claude Code" }
		])
		.to_string();
		let parsed = build_parsed_bundle(&[
			(
				"skills/demo-skill/SKILL.md",
				"---\nversion: 2.0.0\n---\n新内容\n".as_bytes(),
			),
			("agents.json", agents_json.as_bytes()),
		]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Skip).unwrap();

		assert_eq!(outcome.skipped, 1);
		assert_eq!(outcome.status, 1, "被跳过属预期结果, 不应视为部分成功");
		let resource_id = repo_resource::list(&conn, &ListFilter::default())
			.unwrap()
			.into_iter()
			.find(|r| r.name == "demo-skill")
			.unwrap()
			.id;
		assert!(
			repo_assoc::agents_for_resource(&conn, resource_id)
				.unwrap()
				.is_empty(),
			"跳过的资源不应被恢复关联"
		);
	}

	// import_bundle: settings.json(cfg_key -> cfg_value 扁平对象)应整表 upsert 回 setting 表
	#[test]
	fn import_bundle_restores_settings_from_settings_json() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();

		let settings_json = serde_json::json!({
			"net.proxy": "http://127.0.0.1:7890",
			"sync.pref": "auto"
		})
		.to_string();
		let parsed = build_parsed_bundle(&[
			(
				"skills/demo-skill/SKILL.md",
				"---\nversion: 1.0.0\n---\n正文\n".as_bytes(),
			),
			("settings.json", settings_json.as_bytes()),
		]);

		import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Overwrite).unwrap();

		let settings = repo_setting::list_all(&conn).unwrap();
		assert_eq!(settings.len(), 2);
		assert!(settings
			.iter()
			.any(|s| s.cfg_key == "net.proxy" && s.cfg_value == "http://127.0.0.1:7890"));
		assert!(settings
			.iter()
			.any(|s| s.cfg_key == "sync.pref" && s.cfg_value == "auto"));
	}

	// import_bundle: 往返测试 —— 用 export_bundle 从一份含 1 Skill+1 Mcp 的库导出真实 zip,
	// 再用 parse_bundle 解析, 最后导入到一个全新(空)的库/数据目录, 内容与元数据应精确一致
	#[test]
	fn import_bundle_round_trips_export_output_into_fresh_library() {
		let source_conn = setup_conn();
		let (source_data_dir, _skill_id, _mcp_id) = seed_data_dir_and_resources(&source_conn);
		let bundle_path = source_data_dir.path().join("out.zip");
		export_bundle(
			&source_conn,
			source_data_dir.path(),
			&full_options(BundleFormat::Zip),
			&bundle_path,
		)
		.unwrap();

		let parsed = parse_bundle(&bundle_path).unwrap();

		let target_conn = setup_conn();
		let target_data_dir = tempdir().unwrap();
		let outcome = import_bundle(
			&target_conn,
			target_data_dir.path(),
			parsed,
			ConflictStrategy::Overwrite,
		)
		.unwrap();

		assert_eq!(
			outcome,
			ImportOutcomeRespVO {
				imported: 2,
				skipped: 0,
				renamed: 0,
				status: 1,
			}
		);

		let resources = repo_resource::list(&target_conn, &ListFilter::default()).unwrap();
		assert_eq!(resources.len(), 2);
		let skill = resources.iter().find(|r| r.name == "demo-skill").unwrap();
		assert_eq!(skill.version, "1.2.0");
		let mcp = resources.iter().find(|r| r.name == "demo-mcp").unwrap();
		assert_eq!(mcp.version, "");

		assert_eq!(
			fs::read_to_string(target_data_dir.path().join("skills/demo-skill/SKILL.md")).unwrap(),
			fs::read_to_string(source_data_dir.path().join("skills/demo-skill/SKILL.md")).unwrap(),
		);
		assert_eq!(
			fs::read_to_string(
				target_data_dir
					.path()
					.join("skills/demo-skill/scripts/run.sh")
			)
			.unwrap(),
			"#!/bin/sh\necho hi\n"
		);
		assert_eq!(
			fs::read_to_string(target_data_dir.path().join("mcp/demo-mcp.json")).unwrap(),
			fs::read_to_string(source_data_dir.path().join("mcp/demo-mcp.json")).unwrap(),
		);
	}

	// sync_online_agents: 应只对 status=true(在线)的 Agent 触发同步, 离线 Agent 应被跳过
	// (不产生 sync_run, 也不出现在返回的汇总列表里)
	#[test]
	fn sync_online_agents_skips_offline_agents() {
		let conn = setup_conn();
		let home = tempdir().unwrap();
		fs::write(home.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();

		let online_id = repo_agent::upsert(
			&conn,
			&DetectedAgent {
				kind: AgentKind::ClaudeCode,
				name: "Online Agent".to_string(),
				config_path: home
					.path()
					.join(".claude.json")
					.to_string_lossy()
					.into_owned(),
				scope: AgentScope::Global,
				online: true,
			},
		)
		.unwrap();
		repo_agent::upsert(
			&conn,
			&DetectedAgent {
				kind: AgentKind::ClaudeCode,
				name: "Offline Agent".to_string(),
				config_path: home
					.path()
					.join("offline/.claude.json")
					.to_string_lossy()
					.into_owned(),
				scope: AgentScope::Global,
				online: false,
			},
		)
		.unwrap();

		let def_path = home.path().join("demo-mcp.json");
		fs::write(&def_path, r#"{"command":"node","args":["index.js"]}"#).unwrap();
		let resource_id = repo_resource::insert(
			&conn,
			&repo_resource::NewResource {
				res_type: ResourceType::Mcp,
				name: "demo-mcp".to_string(),
				display_name: "demo-mcp".to_string(),
				version: String::new(),
				source_type: SourceType::LocalImport,
				local_path: def_path.to_string_lossy().into_owned(),
				enabled: true,
			},
		)
		.unwrap();
		repo_assoc::set(&conn, resource_id, online_id, true).unwrap();

		let summaries = sync_online_agents(&conn, home.path()).unwrap();

		assert_eq!(summaries.len(), 1, "只应处理在线的那个 Agent");
		assert_eq!(summaries[0].success, 1);

		let runs = repo_sync::recent_runs(&conn, 10).unwrap();
		assert_eq!(runs.len(), 1, "离线 Agent 不应产生 sync_run");
		assert_eq!(runs[0].agent_id, online_id);
	}

	// import_bundle + sync_online_agents 端到端: agents.json 恢复的关联应能被后续的
	// sync_online_agents 真正应用到该 Agent 的配置文件里(证明 auto_sync=true 分支确实链得上
	// 导入落地的关联结果, 而不只是各自独立测试通过)
	#[test]
	fn import_then_sync_online_agents_applies_restored_association() {
		let conn = setup_conn();
		let data_dir = tempdir().unwrap();
		let home = tempdir().unwrap();
		fs::write(home.path().join(".claude.json"), r#"{"mcpServers":{}}"#).unwrap();

		let agent_id = repo_agent::upsert(
			&conn,
			&DetectedAgent {
				kind: AgentKind::ClaudeCode,
				name: "Claude Code".to_string(),
				config_path: home
					.path()
					.join(".claude.json")
					.to_string_lossy()
					.into_owned(),
				scope: AgentScope::Global,
				online: true,
			},
		)
		.unwrap();

		let agents_json = serde_json::json!([
			{ "resourceName": "demo-mcp", "resType": 2, "agentName": "Claude Code" }
		])
		.to_string();
		let parsed = build_parsed_bundle(&[
			(
				"mcp/demo-mcp.json",
				br#"{"command":"node","args":["index.js"]}"#,
			),
			("agents.json", agents_json.as_bytes()),
		]);

		let outcome =
			import_bundle(&conn, data_dir.path(), parsed, ConflictStrategy::Overwrite).unwrap();
		assert_eq!(outcome.status, 1);

		let summaries = sync_online_agents(&conn, home.path()).unwrap();
		assert_eq!(summaries.len(), 1);
		assert_eq!(summaries[0].success, 1);

		let root: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(home.path().join(".claude.json")).unwrap())
				.unwrap();
		assert_eq!(root["mcpServers"]["demo-mcp"]["command"], "node");

		let runs = repo_sync::recent_runs(&conn, 10).unwrap();
		assert_eq!(runs.len(), 1);
		assert_eq!(runs[0].agent_id, agent_id);
	}
}
