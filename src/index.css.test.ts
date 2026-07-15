// 文件作用: src/index.css 全局交互态光标规则的文本层回归锁定 —— jsdom 不支持 CSS 级联层
//           (@layer)语义, 无法真正验证 cursor 的计算值/跨层覆盖是否生效, 故只做"规则文本仍
//           存在且选择器/声明未被误删改"的存在性校验; 真正的层序正确性(utilities 层晚于 base
//           层声明, 需 !important 才能压过 Radix 菜单/下拉基元自带的 cursor-default 工具类)
//           已经由 vite build 产物人工核对确认, 见 src/index.css 内联注释与本任务报告
// 创建日期: 2026-07-11
// 修改日期: 2026-07-13
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// vitest 由 pnpm test(即 package.json 的 "vitest run")在仓库根目录下执行, 用 process.cwd()
// 拼路径即可; import.meta.url 在 Vite 的 Node 测试转换环境下不保证是合法 file:// URL(实测
// new URL('./index.css', import.meta.url) 会抛 "The URL must be of scheme file"), 故不采用
const css = readFileSync(resolve(process.cwd(), 'src/index.css'), 'utf-8');

describe('index.css 全局交互态光标规则', () => {
	it('按钮/下拉触发器与菜单项/标签页/单选/开关/复选框/select/summary 应声明 cursor: pointer', () => {
		expect(css).toMatch(/button:not\(:disabled\)/);
		expect(css).toMatch(/\[role=['"]button['"]\]/);
		expect(css).toMatch(/\[role=['"]menuitem['"]\]/);
		expect(css).toMatch(/\[role=['"]menuitemcheckbox['"]\]/);
		expect(css).toMatch(/\[role=['"]menuitemradio['"]\]/);
		expect(css).toMatch(/\[role=['"]option['"]\]/);
		expect(css).toMatch(/\[role=['"]tab['"]\]/);
		expect(css).toMatch(/\[role=['"]radio['"]\]/);
		expect(css).toMatch(/\[role=['"]switch['"]\]/);
		expect(css).toMatch(/\[role=['"]checkbox['"]\]/);
		expect(css).toMatch(/\bselect\b[\s\S]*\bsummary\b[\s\S]*\{/);
		expect(css).toMatch(/cursor:\s*pointer\s*!important/);
	});

	it('禁用态(:disabled/[disabled]/aria-disabled/data-disabled)应声明 cursor: not-allowed', () => {
		expect(css).toMatch(/:disabled,/);
		expect(css).toMatch(/\[disabled\]/);
		expect(css).toMatch(/\[aria-disabled=['"]true['"]\]/);
		expect(css).toMatch(/\[data-disabled\]/);
		expect(css).toMatch(/cursor:\s*not-allowed\s*!important/);
	});

	it('两条规则均应落在 @layer base 内(而非裸规则或 utilities 层)', () => {
		// 不做括号配对解析(易因嵌套花括号误判), 只验证两条 cursor 规则的文本位置晚于
		// "@layer base {" 这一唯一入口且早于文件末尾, 配合本文件就一个 @layer base 块的
		// 既有事实(见 src/index.css), 足以锁定"仍在 base 层内"这一点
		const baseStart = css.indexOf('@layer base {');
		expect(baseStart).toBeGreaterThan(-1);
		expect(css.indexOf('cursor: pointer !important')).toBeGreaterThan(baseStart);
		expect(css.indexOf('cursor: not-allowed !important')).toBeGreaterThan(baseStart);
	});
});
