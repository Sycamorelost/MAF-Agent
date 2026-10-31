/**
 * 应用内确认对话框 —— 替代 window.confirm(Tauri/WebView 环境下原生确认框的
 * 显示时机与调用返回值可能不同步,导致"先执行后确认")。
 * 复用 .dialog-mask / .dialog 样式,不新增色值。
 */
import { useEffect } from 'react';

export function ConfirmDialog({
  title,
  message,
  confirmText = '确认',
  cancelText = '取消',
  danger = false,
  busy = false,
  onConfirm,
  onCancel,
}: {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onCancel]);

  return (
    <div className="dialog-mask" onClick={onCancel}>
      <div
        className="dialog"
        style={{ width: 'min(420px, 92vw)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="dialog-head">
          {title}
        </div>
        <div className="dialog-body" style={{ padding: 'var(--space-3) var(--pad-x)' }}>
          <div
            style={{
              fontSize: 'var(--font-body)',
              color: 'var(--text-2)',
              lineHeight: 1.6,
              whiteSpace: 'pre-wrap',
            }}
          >
            {message}
          </div>
        </div>
        <div className="dialog-foot" style={{ justifyContent: 'flex-end' }}>
          <button className="btn btn-ghost" disabled={busy} onClick={onCancel}>
            {cancelText}
          </button>
          <button
            className="btn btn-primary"
            disabled={busy}
            onClick={onConfirm}
            style={danger ? { background: 'var(--warn)', borderColor: 'var(--warn)' } : undefined}
          >
            {busy ? '处理中…' : confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}
