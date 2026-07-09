// 文件作用: Skill 落地策略 —— 描述各 AI 工具把"已装 Skill 清单"落在磁盘上的三种形态
//           (SkillTarget), 并提供 read_skills 统一读出当前已装清单, 供各 AgentAdapter::
//           read_state 组装 ActualState.skills。三种形态覆盖 Task 3/4 接入的 8 款工具,
//           具体每款工具映射到哪种形态见 adapter::mod::json_mcp_agent_configs 与
//           adapter::mod::all_adapters。
//           InstructionsFile 变体依赖 SkillHub 自定义的标记块格式登记"这段内容是 SkillHub
//           装的哪个 Skill":
//               <!-- skillhub:start:<name>@<version> -->
//               ...(注入的 Skill 内容)...
//               <!-- skillhub:end:<name> -->
//           读取时按"起止标签配对"还原清单: 只有起始标签、缺失匹配结束标签的残缺块视为一次
//           没写完的失败写入, 不计入。Task 7 apply 写入 InstructionsFile 形态时必须复用同一
//           格式包裹注入内容(即 MARK_START_PREFIX/MARK_SUFFIX 两个常量拼出的样式), 否则本文件
//           的读取逻辑会读不到。
// 创建日期: 2026-07-09

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::domain::agent::SkillRef;

use super::util::backup_file;

/// SkillHub 标记块的起始标签前缀与通用后缀; 完整起始标签形如
/// `<!-- skillhub:start:<name>@<version> -->`, 结束标签形如 `<!-- skillhub:end:<name> -->`
/// (拼接规则见 parse_marked_skills; Task 7 apply 写入时必须复用这两个常量拼出的格式)
const MARK_START_PREFIX: &str = "<!-- skillhub:start:";
const MARK_SUFFIX: &str = " -->";

/// Skill 落地策略: 一个工具具体用哪种形态把"已装 Skill 清单"存在磁盘上
pub enum SkillTarget {
	/// Claude 家族(ClaudeCode/ClaudeDesktop): 每个 Skill 是一个含 `SKILL.md` 的子目录,
	/// name 取子目录名, version 从 `SKILL.md` 的 YAML frontmatter `version:` 字段读取
	/// (读不到给空串)。字段为相对家目录的路径, 如 `.claude/skills`
	ClaudeSkillsDir(PathBuf),
	/// rules 目录家族(Cursor/Windsurf/Cline/VsCode): 每个 Skill 是该目录下扩展名匹配
	/// `ext` 的一个文件, name 取文件名去扩展名, version 恒为空串(该形态本身不带版本信息)。
	/// `dir` 为相对家目录的路径, 如 `.cursor/rules`; `ext` 不含前导点, 如 `"mdc"`
	RulesDir { dir: PathBuf, ext: String },
	/// 单文件聚合家族(GeminiCli/Codex): 用单个指令文件(如 `GEMINI.md`/`AGENTS.md`)登记
	/// SkillHub 已装的 Skill, 按文件头注释所述的标记块格式解析。字段为相对家目录的路径
	InstructionsFile(PathBuf),
}

impl SkillTarget {
	/// 按本变体描述的落地形态, 在 `home` 下读出当前已装的 Skill 清单; 目标目录/文件不存在,
	/// 或内容缺失关键信息(如 frontmatter 没写 version、标记块缺失), 都不视为错误, 分别兜底
	/// 为"空清单"或"该字段空串", 与 AgentAdapter::read_state 的整体宽松解析风格保持一致
	/// (本方法不返回 Result, 调用方无需处理"Skill 读取失败", 任何异常都已在内部兜底掉)
	pub fn read_skills(&self, home: &Path) -> Vec<SkillRef> {
		match self {
			SkillTarget::ClaudeSkillsDir(rel) => read_claude_skills_dir(&home.join(rel)),
			SkillTarget::RulesDir { dir, ext } => read_rules_dir(&home.join(dir), ext),
			SkillTarget::InstructionsFile(rel) => read_instructions_file(&home.join(rel)),
		}
	}

	/// 按本变体描述的落地形态, 把 `src_dir` 指向的 Skill 源内容写入/更新到 `home` 下(Add/Update
	/// 复用同一实现: 直接覆盖为 src_dir 的最新内容, 不做增量合并)。`version` 仅 InstructionsFile
	/// 形态需要写进标记块; 其余两种形态各自的版本语义已在类型注释与 read_skills 里说明
	/// (ClaudeSkillsDir 从 SKILL.md frontmatter 读, RulesDir 恒为空), 写入时无需额外处理。
	/// 被改的目标文件在写入前都会先经 backup_file 备份(呼应"任何写入/删除配置文件之前都先
	/// 备份"的安全约束); ClaudeSkillsDir 的目标是整个子目录而非单个文件, 不适用 backup_file
	/// (其内容本就整体归 SkillHub 管理, 不存在需要保护的"用户其它内容")
	pub fn write_skill(
		&self,
		home: &Path,
		name: &str,
		version: &str,
		src_dir: &Path,
	) -> Result<()> {
		match self {
			SkillTarget::ClaudeSkillsDir(rel) => {
				write_claude_skills_dir(&home.join(rel), name, src_dir)
			}
			SkillTarget::RulesDir { dir, ext } => {
				write_rules_dir(&home.join(dir), ext, name, src_dir)
			}
			SkillTarget::InstructionsFile(rel) => {
				write_instructions_file(&home.join(rel), name, version, src_dir)
			}
		}
	}

