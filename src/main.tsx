import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles/_tokens.css';
import { applyUiScale, loadUiScale } from './lib/uiScale';
import { applyContentWidth, loadContentWidth } from './lib/contentWidth';

// 全局字号:首帧前写入 --ui-scale,避免启动时先按默认字号渲染再跳变
applyUiScale(loadUiScale());
// 正文列宽:首帧前写入 --content-w,正文与输入卡共用同一宽度
applyContentWidth(loadContentWidth());

// 屏蔽 WebView 原生右键菜单(刷新 / 另存为 / 打印 / 检查等调试入口);
// 应用内自定义右键菜单由各组件自行渲染,不受影响。
document.addEventListener('contextmenu', (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
