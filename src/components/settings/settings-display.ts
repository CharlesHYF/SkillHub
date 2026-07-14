// 文件作用: 设置(SettingRespVO)界面展示态的可选项文案 —— 代理模式下拉、更新通道单选的可选项, 措辞与
//           原型截图第 7 屏一致, 供 network-section/update-channel-section 共用, 避免同一套
//           文案写多份(与 portability/impexp-display 同一惯例)
// 创建日期: 2026-07-10

/** 单个可选项: value 供 Select/RadioGroup 绑定(二者的值本身都是字符串, 由调用方转换), label/
 * description 供展示(与 portability/impexp-display 的 RadioOption 同一形状, 就地定义避免
 * 跨 feature 目录相互引用) */
export interface SettingsOption<T extends number> {
	value: T;
	label: string;
	description?: string;
}

/** 网络代理模式可选项, 措辞与原型截图一致 */
export const PROXY_MODE_OPTIONS: SettingsOption<0 | 1 | 2>[] = [
	{ value: 0, label: '系统默认' },
	{ value: 1, label: '不使用' },
	{ value: 2, label: '手动' },
];

/** 更新通道可选项, 措辞与原型截图一致(含各通道说明文案) */
export const UPDATE_CHANNEL_OPTIONS: SettingsOption<0 | 1>[] = [
	{ value: 0, label: 'Stable (稳定版)', description: '推荐用于生产环境, 提供稳定可靠的功能' },
	{ value: 1, label: 'Beta (测试版)', description: '提前体验新功能, 可能包含未完全稳定的特性' },
];
