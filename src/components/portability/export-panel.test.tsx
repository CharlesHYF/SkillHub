// 文件作用: ExportPanel 组件单测(勾选/单选交互回调正确性 + 一键导出按钮)
// 创建日期: 2026-07-10
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ExportOptions } from '@/api/portability';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ExportPanel } from './export-panel';

const baseOptions: ExportOptions = {
	includeSkills: true,
	includeMcp: true,
	scope: 0,
	format: 1,
	includeConfig: true,
	includeVersionLock: true,
};

function renderPanel(overrides: Partial<React.ComponentProps<typeof ExportPanel>> = {}) {
	const onOptionsChange = vi.fn();
	const onOutPathChange = vi.fn();
	const onBrowseOutPath = vi.fn();
	const onExport = vi.fn();
	render(
		<TooltipProvider>
			<ExportPanel
				options={baseOptions}
				onOptionsChange={onOptionsChange}
				outPath=""
				onOutPathChange={onOutPathChange}
				onBrowseOutPath={onBrowseOutPath}
				onExport={onExport}
				isExporting={false}
				{...overrides}
			/>
		</TooltipProvider>,
	);
	return { onOptionsChange, onOutPathChange, onBrowseOutPath, onExport };
}

describe('ExportPanel', () => {
	it('应渲染导出标题与四组配置区', () => {
		renderPanel();
		expect(screen.getByText('导出 Export')).toBeInTheDocument();
		expect(screen.getByText('导出全部 Skill')).toBeInTheDocument();
		expect(screen.getByText('导出全部 MCP')).toBeInTheDocument();
		expect(screen.getByText('选择导出范围')).toBeInTheDocument();
		expect(screen.getByText('目标文件格式')).toBeInTheDocument();
	});

	it('取消勾选"导出全部 MCP"应以 includeMcp=false 调用 onOptionsChange', async () => {
		const user = userEvent.setup();
		const { onOptionsChange } = renderPanel();

		await user.click(screen.getByRole('checkbox', { name: '导出全部 MCP' }));

		expect(onOptionsChange).toHaveBeenCalledWith({ ...baseOptions, includeMcp: false });
	});

	it('切换导出范围为"按类型选择"应以 scope=1 调用 onOptionsChange', async () => {
		const user = userEvent.setup();
		const { onOptionsChange } = renderPanel();

		await user.click(screen.getByRole('radio', { name: '按类型选择' }));

		expect(onOptionsChange).toHaveBeenCalledWith({ ...baseOptions, scope: 1 });
	});

	it('切换目标格式为 json 应以 format=2 调用 onOptionsChange', async () => {
		const user = userEvent.setup();
		const { onOptionsChange } = renderPanel();

		await user.click(screen.getByRole('radio', { name: 'json' }));

		expect(onOptionsChange).toHaveBeenCalledWith({ ...baseOptions, format: 2 });
	});

	it('切换"是否包含配置"为不包含应以 includeConfig=false 调用 onOptionsChange', async () => {
		const user = userEvent.setup();
		const { onOptionsChange } = renderPanel();

		await user.click(screen.getByRole('radio', { name: '配置-不包含' }));

		expect(onOptionsChange).toHaveBeenCalledWith({ ...baseOptions, includeConfig: false });
	});

	it('切换"是否包含版本锁定"为不包含应以 includeVersionLock=false 调用 onOptionsChange', async () => {
		const user = userEvent.setup();
		const { onOptionsChange } = renderPanel();

		await user.click(screen.getByRole('radio', { name: '版本锁定-不包含' }));

		expect(onOptionsChange).toHaveBeenCalledWith({
			...baseOptions,
			includeVersionLock: false,
		});
	});

	it('在导出目标路径输入框中输入应调用 onOutPathChange', async () => {
		const user = userEvent.setup();
		const { onOutPathChange } = renderPanel();

		await user.type(screen.getByPlaceholderText(/导出文件/), 'a');

		expect(onOutPathChange).toHaveBeenCalled();
	});

	it('点击"选择保存位置"应调用 onBrowseOutPath', async () => {
		const user = userEvent.setup();
		const { onBrowseOutPath } = renderPanel();

		await user.click(screen.getByRole('button', { name: /选择保存位置/ }));

		expect(onBrowseOutPath).toHaveBeenCalled();
	});

	it('点击"一键导出全部"应调用 onExport', async () => {
		const user = userEvent.setup();
		const { onExport } = renderPanel();

		await user.click(screen.getByRole('button', { name: /一键导出全部/ }));

		expect(onExport).toHaveBeenCalled();
	});

	it('isExporting=true 时"一键导出全部"按钮应禁用', () => {
		renderPanel({ isExporting: true });
		expect(screen.getByRole('button', { name: /一键导出全部/ })).toBeDisabled();
	});
});
