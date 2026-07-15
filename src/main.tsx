// 文件作用: React 入口
// 创建日期: 2026-07-09
// 修改日期: 2026-07-13
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
	<React.StrictMode>
		<App />
	</React.StrictMode>,
);
