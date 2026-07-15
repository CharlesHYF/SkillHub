// 文件作用: 已安装界面右侧详情面板 —— 选中资源的完整信息 + 关联 Agent + 底部动作;
//           纯展示 + 回调, 数据获取/选中态/mutation 由 pages/installed 统一持有
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { Copy, Sparkles, Plug } from 'lucide-react';

import type { ResourceRespVO } from '@/api/library';
import { DetailPanel } from '@/components/common/detail-panel';
import { TypeBadge } from '@/components/common/type-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { formatDateTime, formatRelativeTime } from '@/lib/utils';
import { SOURCE_LABEL, deriveDescription, toResourceKind } from './resource-display';

interface ResourceDetailPanelProps {
	resource: ResourceRespVO;
	/** 该资源当前已关联(desired=1)的 Agent 展示名列表, 由 pages/installed 从
	 * resourceAgentLinks 按 resourceId 过滤而来 */
	linkedAgentNames: string[];
	onClose: () => void;
	onSyncToAllAgents: () => void;
	onRequestDelete: (resource: ResourceRespVO) => void;
	/** 是否正在执行"同步到全部 Agent"(禁用按钮 + 文案提示, 避免重复触发) */
	isSyncing?: boolean;
}

/** 复制本地路径到系统剪贴板; 剪贴板 API 在部分环境(如无权限/非安全上下文)可能不可用,
 * 静默失败即可, 不因此打断用户操作(复制失败顶多是没反馈, 不是破坏性错误) */
function copyToClipboard(text: string) {
	void navigator.clipboard?.writeText(text).catch(() => {});
}

/** 已安装界面右侧详情面板: 图标/名称/徽标 + 描述 + 本地路径 + 最后更新 + Changelog(占位) +
 * 已关联 Agent + 底部动作(同步到全部 Agent/仅导出此项/查看详情/卸载) */
export function ResourceDetailPanel({
	resource,
	linkedAgentNames,
	onClose,
	onSyncToAllAgents,
	onRequestDelete,
	isSyncing = false,
}: ResourceDetailPanelProps) {
	const description = deriveDescription(resource);
	const Icon = resource.resType === 'Mcp' ? Plug : Sparkles;

	return (
		<DetailPanel title={resource.name} onClose={onClose} className="w-90 shrink-0">
			<div className="flex flex-col gap-5">
				<div className="flex items-center gap-3">
					<span
						className="flex size-10 shrink-0 items-center justify-center rounded-lg"
						style={{ background: 'var(--sh-brand-tint)' }}
					>
						<Icon size={20} color="var(--sh-brand)" />
					</span>
					<div className="flex flex-wrap items-center gap-1.5">
						<TypeBadge type={toResourceKind(resource.resType)} />
						<Badge variant="outline">{SOURCE_LABEL[resource.sourceType]}</Badge>
					</div>
				</div>

				<p className="text-sm text-muted-foreground">版本 {resource.version || '-'}</p>

				<section>
					<h3 className="mb-1 text-xs font-medium text-muted-foreground">描述</h3>
					<p className="text-sm text-foreground">{description ?? '暂无描述'}</p>
				</section>

				<section>
					<h3 className="mb-1 text-xs font-medium text-muted-foreground">本地路径</h3>
					<div className="flex items-center gap-1.5">
						<code className="min-w-0 flex-1 truncate rounded-md bg-muted px-2 py-1 text-xs">
							{resource.localPath}
						</code>
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label="复制本地路径"
							onClick={() => copyToClipboard(resource.localPath)}
						>
							<Copy size={14} />
						</Button>
					</div>
				</section>

				<section>
					<h3 className="mb-1 text-xs font-medium text-muted-foreground">最后更新</h3>
					<p className="text-sm text-foreground">
						{formatDateTime(resource.updateTime)} (
						{formatRelativeTime(resource.updateTime)})
					</p>
				</section>

				<section>
					<h3 className="mb-1 text-xs font-medium text-muted-foreground">Changelog</h3>
					{/* M1 resource 表未落版本历史字段, 留空占位; 待后续任务补齐数据来源后再实现 */}
					<p className="text-sm text-muted-foreground">暂无版本记录</p>
				</section>

				<section>
					<h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
						已关联 Agent ({linkedAgentNames.length})
					</h3>
					{linkedAgentNames.length > 0 ? (
						<div className="flex flex-wrap gap-1.5">
							{linkedAgentNames.map((name) => (
								<Badge key={name} variant="secondary">
									{name}
								</Badge>
							))}
						</div>
					) : (
						<p className="text-sm text-muted-foreground">暂无关联</p>
					)}
				</section>

				<div className="flex flex-col gap-2 pt-2">
					<Button onClick={onSyncToAllAgents} disabled={isSyncing}>
						{isSyncing ? '同步中...' : '同步到全部 Agent'}
					</Button>
					<Button variant="outline" disabled title="导出为独立包, M3 再实现">
						仅导出此项
					</Button>
					<div className="flex gap-2">
						<Button
							variant="outline"
							className="flex-1"
							disabled
							title="完整信息已在本面板展示"
						>
							查看详情
						</Button>
						<Button
							variant="destructive"
							className="flex-1"
							onClick={() => onRequestDelete(resource)}
						>
							卸载
						</Button>
					</div>
				</div>
			</div>
		</DetailPanel>
	);
}