	/// 按本变体描述的落地形态, 从 `home` 下移除 `name` 对应的 Skill 内容; 目标本就不存在视为
	/// 已达成目的, 不算错误(呼应各形态 read 侧"不存在即空清单"的一贯宽松风格)
	pub fn remove_skill(&self, home: &Path, name: &str) -> Result<()> {
		match self {
			SkillTarget::ClaudeSkillsDir(rel) => remove_claude_skills_dir(&home.join(rel), name),
			SkillTarget::RulesDir { dir, ext } => remove_rules_dir(&home.join(dir), ext, name),
			SkillTarget::InstructionsFile(rel) => remove_instructions_file(&home.join(rel), name),
		}
	}
}

/// ClaudeSkillsDir 形态: 遍历 `dir` 下的直接子目录, 含 `SKILL.md` 的子目录即一个 Skill;
/// name 取子目录名, version 从 SKILL.md 解析(解析不到给空串)。`dir` 不存在/不是目录/无
/// 权限读取都返回空清单(工具未安装或尚未装任何 Skill, 不视为错误)
fn read_claude_skills_dir(dir: &Path) -> Vec<SkillRef> {
	let Ok(entries) = fs::read_dir(dir) else {
		return Vec::new();
	};
	let mut skills: Vec<SkillRef> = entries
		.filter_map(Result::ok)
		.filter(|entry| entry.path().is_dir())
		.filter_map(|entry| {
			let name = entry.file_name().to_string_lossy().into_owned();
			let text = fs::read_to_string(entry.path().join("SKILL.md")).ok()?;
			Some(SkillRef {
				name,
				version: parse_frontmatter_version(&text),
			})
		})
		.collect();
	skills.sort_by(|a, b| a.name.cmp(&b.name));
	skills
}

/// RulesDir 形态: 列出 `dir` 下扩展名精确匹配 `ext` 的文件, name 取文件名去扩展名, version
/// 恒为空串。`dir` 不存在/不是目录都返回空清单
fn read_rules_dir(dir: &Path, ext: &str) -> Vec<SkillRef> {
	let Ok(entries) = fs::read_dir(dir) else {
		return Vec::new();
	};
	let mut skills: Vec<SkillRef> = entries
		.filter_map(Result::ok)
		.filter(|entry| entry.path().is_file())
		.filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some(ext))
		.filter_map(|entry| {
			let name = entry.path().file_stem()?.to_string_lossy().into_owned();
			Some(SkillRef {
				name,
				version: String::new(),
			})
		})
		.collect();
	skills.sort_by(|a, b| a.name.cmp(&b.name));
	skills
}

/// InstructionsFile 形态: 读取 `path` 全文, 按文件头注释所述的标记块格式还原已装清单;
/// `path` 不存在或读取失败都返回空清单
fn read_instructions_file(path: &Path) -> Vec<SkillRef> {
	match fs::read_to_string(path) {
		Ok(text) => parse_marked_skills(&text),
		Err(_) => Vec::new(),
	}
}

/// ClaudeSkillsDir 形态的 write: 若目标子目录 `root/<name>/` 已存在则先整体删除(覆盖式更新,
/// 不做增量合并 —— 旧版本可能包含新版本已移除的文件, 增量合并会遗留脏文件), 再把 `src_dir`
/// 的内容整树复制过去。该子目录整体归 SkillHub 管理, 不涉及"保留用户其它内容"的顾虑
fn write_claude_skills_dir(root: &Path, name: &str, src_dir: &Path) -> Result<()> {
	let target = root.join(name);
	if target.exists() {
		fs::remove_dir_all(&target)
			.with_context(|| format!("删除旧 Skill 目录失败: {}", target.display()))?;
	}
	copy_dir_recursive(src_dir, &target).with_context(|| {
		format!(
			"复制 Skill 内容失败: {} -> {}",
			src_dir.display(),
			target.display()
		)
	})?;
	Ok(())
}

/// ClaudeSkillsDir 形态的 remove: 目标子目录不存在视为已达成目的, 不算错误(呼应各形态一贯
/// 的宽松风格)
fn remove_claude_skills_dir(root: &Path, name: &str) -> Result<()> {
	let target = root.join(name);
	if target.is_dir() {
		fs::remove_dir_all(&target)
			.with_context(|| format!("删除 Skill 目录失败: {}", target.display()))?;
	}
	Ok(())
}

