// 文件作用: 认证相关展示态派生 —— 安装/授权固定权限说明清单, 供 AuthModal"授权后将允许"清单与
//           pages/marketplace-detail"权限说明"卡片共用同一份文案, 避免中文字符串复制两份
// 创建日期: 2026-07-10

/** 单条权限说明: 标题 + 描述, 与原型截图"权限说明"/"授权后将允许 SkillHub"的清单项一一对应 */
export interface PermissionItem {
	title: string;
	description: string;
}

/** 安装一条市场资源固有需要的权限动作: 与具体资源无关的通用静态说明(下载安装这件事本身就需要
 * 读取元数据/下载文件/同步到 Agent), 不是从后端按资源查询到的字段, 故直接以常量形式维护 */
export const INSTALL_PERMISSIONS: PermissionItem[] = [
	{ title: '读取资源', description: '读取资源元数据与配置信息' },
	{ title: '下载更新', description: '下载资源文件与版本更新' },
	{ title: '同步到 Agent', description: '将资源同步并配置到已连接的 Agent' },
];
