// 文件作用: i18next 初始化(中英双语, 默认中文)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import zh from './zh.json';
import en from './en.json';

void i18n.use(initReactI18next).init({
	resources: { zh: { translation: zh }, en: { translation: en } },
	lng: 'zh',
	fallbackLng: 'en',
	interpolation: { escapeValue: false },
});

export default i18n;