/// 把 `src` 目录树逐层递归复制到 `dst`(`dst` 不存在会先创建); 只处理普通文件与子目录,
/// 符号链接等特殊类型目前场景(Skill 源目录)不涉及, 按需可后续扩展
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
	fs::create_dir_all(dst).with_context(|| format!("创建目录失败: {}", dst.display()))?;
	let entries = fs::read_dir(src).with_context(|| format!("读取目录失败: {}", src.display()))?;
	for entry in entries {
		let entry = entry.with_context(|| format!("读取目录项失败: {}", src.display()))?;
		let file_type = entry
			.file_type()
			.with_context(|| format!("读取文件类型失败: {}", entry.path().display()))?;
		let dst_path = dst.join(entry.file_name());
		if file_type.is_dir() {
			copy_dir_recursive(&entry.path(), &dst_path)?;
		} else if file_type.is_file() {
			fs::copy(entry.path(), &dst_path)
				.with_context(|| format!("复制文件失败: {}", entry.path().display()))?;
		}
	}
	Ok(())
}

/// RulesDir 形态的 write: 目标文件 `dir/<name>.<ext>` 写前先 backup, 内容取
/// `src_dir/SKILL.md` 全文; 读不到 SKILL.md(源目录残缺)则退化写一行占位说明, 不让整次
/// apply 因此报错中断
fn write_rules_dir(dir: &Path, ext: &str, name: &str, src_dir: &Path) -> Result<()> {
	fs::create_dir_all(dir).with_context(|| format!("创建目录失败: {}", dir.display()))?;
	let target = dir.join(format!("{name}.{ext}"));
	backup_file(&target).with_context(|| format!("备份文件失败: {}", target.display()))?;
	let content = read_skill_md_or_placeholder(name, src_dir);
	fs::write(&target, content).with_context(|| format!("写入文件失败: {}", target.display()))?;
	Ok(())
}

/// RulesDir 形态的 remove: 目标文件写前先 backup 再删除; 不存在视为已达成目的
fn remove_rules_dir(dir: &Path, ext: &str, name: &str) -> Result<()> {
	let target = dir.join(format!("{name}.{ext}"));
	if !target.is_file() {
		return Ok(());
	}
	backup_file(&target).with_context(|| format!("备份文件失败: {}", target.display()))?;
	fs::remove_file(&target).with_context(|| format!("删除文件失败: {}", target.display()))?;
	Ok(())
}

/// InstructionsFile 形态的 write: 目标聚合文件写前先 backup, 按文件头注释所述的标记块格式
/// 把内容 upsert 进去(已存在同 name 的块则整块替换, 否则追加到文件末尾), 保留文件里其它
/// (非本 skill)全部内容不受影响
fn write_instructions_file(path: &Path, name: &str, version: &str, src_dir: &Path) -> Result<()> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)
			.with_context(|| format!("创建目录失败: {}", parent.display()))?;
	}
	backup_file(path).with_context(|| format!("备份文件失败: {}", path.display()))?;

	let existing = fs::read_to_string(path).unwrap_or_default();
	let content = read_skill_md_or_placeholder(name, src_dir);
	let start_tag = format!("{MARK_START_PREFIX}{name}@{version}{MARK_SUFFIX}");
	let end_tag = format!("<!-- skillhub:end:{name} -->");
	let block = format!("{start_tag}\n{}\n{end_tag}", content.trim_end());

	let updated = upsert_marked_block(&existing, name, &block);
	fs::write(path, updated).with_context(|| format!("写入文件失败: {}", path.display()))?;
	Ok(())
}

/// InstructionsFile 形态的 remove: 从聚合文件里剥离 `name` 对应的标记块, 保留其它内容;
/// 文件不存在、或该 name 的块本就不存在(含残缺块, 视同不存在)都视为已达成目的, 不算错误、
/// 也不产生多余的备份与写入
fn remove_instructions_file(path: &Path, name: &str) -> Result<()> {
	let Ok(existing) = fs::read_to_string(path) else {
		return Ok(());
	};
	let Some(updated) = strip_marked_block(&existing, name) else {
		return Ok(());
	};
	backup_file(path).with_context(|| format!("备份文件失败: {}", path.display()))?;
	fs::write(path, updated).with_context(|| format!("写入文件失败: {}", path.display()))?;
	Ok(())
}

/// 读取 `src_dir/SKILL.md` 全文; 读不到(源目录残缺/路径不存在)时退化为一行占位说明,
/// 不让 apply 因源内容缺失而报错中断(呼应整体"宽松兜底, 单项失败不拖累整体"的风格)
fn read_skill_md_or_placeholder(name: &str, src_dir: &Path) -> String {
	fs::read_to_string(src_dir.join("SKILL.md"))
		.unwrap_or_else(|_| format!("(占位) 未找到 SKILL.md, 无法读取 {name} 的内容"))
}

/// 从 SKILL.md 全文提取 YAML frontmatter 里的 version 字段; 只做逐行字符串扫描, 不引入
/// YAML 解析依赖(frontmatter 目前只需读这一个字段, 用不到完整 YAML 语义)。frontmatter 必须
/// 以独占一行的 `---` 开头, 扫描到下一个独占一行的 `---` 为止; 边界不存在或界内找不到
/// `version:` 前缀的行都返回空串, 不视为错误。取值支持前后空白与成对的单/双引号包裹
/// (如 `version: "1.2.0"`), 引号会被裁掉(见 strip_matching_quotes)
fn parse_frontmatter_version(text: &str) -> String {
	let mut lines = text.lines();
	let Some(first) = lines.next() else {
		return String::new();
	};
	if first.trim() != "---" {
		return String::new();
	}
	for line in lines {
		if line.trim() == "---" {
			break;
		}
		if let Some(raw) = line.trim().strip_prefix("version:") {
			return strip_matching_quotes(raw.trim()).to_string();
		}
	}
	String::new()
}

