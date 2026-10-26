import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles/_tokens.css';
import { applyUiScale, loadUiScale } from './lib/uiScale';

// 全局字号:首帧前写入 --ui-scale,避免启动时先按默认字号渲染再跳变
applyUiScale(loadUiScale());

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
