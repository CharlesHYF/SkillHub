// 文件作用: 单选组基元(手写补齐, CLI registry 未含此组件; 参照同目录 Checkbox/Select 等 CLI 生成
//           基元的数据槽/语义色桥接约定手写, 选中态选择器用 Radix 实际会写入的 data-state, 不手改
//           radix-ui 内部逻辑, 仅做外层样式包装)
// 创建日期: 2026-07-10
import * as React from 'react';
import { RadioGroup as RadioGroupPrimitive } from 'radix-ui';

import { cn } from '@/lib/utils';

function RadioGroup({
	className,
	...props
}: React.ComponentProps<typeof RadioGroupPrimitive.Root>) {
	return (
		<RadioGroupPrimitive.Root
			data-slot="radio-group"
			className={cn('grid gap-2', className)}
			{...props}
		/>
	);
}

function RadioGroupItem({
	className,
	...props
}: React.ComponentProps<typeof RadioGroupPrimitive.Item>) {
	return (
		<RadioGroupPrimitive.Item
			data-slot="radio-group-item"
			className={cn(
				'aspect-square size-4 shrink-0 rounded-full border border-input shadow-xs outline-none transition-shadow focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 data-[state=checked]:border-primary dark:bg-input/30',
				className,
			)}
			{...props}
		>
			<RadioGroupPrimitive.Indicator
				data-slot="radio-group-indicator"
				// after:content-[''] 不可省略: Tailwind 的 after: 伪元素默认 content 为 none,
				// 不显式声明 content 时浏览器根本不会为 ::after 生成盒子(jsdom 不渲染伪元素, 单测测不出
				// 这个缺陷, 真实浏览器里选中态圆点会完全不可见), 见本任务报告"item4"一节
				className="relative flex items-center justify-center after:size-2 after:rounded-full after:bg-primary after:content-['']"
			/>
		</RadioGroupPrimitive.Item>
	);
}

export { RadioGroup, RadioGroupItem };
