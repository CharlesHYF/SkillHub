// 文件作用: 资源类型徽标(Skill/MCP), 中性描边 + 图标 + 文案区分, 不做高饱和撞色(见 DESIGN.md)
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import { Sparkles, Plug, type LucideIcon } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

/** 资源类型, 对应展示层的 Skill/MCP 两类(与后端 wire 层 ResourceType 大小写不同, 调用方按需映射) */
export type ResourceKind = 'skill' | 'mcp';

interface TypeMeta {
	label: string;
	icon: LucideIcon;
}

const TYPE_META: Record<ResourceKind, TypeMeta> = {
	skill: { label: 'Skill', icon: Sparkles },
	mcp: { label: 'MCP', icon: Plug },
};

interface TypeBadgeProps {
	/** 资源类型: skill 或 mcp */
	type: ResourceKind;
	className?: string;
}

/** 资源类型徽标: Skill/MCP 用中性描边徽标 + 图标 + 文案区分, 两类不做高饱和撞色 */
export function TypeBadge({ type, className }: TypeBadgeProps) {
	const { label, icon: Icon } = TYPE_META[type];
	return (
		<Badge variant="outline" className={cn('gap-1', className)}>
			<Icon />
			{label}
		</Badge>
	);
}
