/**
 * 应用内文本输入对话框 —— 替代原生 window.prompt
 * (WebView 下原生 prompt 位置固定于窗口顶部、且不遵循应用主题)。
 * 复用 .dialog-mask / .dialog / .input 样式,不新增色值。
 */
import { useEffect, useRef, useState } from 'react';

export function PromptDialog({
  title,
  label,
  initialValue = '',
  confirmText = '确定',
  cancelText = '取消',
  placeholder,
  onConfirm,
  onCancel,
}: {
  title: string;
  label?: string;
  initialValue?: string;
  confirmText?: string;
  cancelText?: string;
  placeholder?: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    el.select();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onCancel]);

  const submit = () => {
    const v = value.trim();
    if (!v) return;
    onConfirm(v);
  };

  return (
    <div className="dialog-mask" onClick={onCancel}>
      <div
        className="dialog"
        style={{ width: 'min(420px, 92vw)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="dialog-head">{title}</div>
        <div className="dialog-body" style={{ padding: 'var(--space-3) var(--pad-x)' }}>
          {label && (
            <div
              style={{
                marginBottom: 'var(--space-2)',
                fontSize: 'var(--font-caption)',
                color: 'var(--text-3)',
              }}
            >
              {label}
            </div>
          )}
          <input
            ref={inputRef}
            className="input"
            value={value}
            placeholder={placeholder}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                submit();
              }
            }}
          />
        </div>
        <div className="dialog-foot" style={{ justifyContent: 'flex-end' }}>
          <button className="btn btn-ghost" onClick={onCancel}>
            {cancelText}
          </button>
          <button className="btn btn-primary" disabled={!value.trim()} onClick={submit}>
            {confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}
