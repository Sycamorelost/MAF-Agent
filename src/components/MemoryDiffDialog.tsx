//! 记忆版本 diff 并排视图(迭代 D3.3):行级 LCS diff,左旧右新。

import { useEffect, useMemo, useState } from 'react';
import * as ipc from '../lib/ipc';
import { diffLines } from '../lib/diffLines';

export interface DiffSide {
  label: string;
  /** null = 当前版本 */
  version: number | null;
}

interface Props {
  sessionId: string;
  left: DiffSide;
  right: DiffSide;
  /** 当前版本完整内容(version 为 null 的一侧使用) */
  currentText: string;
  onClose: () => void;
}

export default function MemoryDiffDialog({ sessionId, left, right, currentText, onClose }: Props) {
  const [leftText, setLeftText] = useState<string | null>(null);
  const [rightText, setRightText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const loadSide = (s: DiffSide) =>
      s.version === null ? Promise.resolve(currentText) : ipc.readMemoryVersion(sessionId, s.version);
    Promise.all([loadSide(left), loadSide(right)])
      .then(([l, r]) => {
        if (!alive) return;
        setLeftText(l);
        setRightText(r);
      })
      .catch((e) => alive && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      alive = false;
    };
  }, [sessionId, left, right, currentText]);

  const rows = useMemo(
    () => (leftText !== null && rightText !== null ? diffLines(leftText, rightText) : []),
    [leftText, rightText],
  );

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog diff-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">
          版本对比
          <div className="diff-labels">
            <span className="diff-label old">{left.label}</span>
            <span className="diff-label-sep">→</span>
            <span className="diff-label new">{right.label}</span>
          </div>
        </div>
        <div className="dialog-body diff-body">
          {error && <div style={{ color: 'var(--err)', padding: 'var(--space-3)' }}>{error}</div>}
          {!error && rows.length === 0 && (
            <div style={{ color: 'var(--text-4)', padding: 'var(--space-3)' }}>加载中…</div>
          )}
          {rows.length > 0 && (
            <div className="diff-grid">
              {rows.map((r, i) => (
                <div className="diff-row" key={i}>
                  <div className={`diff-cell ${r.left?.type ?? 'empty'} ${r.left?.type === 'del' ? 'del' : ''}`}>
                    {r.left?.text ?? ''}
                  </div>
                  <div className={`diff-cell ${r.right?.type ?? 'empty'} ${r.right?.type === 'add' ? 'add' : ''}`}>
                    {r.right?.text ?? ''}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
