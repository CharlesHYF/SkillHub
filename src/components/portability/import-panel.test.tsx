// 文件作用: ImportPanel 组件单测(拖拽/文本路径输入 -> 预览渲染 -> 冲突策略/自动同步 -> 开始导入)
// 创建日期: 2026-07-10
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { EventCallback } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { DragDropEvent } from '@tauri-apps/api/webview';
import type { ImportPreview } from '@/api/portability';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ImportPanel } from './import-panel';

vi.mock('@tauri-apps/api/webview', () => ({
	getCurrentWebview: vi.fn(),
}));

const fullPreview: ImportPreview = { skill: 128, mcp: 45, config: 23, agent: 8, schemaOk: true };

function renderPanel(overrides: Partial<React.ComponentProps<typeof ImportPanel>> = {}) {
	const onPathChange = vi.fn();
	const onConflictStrategyChange = vi.fn();
	const onAutoSyncChange = vi.fn();
	const onStartImport = vi.fn();
	render(
		<TooltipProvider>
			<ImportPanel
				path=""
				onPathChange={onPathChange}
				preview={undefined}
				isPreviewLoading={false}
				conflictStrategy={0}
				onConflictStrategyChange={onConflictStrategyChange}
				autoSync={true}
				onAutoSyncChange={onAutoSyncChange}
				onStartImport={onStartImport}
				isImporting={false}
				{...overrides}
			/>
		</TooltipProvider>,
	);
	return { onPathChange, onConflictStrategyChange, onAutoSyncChange, onStartImport };
}

describe('ImportPanel', () => {
	let onDragDropEvent: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		onDragDropEvent = vi.fn().mockResolvedValue(vi.fn());
		vi.mocked(getCurrentWebview).mockReturnValue({
			onDragDropEvent,
		} as unknown as ReturnType<typeof getCurrentWebview>);
	});

	it('应渲染导入标题、拖拽区提示与禁用的选择文件按钮(原生对话框留后续)', () => {
		renderPanel();
		expect(screen.getByText('导入 Import')).toBeInTheDocument();
		expect(screen.getByText(/拖拽文件到此处/)).toBeInTheDocument();
		expect(screen.getByText(/支持 zip、json、tar/)).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /选择文件/ })).toBeDisabled();
	});

	it('未选择路径时不应展示预览计数', () => {
		renderPanel();
		expect(screen.queryByText('128')).not.toBeInTheDocument();
	});

	it('在路径输入框中输入应调用 onPathChange', async () => {
		const user = userEvent.setup();
		const { onPathChange } = renderPanel();

		await user.type(screen.getByPlaceholderText(/完整路径/), 'a');

		expect(onPathChange).toHaveBeenCalled();
	});

	it('给定 preview 时应渲染 Skill/MCP/配置/Agent 计数', () => {
		renderPanel({ path: '/mock/skillhub_backup.zip', preview: fullPreview });

		expect(screen.getByText('将导入的内容预览')).toBeInTheDocument();
		expect(screen.getByText('128')).toBeInTheDocument();
		expect(screen.getByText('45')).toBeInTheDocument();
		expect(screen.getByText('23')).toBeInTheDocument();
		expect(screen.getByText('8')).toBeInTheDocument();
	});

	it('schemaOk=false 时应展示校验未通过的警示文案', () => {
		renderPanel({
			path: '/mock/bad.zip',
			preview: { ...fullPreview, schemaOk: false },
		});

		expect(screen.getByText(/校验未通过/)).toBeInTheDocument();
	});

	it('选择"跳过"冲突策略应以 1 调用 onConflictStrategyChange', async () => {
		const user = userEvent.setup();
		const { onConflictStrategyChange } = renderPanel({
			path: '/mock/skillhub_backup.zip',
			preview: fullPreview,
		});

		await user.click(screen.getByRole('radio', { name: '跳过' }));

		expect(onConflictStrategyChange).toHaveBeenCalledWith(1);
	});

	it('取消勾选"导入后自动同步 Agent"应以 false 调用 onAutoSyncChange', async () => {
		const user = userEvent.setup();
		const { onAutoSyncChange } = renderPanel({
			path: '/mock/skillhub_backup.zip',
			preview: fullPreview,
		});

		await user.click(screen.getByRole('checkbox', { name: '导入后自动同步 Agent' }));

		expect(onAutoSyncChange).toHaveBeenCalledWith(false);
	});

	it('未选择路径时"开始导入"按钮应禁用', () => {
		renderPanel();
		expect(screen.getByRole('button', { name: /开始导入/ })).toBeDisabled();
	});

	it('有路径与预览时点击"开始导入"应调用 onStartImport', async () => {
		const user = userEvent.setup();
		const { onStartImport } = renderPanel({
			path: '/mock/skillhub_backup.zip',
			preview: fullPreview,
		});

		await user.click(screen.getByRole('button', { name: /开始导入/ }));

		expect(onStartImport).toHaveBeenCalled();
	});

	it('schemaOk=false 时"开始导入"按钮应禁用', () => {
		renderPanel({
			path: '/mock/bad.zip',
			preview: { ...fullPreview, schemaOk: false },
		});
		expect(screen.getByRole('button', { name: /开始导入/ })).toBeDisabled();
	});

	it('挂载时应订阅 Tauri webview 拖拽事件, drop 时以真实路径调用 onPathChange', async () => {
		const { onPathChange } = renderPanel();

		expect(onDragDropEvent).toHaveBeenCalledTimes(1);
		const handler = onDragDropEvent.mock.calls[0][0] as EventCallback<DragDropEvent>;

		handler({
			event: 'tauri://drag-drop',
			id: 1,
			payload: {
				type: 'drop',
				paths: ['/Users/demo/skillhub_backup.zip'],
				position: { x: 0, y: 0 } as never,
			},
		});

		expect(onPathChange).toHaveBeenCalledWith('/Users/demo/skillhub_backup.zip');
	});
});
