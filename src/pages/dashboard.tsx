// 文件作用: 首页(M0 临时挂 health 探针验证前后端链路; M1 换真实内容)
// 创建日期: 2026-07-09
import { useQuery } from '@tanstack/react-query';
import { appHealth } from '../api';

export default function Dashboard() {
	const { data } = useQuery({ queryKey: ['health'], queryFn: appHealth });
	return (
		<div>
			<h1 className="text-2xl font-bold">首页 / Dashboard</h1>
			<p className="mt-2 text-sm" style={{ color: 'var(--sh-muted)' }}>
				version {data?.version ?? '...'} · db {data?.dbOk ? 'ok' : '...'}
			</p>
		</div>
	);
}
