//! 权限档 chip(D11):展示当前生效档位(会话覆盖 > 全局默认),点击切换本会话覆盖。
//! 切换需二次确认;full 档位红色警示。shell 白名单与 GUI 解锁仍只看全局「完全信任」。
//! 放在通用 components:工作台与普通会话的 Composer 内并列复用(val.md §5.8)。

import { useEffect, useRef, useState } from 'react';
import { IconShield } from './icons';
import {
  mcpFullTrustStatus,
  mcpGetApprovalPolicy,
  mcpGrantFullTrust,
  mcpRevokeFullTrust,
  type ApprovalMode,
} from '../lib/ipc/mcp';
import type { PermissionMode } from '../lib/ipc/types';
import { useSessionsStore } from '../stores/sessions';
import { uiConfirm } from './confirmService';

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
  /** P8:本会话可用的完全信任临时授权状态 */
  const [ftActive, setFtActive] = useState(false);
  const [ftRemaining, setFtRemaining] = useState(0);
  const wrapRef = useRef<HTMLDivElement>(null);

  const refreshFt = () =>
    mcpFullTrustStatus(sessionId)
      .then((s) => {
        setFtActive(s.active);
        setFtRemaining(s.remaining_secs);
      })
      .catch(() => {});

  useEffect(() => {
    mcpGetApprovalPolicy()
      .then((p) => setGlobal(p.mode))
      .catch(() => {});
  }, []);

  // P8:定时刷新授权剩余时间(到期后 chip 会提示「未授权」)
  useEffect(() => {
    refreshFt();
    const timer = setInterval(refreshFt, 30_000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

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
  // 声明为完全信任但临时授权缺失/过期:后端实际按工作区档执行
  const unauthorized = effective === 'full' && !ftActive;

  const pick = async (mode: PermissionMode) => {
    setOpen(false);
    // 相同档位默认不重复确认;但完全信任允许再次点击「续期」,
    // 以及全局 full 未授权时允许在此一键补授权
    const reGrantGlobal = mode === 'inherit' && unauthorized;
    if (mode === override && mode !== 'full' && !reGrantGlobal) return;
    const desc = mode === 'inherit' ? `跟随全局(${labelOf(global)})` : labelOf(mode);
    const warn =
      mode === 'full' || reGrantGlobal
        ? '\n\n注意:将签发 60 分钟完全信任临时授权,到期自动回落;期间工具不再询问、命令直接执行(写删仍备份并审计)。'
        : '';
    if (
      !(await uiConfirm({
        title: '切换权限档',
        message: `本会话权限档切为「${desc}」,仅影响当前会话。${warn}`,
      }))
    )
      return;
    try {
      await setPermissionMode(sessionId, mode);
      // P8:进入 full 先落库会话覆盖,再签发绑定本会话的临时授权;离开时按会话吊销
      if (mode === 'full') await mcpGrantFullTrust(sessionId, 60);
      else if (reGrantGlobal) await mcpGrantFullTrust(undefined, 60);
      else if (override === 'full') await mcpRevokeFullTrust(sessionId);
      await refreshFt();
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="perm-chip-wrap" ref={wrapRef}>
      <button
        type="button"
        className={`perm-chip is-${effective}`}
        title={
          unauthorized
            ? `${note}\n完全信任临时授权未生效或已到期,实际按「工作区」档执行;点击可签发 60 分钟授权。`
            : `${note}${override !== 'inherit' ? '(本会话已覆盖全局默认)' : ''}${
                ftActive && effective === 'full' ? `\n临时授权剩余 ${Math.ceil(ftRemaining / 60)} 分钟` : ''
              }`
        }
        onClick={() => setOpen((v) => !v)}
      >
        <IconShield size={12} />
        <span className="ctl-chip-text">
          {labelOf(effective)}
          {unauthorized && ' · 未授权'}
        </span>
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
