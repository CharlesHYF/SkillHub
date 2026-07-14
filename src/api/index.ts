// 文件作用: Tauri command 的类型化封装层(前端唯一调用后端的入口)
// 创建日期: 2026-07-09
import { invoke } from '@tauri-apps/api/core';

/** 应用健康信息 */
export interface AppHealthRespVO {
	version: string;
	dbOk: boolean;
}

/** 调用后端 app_health 命令 */
export async function appHealth(): Promise<AppHealthRespVO> {
	return invoke<AppHealthRespVO>('app_health');
}
