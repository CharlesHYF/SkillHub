// 文件作用: shadcn/ui className 合并工具(clsx+tailwind-merge)(CLI 生成, 语义色由 src/index.css 桥接到 --sh-*, 不手改内部逻辑)
// 创建日期: 2026-07-09
import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}
