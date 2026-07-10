// 文件作用: Tauri 命令层模块聚合 + 命令间共享的小工具函数(取家目录)
// 创建日期: 2026-07-09

use std::path::PathBuf;

pub mod agent;
pub mod auth;
pub mod dashboard;
pub mod health;
pub mod library;
pub mod market;
pub mod sync;

/// 取当前用户家目录; 取不到(容器/极端环境变量缺失等罕见场景)时返回错误信息而非 panic,
/// 供需要探测/读写本机配置文件的命令(agent_detect/sync_diff/sync_apply)统一复用
pub(crate) fn home_dir() -> Result<PathBuf, String> {
	dirs::home_dir().ok_or_else(|| "无法获取用户家目录".to_string())
}
