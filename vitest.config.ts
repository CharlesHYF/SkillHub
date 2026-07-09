// 文件作用: Vitest 配置(jsdom 环境 + setup)
// 创建日期: 2026-07-09
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
	plugins: [react()],
	test: { environment: 'jsdom', setupFiles: ['./vitest.setup.ts'], globals: true },
});