/// 裁掉字符串两端成对包裹的引号(单引号或双引号各自成对才裁, 不成对原样返回); 供
/// parse_frontmatter_version 处理 `version: "1.2.0"` 这类带引号写法
fn strip_matching_quotes(value: &str) -> &str {
	for quote in ['"', '\''] {
		if let Some(inner) = value
			.strip_prefix(quote)
			.and_then(|v| v.strip_suffix(quote))
		{
			return inner;
		}
	}
	value
}

/// 扫描全文里配对的 SkillHub 标记块(起始标签携带 `name@version`, 结束标签携带 `name` 用于
/// 校验配对), 还原出已装 Skill 清单。只有同时出现起始标签与匹配的结束标签才计入; 只有起始
/// 标签、缺失匹配结束标签的残缺块视为一次没写完的失败写入, 不计入(呼应文件头注释里的约定)
fn parse_marked_skills(text: &str) -> Vec<SkillRef> {
	let lines: Vec<&str> = text.lines().collect();
	let mut skills = Vec::new();
	for (idx, line) in lines.iter().enumerate() {
		let Some(rest) = line.trim().strip_prefix(MARK_START_PREFIX) else {
			continue;
		};
		let Some(name_version) = rest.strip_suffix(MARK_SUFFIX) else {
			continue;
		};
		let Some((name, version)) = name_version.split_once('@') else {
			continue;
		};
		if name.is_empty() {
			continue;
		}
		let end_tag = format!("<!-- skillhub:end:{name} -->");
		let has_matching_end = lines[idx + 1..].iter().any(|l| l.trim() == end_tag);
		if has_matching_end {
			skills.push(SkillRef {
				name: name.to_string(),
				version: version.to_string(),
			});
		}
	}
	skills
}

/// 在按行拆分的 `lines` 里查找 `name` 对应的完整标记块(起始标签携带 `name@<任意版本>`,
/// 且其后存在匹配的结束标签), 返回起止行下标(闭区间, 含起止标签本身两行); 找不到完整配对
/// (包括没有起始标签、或只有起始标签没有匹配结束标签的残缺情形)都返回 None。扫描规则与
/// parse_marked_skills 一致, 区别是这里只关心某一个指定 name, 供 upsert/strip 定位替换范围
fn find_marked_block_range(lines: &[&str], name: &str) -> Option<(usize, usize)> {
	let end_tag = format!("<!-- skillhub:end:{name} -->");
	for (idx, line) in lines.iter().enumerate() {
		let Some(rest) = line.trim().strip_prefix(MARK_START_PREFIX) else {
			continue;
		};
		let Some(name_version) = rest.strip_suffix(MARK_SUFFIX) else {
			continue;
		};
		let Some((line_name, _version)) = name_version.split_once('@') else {
			continue;
		};
		if line_name != name {
			continue;
		}
		if let Some(rel_end) = lines[idx + 1..].iter().position(|l| l.trim() == end_tag) {
			return Some((idx, idx + 1 + rel_end));
		}
	}
	None
}

/// 在 `text` 里 upsert(更新或插入) `name` 对应的标记块: 已存在该 name 的完整块(起止标签
/// 配对)则整块替换为 `block`; 不存在(含只有残缺起始标签、没有匹配结束标签的情形, 视同不
/// 存在, 呼应 parse_marked_skills 对残缺块的处理)则把 `block` 追加到文件末尾。`block` 应
/// 已是完整的"起始标签行+内容+结束标签行"三段式文本, 本函数只负责定位/替换/追加, 不关心
/// block 内部格式; 除被替换或追加的部分外, 其它原有行原样保留(逐行搬运, 不改动)
fn upsert_marked_block(text: &str, name: &str, block: &str) -> String {
	let lines: Vec<&str> = text.lines().collect();
	let existing_range = find_marked_block_range(&lines, name);

	let mut result: Vec<String> = Vec::new();
	match existing_range {
		Some((start, end)) => {
			result.extend(lines[..start].iter().map(|line| line.to_string()));
			result.extend(block.lines().map(|line| line.to_string()));
			result.extend(lines[end + 1..].iter().map(|line| line.to_string()));
		}
		None => {
			result.extend(lines.iter().map(|line| line.to_string()));
			if !text.trim().is_empty() {
				result.push(String::new());
			}
			result.extend(block.lines().map(|line| line.to_string()));
		}
	}

	let mut joined = result.join("\n");
	joined.push('\n');
	joined
}

