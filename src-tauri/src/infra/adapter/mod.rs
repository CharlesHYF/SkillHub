// 文件作用: AgentAdapter 抽象 —— 统一各 AI 工具(Claude Code/Desktop/Cursor/...)的探测/读态/应用接口,
//           并提供全量适配器注册表 all_adapters; 具体适配器实现见后续任务(Task 3-5)
// 创建日期: 2026-07-09

use std::path::Path;

use anyhow::Result;

use crate::domain::agent::{ActualState, AgentKind, DetectedAgent};
use crate::domain::resource::ResourceType;
use crate::domain::sync::{DiffPlan, ItemOutcome};

/// 统一封装"探测本机某类 AI 工具 + 读取其配置实际态 + 把差异计划应用回配置文件"的能力。
/// 每种 AgentKind(Claude Code/Cursor/...) 各实现一个适配器; 方法均不含泛型/`Self` 返回值,
/// trait 对象安全, 供 all_adapters 统一装箱为 `Vec<Box<dyn AgentAdapter>>`。
pub trait AgentAdapter {
	/// 本适配器对应的 Agent 种类
	fn kind(&self) -> AgentKind;

	/// 本适配器是否支持同步给定资源类型(如某工具暂不支持 MCP)
	fn supports(&self, ty: ResourceType) -> bool;

	/// 探测本机是否安装/配置了该工具; 可能发现多个实例(全局 + 若干项目级)
	fn detect(&self) -> Vec<DetectedAgent>;

	/// 读取某个已探测到的 Agent 实例当前的实际态(已配置的 MCP/Skill 清单)
	fn read_state(&self, agent: &DetectedAgent) -> Result<ActualState>;

	/// 把差异计划应用到该 Agent 的配置文件, 返回每一项的执行结果
	fn apply(&self, agent: &DetectedAgent, plan: &DiffPlan) -> Result<Vec<ItemOutcome>>;
}

/// 全量适配器注册表; `home` 为家目录(测试时可注入临时目录, 避免探测逻辑触碰真实机器配置)。
/// 本任务(Task 2)先不注册具体适配器, 返回空表; 具体适配器由 Task 3-5 陆续接入并 push 进本函数。
pub fn all_adapters(_home: &Path) -> Vec<Box<dyn AgentAdapter>> {
	Vec::new()
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::*;
	use crate::domain::agent::AgentScope;

	/// 占位实现: 仅用于验证 AgentAdapter trait 对象安全 —— 可被具体类型实现并装箱进
	/// `Vec<Box<dyn AgentAdapter>>`, 且各方法均可正常调用
	struct FakeAdapter;

	impl AgentAdapter for FakeAdapter {
		fn kind(&self) -> AgentKind {
			AgentKind::ClaudeCode
		}

		fn supports(&self, ty: ResourceType) -> bool {
			matches!(ty, ResourceType::Skill)
		}

		fn detect(&self) -> Vec<DetectedAgent> {
			Vec::new()
		}

		fn read_state(&self, _agent: &DetectedAgent) -> Result<ActualState> {
			Ok(ActualState {
				mcp: Vec::new(),
				skills: Vec::new(),
			})
		}

		fn apply(&self, _agent: &DetectedAgent, _plan: &DiffPlan) -> Result<Vec<ItemOutcome>> {
			Ok(Vec::new())
		}
	}

	// AgentAdapter 应对象安全: 可装箱为 trait object 并逐一调用全部方法
	#[test]
	fn agent_adapter_trait_is_object_safe() {
		let adapters: Vec<Box<dyn AgentAdapter>> = vec![Box::new(FakeAdapter)];
		let adapter = &adapters[0];
		assert_eq!(adapter.kind(), AgentKind::ClaudeCode);
		assert!(adapter.supports(ResourceType::Skill));
		assert!(!adapter.supports(ResourceType::Mcp));
		assert!(adapter.detect().is_empty());

		let probe = DetectedAgent {
			kind: AgentKind::ClaudeCode,
			name: "Claude Code".to_string(),
			config_path: "/tmp/does-not-matter".to_string(),
			scope: AgentScope::Global,
			online: true,
		};
		let state = adapter.read_state(&probe).unwrap();
		assert!(state.mcp.is_empty());
		assert!(state.skills.is_empty());

		let outcomes = adapter
			.apply(&probe, &DiffPlan { items: Vec::new() })
			.unwrap();
		assert!(outcomes.is_empty());
	}

	// all_adapters 本任务先返回空表, 具体适配器留给 Task 3-5 接入
	#[test]
	fn all_adapters_returns_empty_for_now() {
		let home = PathBuf::from("/tmp/skillhub-test-home");
		assert!(all_adapters(&home).is_empty());
	}
}
