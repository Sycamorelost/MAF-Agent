import { useEffect, useMemo, useRef, useState } from 'react';
import * as ipc from '../../lib/ipc';
import type { AttachmentEntry, AttachmentMeta } from '../../lib/ipc';
import { uiConfirm } from '../confirmService';
import { IconCheck } from '../icons';
import { KIND_LABEL, fmtSize } from '../../pages/attachments/shared';

interface Props {
  /** 目标会话(当前对话) */
  sessionId: string;
  onClose: () => void;
  onImported: (metas: AttachmentMeta[]) => void;
  onError: (msg: string) => void;
}

/** 未解析且非图片的附件不能复用解析结果,勾选前直接禁用 */
function unavailable(e: AttachmentEntry): boolean {
  return (e.meta.kind !== 'image' && (e.meta.chunk_count ?? 0) === 0 && !!e.meta.extract_note);
}

/** 附件中心选择器:勾选 → 确认 → 导入到当前会话(已解析的复用 doc_chunks)。 */
export default function AttachmentCenterPicker({
  sessionId,
  onClose,
  onImported,
  onError,
}: Props) {
  const [entries, setEntries] = useState<AttachmentEntry[] | null>(null);
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  // onError 由父组件传入,放进依赖会导致重复拉取,这里用 ref 固定引用
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  useEffect(() => {
    let alive = true;
    ipc
      .listAllAttachments()
      .then((list) => {
        if (alive) setEntries(list);
      })
      .catch((e) => {
        if (!alive) return;
        setEntries([]);
        onErrorRef.current(`加载附件中心失败:${e instanceof Error ? e.message : String(e)}`);
      });
    return () => {
      alive = false;
    };
  }, []);

  /** 按文件路径去重(同一附件可能出现在多条消息),并过滤无法复用的条目 */
  const list = useMemo(() => {
    if (!entries) return [];
    const seen = new Set<string>();
    const uniq: AttachmentEntry[] = [];
    for (const e of entries) {
      if (e.meta.kind === 'other' || seen.has(e.meta.path)) continue;
      seen.add(e.meta.path);
      uniq.push(e);
    }
    const kw = query.trim().toLowerCase();
    return kw ? uniq.filter((e) => e.meta.name.toLowerCase().includes(kw)) : uniq;
  }, [entries, query]);

  const toggle = (e: AttachmentEntry) => {
    if (busy || unavailable(e)) return;
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(e.meta.path)) next.delete(e.meta.path);
      else next.add(e.meta.path);
      return next;
    });
  };

  const onImport = async () => {
    if (busy || picked.size === 0) return;
    const chosen = list.filter((e) => picked.has(e.meta.path));
    if (chosen.length === 0) return;
    // 硬约束:未确认不执行
    const ok = await uiConfirm({
      title: '确认导入附件',
      message: `将把以下 ${chosen.length} 个附件导入当前会话：\n${chosen
        .map((e) => `· ${e.meta.name}`)
        .join('\n')}\n\n已解析的文档会复用现有解析结果，不会重新读取源文件。`,
      confirmText: '导入',
    });
    if (!ok) return;
    setBusy(true);
    const metas: AttachmentMeta[] = [];
    const fails: string[] = [];
    for (const e of chosen) {
      try {
        metas.push(
          await ipc.importAttachmentFromCenter(sessionId, e.message_id, e.meta.path),
        );
      } catch (err) {
        fails.push(`${e.meta.name}:${err instanceof Error ? err.message : String(err)}`);
      }
    }
    setBusy(false);
    if (fails.length > 0) onError(`部分附件导入失败：\n${fails.join('\n')}`);
    if (metas.length > 0) {
      onImported(metas);
      onClose();
    }
  };

  return (
    <div className="dialog-mask" onClick={() => !busy && onClose()}>
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 'min(calc(720px * var(--ui-scale)), 92vw)',
          maxHeight: '80vh',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <div className="dialog-head">从附件中心导入</div>
        <div className="dialog-body" style={{ overflow: 'auto', flex: 1 }}>
          <div style={{ display: 'flex', gap: 'var(--space-2)', marginBottom: 'var(--space-3)' }}>
            <input
              className="input"
              placeholder="搜索文件名…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              style={{ flex: 1, minWidth: 0 }}
            />
          </div>
          {entries === null ? (
            <div className="att-pick-empty">加载中…</div>
          ) : list.length === 0 ? (
            <div className="att-pick-empty">附件中心还没有可导入的文件</div>
          ) : (
            <div className="att-pick-list">
              {list.map((e) => {
                const checked = picked.has(e.meta.path);
                const disabled = busy || unavailable(e);
                return (
                  <button
                    type="button"
                    key={e.meta.path}
                    className={`att-pick-item ${checked ? 'on' : ''} ${disabled ? 'off' : ''}`}
                    onClick={() => toggle(e)}
                    disabled={disabled}
                    title={e.meta.summary ?? e.meta.name}
                  >
                    <span className={`att-pick-box ${checked ? 'on' : ''}`}>
                      {checked && <IconCheck size={11} />}
                    </span>
                    <span className="att-pick-main">
                      <span className="att-pick-name">{e.meta.name}</span>
                      <span className="att-pick-meta">
                        {KIND_LABEL[e.meta.kind]} · {fmtSize(e.meta.size)}
                        {(e.meta.chunk_count ?? 0) > 0 && ` · ${e.meta.chunk_count} 块`}
                        {` · 来源:${e.session_title}`}
                      </span>
                      {e.meta.summary && <span className="att-pick-sum">{e.meta.summary}</span>}
                      {unavailable(e) && <span className="att-pick-sum">未解析,无法复用</span>}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
        <div className="dialog-foot" style={{ justifyContent: 'flex-end' }}>
          <button type="button" className="btn" onClick={onClose} disabled={busy}>
            取消
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={() => void onImport()}
            disabled={busy || picked.size === 0}
          >
            {busy ? '导入中…' : `导入选中(${picked.size})`}
          </button>
        </div>
      </div>
    </div>
  );
}
