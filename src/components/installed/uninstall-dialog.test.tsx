// 文件作用: UninstallDialog 渲染与交互单测(打开/关闭态, 确认/取消回调)
// 创建日期: 2026-07-09
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Resource } from '@/api/library';
import { UninstallDialog } from './uninstall-dialog';

function makeResource(overrides: Partial<Resource> = {}): Resource {
	const name = overrides.name ?? 'demo-skill';
	return {
		id: 1,
		resType: 'Skill',
		name,
		displayName: name,
		version: '1.0.0',
		sourceType: 'LocalImport',
		localPath: '/tmp/demo-skill',
		enabled: true,
		createTime: '2026-07-09 00:00:00',
		updateTime: '2026-07-09 00:00:00',
		...overrides,
	};
}

describe('UninstallDialog', () => {
	it('resource 为 null 时不应显示确认内容', () => {
		render(<UninstallDialog resource={null} onConfirm={vi.fn()} onCancel={vi.fn()} />);
		expect(screen.queryByText(/确认卸载/)).not.toBeInTheDocument();
	});

	it('resource 存在时应显示该资源名的确认提示', () => {
		render(
			<UninstallDialog
				resource={makeResource({ name: 'demo-skill' })}
				onConfirm={vi.fn()}
				onCancel={vi.fn()}
			/>,
		);
		expect(screen.getByText(/确认卸载/)).toBeInTheDocument();
		expect(screen.getByText(/demo-skill/)).toBeInTheDocument();
	});

	it('点击取消应触发 onCancel', async () => {
		const user = userEvent.setup();
		const onCancel = vi.fn();
		render(
			<UninstallDialog resource={makeResource()} onConfirm={vi.fn()} onCancel={onCancel} />,
		);
		await user.click(screen.getByRole('button', { name: '取消' }));
		expect(onCancel).toHaveBeenCalledTimes(1);
	});

	it('点击确认卸载应触发 onConfirm', async () => {
		const user = userEvent.setup();
		const onConfirm = vi.fn();
		render(
			<UninstallDialog resource={makeResource()} onConfirm={onConfirm} onCancel={vi.fn()} />,
		);
		await user.click(screen.getByRole('button', { name: '卸载' }));
		expect(onConfirm).toHaveBeenCalledTimes(1);
	});

	it('isDeleting 为真时确认按钮应禁用且文案变化', () => {
		render(
			<UninstallDialog
				resource={makeResource()}
				onConfirm={vi.fn()}
				onCancel={vi.fn()}
				isDeleting
			/>,
		);
		expect(screen.getByRole('button', { name: '卸载中...' })).toBeDisabled();
	});
});
