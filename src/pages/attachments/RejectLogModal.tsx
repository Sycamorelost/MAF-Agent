//! 导入拒绝记录弹窗:列出被拒绝的文件与原因(可滚动),支持刷新与清空。

import { useEffect, useState } from 'react';
import type { AttachmentRejectList } from '../../lib/ipc';
import { IconRefresh, IconTrash, IconX } from '../../components/icons';
import { rejectSourceLabel } from './shared';

export interface RejectLogModalProps {
  data: AttachmentRejectList | null;
  loading: boolean;
  clearing: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onClear: () => void;
}

export function RejectLogModal({
  data,
  loading,
  clearing,
  onClose,
  onRefresh,
  onClear,
}: RejectLogModalProps) {
  // 清空为两态确认,3 秒未确认自动复位
  const [confirmClear, setConfirmClear] = useState(false);
  useEffect(() => {
    if (!confirmClear) return;
    const t = window.setTimeout(() => setConfirmClear(false), 3000);
    return () => window.clearTimeout(t);
  }, [confirmClear]);

  const items = data?.items ?? [];
  return (
    <div className="att-modal-mask" onClick={onClose}>
      <div className="att-modal att-reject" onClick={(e) => e.stopPropagation()}>
        <div className="att-modal-head">
          <span className="t">导入拒绝记录</span>
          <span className="att-modal-sub">
            {data ? `共 ${data.total} 条 · 显示最近 ${items.length} 条` : '加载中…'}
          </span>
          <button title="刷新" onClick={onRefresh} disabled={loading}>
            <IconRefresh size={14} />
          </button>
          <button title="关闭" onClick={onClose}>
            <IconX size={14} />
          </button>
        </div>

        <div className="att-reject-body">
          {loading && <div className="att-empty">加载中…</div>}
          {!loading && items.length === 0 && (
            <div className="att-empty">
              <IconRefresh size={30} strokeWidth={1.1} />
              <span>还没有拒绝记录。被格式黑名单、空文件、超限或配额拦截的文件会留在这里。</span>
            </div>
          )}
          {!loading &&
            items.map((r, i) => (
              <div className="att-reject-row" key={`${r.time}-${r.name}-${i}`}>
                <div className="r1">
                  <span className="n" title={r.name}>
                    {r.name}
                  </span>
                  <span className={`tag ${r.source === 'save' ? 'err' : 'warn'}`}>
                    {rejectSourceLabel(r.source)}
                  </span>
                </div>
                <div className="r2">
                  <span className="tm">{r.time}</span>
                  <span className="sn" title={r.session_id || '未关联会话'}>
                    {r.session_title || (r.session_id ? r.session_id : '未关联会话')}
                  </span>
                  <span className="rs">{r.reason}</span>
                </div>
              </div>
            ))}
        </div>

        <div className="att-modal-foot">
          <span className="att-modal-hint">仅保留最近 2000 条,用于排查「文件为什么没进来」</span>
          <span className="sp" />
          <button
            className={`att-btn${confirmClear ? ' danger' : ''}`}
            disabled={clearing || !data || data.total === 0}
            onClick={() => {
              if (!confirmClear) {
                setConfirmClear(true);
                return;
              }
              setConfirmClear(false);
              onClear();
            }}
            title={confirmClear ? '再次点击确认清空' : '清空全部拒绝记录'}
          >
            <IconTrash size={13} />
            {confirmClear ? '确认清空' : '清空记录'}
          </button>
        </div>
      </div>
    </div>
  );
}
