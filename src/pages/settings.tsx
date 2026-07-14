// 文件作用: 设置(SettingRespVO)界面(原型第 7 屏) —— 账号与认证(account-section)、存储目录
//           (storage-section)双列 + 同步偏好(sync-preferences-section)、网络与代理
//           (network-section)双列 + 底部更新通道(update-channel-section)通栏 + 右下操作条
//           (恢复默认/保存更改)。设置经 settingsGet 拉取后接入本地可编辑态(draft), 各分区改动
//           只影响 draft, 点击"保存更改"才经 settingsSave 整份提交; "恢复默认"把 draft 重置为
//           本文件硬编码的默认 SettingRespVO(与后端 domain::setting::SettingRespVO 的 Default 同口径,
//           见下方 DEFAULT_SETTINGS 注释), 而不是回退到 settingsGet 加载到的值——存储目录两项
//           例外, 见 handleReset 注释。账号与认证区复用既有 src/api/auth.ts 封装
//           (auth_accounts/auth_login/auth_logout/auth_enter_token), 不新造认证相关 command
// 创建日期: 2026-07-10
import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { authAccounts, authEnterToken, authLogin, authLogout } from '@/api/auth';
import { settingsGet, settingsSave, type SettingRespVO } from '@/api/setting';
import { PageHeader } from '@/components/common/page-header';
import { AccountSection } from '@/components/settings/account-section';
import { NetworkSection } from '@/components/settings/network-section';
import { StorageSection } from '@/components/settings/storage-section';
import { SyncPreferencesSection } from '@/components/settings/sync-preferences-section';
import { UpdateChannelSection } from '@/components/settings/update-channel-section';
import { Button } from '@/components/ui/button';
import { pickDirectory } from '@/lib/dialog';

const SETTINGS_KEY = 'settings';
const AUTH_ACCOUNTS_KEY = 'auth-accounts';

/** 设置默认值, 与后端 domain::setting::SettingRespVO 的 Default 实现同口径(见本任务契约: 目录空串,
 * 前 3 个同步开关默认开/仅同步已启用项默认关, 代理默认系统模式且地址均为空串, 超时默认 30 秒,
 * 更新通道默认 Stable)。首次加载在 settingsGet 结果返回前先用它兜底渲染, 避免控件短暂处于
 * undefined 态。"恢复默认"按钮也据此重置本地编辑态, 但存储目录两项(storageSkillDir/
 * storageMcpDir)是例外, 不会真的回到这里的空串, 见 handleReset 注释 */
const DEFAULT_SETTINGS: SettingRespVO = {
	storageSkillDir: '',
	storageMcpDir: '',
	syncAutoNewAgent: true,
	syncCheckUpdateOnStart: true,
	syncConflictPrompt: true,
	syncOnlyEnabled: false,
	netProxyMode: 0,
	netHttpProxy: '',
	netHttpsProxy: '',
	netNoProxy: '',
	netTimeoutSec: 30,
	updateChannel: 0,
};

const SETTINGS_FIELD_KEYS = Object.keys(DEFAULT_SETTINGS) as (keyof SettingRespVO)[];

/** 逐字段比较两份 SettingRespVO 是否完全一致; 字段均为原始值(string/boolean/number), 不需要深比较 */
function settingsEqual(a: SettingRespVO, b: SettingRespVO): boolean {
	return SETTINGS_FIELD_KEYS.every((key) => a[key] === b[key]);
}

