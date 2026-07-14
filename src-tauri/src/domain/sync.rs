// 文件作用: 同步引擎领域类型与声明式协调算法 —— diff 计划/执行结果的数据形状(DiffAction/
//           DiffItem/DiffPlanRespVO/ItemOutcome)、期望资源的富类型(DesiredPayload/DesiredResource),
//           以及纯函数 reconcile(比较期望态与某 Agent 实际态, 产出待应用的差异计划), 供
//           infra::adapter::AgentAdapter::apply(Task 7b)与 services::sync 编排(Task 7c)使用。
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::agent::{ActualState, McpServerDef};
use crate::domain::resource::ResourceType;

/// diff 动作: 对应 sync_item.action 列
/// 1-新增, 2-更新, 3-移除
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffAction {
	Add,
	Update,
	Remove,
}

/// 期望资源的可落地内容: MCP 是服务定义(可直接写回配置文件), Skill 是本地源目录路径(供 apply
/// 复制到工具的 Skill 落地形态, 见 Task 7b)。Skill 变体单独声明 camelCase, 使其字段在 JSON 里
/// 为 srcDir; 变体标签本身不转 case(与 ResourceType 等既有枚举"标签保持 PascalCase, 只转字段名"
/// 的约定一致)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum DesiredPayload {
	Mcp(McpServerDef),
	#[serde(rename_all = "camelCase")]
	Skill {
		src_dir: String,
	},
}

/// 期望同步的一个资源目标: 类型 + 名称 + 版本 + 可落地内容; reconcile 的输入之一, 与某 Agent
/// 的 ActualState 比较后产出 DiffPlanRespVO。`res_type` 应与 `payload` 的变体形状保持一致(通常两者
/// 都源自同一份 ResourceRespVO 记录), reconcile 对不一致的脏数据只做静默跳过, 不 panic
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesiredResource {
	pub res_type: ResourceType,
	pub name: String,
	pub version: String,
	pub payload: DesiredPayload,
}

/// 一条资源相对某 Agent 实际态的差异
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffItem {
	pub res_type: ResourceType,
	pub name: String,
	pub action: DiffAction,
	pub local_ver: String,
	pub agent_ver: String,
	/// 待写入内容: Add/Update 为 Some(携带 reconcile 时刻的期望内容, 供 apply 直接落地写入,
	/// 不必回头再查一次 desired); Remove 为 None(删除操作不需要内容)
	pub payload: Option<DesiredPayload>,
}

/// 一次同步中某 Agent 待处理的完整差异计划
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffPlanRespVO {
	pub items: Vec<DiffItem>,
}

/// 单个 diff 项的执行结果(由 AgentAdapter::apply 产出)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemOutcome {
	pub name: String,
	pub action: DiffAction,
	pub ok: bool,
	pub err: String,
}

