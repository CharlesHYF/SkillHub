// 文件作用: 测试全局 setup(jest-dom 断言 + matchMedia 兜底)
// 创建日期: 2026-07-09
import '@testing-library/jest-dom';

if (!window.matchMedia) {
	window.matchMedia = ((q: string) => ({
		matches: false,
		media: q,
		onchange: null,
		addEventListener: () => {},
		removeEventListener: () => {},
		addListener: () => {},
		removeListener: () => {},
		dispatchEvent: () => false,
	})) as unknown as typeof window.matchMedia;
}
