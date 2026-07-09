// 文件作用: Vite 构建配置, 适配 Tauri 开发所需的固定端口与 HMR 设置
// 创建日期: 2026-07-09
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process 是 nodejs 全局对象
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

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
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. 告知 Vite 忽略 `src-tauri` 目录的文件变更
      ignored: ["**/src-tauri/**"],
    },
  },
}));
