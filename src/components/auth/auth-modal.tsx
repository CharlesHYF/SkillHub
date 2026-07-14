// 文件作用: 应用内认证弹窗(还原原型第 3 屏弹层) —— 三个 OAuth 登录按钮(GitHub/Google/Microsoft,
//           点击即调 authLogin 起本地二级窗口完成授权)+ 手动录入访问令牌(展开输入框调
//           authEnterToken); 展示授权后将允许 SkillHub 执行的权限说明清单。任一方式认证成功后
//           把入库账号交回调用方(pages/marketplace-detail), 由调用方决定关闭弹窗并重试安装
// 创建日期: 2026-07-10
import { useEffect, useState } from 'react';
import { AppWindow, Check, Code2, Globe, KeyRound, Lock } from 'lucide-react';

import { authEnterToken, authLogin, type AuthAccountRespVO } from '@/api/auth';
import { INSTALL_PERMISSIONS } from './auth-display';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '@/components/ui/dialog';

/** provider 数值编码, 与后端 domain::auth::ProviderKind 的 i64 互转约定一一对应(1-GitHub,
 * 2-Google, 3-Microsoft, 4-Token); 弹窗内三个 OAuth 按钮与令牌录入各自固定绑定其中一个,
 * 就地定义常量即可, 不需要从 api/auth.ts 的 ProviderKind 字符串联合类型反向转换(那是"已连接
 * 账号"的展示态, 这里是"发起登录"的入参, 两回事) */
const PROVIDER = { GITHUB: 1, GOOGLE: 2, MICROSOFT: 3, TOKEN: 4 } as const;

interface AuthModalProps {
	/** 弹窗开关态, 由调用方(pages/marketplace-detail)持有 */
	open: boolean;
	/** 请求关闭弹窗(取消/遮罩/右上角关闭均触发), 调用方应据此把 open 置回 false */
	onOpenChange: (open: boolean) => void;
	/** 触发本次弹窗的目标 provider 数值编码(由 market_install 的 "AUTH_REQUIRED:<provider>"
	 * 错误解析得到); 仅当为 Token(4) 时用于自动展开令牌输入区, 省去用户一次点击, 三个 OAuth
	 * 按钮与令牌选项始终全部可选, 不因此限制可选项 */
	defaultProvider?: number;
	/** 任一方式认证成功后回调(携带入库账号), 调用方应据此关闭弹窗并重试安装 */
	onAuthenticated: (account: AuthAccountRespVO) => void;
}

/** 应用内认证弹窗: 还原原型第 3 屏的"需要登录"弹层 —— GitHub/Google/Microsoft 一键登录 +
 * 手动录入访问令牌, 以及授权后将允许 SkillHub 执行的权限说明 */
export function AuthModal({
	open,
	onOpenChange,
	defaultProvider,
	onAuthenticated,
}: AuthModalProps) {
	const [tokenExpanded, setTokenExpanded] = useState(defaultProvider === PROVIDER.TOKEN);
	const [token, setToken] = useState('');
	const [pendingProvider, setPendingProvider] = useState<number | null>(null);
	const [error, setError] = useState<string | null>(null);

	// 弹窗每次重新打开都应回到干净的初始态: 若这次是因为需要 Token 认证而弹出, 直接展开令牌
	// 输入区省去一次点击; 否则收起。同一个 AuthModal 实例会随安装重试反复开关复用, 不能只靠
	// useState 的初始值(那只在首次挂载时生效)
	useEffect(() => {
		if (!open) return;
		setTokenExpanded(defaultProvider === PROVIDER.TOKEN);
		setToken('');
		setPendingProvider(null);
		setError(null);
	}, [open, defaultProvider]);

	/** 三个 OAuth 按钮共用: 起应用内二级窗口完成登录, 成功后把账号交回调用方 */
	async function handleOAuthLogin(provider: number) {
		setPendingProvider(provider);
		setError(null);
		try {
			const account = await authLogin(provider);
			onAuthenticated(account);
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setPendingProvider(null);
		}
	}

	/** 提交手动录入的访问令牌; 空令牌不提交(按钮本身也据此禁用, 这里是双重保险) */
	async function handleSubmitToken() {
		const trimmed = token.trim();
		if (!trimmed) return;
		setPendingProvider(PROVIDER.TOKEN);
		setError(null);
		try {
			const account = await authEnterToken(PROVIDER.TOKEN, trimmed);
			onAuthenticated(account);
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setPendingProvider(null);
		}
	}

	const isPending = pendingProvider !== null;

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle className="flex items-center gap-2">
						<Lock size={16} />
						需要登录 / Authentication Required
					</DialogTitle>
					<DialogDescription>
						此资源需要账户认证才能下载和安装。登录过程将在 SkillHub 内部完成, 确保安全。
					</DialogDescription>
				</DialogHeader>

				<div className="flex flex-col gap-2">
					<Button
						variant="outline"
						className="justify-start"
						disabled={isPending}
						onClick={() => handleOAuthLogin(PROVIDER.GITHUB)}
					>
						<Code2 size={14} />
						{pendingProvider === PROVIDER.GITHUB ? '登录中...' : '使用 GitHub 登录'}
					</Button>
					<Button
						variant="outline"
						className="justify-start"
						disabled={isPending}
						onClick={() => handleOAuthLogin(PROVIDER.GOOGLE)}
					>
						<Globe size={14} />
						{pendingProvider === PROVIDER.GOOGLE ? '登录中...' : '使用 Google 登录'}
					</Button>
					<Button
						variant="outline"
						className="justify-start"
						disabled={isPending}
						onClick={() => handleOAuthLogin(PROVIDER.MICROSOFT)}
					>
						<AppWindow size={14} />
						{pendingProvider === PROVIDER.MICROSOFT
							? '登录中...'
							: '使用 Microsoft 登录'}
					</Button>
					<Button
						variant="outline"
						className="justify-start"
						disabled={isPending}
						onClick={() => setTokenExpanded((prev) => !prev)}
					>
						<KeyRound size={14} />
						输入访问令牌
					</Button>
					{tokenExpanded ? (
						<Input
							type="password"
							placeholder="访问令牌 / Personal Access Token"
							value={token}
							disabled={isPending}
							onChange={(e) => setToken(e.target.value)}
						/>
					) : null}
				</div>

				<div>
					<h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
						授权后将允许 SkillHub:
					</h3>
					<ul className="flex flex-col gap-1.5">
						{INSTALL_PERMISSIONS.map((item) => (
							<li key={item.title} className="flex items-start gap-2 text-sm">
								<Check
									size={14}
									color="var(--sh-brand)"
									className="mt-0.5 shrink-0"
								/>
								<span>
									<span className="font-medium text-foreground">
										{item.title}
									</span>
									<span className="text-muted-foreground">
										{' '}
										{item.description}
									</span>
								</span>
							</li>
						))}
					</ul>
				</div>

				{error ? (
					<p role="alert" className="text-sm" style={{ color: 'var(--sh-danger)' }}>
						{error}
					</p>
				) : null}

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						取消
					</Button>
					<Button
						disabled={!tokenExpanded || !token.trim() || isPending}
						onClick={handleSubmitToken}
					>
						{isPending && pendingProvider === PROVIDER.TOKEN ? '认证中...' : '继续'}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