/// 纯函数: 比较期望态(desired)与某 Agent 的实际态(actual), 产出待应用的差异计划。
///
/// 安全边界(第一优先级): `managed` 登记的是"SkillHub 曾经在这台 Agent 上落地过的
/// (res_type,name)"集合。只有同时满足"在 managed 里"且"本次 desired 未再提及"的实际项才会被
/// 判定为 Remove; 不在 managed 里的实际项——不管是用户手写的还是别的工具装的——一律不碰, 即使
/// 它与某个 desired 同名同类型也不会被误判成"已托管"(managed 是唯一的托管凭证, 不做同名推断,
/// 绝不动用户自己的、SkillHub 从未托管过的配置)。managed 集合只参与 Remove 判定, 不参与
/// Add/Update 判定(desired 里出现的资源始终会被正常 Add/Update, 与是否已在 managed 无关)。
///
/// Add/Update 按资源类型分别判定:
/// - MCP: 按 name 在 `actual.mcp` 里找。找不到 -> Add; 找到但内容(McpServerDef 的
///   command/args/env/url, 全部参与 PartialEq)有差异 -> Update; 完全相同 -> no-op(不入 plan)。
///   MCP 无版本概念(McpServerDef 不带 version 字段), 故 Add/Update 的 agent_ver 恒为空串。
/// - Skill: 按 name 在 `actual.skills` 里找。找不到 -> Add; 找到但 version 不同 -> Update
///   (agent_ver 取 actual 侧当前版本); 版本相同 -> no-op。
pub fn reconcile(
	desired: &[DesiredResource],
	actual: &ActualState,
	managed: &BTreeSet<(ResourceType, String)>,
) -> DiffPlanRespVO {
	let mut items: Vec<DiffItem> = Vec::new();

	for item in desired {
		if let Some(diff_item) = add_or_update_item(item, actual) {
			items.push(diff_item);
		}
	}

	let desired_keys: BTreeSet<(ResourceType, String)> = desired
		.iter()
		.map(|item| (item.res_type, item.name.clone()))
		.collect();

	for mcp in &actual.mcp {
		let key = (ResourceType::Mcp, mcp.name.clone());
		if managed.contains(&key) && !desired_keys.contains(&key) {
			items.push(DiffItem {
				res_type: ResourceType::Mcp,
				name: mcp.name.clone(),
				action: DiffAction::Remove,
				local_ver: String::new(),
				agent_ver: String::new(),
				payload: None,
			});
		}
	}

	for skill in &actual.skills {
		let key = (ResourceType::Skill, skill.name.clone());
		if managed.contains(&key) && !desired_keys.contains(&key) {
			items.push(DiffItem {
				res_type: ResourceType::Skill,
				name: skill.name.clone(),
				action: DiffAction::Remove,
				local_ver: String::new(),
				agent_ver: skill.version.clone(),
				payload: None,
			});
		}
	}

	DiffPlanRespVO { items }
}

/// 对单个 desired 项按其 res_type/payload 形状分派到 MCP/Skill 各自的 Add/Update 判定;
/// 两者形状不一致(调用方构造出的脏数据)时静默跳过, 返回 None(既不 Add/Update, 也不 panic)
fn add_or_update_item(item: &DesiredResource, actual: &ActualState) -> Option<DiffItem> {
	match (item.res_type, &item.payload) {
		(ResourceType::Mcp, DesiredPayload::Mcp(desired_def)) => {
			add_or_update_mcp(item, desired_def, actual)
		}
		(ResourceType::Skill, DesiredPayload::Skill { .. }) => add_or_update_skill(item, actual),
		_ => None,
	}
}

/// MCP 一条的 Add/Update 判定: 按 name 在 actual.mcp 里找同名项, 按内容(McpServerDef 的
/// PartialEq, 覆盖 command/args/env/url 全部字段)比较是否需要 Update
fn add_or_update_mcp(
	item: &DesiredResource,
	desired_def: &McpServerDef,
	actual: &ActualState,
) -> Option<DiffItem> {
	match actual.mcp.iter().find(|mcp| mcp.name == item.name) {
		None => Some(DiffItem {
			res_type: ResourceType::Mcp,
			name: item.name.clone(),
			action: DiffAction::Add,
			local_ver: item.version.clone(),
			agent_ver: String::new(),
			payload: Some(item.payload.clone()),
		}),
		Some(actual_def) if actual_def != desired_def => Some(DiffItem {
			res_type: ResourceType::Mcp,
			name: item.name.clone(),
			action: DiffAction::Update,
			local_ver: item.version.clone(),
			agent_ver: String::new(),
			payload: Some(item.payload.clone()),
		}),
		Some(_) => None,
	}
}

