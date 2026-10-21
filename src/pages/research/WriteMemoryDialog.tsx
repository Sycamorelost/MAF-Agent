import { useEffect, useState } from 'react';
import type { ActiveResearch } from '../../stores/research';
import * as ipc from '../../lib/ipc';
import { IconBrain, IconCheck } from '../../components/icons';
import { extractTldr, fmtDate } from './helpers';

export function WriteMemoryDialog({
  active,
  onClose,
}: {
  active: ActiveResearch;
  onClose: () => void;
}) {
  const [sessions, setSessions] = useState<ipc.SessionRow[] | null>(null);
  const [savedId, setSavedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .listSessions()
      .then(setSessions)
      .catch((e) => setError(String(e)));
  }, []);

  const writeTo = async (s: ipc.SessionRow) => {
    try {
      const tldr = extractTldr(active.report);
      const text = [
        `## 深度研究:${active.topic}`,
        ``,
        `- 时间:${fmtDate(new Date().toISOString())}`,
        `- 来源:${active.sources.length} 条`,
        `- 研究 ID:${active.id}`,
        ``,
        `### TL;DR`,
        tldr,
      ].join('\n');
      await ipc.appendMemory(s.id, text);
      setSavedId(s.id);
      setTimeout(onClose, 900);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog" style={{ width: 440 }} onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">写入会话记忆</div>
        <div className="dialog-body">
          <p style={{ margin: '0 0 var(--space-3)', color: 'var(--text-3)', fontSize: 'var(--font-caption)' }}>
            将本次研究的 TL;DR 摘要追加到所选会话的 memory.md。
          </p>
          {error && <div className="pg-warn">{error}</div>}
          {!sessions && !error && <div className="rp-empty">加载会话列表…</div>}
          {sessions?.length === 0 && <div className="rp-empty">还没有会话,请先新建对话</div>}
          <div className="rp-sess-list">
            {sessions?.map((s) => (
              <button key={s.id} className="rp-sess" onClick={() => void writeTo(s)} disabled={!!savedId}>
                <span className="ellipsis">{s.title || '未命名会话'}</span>
                {savedId === s.id ? (
                  <IconCheck size={15} className="ok" />
                ) : (
                  <IconBrain size={15} />
                )}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
