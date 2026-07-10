// 文件作用: Portability 页面集成测试(mock src/api/portability 与 @tauri-apps/api/webview) ——
//           默认导出选项 + 一键导出全部触发 exportBundle/输入导入路径触发 importPreview 渲染计数/
//           选择冲突策略后开始导入触发 importBundle/历史表渲染 impexpHistory 结果
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type {
	ExportOptions,
	ImportPreview,
	ImpexpRow,
	Manifest,
	ImportOutcome,
} from '@/api/portability';
import { TooltipProvider } from '@/components/ui/tooltip';
import Portability from './portability';

vi.mock('@tauri-apps/api/webview', () => ({
	getCurrentWebview: vi.fn(() => ({
		onDragDropEvent: vi.fn().mockResolvedValue(vi.fn()),
	})),
}));

vi.mock('@/api/portability', () => ({
	exportBundle: vi.fn(),
	importPreview: vi.fn(),
	importBundle: vi.fn(),
	impexpHistory: vi.fn(),
}));

import { exportBundle, importPreview, importBundle, impexpHistory } from '@/api/portability';

const defaultExportOptions: ExportOptions = {
	includeSkills: true,
	includeMcp: true,
	scope: 0,
	format: 1,
	includeConfig: true,
	includeVersionLock: true,
};

const fullPreview: ImportPreview = { skill: 128, mcp: 45, config: 23, agent: 8, schemaOk: true };

function renderPortability() {
	const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return render(
		<QueryClientProvider client={queryClient}>
			<TooltipProvider>
				<Portability />
			</TooltipProvider>
		</QueryClientProvider>,
	);
}

describe('Portability 页面', () => {
	beforeEach(() => {
		vi.mocked(exportBundle)
			.mockReset()
			.mockResolvedValue({
				schemaVersion: 1,
				exportedAt: '2026-07-10 00:00:00',
				counts: { skill: 128, mcp: 45, config: 23, agent: 8 },
				checksums: {},
			} satisfies Manifest);
		vi.mocked(importPreview).mockReset().mockResolvedValue(fullPreview);
		vi.mocked(importBundle)
			.mockReset()
			.mockResolvedValue({
				imported: 3,
				skipped: 0,
				renamed: 0,
				status: 1,
			} satisfies ImportOutcome);
		vi.mocked(impexpHistory).mockReset().mockResolvedValue([]);
	});

	it('应渲染标题、导出/导入面板与历史表', async () => {
		renderPortability();
		expect(screen.getByText('导入导出 / Import & Export')).toBeInTheDocument();
		expect(screen.getByText('导出 Export')).toBeInTheDocument();
		expect(screen.getByText('导入 Import')).toBeInTheDocument();
		await waitFor(() => expect(impexpHistory).toHaveBeenCalledWith(20));
	});

	it('点击"一键导出全部"应以默认选项与空路径调用 exportBundle', async () => {
		const user = userEvent.setup();
		renderPortability();

		await user.click(screen.getByRole('button', { name: /一键导出全部/ }));

		expect(exportBundle).toHaveBeenCalledWith(defaultExportOptions, '');
	});

	it('切换导出格式后点击导出应带上新选项调用 exportBundle', async () => {
		const user = userEvent.setup();
		renderPortability();

		await user.click(screen.getByRole('radio', { name: 'json' }));
		await user.click(screen.getByRole('button', { name: /一键导出全部/ }));

		expect(exportBundle).toHaveBeenCalledWith({ ...defaultExportOptions, format: 2 }, '');
	});

	it('在导入路径输入框填入路径应触发 importPreview 并渲染计数', async () => {
		const user = userEvent.setup();
		renderPortability();

		await user.type(screen.getByPlaceholderText(/完整路径/), '/mock/skillhub_backup.zip');

		await waitFor(() =>
			expect(importPreview).toHaveBeenCalledWith('/mock/skillhub_backup.zip'),
		);
		expect(await screen.findByText('128')).toBeInTheDocument();
		expect(screen.getByText('45')).toBeInTheDocument();
	});

	it('选择路径与冲突策略后点击"开始导入"应调用 importBundle(path, strategy, autoSync)', async () => {
		const user = userEvent.setup();
		renderPortability();

		await user.type(screen.getByPlaceholderText(/完整路径/), '/mock/skillhub_backup.zip');
		await screen.findByText('128');

		await user.click(screen.getByRole('radio', { name: '跳过' }));
		await user.click(screen.getByRole('button', { name: /开始导入/ }));

		await waitFor(() =>
			expect(importBundle).toHaveBeenCalledWith('/mock/skillhub_backup.zip', 1, true),
		);
	});

	it('导入成功后应清空路径并刷新历史(impexpHistory 被再次调用)', async () => {
		const user = userEvent.setup();
		renderPortability();
		await waitFor(() => expect(impexpHistory).toHaveBeenCalledTimes(1));

		await user.type(screen.getByPlaceholderText(/完整路径/), '/mock/skillhub_backup.zip');
		await screen.findByText('128');
		await user.click(screen.getByRole('button', { name: /开始导入/ }));

		await waitFor(() => expect(impexpHistory).toHaveBeenCalledTimes(2));
		expect(screen.getByPlaceholderText(/完整路径/)).toHaveValue('');
	});

	it('历史表应渲染 impexpHistory 返回的记录', async () => {
		const rows: ImpexpRow[] = [
			{
				id: 1,
				direction: 0,
				fileName: 'skillhub_backup_2024-05-23.zip',
				fileFormat: 1,
				summary: 'Skill 128 · MCP 45 · 配置 23 · Agent 8',
				status: 1,
				runTime: '2026-07-10 00:00:00',
			},
		];
		vi.mocked(impexpHistory).mockResolvedValue(rows);

		renderPortability();

		expect(await screen.findByText('skillhub_backup_2024-05-23.zip')).toBeInTheDocument();
	});

	it('点击顶部刷新按钮应重新调用 impexpHistory', async () => {
		const user = userEvent.setup();
		renderPortability();
		await waitFor(() => expect(impexpHistory).toHaveBeenCalledTimes(1));

		await user.click(screen.getByRole('button', { name: /刷新/ }));

		await waitFor(() => expect(impexpHistory).toHaveBeenCalledTimes(2));
	});
});
