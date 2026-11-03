import { useEffect, useState } from 'react';
import type { AppSettings } from '../../lib/ipc/memory';

/**
 * 角色设置:自定义底层物理契约(custom_base_prompt)。
 * 应用默认的物理契约始终不展示(最底层系统不可见);
 * 用户在此填写的内容会覆盖默认契约,留空则恢复默认。
 */
export function RoleSettingsCard({
  settings,
  onSetCustomBasePrompt,
}: {
  settings: AppSettings | null;
  onSetCustomBasePrompt: (text: string) => Promise<void>;
}) {
  const [text, setText] = useState(settings?.custom_base_prompt ?? '');
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setText(settings?.custom_base_prompt ?? '');
  }, [settings?.custom_base_prompt]);

  const onSave = async () => {
    setSaving(true);
    try {
      await onSetCustomBasePrompt(text);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 1800);
    } finally {
      setSaving(false);
    }
  };

  const onReset = async () => {
    setText('');
    setSaving(true);
    try {
      await onSetCustomBasePrompt('');
      setSaved(true);
      window.setTimeout(() => setSaved(false), 1800);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mcp-time-card" style={{ marginBottom: 'var(--space-3)' }}>
      <div className="mcp-time-label">角色设置(底层契约)</div>
      <div className="mcp-time-hint" style={{ marginBottom: 'var(--space-2)' }}>
        应用内置的底层物理契约(工具调用、文件操作、审批等约束)在此不展示。
        你可以在此填写自定义契约覆盖默认行为;留空并保存即恢复应用默认契约。
      </div>
      <textarea
        className="input"
        rows={8}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="留空 = 使用应用默认底层契约;填写后将完全替换默认契约…"
        style={{ fontFamily: 'var(--font-mono)', fontSize: 'calc(var(--font-size-sm) * 0.92)' }}
      />
      <div style={{ display: 'flex', gap: 'var(--space-2)', marginTop: 'var(--space-2)', alignItems: 'center' }}>
        <button type="button" className="btn primary" onClick={onSave} disabled={saving}>
          {saving ? '保存中…' : '保存'}
        </button>
        <button type="button" className="btn ghost" onClick={onReset} disabled={saving || !text}>
          恢复默认
        </button>
        {saved && <span className="set-hint" style={{ color: 'var(--brand)' }}>已保存</span>}
      </div>
    </div>
  );
}
