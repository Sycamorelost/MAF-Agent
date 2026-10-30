//! 权限档 chip(D11):展示当前生效档位(会话覆盖 > 全局默认),点击切换本会话覆盖。
//! 切换需二次确认;full 档位红色警示。shell 白名单与 GUI 解锁仍只看全局「完全信任」。
//! 放在通用 components:工作台与普通会话的 Composer 内并列复用(val.md §5.8)。

import { useEffect, useRef, useState } from 'react';
import { IconShield } from './icons';
import { mcpGetApprovalPolicy, type ApprovalMode } from '../lib/ipc/mcp';
import type { PermissionMode } from '../lib/ipc/types';
import { useSessionsStore } from '../stores/sessions';

const MODES: { id: ApprovalMode; label: string; note: string }[] = [
  { id: 'ask', label: '逐次审批', note: '每个工具调用都弹窗确认' },
  { id: 'workspace', label: '工作区写入', note: '工作区内写入自动放行,shell / 删除 / 越界写仍询问' },
  { id: 'full', label: '完全信任', note: '不再询问,命令直接执行(写删仍备份)' },
];

const labelOf = (m: string) => MODES.find((x) => x.id === m)?.label ?? m;

interface Props {
  sessionId: string;
  onError: (msg: string) => void;
}

export function PermissionChip({ sessionId, onError }: Props) {
  const session = useSessionsStore((s) => s.sessions.find((x) => x.id === sessionId));
  const setPermissionMode = useSessionsStore((s) => s.setPermissionMode);
  const [global, setGlobal] = useState<ApprovalMode>('workspace');
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    mcpGetApprovalPolicy()
      .then((p) => setGlobal(p.mode))
      .catch(() => {});
  }, []);

  // 点击外部收起菜单
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open]);

  const override: PermissionMode = session?.permission_mode ?? 'inherit';
  const effective: ApprovalMode = override === 'inherit' ? global : override;
  const note = MODES.find((m) => m.id === effective)?.note ?? '';

  const pick = async (mode: PermissionMode) => {
    setOpen(false);
    if (mode === override) return;
    const desc = mode === 'inherit' ? `跟随全局(${labelOf(global)})` : labelOf(mode);
    const warn =
      mode === 'full'
        ? '\n\n注意:本会话工具调用将不再询问,命令直接执行。shell 白名单与 GUI 自动化仍由全局「完全信任」控制。'
        : '';
    if (!window.confirm(`本会话权限档切为「${desc}」,仅影响当前会话。${warn}`)) return;
    try {
      await setPermissionMode(sessionId, mode);
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="perm-chip-wrap" ref={wrapRef}>
      <button
        type="button"
        className={`perm-chip is-${effective}`}
        title={`${note}${override !== 'inherit' ? '(本会话已覆盖全局默认)' : ''}`}
        onClick={() => setOpen((v) => !v)}
      >
        <IconShield size={12} />
        {labelOf(effective)}
        {override !== 'inherit' && <i className="perm-dot" />}
      </button>
      {open && (
        <div className="perm-menu">
          <div className="perm-menu-note">{note}</div>
          <button
            type="button"
            className={override === 'inherit' ? 'on' : ''}
            onClick={() => void pick('inherit')}
          >
            跟随全局({labelOf(global)})
          </button>
          {MODES.map((m) => (
            <button
              key={m.id}
              type="button"
              className={`${override === m.id ? 'on' : ''}${m.id === 'full' ? ' danger' : ''}`}
              onClick={() => void pick(m.id)}
            >
              {m.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
