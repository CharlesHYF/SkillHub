// 文件作用: 通用工具函数 —— className 合并(clsx+tailwind-merge, shadcn/ui CLI 生成部分不手改
//           内部逻辑)、后端时间戳(SQLite datetime('now'), "YYYY-MM-DD HH:MM:SS" UTC)的展示格式化
// 创建日期: 2026-07-09
import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

/** 把后端 "YYYY-MM-DD HH:MM:SS" 时间戳解析为 Date; 空串返回 null。SQLite datetime('now') 落的
 * 是 UTC 时间但不带时区标记, 显式补 "Z" 按 UTC 解析, 避免被 Date 构造函数当成本地时间 */
function parseBackendTimestamp(value: string): Date | null {
	if (!value) return null;
	const iso = `${value.replace(' ', 'T')}Z`;
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? null : date;
}

/** 展示用日期时间: 裁到分钟精度("YYYY-MM-DD HH:MM:SS" -> "YYYY-MM-DD HH:MM"); 空串/解析失败
 * 原样返回入参, 不抛错(展示层容错优先) */
export function formatDateTime(value: string): string {
	const date = parseBackendTimestamp(value);
	if (!date) return value;
	return value.slice(0, 16);
}

/** 展示用相对时间(如 "5 分钟前"), 精度从分钟到周: <1 分钟="刚刚", <1 小时按分钟, <1 天按小时,
 * <7 天按天, 否则按周(向下取整); 空串/解析失败原样返回入参 */
export function formatRelativeTime(value: string): string {
	const date = parseBackendTimestamp(value);
	if (!date) return value;

	const diffMs = Date.now() - date.getTime();
	const diffMinutes = Math.floor(diffMs / (60 * 1000));
	if (diffMinutes < 1) return '刚刚';
	if (diffMinutes < 60) return `${diffMinutes} 分钟前`;

	const diffHours = Math.floor(diffMinutes / 60);
	if (diffHours < 24) return `${diffHours} 小时前`;

	const diffDays = Math.floor(diffHours / 24);
	if (diffDays < 7) return `${diffDays} 天前`;

	const diffWeeks = Math.floor(diffDays / 7);
	return `${diffWeeks} 周前`;
}
