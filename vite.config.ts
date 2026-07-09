// 文件作用: Vite 构建配置, 适配 Tauri 开发所需的固定端口与 HMR 设置
// 创建日期: 2026-07-09
import path from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
	// Tailwind 用 Vite 插件而非 PostCSS 接入: 让 Vite 自身的 CSS 资源管线正确处理
	// node_modules 内 @import 的字体包(@fontsource-variable/inter)里的相对 url() 引用
	plugins: [react(), tailwindcss()],

	// 路径别名: @/* 指向 src/*, 供 shadcn/ui 组件与业务代码统一引用
	resolve: {
		alias: {
			'@': path.resolve(__dirname, './src'),
		},
	},

	// 以下配置专为 Tauri 开发场景适配, 仅在 `tauri dev` / `tauri build` 时生效
	//
	// 1. 避免 Vite 的报错遮住 Rust 侧的报错
	clearScreen: false,
	// 2. Tauri 需要固定端口, 端口被占用时直接失败而不是自动换端口
	server: {
		port: 1420,
		strictPort: true,
		host: host || false,
		hmr: host
			? {
					protocol: 'ws',
					host,
					port: 1421,
				}
			: undefined,
		watch: {
			// 3. 告知 Vite 忽略 `src-tauri` 目录的文件变更
			ignored: ['**/src-tauri/**'],
		},
	},
}));
