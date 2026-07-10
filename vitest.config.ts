// 文件作用: Vitest 配置(jsdom 环境 + setup)
// 创建日期: 2026-07-09
import path from 'node:path';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
	plugins: [react()],
	// 与 vite.config.ts 保持一致: @/* 指向 src/*, 供测试文件内 import 的 shadcn/ui 组件解析
	resolve: {
		alias: {
			'@': path.resolve(__dirname, './src'),
		},
	},
	test: { environment: 'jsdom', setupFiles: ['./vitest.setup.ts'], globals: true },
});
