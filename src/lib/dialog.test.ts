// 文件作用: dialog.ts(原生文件对话框薄封装)单测 —— mock @tauri-apps/plugin-dialog, 断言
//           pickSaveFile/pickOpenFile/pickDirectory 各自以正确参数调用 save/open, 且用户取消
//           (返回 null)与选中(返回路径字符串)两种情形均正确透传
// 创建日期: 2026-07-10
// 修改日期: 2026-07-13
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/plugin-dialog', () => ({
	save: vi.fn(),
	open: vi.fn(),
}));

import { save, open } from '@tauri-apps/plugin-dialog';
import { pickSaveFile, pickOpenFile, pickDirectory } from './dialog';

describe('dialog', () => {
	beforeEach(() => {
		vi.mocked(save).mockReset();
		vi.mocked(open).mockReset();
	});

	describe('pickSaveFile', () => {
		it('应调用 save 并原样返回选中的路径', async () => {
			vi.mocked(save).mockResolvedValue('/Users/demo/skillhub_backup.zip');

			const result = await pickSaveFile({
				defaultPath: '/Users/demo/skillhub_backup.zip',
				filters: [{ name: '导出包 (.zip)', extensions: ['zip'] }],
			});

			expect(save).toHaveBeenCalledWith({
				defaultPath: '/Users/demo/skillhub_backup.zip',
				filters: [{ name: '导出包 (.zip)', extensions: ['zip'] }],
			});
			expect(result).toBe('/Users/demo/skillhub_backup.zip');
		});

		it('未传参数时应以 undefined 的 defaultPath/filters 调用 save', async () => {
			vi.mocked(save).mockResolvedValue(null);

			await pickSaveFile();

			expect(save).toHaveBeenCalledWith({ defaultPath: undefined, filters: undefined });
		});

		it('用户取消(save 返回 null)应原样透传 null', async () => {
			vi.mocked(save).mockResolvedValue(null);

			const result = await pickSaveFile({ filters: [{ name: 'zip', extensions: ['zip'] }] });

			expect(result).toBeNull();
		});
	});

	describe('pickOpenFile', () => {
		it('应以 multiple:false、directory:false 调用 open, 并原样返回选中的路径', async () => {
			vi.mocked(open).mockResolvedValue('/Users/demo/skillhub_backup.zip');

			const result = await pickOpenFile({
				filters: [{ name: '导入包', extensions: ['zip', 'json', 'tar', 'gz'] }],
			});

			expect(open).toHaveBeenCalledWith({
				multiple: false,
				directory: false,
				filters: [{ name: '导入包', extensions: ['zip', 'json', 'tar', 'gz'] }],
			});
			expect(result).toBe('/Users/demo/skillhub_backup.zip');
		});

		it('用户取消(open 返回 null)应原样透传 null', async () => {
			vi.mocked(open).mockResolvedValue(null);

			const result = await pickOpenFile();

			expect(result).toBeNull();
		});
	});

	describe('pickDirectory', () => {
		it('应以 directory:true、multiple:false 调用 open, 并原样返回选中的目录路径', async () => {
			vi.mocked(open).mockResolvedValue('/Users/demo/.skillhub/skills');

			const result = await pickDirectory({ defaultPath: '/Users/demo/.skillhub' });

			expect(open).toHaveBeenCalledWith({
				directory: true,
				multiple: false,
				defaultPath: '/Users/demo/.skillhub',
			});
			expect(result).toBe('/Users/demo/.skillhub/skills');
		});

		it('用户取消(open 返回 null)应原样透传 null', async () => {
			vi.mocked(open).mockResolvedValue(null);

			const result = await pickDirectory();

			expect(result).toBeNull();
		});
	});
});
