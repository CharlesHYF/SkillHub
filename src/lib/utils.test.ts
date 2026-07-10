// 文件作用: lib/utils 时间格式化函数单测(formatDateTime/formatRelativeTime)
// 创建日期: 2026-07-09
import { describe, it, expect, vi, afterEach } from 'vitest';
import { formatDateTime, formatRelativeTime } from './utils';

describe('formatDateTime', () => {
	it('应把 "YYYY-MM-DD HH:MM:SS" 裁到分钟精度', () => {
		expect(formatDateTime('2025-05-18 14:32:07')).toBe('2025-05-18 14:32');
	});

	it('空字符串应原样返回', () => {
		expect(formatDateTime('')).toBe('');
	});
});

describe('formatRelativeTime', () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it('不到 1 分钟应显示"刚刚"', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-07-09T12:00:30Z'));
		expect(formatRelativeTime('2026-07-09 12:00:00')).toBe('刚刚');
	});

	it('不到 1 小时应显示"N 分钟前"', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-07-09T12:05:00Z'));
		expect(formatRelativeTime('2026-07-09 12:00:00')).toBe('5 分钟前');
	});

	it('不到 1 天应显示"N 小时前"', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-07-09T14:00:00Z'));
		expect(formatRelativeTime('2026-07-09 12:00:00')).toBe('2 小时前');
	});

	it('不到 7 天应显示"N 天前"', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-07-11T12:00:00Z'));
		expect(formatRelativeTime('2026-07-09 12:00:00')).toBe('2 天前');
	});

	it('超过 7 天应显示"N 周前"', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-07-16T12:00:00Z'));
		expect(formatRelativeTime('2026-07-09 12:00:00')).toBe('1 周前');
	});

	it('空字符串应原样返回', () => {
		expect(formatRelativeTime('')).toBe('');
	});
});
