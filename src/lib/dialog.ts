// 文件作用: 原生文件对话框薄封装(@tauri-apps/plugin-dialog 的 save/open) —— 供导入导出页选择
//           保存位置/选择导入文件、设置页浏览存储目录等场景统一调用; 用户取消对话框时插件本身
//           就返回 null, 本文件各函数原样透传该 null, 不做额外语义转换
// 创建日期: 2026-07-10
import { open, save, type DialogFilter } from '@tauri-apps/plugin-dialog';

/** 保存文件对话框选项: 默认路径(含建议文件名) + 文件类型过滤器 */
export interface PickSaveFileOptions {
	defaultPath?: string;
	filters?: DialogFilter[];
}

/** 打开单个文件对话框选项: 仅文件类型过滤器, 目录/多选场景见 pickDirectory */
export interface PickOpenFileOptions {
	filters?: DialogFilter[];
}

/** 选择目录对话框选项: 仅默认路径 */
export interface PickDirectoryOptions {
	defaultPath?: string;
}

/** 弹出原生"保存文件"对话框, 返回用户选定的目标路径; 用户取消时返回 null */
export async function pickSaveFile(opts?: PickSaveFileOptions): Promise<string | null> {
	return save({
		defaultPath: opts?.defaultPath,
		filters: opts?.filters,
	});
}

/** 弹出原生"打开文件"对话框(单选、非目录), 返回用户选定的文件路径; 用户取消时返回 null */
export async function pickOpenFile(opts?: PickOpenFileOptions): Promise<string | null> {
	return open({
		multiple: false,
		directory: false,
		filters: opts?.filters,
	});
}

/** 弹出原生"选择目录"对话框(单选), 返回用户选定的目录路径; 用户取消时返回 null */
export async function pickDirectory(opts?: PickDirectoryOptions): Promise<string | null> {
	return open({
		directory: true,
		multiple: false,
		defaultPath: opts?.defaultPath,
	});
}