/// Skill 一条的 Add/Update 判定: 按 name 在 actual.skills 里找同名项, 按 version 字段是否
/// 相同判定是否需要 Update
fn add_or_update_skill(item: &DesiredResource, actual: &ActualState) -> Option<DiffItem> {
	match actual.skills.iter().find(|skill| skill.name == item.name) {
		None => Some(DiffItem {
			res_type: ResourceType::Skill,
			name: item.name.clone(),
			action: DiffAction::Add,
			local_ver: item.version.clone(),
			agent_ver: String::new(),
			payload: Some(item.payload.clone()),
		}),
		Some(actual_ref) if actual_ref.version != item.version => Some(DiffItem {
			res_type: ResourceType::Skill,
			name: item.name.clone(),
			action: DiffAction::Update,
			local_ver: item.version.clone(),
			agent_ver: actual_ref.version.clone(),
			payload: Some(item.payload.clone()),
		}),
		Some(_) => None,
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use super::*;
	use crate::domain::agent::SkillRef;

	// DiffAction: 三个动作变体应可构造且两两不等
	#[test]
	fn diff_action_variants_are_distinct() {
		assert_ne!(DiffAction::Add, DiffAction::Update);
		assert_ne!(DiffAction::Update, DiffAction::Remove);
		assert_ne!(DiffAction::Add, DiffAction::Remove);
	}

	// DiffItem: 序列化应使用 camelCase 字段名(resType/localVer/agentVer), payload 字段本身
	// 也已是 camelCase(单词, 无需转换)
	#[test]
	fn diff_item_serializes_as_camel_case() {
		let item = DiffItem {
			res_type: ResourceType::Skill,
			name: "charles-coding".to_string(),
			action: DiffAction::Add,
			local_ver: "1.0.0".to_string(),
			agent_ver: String::new(),
			payload: None,
		};
		let json = serde_json::to_value(&item).unwrap();
		assert_eq!(json["resType"], "Skill");
		assert_eq!(json["localVer"], "1.0.0");
		assert_eq!(json["agentVer"], "");
		assert!(json["payload"].is_null());
		assert!(json.get("local_ver").is_none());
		assert!(json.get("res_type").is_none());
	}

	// DiffPlanRespVO: 内嵌 DiffItem 列表应整体序列化成功, 数组元素字段亦为 camelCase
	#[test]
	fn diff_plan_serializes_nested_items() {
		let plan = DiffPlanRespVO {
			items: vec![DiffItem {
				res_type: ResourceType::Mcp,
				name: "filesystem".to_string(),
				action: DiffAction::Remove,
				local_ver: String::new(),
				agent_ver: "0.9.0".to_string(),
				payload: None,
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

	// DesiredPayload::Skill: 字段应序列化为 camelCase(srcDir); 变体标签本身保持 PascalCase
	// ("Skill"), 与 ResourceType 等既有枚举"标签不转 case, 只转结构体字段"的约定一致
	#[test]
	fn desired_payload_skill_variant_serializes_field_as_camel_case() {
		let payload = DesiredPayload::Skill {
			src_dir: "/src/demo".to_string(),
		};
		let json = serde_json::to_value(&payload).unwrap();
		assert_eq!(json["Skill"]["srcDir"], "/src/demo");
		assert!(json["Skill"].get("src_dir").is_none());
	}

	// DesiredResource: 应整体序列化为 camelCase(resType/name/version/payload), 且能通过 JSON
	// 往返还原(验证新增的 Deserialize 派生生效)
	#[test]
	fn desired_resource_round_trips_through_json_with_camel_case_fields() {
		let resource = DesiredResource {
			res_type: ResourceType::Mcp,
			name: "server-a".to_string(),
			version: "1.0.0".to_string(),
			payload: DesiredPayload::Mcp(McpServerDef {
				name: "server-a".to_string(),
				command: Some("node".to_string()),
				args: vec!["index.js".to_string()],
				env: BTreeMap::new(),
				url: None,
			}),
		};
		let json = serde_json::to_value(&resource).unwrap();
		assert_eq!(json["resType"], "Mcp");
		assert_eq!(json["version"], "1.0.0");
		assert!(json.get("res_type").is_none());

		let text = serde_json::to_string(&resource).unwrap();
		let back: DesiredResource = serde_json::from_str(&text).unwrap();
		assert_eq!(back, resource);
	}

	// DiffItem.payload: Some/None 两种取值都应能完整 JSON 往返(验证新增字段 + 新增 Deserialize)
	#[test]
	fn diff_item_payload_field_round_trips_both_some_and_none() {
		let with_payload = DiffItem {
			res_type: ResourceType::Skill,
			name: "demo-skill".to_string(),
			action: DiffAction::Add,
			local_ver: "1.0.0".to_string(),
			agent_ver: String::new(),
			payload: Some(DesiredPayload::Skill {
				src_dir: "/src/demo-skill".to_string(),
			}),
		};
		let back: DiffItem =
			serde_json::from_str(&serde_json::to_string(&with_payload).unwrap()).unwrap();
		assert_eq!(back, with_payload);

		let without_payload = DiffItem {
			payload: None,
			..with_payload
		};
		let json = serde_json::to_value(&without_payload).unwrap();
		assert!(json["payload"].is_null());
		let back2: DiffItem =
			serde_json::from_str(&serde_json::to_string(&without_payload).unwrap()).unwrap();
		assert_eq!(back2, without_payload);
	}

	/// 构造一个 MCP 型 DesiredResource(name 同时用作 McpServerDef.name), env 固定为空、url 固定
	/// 为 None, 覆盖测试所需的最小字段集
	fn desired_mcp(name: &str, version: &str, command: &str, args: &[&str]) -> DesiredResource {
		DesiredResource {
			res_type: ResourceType::Mcp,
			name: name.to_string(),
			version: version.to_string(),
			payload: DesiredPayload::Mcp(actual_mcp(name, command, args)),
		}
	}

	/// 构造一个 Skill 型 DesiredResource
	fn desired_skill(name: &str, version: &str, src_dir: &str) -> DesiredResource {
		DesiredResource {
			res_type: ResourceType::Skill,
			name: name.to_string(),
			version: version.to_string(),
			payload: DesiredPayload::Skill {
				src_dir: src_dir.to_string(),
			},
		}
	}

	/// 构造 ActualState.mcp 里的一条 McpServerDef(env 固定为空、url 固定为 None)
	fn actual_mcp(name: &str, command: &str, args: &[&str]) -> McpServerDef {
		McpServerDef {
			name: name.to_string(),
			command: Some(command.to_string()),
			args: args.iter().map(|a| a.to_string()).collect(),
			env: BTreeMap::new(),
			url: None,
		}
	}

	/// 构造 ActualState.skills 里的一条 SkillRef
	fn actual_skill(name: &str, version: &str) -> SkillRef {
		SkillRef {
			name: name.to_string(),
			version: version.to_string(),
		}
	}

	// reconcile: desired 有、actual 无 -> 各产出 1 条 Add, payload 携带期望内容(MCP/Skill 各一例)
	#[test]
	fn reconcile_produces_add_for_items_missing_from_actual() {
		let desired = vec![
			desired_mcp("server-a", "1.0.0", "node", &["index.js"]),
			desired_skill("skill-a", "2.0.0", "/src/skill-a"),
		];
		let actual = ActualState {
			mcp: Vec::new(),
			skills: Vec::new(),
		};
		let managed = BTreeSet::new();

		let plan = reconcile(&desired, &actual, &managed);

		assert_eq!(plan.items.len(), 2);

		let mcp_item = plan.items.iter().find(|i| i.name == "server-a").unwrap();
		assert_eq!(mcp_item.res_type, ResourceType::Mcp);
		assert_eq!(mcp_item.action, DiffAction::Add);
		assert_eq!(mcp_item.local_ver, "1.0.0");
		assert_eq!(mcp_item.agent_ver, "");
		assert_eq!(mcp_item.payload, Some(desired[0].payload.clone()));

		let skill_item = plan.items.iter().find(|i| i.name == "skill-a").unwrap();
		assert_eq!(skill_item.res_type, ResourceType::Skill);
		assert_eq!(skill_item.action, DiffAction::Add);
		assert_eq!(skill_item.local_ver, "2.0.0");
		assert_eq!(skill_item.agent_ver, "");
		assert_eq!(skill_item.payload, Some(desired[1].payload.clone()));
	}

	// reconcile: MCP 同名但 command/args 与实际不同 -> Update, payload 携带期望(新)内容
	#[test]
	fn reconcile_produces_update_for_mcp_with_different_content() {
		let desired = vec![desired_mcp("server-a", "1.1.0", "node", &["new.js"])];
		let actual = ActualState {
			mcp: vec![actual_mcp("server-a", "node", &["old.js"])],
			skills: Vec::new(),
		};
		let managed = BTreeSet::new();

		let plan = reconcile(&desired, &actual, &managed);

		assert_eq!(plan.items.len(), 1);
		let item = &plan.items[0];
		assert_eq!(item.res_type, ResourceType::Mcp);
		assert_eq!(item.name, "server-a");
		assert_eq!(item.action, DiffAction::Update);
		assert_eq!(item.local_ver, "1.1.0");
		assert_eq!(item.agent_ver, "");
		assert_eq!(item.payload, Some(desired[0].payload.clone()));
	}

	// reconcile: MCP 同名且内容(command/args/env/url)全同 -> 不产生 item(no-op)
	#[test]
	fn reconcile_produces_no_item_for_mcp_with_identical_content() {
		let desired = vec![desired_mcp("server-a", "1.0.0", "node", &["index.js"])];
		let actual = ActualState {
			mcp: vec![actual_mcp("server-a", "node", &["index.js"])],
			skills: Vec::new(),
		};
		let managed = BTreeSet::new();

		let plan = reconcile(&desired, &actual, &managed);

		assert!(plan.items.is_empty());
	}

	// reconcile: Skill 同名但 version 与实际不同 -> Update, agent_ver 取 actual 侧当前版本
	#[test]
	fn reconcile_produces_update_for_skill_with_different_version() {
		let desired = vec![desired_skill("skill-a", "2.0.0", "/src/skill-a")];
		let actual = ActualState {
			mcp: Vec::new(),
			skills: vec![actual_skill("skill-a", "1.0.0")],
		};
		let managed = BTreeSet::new();

		let plan = reconcile(&desired, &actual, &managed);

		assert_eq!(plan.items.len(), 1);
		let item = &plan.items[0];
		assert_eq!(item.res_type, ResourceType::Skill);
		assert_eq!(item.action, DiffAction::Update);
		assert_eq!(item.local_ver, "2.0.0");
		assert_eq!(item.agent_ver, "1.0.0");
		assert_eq!(item.payload, Some(desired[0].payload.clone()));
	}

	// reconcile: actual 有、desired 未再提及、且 (res_type,name) 在 managed 里 -> Remove
	// (MCP/Skill 各一例)
	#[test]
	fn reconcile_produces_remove_for_managed_items_absent_from_desired() {
		let desired: Vec<DesiredResource> = Vec::new();
		let actual = ActualState {
			mcp: vec![actual_mcp("server-a", "node", &["index.js"])],
			skills: vec![actual_skill("skill-a", "1.0.0")],
		};
		let mut managed = BTreeSet::new();
		managed.insert((ResourceType::Mcp, "server-a".to_string()));
		managed.insert((ResourceType::Skill, "skill-a".to_string()));

		let plan = reconcile(&desired, &actual, &managed);

		assert_eq!(plan.items.len(), 2);

		let mcp_item = plan.items.iter().find(|i| i.name == "server-a").unwrap();
		assert_eq!(mcp_item.res_type, ResourceType::Mcp);
		assert_eq!(mcp_item.action, DiffAction::Remove);
		assert_eq!(mcp_item.local_ver, "");
		assert_eq!(mcp_item.agent_ver, "");
		assert_eq!(mcp_item.payload, None);

		let skill_item = plan.items.iter().find(|i| i.name == "skill-a").unwrap();
		assert_eq!(skill_item.res_type, ResourceType::Skill);
		assert_eq!(skill_item.action, DiffAction::Remove);
		assert_eq!(skill_item.local_ver, "");
		assert_eq!(skill_item.agent_ver, "1.0.0");
		assert_eq!(skill_item.payload, None);
	}

	// reconcile: actual 有、desired 未再提及, 但 (res_type,name) 不在 managed 里(用户自己配的,
	// SkillHub 从未托管) -> 不产生 item, 绝不误删
	#[test]
	fn reconcile_retains_unmanaged_items_absent_from_desired() {
		let desired: Vec<DesiredResource> = Vec::new();
		let actual = ActualState {
			mcp: vec![actual_mcp("user-own-server", "python", &["server.py"])],
			skills: vec![actual_skill("user-own-skill", "9.9.9")],
		};
		let managed = BTreeSet::new(); // 均未被 SkillHub 托管过

		let plan = reconcile(&desired, &actual, &managed);

		assert!(plan.items.is_empty());
	}

	// reconcile: 混合场景 -> 一次调用里同时验证 Add/Update/no-op/Remove(managed)/保留 unmanaged
	#[test]
	fn reconcile_mixed_scenario_combines_all_actions() {
		let desired = vec![
			desired_mcp("added-server", "1.0.0", "node", &["a.js"]), // actual 无 -> Add
			desired_mcp("changed-server", "1.2.0", "node", &["new.js"]), // 内容不同 -> Update
			desired_mcp("same-server", "1.0.0", "node", &["same.js"]), // 内容相同 -> no-op
			desired_skill("changed-skill", "3.0.0", "/src/changed-skill"), // 版本不同 -> Update
		];
		let actual = ActualState {
			mcp: vec![
				actual_mcp("changed-server", "node", &["old.js"]),
				actual_mcp("same-server", "node", &["same.js"]),
				actual_mcp("removed-server", "node", &["gone.js"]), // 未再提及 + managed -> Remove
				actual_mcp("user-own-server", "python", &["server.py"]), // 未再提及 + 非 managed -> 保留
			],
			skills: vec![actual_skill("changed-skill", "2.0.0")],
		};
		let mut managed = BTreeSet::new();
		managed.insert((ResourceType::Mcp, "changed-server".to_string()));
		managed.insert((ResourceType::Mcp, "same-server".to_string()));
		managed.insert((ResourceType::Mcp, "removed-server".to_string()));
		managed.insert((ResourceType::Skill, "changed-skill".to_string()));
		// user-own-server 故意不放进 managed, 模拟用户自己配的、SkillHub 从未托管过的 MCP

		let plan = reconcile(&desired, &actual, &managed);

		assert_eq!(
			plan.items.len(),
			4,
			"应为 added/changed-server/changed-skill/removed-server 四条; \
			 same-server no-op、user-own-server 保留, 均不入 plan"
		);

		let added = plan
			.items
			.iter()
			.find(|i| i.name == "added-server")
			.unwrap();
		assert_eq!(added.action, DiffAction::Add);

		let changed_server = plan
			.items
			.iter()
			.find(|i| i.name == "changed-server")
			.unwrap();
		assert_eq!(changed_server.action, DiffAction::Update);

		let changed_skill = plan
			.items
			.iter()
			.find(|i| i.name == "changed-skill")
			.unwrap();
		assert_eq!(changed_skill.action, DiffAction::Update);
		assert_eq!(changed_skill.agent_ver, "2.0.0");

		let removed = plan
			.items
			.iter()
			.find(|i| i.name == "removed-server")
			.unwrap();
		assert_eq!(removed.action, DiffAction::Remove);

		assert!(
			plan.items.iter().all(|i| i.name != "same-server"),
			"same-server 内容相同应 no-op"
		);
		assert!(
			plan.items.iter().all(|i| i.name != "user-own-server"),
			"user-own-server 未被 managed 登记, 不应被删"
		);
	}
}
