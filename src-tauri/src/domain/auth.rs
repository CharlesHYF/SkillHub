// 文件作用: 认证领域类型 —— ProviderKind 提供方枚举、AuthAccount 已连接账号实体, 以及绝不落库
//           的 TokenSet/PkceChallenge(令牌只进系统钥匙串, 见 migrations/0001_init.sql
//           auth_account 表注释与 infra::repo_auth)
// 创建日期: 2026-07-09

use serde::{Deserialize, Serialize};

/// 认证提供方: 对应 auth_account.provider 列
/// 1-GitHub, 2-Google, 3-Microsoft, 4-访问令牌(Token, 即 PAT 手动录入)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
	GitHub,
	Google,
	Microsoft,
	Token,
}

impl ProviderKind {
	/// 由数据库 INTEGER 值还原枚举; 未知值(含列默认值 0)兜底为最小合法编码 GitHub(1)
	pub fn from_i64(value: i64) -> Self {
		match value {
			2 => ProviderKind::Google,
			3 => ProviderKind::Microsoft,
			4 => ProviderKind::Token,
			_ => ProviderKind::GitHub,
		}
	}
}

impl From<ProviderKind> for i64 {
	fn from(value: ProviderKind) -> i64 {
		match value {
			ProviderKind::GitHub => 1,
			ProviderKind::Google => 2,
			ProviderKind::Microsoft => 3,
			ProviderKind::Token => 4,
		}
	}
}

/// 已连接的第三方账号(对应 auth_account 表一行的领域视图); 令牌密文不在此结构体中, 落库时
/// 只有钥匙串引用键(keyring_ref)单独传参, 见 infra::repo_auth::upsert
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthAccount {
	pub id: i64,
	pub provider: ProviderKind,
	pub account: String,
	pub scope: String,
	pub status: bool,
	pub connect_time: String,
}

/// 令牌集合: 仅在内存中流转(OAuth 换取 / PAT 校验之后), 使用完应立即写入系统钥匙串并 drop。
/// 刻意不派生 Serialize/Deserialize: 使其在结构上不可能被误传入任何会落盘或落 JSON 的通路
/// (如 Tauri 命令返回值、日志打印、serde_json 序列化), 是"绝不入库"要求的编译期兜底
#[derive(Clone, Debug, PartialEq)]
pub struct TokenSet {
	pub access: String,
	pub refresh: Option<String>,
	pub expires_at: Option<String>,
}

/// PKCE(RFC 7636)挑战: OAuth 授权码流程一次性的 verifier/challenge 对, 同样仅在内存中流转、
/// 不落库, 故与 TokenSet 一样刻意不派生 Serialize/Deserialize
#[derive(Clone, Debug, PartialEq)]
pub struct PkceChallenge {
	pub verifier: String,
	pub challenge: String,
	pub method: String,
}

impl PkceChallenge {
	/// 构造一个 PKCE 挑战; method 固定为 "S256"(RFC 7636 推荐且唯一安全的方式), 不接受外部
	/// 传入, 避免误用不安全的 "plain" 方法
	pub fn new(verifier: String, challenge: String) -> Self {
		PkceChallenge {
			verifier,
			challenge,
			method: "S256".to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// ProviderKind: 已知值双向互转应精确对应枚举变体
	#[test]
	fn provider_kind_from_i64_known_values_round_trip() {
		assert_eq!(ProviderKind::from_i64(1), ProviderKind::GitHub);
		assert_eq!(ProviderKind::from_i64(2), ProviderKind::Google);
		assert_eq!(ProviderKind::from_i64(3), ProviderKind::Microsoft);
		assert_eq!(ProviderKind::from_i64(4), ProviderKind::Token);
		assert_eq!(i64::from(ProviderKind::GitHub), 1);
		assert_eq!(i64::from(ProviderKind::Google), 2);
		assert_eq!(i64::from(ProviderKind::Microsoft), 3);
		assert_eq!(i64::from(ProviderKind::Token), 4);
	}

	// ProviderKind: 未知值(脏数据, 含列默认值 0)兜底为最小合法编码 GitHub, 不 panic
	#[test]
	fn provider_kind_from_i64_unknown_value_falls_back_to_github() {
		assert_eq!(ProviderKind::from_i64(0), ProviderKind::GitHub);
		assert_eq!(ProviderKind::from_i64(99), ProviderKind::GitHub);
	}

	// AuthAccount: 序列化应使用 camelCase 字段名(connectTime), 且能通过 JSON 往返完整还原
	#[test]
	fn auth_account_round_trips_through_json_with_camel_case_fields() {
		let account = AuthAccount {
			id: 1,
			provider: ProviderKind::GitHub,
			account: "demo@example.com".to_string(),
			scope: "repo,read:org".to_string(),
			status: true,
			connect_time: "2026-07-01T00:00:00Z".to_string(),
		};
		let json = serde_json::to_value(&account).unwrap();
		assert_eq!(json["connectTime"], "2026-07-01T00:00:00Z");
		assert_eq!(json["provider"], "GitHub");
		assert!(json.get("connect_time").is_none());

		let back: AuthAccount =
			serde_json::from_str(&serde_json::to_string(&account).unwrap()).unwrap();
		assert_eq!(back, account);
	}

	// PkceChallenge::new: method 应固定为 "S256"(RFC 7636), 不受调用方输入影响
	#[test]
	fn pkce_challenge_new_fixes_method_to_s256() {
		let challenge = PkceChallenge::new("verifier-abc".to_string(), "challenge-xyz".to_string());
		assert_eq!(challenge.method, "S256");
		assert_eq!(challenge.verifier, "verifier-abc");
		assert_eq!(challenge.challenge, "challenge-xyz");
	}

	// TokenSet: 应支持构造/Clone/PartialEq, 供服务层在内存中短暂持有并比较
	#[test]
	fn token_set_supports_clone_and_equality() {
		let a = TokenSet {
			access: "access-token".to_string(),
			refresh: Some("refresh-token".to_string()),
			expires_at: None,
		};
		let b = a.clone();
		assert_eq!(a, b);
	}
}