/** 设置(SettingRespVO)界面: 还原原型第 7 屏 —— 五个分区 + 右下保存/恢复默认操作条 */
export default function SettingRespVO() {
	const queryClient = useQueryClient();

	const settingsQuery = useQuery({ queryKey: [SETTINGS_KEY], queryFn: settingsGet });

	const [draft, setDraft] = useState<SettingRespVO>(DEFAULT_SETTINGS);
	const [loadedFromServer, setLoadedFromServer] = useState(false);

	// settingsGet 结果到达后, 只在首次把它接入本地编辑态, 避免后台静默 refetch(如窗口重新聚焦)
	// 覆盖用户正在编辑但尚未保存的改动
	useEffect(() => {
		if (settingsQuery.data && !loadedFromServer) {
			setDraft(settingsQuery.data);
			setLoadedFromServer(true);
		}
	}, [settingsQuery.data, loadedFromServer]);

	/** 合并式更新本地编辑态, 供各分区组件的 onChange 回调调用(与 export-panel 的 patch 同一惯例) */
	function patch(next: Partial<SettingRespVO>) {
		setDraft((prev) => ({ ...prev, ...next }));
	}

	const saveMutation = useMutation({
		mutationFn: (next: SettingRespVO) => settingsSave(next),
		onSuccess: (saved) => {
			queryClient.setQueryData([SETTINGS_KEY], saved);
			setDraft(saved);
		},
	});

	// 脏态: 与 settingsGet 加载到的基准逐字段比较; 基准尚未加载完成时视为"无改动"(保存按钮禁用),
	// 避免首屏就能点保存把兜底默认值当成用户改动提交上去
	const isDirty = settingsQuery.data ? !settingsEqual(draft, settingsQuery.data) : false;

	// "恢复默认": 存储目录两项不回退到硬编码空串, 而是保留 settingsGet 已加载到的真实默认目录
	// (后端 M5 起 settings_get 回填"空则填 data_dir/skills·mcp 并持久化", 见 commit 319be75),
	// 否则用户会看到目录被清空成空串这一明显倒退; 其余偏好字段维持"回到硬编码默认值"的既有语义
	// (settingsQuery.data 未加载完成时兜底 DEFAULT_SETTINGS 的空串, 与首屏渲染兜底同一惯例)
	function handleReset() {
		setDraft({
			...DEFAULT_SETTINGS,
			storageSkillDir:
				settingsQuery.data?.storageSkillDir ?? DEFAULT_SETTINGS.storageSkillDir,
			storageMcpDir: settingsQuery.data?.storageMcpDir ?? DEFAULT_SETTINGS.storageMcpDir,
		});
	}

	function handleSave() {
		saveMutation.mutate(draft);
	}

	// 账号与认证: 复用既有 src/api/auth.ts 封装(auth_accounts/auth_login/auth_logout/
	// auth_enter_token), 不新造认证相关 command 封装
	const accountsQuery = useQuery({ queryKey: [AUTH_ACCOUNTS_KEY], queryFn: authAccounts });

	function invalidateAccounts() {
		queryClient.invalidateQueries({ queryKey: [AUTH_ACCOUNTS_KEY] });
	}

	const loginMutation = useMutation({
		mutationFn: (provider: number) => authLogin(provider),
		onSuccess: invalidateAccounts,
	});
	const logoutMutation = useMutation({
		mutationFn: (provider: number) => authLogout(provider),
		onSuccess: invalidateAccounts,
	});
	const enterTokenMutation = useMutation({
		mutationFn: ({ provider, token }: { provider: number; token: string }) =>
			authEnterToken(provider, token),
		onSuccess: invalidateAccounts,
	});

	const pendingProvider = loginMutation.isPending
		? (loginMutation.variables ?? null)
		: logoutMutation.isPending
			? (logoutMutation.variables ?? null)
			: enterTokenMutation.isPending
				? (enterTokenMutation.variables?.provider ?? null)
				: null;

	// 本地 Skill/MCP 目录"浏览"按钮: 弹出原生目录选择对话框(src/lib/dialog.ts 的 pickDirectory),
	// 结果非 null 才 patch 写回对应字段; 用户取消(结果为 null)时维持 draft 原值不变, 与导入导出页
	// "选择保存位置"/"选择文件"两个入口的取消处理同一惯例
	async function handleBrowseSkillDir() {
		const result = await pickDirectory({ defaultPath: draft.storageSkillDir || undefined });
		if (result !== null) patch({ storageSkillDir: result });
	}
	async function handleBrowseMcpDir() {
		const result = await pickDirectory({ defaultPath: draft.storageMcpDir || undefined });
		if (result !== null) patch({ storageMcpDir: result });
	}

	return (
		<div className="flex h-full flex-col gap-4">
			<PageHeader
				title="设置 / SettingRespVO"
				description="账号认证、存储目录、同步偏好与网络代理"
			/>

			<div className="grid grid-cols-2 gap-4">
				<AccountSection
					accounts={accountsQuery.data ?? []}
					pendingProvider={pendingProvider}
					onLogin={(provider) => loginMutation.mutate(provider)}
					onLogout={(provider) => logoutMutation.mutate(provider)}
					onSaveToken={(provider, token) =>
						enterTokenMutation.mutate({ provider, token })
					}
				/>
				<StorageSection
					settings={draft}
					onChange={patch}
					onBrowseSkillDir={handleBrowseSkillDir}
					onBrowseMcpDir={handleBrowseMcpDir}
				/>
			</div>

			<div className="grid grid-cols-2 gap-4">
				<SyncPreferencesSection settings={draft} onChange={patch} />
				<NetworkSection settings={draft} onChange={patch} />
			</div>

			<UpdateChannelSection settings={draft} onChange={patch} />

			<div className="flex items-center justify-end gap-2">
				<Button variant="outline" onClick={handleReset}>
					恢复默认
				</Button>
				<Button onClick={handleSave} disabled={!isDirty || saveMutation.isPending}>
					{saveMutation.isPending ? '保存中...' : '保存更改'}
				</Button>
			</div>
		</div>
	);
}
