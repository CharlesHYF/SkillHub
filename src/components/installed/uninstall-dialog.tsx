// 文件作用: 卸载资源前的二次确认弹窗; resource 为 null 表示关闭态, 由调用方(pages/installed)
//           持有"待卸载资源"这一状态, 本组件只负责展示确认文案与转发确认/取消
// 创建日期: 2026-07-09
import type { Resource } from '@/api/library';
import { Button } from '@/components/ui/button';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '@/components/ui/dialog';

interface UninstallDialogProps {
	/** 待卸载的资源; null 表示对话框关闭 */
	resource: Resource | null;
	onConfirm: () => void;
	onCancel: () => void;
	/** 是否正在执行卸载(禁用确认按钮 + 文案提示, 避免重复触发) */
	isDeleting?: boolean;
}

/** 卸载二次确认弹窗: 提示将删库记录并清理本地内容, 需用户明确确认 */
export function UninstallDialog({
	resource,
	onConfirm,
	onCancel,
	isDeleting = false,
}: UninstallDialogProps) {
	return (
		<Dialog open={resource !== null} onOpenChange={(open) => !open && onCancel()}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>确认卸载 {resource?.name}?</DialogTitle>
					<DialogDescription>
						卸载后将从本地库移除该资源记录, 并清理其在 SkillHub 存储目录下的内容,
						此操作不可撤销。
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button variant="outline" onClick={onCancel}>
						取消
					</Button>
					<Button variant="destructive" onClick={onConfirm} disabled={isDeleting}>
						{isDeleting ? '卸载中...' : '卸载'}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
