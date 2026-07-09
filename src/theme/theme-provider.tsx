// 文件作用: 亮暗主题 Provider, 读系统偏好并在根元素打 data-theme
// 创建日期: 2026-07-09
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';

type Theme = 'light' | 'dark';
interface ThemeCtx {
	theme: Theme;
	toggle: () => void;
}

const Ctx = createContext<ThemeCtx | null>(null);

/** 读取系统偏好作为初始主题 */
function systemTheme(): Theme {
	return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/** 主题 Provider: 管理亮暗状态并同步到根元素 */
export function ThemeProvider({ children }: { children: ReactNode }) {
	const [theme, setTheme] = useState<Theme>(systemTheme);

	useEffect(() => {
		document.documentElement.setAttribute('data-theme', theme);
	}, [theme]);

	const toggle = () => setTheme((t) => (t === 'dark' ? 'light' : 'dark'));
	return <Ctx.Provider value={{ theme, toggle }}>{children}</Ctx.Provider>;
}

/** 主题 Hook */
export function useTheme(): ThemeCtx {
	const c = useContext(Ctx);
	if (!c) throw new Error('useTheme 必须在 ThemeProvider 内使用');
	return c;
}