/// 在 `text` 里剥离 `name` 对应的标记块, 返回移除后的全文; 该 name 的块本就不存在(含残缺块,
/// 视同不存在)时返回 None, 供调用方(remove_instructions_file)据此判断"本就没有, 无需
/// 备份/写入"从而跳过多余操作
fn strip_marked_block(text: &str, name: &str) -> Option<String> {
	let lines: Vec<&str> = text.lines().collect();
	let (start, end) = find_marked_block_range(&lines, name)?;

	let mut result: Vec<&str> = Vec::new();
	result.extend_from_slice(&lines[..start]);
	result.extend_from_slice(&lines[end + 1..]);

	let mut joined = result.join("\n");
	if !joined.is_empty() {
		joined.push('\n');
	}
	Some(joined)
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::*;

	// ClaudeSkillsDir: foo 带 frontmatter version, bar 无 version 字段均应能读出;
	// 非 Skill 子目录(没有 SKILL.md)应被排除
	#[test]
	fn claude_skills_dir_reads_version_from_frontmatter_and_ignores_non_skill_dirs() {
		let dir = tempdir().unwrap();
		let skills_root = dir.path().join(".claude/skills");

		let foo_dir = skills_root.join("foo");
		fs::create_dir_all(&foo_dir).unwrap();
		fs::write(
			foo_dir.join("SKILL.md"),
			"---\nname: foo\nversion: 1.2.0\n---\n\n# Foo\n",
		)
		.unwrap();

		let bar_dir = skills_root.join("bar");
		fs::create_dir_all(&bar_dir).unwrap();
		fs::write(
			bar_dir.join("SKILL.md"),
			"---\nname: bar\ndescription: 没有 version 字段\n---\n\n# Bar\n",
		)
		.unwrap();

		let not_a_skill_dir = skills_root.join("not-a-skill");
		fs::create_dir_all(&not_a_skill_dir).unwrap();
		fs::write(not_a_skill_dir.join("readme.txt"), "无 SKILL.md").unwrap();

		let target = SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills"));
		let skills = target.read_skills(dir.path());

		assert_eq!(skills.len(), 2, "not-a-skill 不含 SKILL.md, 应被排除");
		let foo = skills.iter().find(|s| s.name == "foo").expect("应含 foo");
		assert_eq!(foo.version, "1.2.0");
		let bar = skills.iter().find(|s| s.name == "bar").expect("应含 bar");
		assert_eq!(bar.version, "");
	}

	// ClaudeSkillsDir: 目标目录不存在(工具未装/未装任何 Skill)应返回空清单, 不报错不 panic
	#[test]
	fn claude_skills_dir_returns_empty_when_dir_missing() {
		let dir = tempdir().unwrap();
		let target = SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills"));
		assert!(target.read_skills(dir.path()).is_empty());
	}

	// RulesDir: 只应读出扩展名精确匹配 ext 的文件(a.mdc/b.mdc), 排除扩展名不符的文件(c.txt);
	// version 恒为空串
	#[test]
	fn rules_dir_reads_matching_extension_files_only() {
		let dir = tempdir().unwrap();
		let rules_dir = dir.path().join(".cursor/rules");
		fs::create_dir_all(&rules_dir).unwrap();
		fs::write(rules_dir.join("a.mdc"), "# rule a").unwrap();
		fs::write(rules_dir.join("b.mdc"), "# rule b").unwrap();
		fs::write(rules_dir.join("c.txt"), "非 mdc, 应被排除").unwrap();

		let target = SkillTarget::RulesDir {
			dir: PathBuf::from(".cursor/rules"),
			ext: "mdc".to_string(),
		};
		let skills = target.read_skills(dir.path());

		assert_eq!(skills.len(), 2);
		assert!(skills.iter().all(|s| s.version.is_empty()));
		assert!(skills.iter().any(|s| s.name == "a"));
		assert!(skills.iter().any(|s| s.name == "b"));
	}

	// RulesDir: 目标目录不存在应返回空清单
	#[test]
	fn rules_dir_returns_empty_when_dir_missing() {
		let dir = tempdir().unwrap();
		let target = SkillTarget::RulesDir {
			dir: PathBuf::from(".cursor/rules"),
			ext: "mdc".to_string(),
		};
		assert!(target.read_skills(dir.path()).is_empty());
	}

	// InstructionsFile: 含两个完整标记块的文件应解析出两个 SkillRef(含无 version 的场景)
	#[test]
	fn instructions_file_parses_two_marked_blocks() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("GEMINI.md");
		fs::write(
			&path,
			"# 我的指令\n\n\
			<!-- skillhub:start:foo@1.0.0 -->\nfoo 的内容\n<!-- skillhub:end:foo -->\n\n\
			<!-- skillhub:start:bar@ -->\nbar 的内容(无 version)\n<!-- skillhub:end:bar -->\n\n\
			# 其余手写内容\n",
		)
		.unwrap();

		let target = SkillTarget::InstructionsFile(PathBuf::from("GEMINI.md"));
		let skills = target.read_skills(dir.path());

		assert_eq!(skills.len(), 2);
		let foo = skills.iter().find(|s| s.name == "foo").expect("应含 foo");
		assert_eq!(foo.version, "1.0.0");
		let bar = skills.iter().find(|s| s.name == "bar").expect("应含 bar");
		assert_eq!(bar.version, "");
	}

	// InstructionsFile: 没有任何标记块的文件应返回空清单(不代表读取失败, 只是没装过 Skill)
	#[test]
	fn instructions_file_returns_empty_when_no_marker_blocks() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("GEMINI.md");
		fs::write(&path, "# 纯手写的指令文件, 没有任何 SkillHub 标记\n").unwrap();

		let target = SkillTarget::InstructionsFile(PathBuf::from("GEMINI.md"));
		assert!(target.read_skills(dir.path()).is_empty());
	}

	// InstructionsFile: 目标文件不存在应返回空清单
	#[test]
	fn instructions_file_returns_empty_when_file_missing() {
		let dir = tempdir().unwrap();
		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));
		assert!(target.read_skills(dir.path()).is_empty());
	}

	// InstructionsFile: 只有起始标签、缺失匹配结束标签的残缺块应被排除(一次没写完的失败写入)
	#[test]
	fn instructions_file_ignores_start_marker_without_matching_end_marker() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("AGENTS.md");
		fs::write(
			&path,
			"<!-- skillhub:start:broken@1.0.0 -->\n内容写到一半没收尾\n",
		)
		.unwrap();

		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));
		assert!(target.read_skills(dir.path()).is_empty());
	}

	/// 造一个最小 Skill 源目录: `src_dir/SKILL.md` + `src_dir/<sub_file>`(验证递归复制时
	/// 用到), 返回源目录路径
	fn make_src_dir(root: &Path, skill_md_body: &str) -> PathBuf {
		let src_dir = root.join("src-demo-skill");
		fs::create_dir_all(src_dir.join("scripts")).unwrap();
		fs::write(src_dir.join("SKILL.md"), skill_md_body).unwrap();
		fs::write(src_dir.join("scripts/run.sh"), "#!/bin/sh\necho hi\n").unwrap();
		src_dir
	}

	// ClaudeSkillsDir::write_skill: 应把 src_dir 整树(含子目录文件)递归复制到 home/rel/<name>/
	#[test]
	fn claude_skills_dir_write_skill_copies_source_tree_recursively() {
		let dir = tempdir().unwrap();
		let src_dir = make_src_dir(dir.path(), "---\nversion: 1.0.0\n---\n正文\n");

		let target = SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills"));
		target
			.write_skill(dir.path(), "demo-skill", "1.0.0", &src_dir)
			.unwrap();

		let installed = dir.path().join(".claude/skills/demo-skill");
		assert_eq!(
			fs::read_to_string(installed.join("SKILL.md")).unwrap(),
			"---\nversion: 1.0.0\n---\n正文\n"
		);
		assert_eq!(
			fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
			"#!/bin/sh\necho hi\n"
		);
		// read_skills 应能读到刚写入的这一个 Skill, 与 write_skill 的落地互相印证
		let skills = target.read_skills(dir.path());
		assert_eq!(skills.len(), 1);
		assert_eq!(skills[0].name, "demo-skill");
		assert_eq!(skills[0].version, "1.0.0");
	}

	// ClaudeSkillsDir::write_skill: 二次写入(Update 场景)应整体覆盖旧目录, 旧版本独有的
	// 残留文件不应留存(覆盖式更新, 非增量合并)
	#[test]
	fn claude_skills_dir_write_skill_overwrites_stale_files_from_previous_version() {
		let dir = tempdir().unwrap();
		let target = SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills"));

		let old_src = dir.path().join("old-src");
		fs::create_dir_all(&old_src).unwrap();
		fs::write(old_src.join("SKILL.md"), "---\nversion: 1.0.0\n---\n").unwrap();
		fs::write(old_src.join("stale.txt"), "旧版本独有文件").unwrap();
		target
			.write_skill(dir.path(), "demo-skill", "1.0.0", &old_src)
			.unwrap();

		let new_src = dir.path().join("new-src");
		fs::create_dir_all(&new_src).unwrap();
		fs::write(new_src.join("SKILL.md"), "---\nversion: 2.0.0\n---\n").unwrap();
		target
			.write_skill(dir.path(), "demo-skill", "2.0.0", &new_src)
			.unwrap();

		let installed = dir.path().join(".claude/skills/demo-skill");
		assert!(
			!installed.join("stale.txt").exists(),
			"旧版本独有文件应随整体覆盖被清除"
		);
		assert_eq!(
			fs::read_to_string(installed.join("SKILL.md")).unwrap(),
			"---\nversion: 2.0.0\n---\n"
		);
	}

	// ClaudeSkillsDir::remove_skill: 应删除整个 <name>/ 子目录; 目标本就不存在应 Ok 而非报错
	#[test]
	fn claude_skills_dir_remove_skill_deletes_directory_and_is_noop_when_missing() {
		let dir = tempdir().unwrap();
		let src_dir = make_src_dir(dir.path(), "---\nversion: 1.0.0\n---\n");
		let target = SkillTarget::ClaudeSkillsDir(PathBuf::from(".claude/skills"));
		target
			.write_skill(dir.path(), "demo-skill", "1.0.0", &src_dir)
			.unwrap();

		target.remove_skill(dir.path(), "demo-skill").unwrap();
		assert!(!dir.path().join(".claude/skills/demo-skill").exists());

		// 再删一次(已不存在)应仍返回 Ok, 不报错
		assert!(target.remove_skill(dir.path(), "demo-skill").is_ok());
	}

	// RulesDir::write_skill: 应把 src_dir/SKILL.md 的内容写到 dir/<name>.<ext>
	#[test]
	fn rules_dir_write_skill_writes_skill_md_content_to_target_file() {
		let dir = tempdir().unwrap();
		let src_dir = make_src_dir(dir.path(), "# Demo Skill 规则内容\n");
		let target = SkillTarget::RulesDir {
			dir: PathBuf::from(".cursor/rules"),
			ext: "mdc".to_string(),
		};

		target
			.write_skill(dir.path(), "demo-skill", "", &src_dir)
			.unwrap();

		let written = fs::read_to_string(dir.path().join(".cursor/rules/demo-skill.mdc")).unwrap();
		assert_eq!(written, "# Demo Skill 规则内容\n");
	}

	// RulesDir::write_skill: src_dir 下没有 SKILL.md(源目录残缺)时应退化写占位说明,
	// 而不是报错中断整次 apply
	#[test]
	fn rules_dir_write_skill_falls_back_to_placeholder_when_skill_md_missing() {
		let dir = tempdir().unwrap();
		let src_dir = dir.path().join("empty-src");
		fs::create_dir_all(&src_dir).unwrap();
		let target = SkillTarget::RulesDir {
			dir: PathBuf::from(".cursor/rules"),
			ext: "mdc".to_string(),
		};

		target
			.write_skill(dir.path(), "demo-skill", "", &src_dir)
			.unwrap();

		let written = fs::read_to_string(dir.path().join(".cursor/rules/demo-skill.mdc")).unwrap();
		assert!(written.contains("demo-skill"), "占位说明应提及 skill 名");
	}

	// RulesDir::write_skill: 覆盖已存在的同名文件前应先生成时间戳备份(安全约束), 且新内容
	// 确实生效
	#[test]
	fn rules_dir_write_skill_backs_up_previous_file_before_overwrite() {
		let dir = tempdir().unwrap();
		let rules_dir = dir.path().join(".cursor/rules");
		fs::create_dir_all(&rules_dir).unwrap();
		fs::write(rules_dir.join("demo-skill.mdc"), "旧内容").unwrap();

		let src_dir = make_src_dir(dir.path(), "新内容\n");
		let target = SkillTarget::RulesDir {
			dir: PathBuf::from(".cursor/rules"),
			ext: "mdc".to_string(),
		};
		target
			.write_skill(dir.path(), "demo-skill", "", &src_dir)
			.unwrap();

		assert_eq!(
			fs::read_to_string(rules_dir.join("demo-skill.mdc")).unwrap(),
			"新内容\n"
		);
		let backups: Vec<_> = fs::read_dir(&rules_dir)
			.unwrap()
			.filter_map(Result::ok)
			.filter(|entry| entry.file_name().to_string_lossy().contains("skillhub-bak"))
			.collect();
		assert_eq!(backups.len(), 1, "覆盖前应生成一份备份");
		assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), "旧内容");
	}

	// RulesDir::remove_skill: 应删除目标文件; 目标本就不存在应 Ok 而非报错
	#[test]
	fn rules_dir_remove_skill_deletes_file_and_is_noop_when_missing() {
		let dir = tempdir().unwrap();
		let rules_dir = dir.path().join(".cursor/rules");
		fs::create_dir_all(&rules_dir).unwrap();
		fs::write(rules_dir.join("demo-skill.mdc"), "内容").unwrap();

		let target = SkillTarget::RulesDir {
			dir: PathBuf::from(".cursor/rules"),
			ext: "mdc".to_string(),
		};
		target.remove_skill(dir.path(), "demo-skill").unwrap();
		assert!(!rules_dir.join("demo-skill.mdc").exists());
		assert!(target.remove_skill(dir.path(), "demo-skill").is_ok());
	}

	// InstructionsFile::write_skill: 聚合文件不存在(首次写入)时应新建并写入一个标记块
	#[test]
	fn instructions_file_write_skill_creates_file_with_marked_block_when_absent() {
		let dir = tempdir().unwrap();
		let src_dir = make_src_dir(dir.path(), "demo-skill 的正文内容\n");
		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));

		target
			.write_skill(dir.path(), "demo-skill", "1.0.0", &src_dir)
			.unwrap();

		let text = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
		assert!(text.contains("<!-- skillhub:start:demo-skill@1.0.0 -->"));
		assert!(text.contains("demo-skill 的正文内容"));
		assert!(text.contains("<!-- skillhub:end:demo-skill -->"));

		let skills = target.read_skills(dir.path());
		assert_eq!(skills.len(), 1);
		assert_eq!(skills[0].version, "1.0.0");
	}

	// InstructionsFile::write_skill: 已存在同名旧版本块 + 其它 skill 的块 + 用户手写内容时,
	// Update 应只整块替换本 skill 的块(新版本/新内容), 其它 skill 的块与手写内容原样保留
	#[test]
	fn instructions_file_write_skill_replaces_existing_block_preserving_other_content() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("AGENTS.md");
		fs::write(
			&path,
			"# 手写的项目指令\n\n\
			<!-- skillhub:start:other-skill@1.0.0 -->\nother 内容\n<!-- skillhub:end:other-skill -->\n\n\
			<!-- skillhub:start:demo-skill@1.0.0 -->\n旧内容\n<!-- skillhub:end:demo-skill -->\n\n\
			# 手写的其它说明\n",
		)
		.unwrap();

		let src_dir = make_src_dir(dir.path(), "全新内容\n");
		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));
		target
			.write_skill(dir.path(), "demo-skill", "2.0.0", &src_dir)
			.unwrap();

		let text = fs::read_to_string(&path).unwrap();
		assert!(text.contains("# 手写的项目指令"), "手写内容应保留");
		assert!(
			text.contains("<!-- skillhub:start:other-skill@1.0.0 -->")
				&& text.contains("other 内容"),
			"其它 skill 的块应原样保留"
		);
		assert!(text.contains("# 手写的其它说明"), "手写内容应保留");
		assert!(
			text.contains("<!-- skillhub:start:demo-skill@2.0.0 -->") && text.contains("全新内容"),
			"本 skill 的块应更新为新版本/新内容"
		);
		assert!(!text.contains("旧内容"), "旧内容应被替换掉");

		let skills = target.read_skills(dir.path());
		assert_eq!(skills.len(), 2);
		let demo = skills.iter().find(|s| s.name == "demo-skill").unwrap();
		assert_eq!(demo.version, "2.0.0");
		assert!(skills.iter().any(|s| s.name == "other-skill"));
	}

	// InstructionsFile::write_skill: 写入前应对已存在的聚合文件生成时间戳备份
	#[test]
	fn instructions_file_write_skill_backs_up_file_before_modifying() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("AGENTS.md");
		fs::write(&path, "# 原始内容\n").unwrap();

		let src_dir = make_src_dir(dir.path(), "新内容\n");
		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));
		target
			.write_skill(dir.path(), "demo-skill", "1.0.0", &src_dir)
			.unwrap();

		let backups: Vec<_> = fs::read_dir(dir.path())
			.unwrap()
			.filter_map(Result::ok)
			.filter(|entry| entry.file_name().to_string_lossy().contains("skillhub-bak"))
			.collect();
		assert_eq!(backups.len(), 1);
		assert_eq!(
			fs::read_to_string(backups[0].path()).unwrap(),
			"# 原始内容\n"
		);
	}

	// InstructionsFile::remove_skill: 应只剥离本 skill 的标记块, 其它 skill 的块与手写内容
	// 原样保留
	#[test]
	fn instructions_file_remove_skill_strips_only_target_block_preserving_other_content() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("AGENTS.md");
		fs::write(
			&path,
			"# 手写内容\n\n\
			<!-- skillhub:start:demo-skill@1.0.0 -->\ndemo 内容\n<!-- skillhub:end:demo-skill -->\n\n\
			<!-- skillhub:start:other-skill@1.0.0 -->\nother 内容\n<!-- skillhub:end:other-skill -->\n",
		)
		.unwrap();

		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));
		target.remove_skill(dir.path(), "demo-skill").unwrap();

		let text = fs::read_to_string(&path).unwrap();
		assert!(!text.contains("demo-skill"), "本 skill 的块应被剥离");
		assert!(text.contains("# 手写内容"), "手写内容应保留");
		assert!(
			text.contains("<!-- skillhub:start:other-skill@1.0.0 -->")
				&& text.contains("other 内容"),
			"其它 skill 的块应原样保留"
		);

		let skills = target.read_skills(dir.path());
		assert_eq!(skills.len(), 1);
		assert_eq!(skills[0].name, "other-skill");
	}

	// InstructionsFile::remove_skill: 该 name 的块本就不存在(或文件本就不存在)时应 Ok 且
	// 不产生任何备份/改动(避免多余写入)
	#[test]
	fn instructions_file_remove_skill_is_noop_when_block_absent() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("AGENTS.md");
		fs::write(&path, "# 纯手写, 没有任何标记块\n").unwrap();

		let target = SkillTarget::InstructionsFile(PathBuf::from("AGENTS.md"));
		target.remove_skill(dir.path(), "demo-skill").unwrap();

		assert_eq!(
			fs::read_to_string(&path).unwrap(),
			"# 纯手写, 没有任何标记块\n",
			"内容应原样不变"
		);
		let backups: Vec<_> = fs::read_dir(dir.path())
			.unwrap()
			.filter_map(Result::ok)
			.filter(|entry| entry.file_name().to_string_lossy().contains("skillhub-bak"))
			.collect();
		assert!(backups.is_empty(), "无实际改动不应产生备份");

		// 文件本就不存在的场景也应是 Ok
		let missing_target = SkillTarget::InstructionsFile(PathBuf::from("MISSING.md"));
		assert!(missing_target
			.remove_skill(dir.path(), "demo-skill")
			.is_ok());
	}
}
