// 文件作用: 系统钥匙串封装 —— OAuth/PAT 令牌只进系统钥匙串, 绝不落库(与 domain::auth::TokenSet
//           "刻意不可序列化"的约定呼应, 见该文件注释); service 固定为 "skillhub", account 由
//           调用方传入(如 "github:demo@example.com"), 每个 account 对应钥匙串里一条独立条目
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13

use anyhow::Result;
use keyring::{Entry, Error as KeyringError};

/// 钥匙串条目的 service 名, 固定值, 与 account 组合定位钥匙串里的一条记录
const SERVICE: &str = "skillhub";

/// 写入(或覆盖已存在的)一条令牌到系统钥匙串
pub fn set_token(account: &str, token: &str) -> Result<()> {
	let entry = Entry::new(SERVICE, account)?;
	entry.set_password(token)?;
	Ok(())
}

/// 读取一条令牌; 钥匙串里不存在该条目时返回 Ok(None)而非 Err, 以区分"从未连接"与"读取故障"
pub fn get_token(account: &str) -> Result<Option<String>> {
	let entry = Entry::new(SERVICE, account)?;
	match entry.get_password() {
		Ok(token) => Ok(Some(token)),
		Err(KeyringError::NoEntry) => Ok(None),
		Err(err) => Err(err.into()),
	}
}

/// 删除一条令牌; 条目本就不存在视为删除目标已达成(幂等), 不报错
pub fn delete_token(account: &str) -> Result<()> {
	let entry = Entry::new(SERVICE, account)?;
	match entry.delete_credential() {
		Ok(()) => Ok(()),
		Err(KeyringError::NoEntry) => Ok(()),
		Err(err) => Err(err.into()),
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use std::sync::{Mutex, MutexGuard};
	use std::time::{SystemTime, UNIX_EPOCH};

	use super::*;

	/// 串行化整个 crate(不止本模块)对真实系统钥匙串的访问。
	/// 原因一(一次性初始化竞态): keyring v4 的 v1 兼容层内部用一个裸 AtomicBool 判断"默认存储
	/// 是否已选定", 只保证 set_credential_store 不被重复调用, 却没有让"没抢到标志位的线程"
	/// 阻塞等待那次调用真正完成(见 keyring::v1::Entry::new 源码), 首次并发调用 Entry::new 会
	/// 报错 "No default store has been set"。
	/// 原因二(后端本身的并发问题): 即便绕开上面这一次性初始化竞态, 多线程并发对 Keychain
	/// 增删查询仍偶发 macOS 底层报错(如 "An invalid record was encountered.")。
	/// 两者均是 apple-native-keyring-store/keyring-core 这一侧的行为, 我们不修改也不依赖其内部
	/// 实现, 只在测试代码这一侧用互斥锁把测试对钥匙串的访问改成串行, 彻底规避。锁只用于
	/// 互斥, 不承载任何需要保持一致性的数据, 中毒(某测试在持锁期间 panic)时直接取内层值继续
	/// 用, 避免一个测试失败连锁"毒死"后面所有测试。
	/// 声明为 pub(crate)(而非仅本模块私有): services::auth 的测试同样会经 store/logout/
	/// token_for 间接触达真实系统钥匙串, cargo test 默认多线程并发跑各模块测试, 若各自持有
	/// 独立的锁实例就起不到互斥作用, 必须共用同一把锁, 见该模块测试对本符号的引用
	pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

	pub(crate) fn lock_keychain_tests() -> MutexGuard<'static, ()> {
		TEST_LOCK
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// 生成一个随机 account 名(进程 id + 纳秒时间戳拼接), 避免测试写入的钥匙串条目与真实账号
	/// 撞名、避免多次运行互相冲突, 也方便一眼识别出是测试残留(前缀 "test-")。声明为
	/// pub(crate)理由同 TEST_LOCK: services::auth 的测试构造账号名时复用本函数, 不重复实现
	pub(crate) fn random_account() -> String {
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos();
		format!("test-{}-{}", std::process::id(), nanos)
	}

	// set_token -> get_token 应原样取回写入的值; 测试结束务必 delete_token 清理, 不残留常驻条目
	#[test]
	fn set_token_then_get_token_round_trips_value() {
		let _guard = lock_keychain_tests();
		let account = random_account();
		set_token(&account, "test-token-value").unwrap();

		let got = get_token(&account).unwrap();
		assert_eq!(got, Some("test-token-value".to_string()));

		delete_token(&account).unwrap();
	}

	// delete_token 之后 get_token 应回落为 None, 而不是 Err
	#[test]
	fn delete_token_then_get_token_returns_none() {
		let _guard = lock_keychain_tests();
		let account = random_account();
		set_token(&account, "to-be-deleted").unwrap();
		delete_token(&account).unwrap();

		assert_eq!(get_token(&account).unwrap(), None);
	}

	// 从未写入过的账号: get_token 应直接返回 None, 不报错
	#[test]
	fn get_token_for_unknown_account_returns_none() {
		let _guard = lock_keychain_tests();
		let account = random_account();
		assert_eq!(get_token(&account).unwrap(), None);
	}

	// delete_token 对不存在的账号应视为幂等成功, 不报错(调用方无需先查询是否存在再决定要不要删)
	#[test]
	fn delete_token_for_unknown_account_is_idempotent() {
		let _guard = lock_keychain_tests();
		let account = random_account();
		assert!(delete_token(&account).is_ok());
	}

	// set_token 覆盖已存在的条目: 应取回最新值而非旧值
	#[test]
	fn set_token_overwrites_existing_value() {
		let _guard = lock_keychain_tests();
		let account = random_account();
		set_token(&account, "old-value").unwrap();
		set_token(&account, "new-value").unwrap();

		assert_eq!(get_token(&account).unwrap(), Some("new-value".to_string()));

		delete_token(&account).unwrap();
	}
}
