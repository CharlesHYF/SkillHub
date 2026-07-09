// 文件作用: adapter 层共享工具函数 —— 写入/删除配置文件前的安全备份(backup_file), 以及
//           ItemOutcome 的成功/失败构造 helper(ok_outcome/err_outcome)与 Skill 类 DiffItem
//           的通用落地分派(apply_skill_item, 供 JsonMcpAdapter/CodexAdapter::apply 共用,
//           避免两个适配器各写一份相同的"按 action 调 SkillTarget::write_skill/remove_skill
//           并包装 ItemOutcome"逻辑, 与 skill_target.rs 把 Skill 读取逻辑集中一处的思路一致)。
// 创建日期: 2026-07-09

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::sync::{DesiredPayload, DiffAction, DiffItem, ItemOutcome};

use super::skill_target::SkillTarget;

/// 任何写入/删除配置文件之前都先调用本函数备份: 目标文件存在时, 复制成带秒级时间戳后缀的
/// 副本(如 `foo.json` -> `foo.json.skillhub-bak.1720000000`), 与原文件同目录; 目标文件不
/// 存在(如该工具尚未生成过这份配置, 本次是首次写入)视为无需备份, 直接返回 Ok, 不算错误
pub fn backup_file(path: &Path) -> std::io::Result<()> {
	if !path.is_file() {
		return Ok(());
	}
	let secs = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| d.as_secs())
		.unwrap_or(0);
	let mut backup_name = path.as_os_str().to_os_string();
	backup_name.push(format!(".skillhub-bak.{secs}"));
	fs::copy(path, PathBuf::from(backup_name))?;
	Ok(())
}

/// 构造一条成功的 ItemOutcome(err 为空串)
pub fn ok_outcome(item: &DiffItem) -> ItemOutcome {
	ItemOutcome {
		name: item.name.clone(),
		action: item.action,
		ok: true,
		err: String::new(),
	}
}

/// 构造一条失败的 ItemOutcome, 携带具体错误信息
pub fn err_outcome(item: &DiffItem, err: impl Into<String>) -> ItemOutcome {
	ItemOutcome {
		name: item.name.clone(),
		action: item.action,
		ok: false,
		err: err.into(),
	}
}

/// 把单个 res_type==Skill 的 DiffItem 应用到给定 SkillTarget: Add/Update 取 payload 里的
/// src_dir 调 write_skill(version 取 item.local_ver); Remove 调 remove_skill。action 与
/// payload 形状不符(调用方传入的脏数据, 如 Add 却没带 Skill payload)时不 panic, 产出
/// ok=false 的 ItemOutcome, 供调用方(各 AgentAdapter::apply)在单项失败时仍继续处理其它项
pub fn apply_skill_item(target: &SkillTarget, home: &Path, item: &DiffItem) -> ItemOutcome {
	let result = match (item.action, &item.payload) {
		(DiffAction::Add, Some(DesiredPayload::Skill { src_dir }))
		| (DiffAction::Update, Some(DesiredPayload::Skill { src_dir })) => {
			target.write_skill(home, &item.name, &item.local_ver, Path::new(src_dir))
		}
		(DiffAction::Remove, _) => target.remove_skill(home, &item.name),
		(action, payload) => Err(anyhow::anyhow!(
			"Skill 项 {} 的 action({action:?})与 payload 形状不符({payload:?}), 无法应用",
			item.name
		)),
	};
	match result {
		Ok(()) => ok_outcome(item),
		Err(err) => err_outcome(item, err.to_string()),
	}
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::*;

	// backup_file: 目标文件存在时应生成一份带 .skillhub-bak.<unix_secs> 后缀的副本,
	// 内容与原文件一致, 且原文件本身保持不变
	#[test]
	fn backup_file_creates_timestamped_copy_when_target_exists() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("foo.json");
		fs::write(&path, r#"{"a":1}"#).unwrap();

		backup_file(&path).unwrap();

		let backups: Vec<_> = fs::read_dir(dir.path())
			.unwrap()
			.filter_map(Result::ok)
			.filter(|entry| entry.file_name().to_string_lossy().contains("skillhub-bak"))
			.collect();
		assert_eq!(backups.len(), 1, "应恰好生成一份备份");
		let backup_content = fs::read_to_string(backups[0].path()).unwrap();
		assert_eq!(backup_content, r#"{"a":1}"#);
		assert_eq!(
			fs::read_to_string(&path).unwrap(),
			r#"{"a":1}"#,
			"原文件应保持不变"
		);
	}

	// backup_file: 目标文件不存在(如首次写入前)应直接返回 Ok, 不产生任何备份文件, 不报错
	#[test]
	fn backup_file_is_noop_when_target_missing() {
		let dir = tempdir().unwrap();
		let path = dir.path().join("missing.json");

		backup_file(&path).unwrap();

		assert_eq!(
			fs::read_dir(dir.path()).unwrap().count(),
			0,
			"不应生成任何文件"
		);
	}

	// ok_outcome/err_outcome: 应正确回填 name/action, ok 与 err 分别对应成功/失败语义
	#[test]
	fn ok_and_err_outcome_helpers_fill_expected_fields() {
		let item = DiffItem {
			res_type: crate::domain::resource::ResourceType::Skill,
			name: "demo-skill".to_string(),
			action: DiffAction::Update,
			local_ver: "1.0.0".to_string(),
			agent_ver: "0.9.0".to_string(),
			payload: None,
		};

		let ok = ok_outcome(&item);
		assert_eq!(ok.name, "demo-skill");
		assert_eq!(ok.action, DiffAction::Update);
		assert!(ok.ok);
		assert_eq!(ok.err, "");

		let err = err_outcome(&item, "写入失败");
		assert_eq!(err.name, "demo-skill");
		assert!(!err.ok);
		assert_eq!(err.err, "写入失败");
	}
}
