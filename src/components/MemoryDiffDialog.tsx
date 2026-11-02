//! 记忆版本 diff 并排视图(迭代 D3.3):行级 LCS diff,左旧右新双栏,可滚动。

import { Fragment, useEffect, useMemo, useState } from 'react';
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

  /** 行级对齐结果 + 双侧行号(空侧不占号)。 */
  const viewRows = useMemo(() => {
    if (leftText === null || rightText === null) return [];
    let l = 0;
    let r = 0;
    return diffLines(leftText, rightText).map((row) => ({
      ...row,
      leftNo: row.left ? ++l : null,
      rightNo: row.right ? ++r : null,
    }));
  }, [leftText, rightText]);

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
          {!error && viewRows.length === 0 && (
            <div style={{ color: 'var(--text-4)', padding: 'var(--space-3)' }}>加载中…</div>
          )}
          {viewRows.length > 0 && (
            <div className="diff-grid">
              <div className="diff-col-head old">{left.label}(旧)</div>
              <div className="diff-col-head new">{right.label}(新)</div>
              {viewRows.map((r, i) => (
                <Fragment key={i}>
                  <div className={`diff-cell side-l ${r.left?.type ?? 'empty'}`}>
                    <span className="diff-ln">{r.leftNo ?? ''}</span>
                    <span className="diff-text">{r.left?.text ?? ''}</span>
                  </div>
                  <div className={`diff-cell side-r ${r.right?.type ?? 'empty'}`}>
                    <span className="diff-ln">{r.rightNo ?? ''}</span>
                    <span className="diff-text">{r.right?.text ?? ''}</span>
                  </div>
                </Fragment>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
