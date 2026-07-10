// 文件作用: 设置界面"账号与认证 Account"分区(原型第 7 屏左上卡片) —— 列出 GitHub/Google/
//           Microsoft 三个 OAuth 服务的连接状态; 已连接展示邮箱 + 退出 + 管理令牌(内联展开令牌
//           输入, 复用 auth_enter_token 流程), 未连接展示登录; 底部"管理全部令牌"一次性展开三项
//           的令牌输入。纯展示 + 回调, 数据获取/mutation 由 pages/settings 统一持有(与
//           export-panel/import-panel 同一惯例), 复用 src/api/auth.ts 既有封装, 不新造认证
//           相关 command
// 创建日期: 2026-07-10
import { useState } from 'react';
import {
	AppWindow,
	Code2,
	Globe,
	KeyRound,
	LogIn,
	LogOut,
	UserRound,
	type LucideIcon,
} from 'lucide-react';

import type { AuthAccount, ProviderKind } from '@/api/auth';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';

/** 三个 OAuth 提供方的展示元信息: 数值编码/图标与 components/auth/auth-modal 的约定一致
 * (1-GitHub/2-Google/3-Microsoft, 图标同用 Code2/Globe/AppWindow), 保持全应用同一 provider
 * 视觉映射一致, 不重复定义一套不同的图标 */
const OAUTH_PROVIDERS: { num: number; key: ProviderKind; label: string; icon: LucideIcon }[] = [
	{ num: 1, key: 'GitHub', label: 'GitHub', icon: Code2 },
	{ num: 2, key: 'Google', label: 'Google', icon: Globe },
	{ num: 3, key: 'Microsoft', label: 'Microsoft', icon: AppWindow },
];

interface AccountSectionProps {
	accounts: AuthAccount[];
	/** 当前正在处理登录/退出/令牌提交的 provider 数值编码, 用于禁用对应行按钮并提示进行中;
	 * 无操作进行时为 null */
	pendingProvider: number | null;
	onLogin: (provider: number) => void;
	onLogout: (provider: number) => void;
	onSaveToken: (provider: number, token: string) => void;
}

/** 设置界面"账号与认证"分区: 还原原型第 7 屏左上卡片 */
export function AccountSection({
	accounts,
	pendingProvider,
	onLogin,
	onLogout,
	onSaveToken,
}: AccountSectionProps) {
	// 令牌录入内联展开态, 逐 provider 独立控制; "管理全部令牌"一次性把三项都置为展开
	const [expanded, setExpanded] = useState<Record<number, boolean>>({});
	const [tokenDrafts, setTokenDrafts] = useState<Record<number, string>>({});

	function toggleExpanded(num: number) {
		setExpanded((prev) => ({ ...prev, [num]: !prev[num] }));
	}

	function expandAll() {
		setExpanded(Object.fromEntries(OAUTH_PROVIDERS.map((p) => [p.num, true])));
	}

	function handleSaveToken(num: number) {
		const token = (tokenDrafts[num] ?? '').trim();
		if (!token) return;
		onSaveToken(num, token);
		setTokenDrafts((prev) => ({ ...prev, [num]: '' }));
		setExpanded((prev) => ({ ...prev, [num]: false }));
	}

	return (
		<Card className="flex h-full flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2 text-base">
					<UserRound size={16} color="var(--sh-brand)" />
					账号与认证 Account
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-1 flex-col gap-4">
				<p className="text-sm text-muted-foreground">已连接的服务</p>
				<div className="flex flex-col gap-3">
					{OAUTH_PROVIDERS.map((provider) => {
						const account = accounts.find(
							(a) => a.provider === provider.key && a.status,
						);
						const isPending = pendingProvider === provider.num;
						const isExpanded = expanded[provider.num] ?? false;
						return (
							<div
								key={provider.num}
								className="flex flex-col gap-2 rounded-md border p-3"
								style={{ borderColor: 'var(--sh-border)' }}
							>
								<div className="flex items-center justify-between gap-3">
									<span className="flex items-center gap-2.5 text-sm">
										<provider.icon size={18} />
										<span className="flex flex-col">
											<span className="font-medium text-foreground">
												{provider.label}
											</span>
											<span className="text-xs text-muted-foreground">
												{account ? `已连接: ${account.account}` : '未连接'}
											</span>
										</span>
									</span>
									<div className="flex items-center gap-2">
										{account ? (
											<>
												<Button
													variant="outline"
													size="sm"
													disabled={isPending}
													onClick={() => onLogout(provider.num)}
												>
													<LogOut size={14} />
													{isPending ? '退出中...' : '退出'}
												</Button>
												<Button
													variant="outline"
													size="sm"
													onClick={() => toggleExpanded(provider.num)}
												>
													<KeyRound size={14} />
													管理令牌
												</Button>
											</>
										) : (
											<Button
												size="sm"
												disabled={isPending}
												onClick={() => onLogin(provider.num)}
											>
												<LogIn size={14} />
												{isPending ? '登录中...' : '登录'}
											</Button>
										)}
									</div>
								</div>
								{isExpanded ? (
									<div className="flex items-center gap-2">
										<Input
											type="password"
											placeholder="访问令牌 / Personal Access Token"
											value={tokenDrafts[provider.num] ?? ''}
											onChange={(e) =>
												setTokenDrafts((prev) => ({
													...prev,
													[provider.num]: e.target.value,
												}))
											}
										/>
										<Button
											size="sm"
											disabled={
												isPending ||
												!(tokenDrafts[provider.num] ?? '').trim()
											}
											onClick={() => handleSaveToken(provider.num)}
										>
											{isPending ? '保存中...' : '保存'}
										</Button>
									</div>
								) : null}
							</div>
						);
					})}
				</div>
				<Button variant="outline" className="mt-auto w-full" onClick={expandAll}>
					<KeyRound size={14} />
					管理全部令牌
				</Button>
			</CardContent>
		</Card>
	);
}
