// 文件作用: 同步引擎领域类型 —— diff 计划与执行结果的数据形状; 仅类型定义, 供
//           infra::adapter::AgentAdapter trait 签名引用。不含 diff/reconcile 算法(见 Task 7)
// 创建日期: 2026-07-09

use serde::Serialize;

use crate::domain::resource::ResourceType;

/// diff 动作: 对应 sync_item.action 列
/// 1-新增, 2-更新, 3-移除
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffAction {
	Add,
	Update,
	Remove,
}

/// 一条资源相对某 Agent 实际态的差异
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffItem {
	pub res_type: ResourceType,
	pub name: String,
	pub action: DiffAction,
	pub local_ver: String,
	pub agent_ver: String,
}

/// 一次同步中某 Agent 待处理的完整差异计划
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffPlan {
	pub items: Vec<DiffItem>,
}

/// 单个 diff 项的执行结果(由 AgentAdapter::apply 产出)
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemOutcome {
	pub name: String,
	pub action: DiffAction,
	pub ok: bool,
	pub err: String,
}

#[cfg(test)]
mod tests {
	use super::*;

	// DiffAction: 三个动作变体应可构造且两两不等
	#[test]
	fn diff_action_variants_are_distinct() {
		assert_ne!(DiffAction::Add, DiffAction::Update);
		assert_ne!(DiffAction::Update, DiffAction::Remove);
		assert_ne!(DiffAction::Add, DiffAction::Remove);
	}

	// DiffItem: 序列化应使用 camelCase 字段名(resType/localVer/agentVer)
	#[test]
	fn diff_item_serializes_as_camel_case() {
		let item = DiffItem {
			res_type: ResourceType::Skill,
			name: "charles-coding".to_string(),
			action: DiffAction::Add,
			local_ver: "1.0.0".to_string(),
			agent_ver: String::new(),
		};
		let json = serde_json::to_value(&item).unwrap();
		assert_eq!(json["resType"], "Skill");
		assert_eq!(json["localVer"], "1.0.0");
		assert_eq!(json["agentVer"], "");
		assert!(json.get("local_ver").is_none());
		assert!(json.get("res_type").is_none());
	}

	// DiffPlan: 内嵌 DiffItem 列表应整体序列化成功, 数组元素字段亦为 camelCase
	#[test]
	fn diff_plan_serializes_nested_items() {
		let plan = DiffPlan {
			items: vec![DiffItem {
				res_type: ResourceType::Mcp,
				name: "filesystem".to_string(),
				action: DiffAction::Remove,
				local_ver: String::new(),
				agent_ver: "0.9.0".to_string(),
			}],
		};
		let json = serde_json::to_value(&plan).unwrap();
		assert_eq!(json["items"][0]["action"], "Remove");
		assert_eq!(json["items"][0]["agentVer"], "0.9.0");
	}

	// ItemOutcome: 序列化应保留 ok/err 字段, err 为空串表示成功
	#[test]
	fn item_outcome_serializes_ok_and_err_fields() {
		let outcome = ItemOutcome {
			name: "charles-coding".to_string(),
			action: DiffAction::Update,
			ok: true,
			err: String::new(),
		};
		let json = serde_json::to_value(&outcome).unwrap();
		assert_eq!(json["ok"], true);
		assert_eq!(json["err"], "");
		assert_eq!(json["action"], "Update");
	}
}
