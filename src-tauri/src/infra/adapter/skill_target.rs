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

use crate::domain::agent::SkillRef;

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
}
