// 文件作用: 认证(Auth)相关 Tauri command 的类型化封装 —— 已连接账号列表/应用内 OAuth 弹窗登录/
//           手动录入访问令牌/断开连接; provider 入参统一用 domain::auth::ProviderKind 的 i64
//           编码(1-GitHub, 2-Google, 3-Microsoft, 4-Token), 与 api/market.ts 的 sourceType/
//           resType 同一"数值编码跨 IPC 边界"约定
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { invoke } from '@tauri-apps/api/core';

/** 认证提供方, 与后端 domain::auth::ProviderKind 枚举变体名一一对应(该枚举未标
 * #[serde(rename_all)], 故 wire 层就是变体名本身, 与 api/market.ts 的 MarketSourceType
 * 同一约定: 展示态用变体名字符串, 数值编码只用于 command 入参) */
export type ProviderKind = 'GitHub' | 'Google' | 'Microsoft' | 'Token';

/** 已连接的第三方账号, 与后端 domain::auth::AuthAccountRespVO 一一对应(该结构体标了
 * #[serde(rename_all = "camelCase")], 故 connect_time -> connectTime) */
export interface AuthAccountRespVO {
	id: number;
	provider: ProviderKind;
	account: string;
	scope: string;
	status: boolean;
	connectTime: string;
}

/** 列出全部已连接账号 */
export async function authAccounts(): Promise<AuthAccountRespVO[]> {
	return invoke<AuthAccountRespVO[]>('auth_accounts');
}

/** 应用内 OAuth 弹窗登录: provider 为数值编码(1-GitHub, 2-Google, 3-Microsoft; 4-Token 没有
 * 对应的 OAuth 授权页, 应改用 authEnterToken, 见后端 commands::auth::auth_login 的显式拒绝)。
 * 登录过程在 SkillHub 内部的二级窗口完成(本地 loopback 回调, 不跳出应用), 成功后返回入库账号 */
export async function authLogin(provider: number): Promise<AuthAccountRespVO> {
	return invoke<AuthAccountRespVO>('auth_login', { provider });
}

/** 手动录入访问令牌(Personal Access Token): 后端先校验令牌有效性, 通过后落库并返回入库账号 */
export async function authEnterToken(provider: number, token: string): Promise<AuthAccountRespVO> {
	return invoke<AuthAccountRespVO>('auth_enter_token', { provider, token });
}

/** 断开连接: 删库记录 + 系统钥匙串对应条目 */
export async function authLogout(provider: number): Promise<void> {
	return invoke('auth_logout', { provider });
}
