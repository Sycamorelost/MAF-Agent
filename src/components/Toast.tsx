//! 通用轻提示组件:沿用既有 .toast 视觉(底部居中深色胶囊)。
//! 多行清单(文件导入被拒)升级为「一次性列出的可滚动面板」,由用户滚动查看后手动关闭。
//! 统一 portal 到 body,避免被 overflow:hidden 容器(侧栏抽屉/面板)裁剪。
//! (SSR/单测 node 环境无 document,直接返回节点;浏览器运行时始终 portal。)

import { createPortal } from 'react-dom';

interface Props {
  text: string | null;
  /** 多行面板的关闭回调(单行提示不显示关闭按钮,到时自动消失) */
  onClose?: () => void;
}

export default function Toast({ text, onClose }: Props) {
  if (!text) return null;
  const node = !text.includes('\n') ? (
    <div className="toast" role="status">
      {text}
    </div>
  ) : (
    <div className="toast toast-list" role="status">
      <div className="toast-head">
        <span className="toast-title">导入提示 · 未加入的文件与限制</span>
        <span className="toast-tip">可滚动查看</span>
        {onClose && (
          <button className="toast-x" type="button" onClick={onClose} title="关闭提示">
            ×
          </button>
        )}
      </div>
      <div className="toast-body">{text}</div>
    </div>
  );
  if (typeof document === 'undefined') return node;
  return createPortal(node, document.body);
}
