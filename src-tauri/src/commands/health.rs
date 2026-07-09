// 文件作用: 应用健康检查命令(M0 用于打通前后端调用链路)
// 创建日期: 2026-07-09

use serde::Serialize;

/// 健康信息: 应用版本 + 数据库是否就绪
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppHealth {
	pub version: String,
	pub db_ok: bool,
}

/// 纯逻辑: 组装健康信息(便于单测, 与 Tauri 运行时解耦)
pub fn build_health(version: &str, db_ok: bool) -> AppHealth {
	AppHealth {
		version: version.to_string(),
		db_ok,
	}
}

/// Tauri 命令: 返回应用健康信息
#[tauri::command]
pub fn app_health(state: tauri::State<'_, crate::AppState>) -> AppHealth {
	build_health(env!("CARGO_PKG_VERSION"), state.db_ok)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn build_health_maps_fields() {
		let h = build_health("0.1.0", true);
		assert_eq!(h.version, "0.1.0");
		assert!(h.db_ok);
	}
}
