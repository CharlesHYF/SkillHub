// 文件作用: 导入导出(Portability)界面展示态的派生逻辑 —— 历史记录状态码/方向码到文案与图标的
//           映射、导出范围/目标格式/导入冲突策略的可选项文案(措辞与原型截图第 6 屏一致), 供
//           export-panel/import-panel/impexp-history-table 共用, 避免同一套映射写多份
// 创建日期: 2026-07-10
import { Upload, Download, type LucideIcon } from 'lucide-react';
import type { SyncStatus } from '@/components/common/sync-status-badge';

/** 历史记录状态码(ImpexpRespVO.status: 0 失败/1 成功/2 部分成功) -> SyncStatusBadge 可渲染的状态文案,
 * 复用 sync-status-badge 既有的语义色映射(见该文件文档注释), 不重复实现一套徽标 */
export const IMPEXP_STATUS_LABEL: Record<0 | 1 | 2, SyncStatus> = {
	0: '失败',
	1: '成功',
	2: '部分成功',
};

/** 历史记录方向码(ImpexpRespVO.direction: 0 导出/1 导入) -> 展示文案 */
export const DIRECTION_LABEL: Record<0 | 1, string> = {
	0: '导出',
	1: '导入',
};

/** 历史记录方向码 -> 图标: 导出=下载态(把包保存到本地), 导入=上传态(把包送入应用), 与 Export/
 * Import 两个面板的标题图标、按钮图标一致 */
export const DIRECTION_ICON: Record<0 | 1, LucideIcon> = {
	0: Download,
	1: Upload,
};

/** 单选项描述: value 供 RadioGroup 绑定(RadioGroup 的值本身是字符串, 由调用方转换), label/
 * description 供展示 */
export interface RadioOption<T extends number> {
	value: T;
	label: string;
	description?: string;
}

/** 导出范围可选项, 措辞与原型截图一致 */
export const SCOPE_OPTIONS: RadioOption<0 | 1 | 2>[] = [
	{ value: 0, label: '全部数据' },
	{ value: 1, label: '按类型选择' },
	{ value: 2, label: '按时间范围' },
];

/** 导出目标文件格式可选项 */
export const FORMAT_OPTIONS: RadioOption<1 | 2 | 3>[] = [
	{ value: 1, label: 'zip' },
	{ value: 2, label: 'json' },
	{ value: 3, label: 'tar' },
];

/** 导入冲突处理策略可选项, 措辞与原型截图一致(含"(推荐)"与各策略说明文案) */
export const CONFLICT_STRATEGY_OPTIONS: RadioOption<0 | 1 | 2>[] = [
	{ value: 0, label: '覆盖 (推荐)', description: '覆盖已存在的同名项' },
	{ value: 1, label: '跳过', description: '跳过已存在的同名项' },
	{ value: 2, label: '保留两者', description: '重命名导入项以保留两者' },
];
