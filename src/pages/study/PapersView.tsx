/** 试卷列表:随机组卷入口 + 历史试卷(整卷测试在独立页面完成)。 */
import { useCallback, useEffect, useState } from 'react';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { IconTrash } from '../../components/icons';
import { useStudyStore } from '../../stores/study';
import type { PaperRow } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';

const fmtDate = (s: string) => {
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
};

const statusText = (p: PaperRow) => (p.status === 'graded' ? '已批改' : '未提交');

export function PapersView({
  onOpenPaper,
  onGoBank,
}: {
  onOpenPaper: (id: string) => void;
  onGoBank: () => void;
}) {
  const plans = useStudyStore((s) => s.list);
  const [planId, setPlanId] = useState('');
  const [rows, setRows] = useState<PaperRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [count, setCount] = useState(10);
  const [busy, setBusy] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<PaperRow | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      setRows(await studyIpc.studyPaperList(planId || null));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [planId]);

  useEffect(() => {
    void load();
  }, [load]);

  const autoCreate = async () => {
    if (busy) return;
    setBusy(true);
    setErr(null);
    try {
      const d = await studyIpc.studyPaperAuto({
        plan_id: planId || null,
        node_ids: [],
        count,
        title: null,
        kind: 'exam',
      });
      onOpenPaper(d.paper.id);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const doDelete = async () => {
    if (!pendingDelete || busy) return;
    setBusy(true);
    try {
      await studyIpc.studyPaperDelete(pendingDelete.id);
      setPendingDelete(null);
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">试卷</span>
        <span className="pg-hint">整卷测试为独立页面,写完后统一提交批改;主观题按参考答案逐条判定</span>
      </div>

      <div className="rp-hist-list">
        <div className="pg-sec st-flex">
          <select className="pg-field" value={planId} onChange={(e) => setPlanId(e.target.value)}>
            <option value="">全部学习计划</option>
            {plans.map((p) => (
              <option key={p.id} value={p.id}>
                {p.topic}
              </option>
            ))}
          </select>
          <span className="pg-hint">题目数量</span>
          <input
            className="pg-field num"
            type="number"
            min={1}
            max={50}
            value={count}
            onChange={(e) => setCount(Math.max(1, Math.min(50, Number(e.target.value) || 1)))}
          />
          <button className="btn-primary" disabled={busy} onClick={() => void autoCreate()}>
            {busy ? '组卷中…' : '从题库随机组卷'}
          </button>
          <button className="btn-ghost" onClick={onGoBank}>
            去试题库自选题
          </button>
        </div>
        <div className="pg-hint">随机组卷只从已有题目中抽取;不足时会明确提示,不会重复凑题。</div>

        {err && <div className="pg-warn">{err}</div>}

        {loading && <div className="pg-hint">加载试卷…</div>}
        {!loading && rows.length === 0 && (
          <div className="rp-empty">还没有试卷:可从题库随机组卷,或在试题库勾选题目组卷</div>
        )}

        {rows.map((p) => (
          <div key={p.id} className="rp-hist-item" onClick={() => onOpenPaper(p.id)}>
            <span className={`stat-dot ${p.status === 'graded' ? 'ok' : 'info'}`} />
            <div className="rp-hist-main">
              <div className="rp-hist-topic ellipsis">{p.title}</div>
              <div className="rp-hist-meta">
                <span>{p.kind === 'quiz' ? '小测' : '整卷'}</span>
                <span>· {p.total} 道</span>
                <span>· 已答 {p.answered} 道</span>
                <span>· {statusText(p)}</span>
                {p.score != null && <span>· 得分 {Math.round(p.score * 100)}%</span>}
                <span>· {fmtDate(p.created)}</span>
              </div>
            </div>
            <button
              className="icon-btn"
              title="删除试卷"
              onClick={(e) => {
                e.stopPropagation();
                setPendingDelete(p);
              }}
            >
              <IconTrash size={14} />
            </button>
          </div>
        ))}
      </div>

      {pendingDelete && (
        <ConfirmDialog
          title="删除试卷"
          message={`确定删除试卷「${pendingDelete.title}」?题目仍保留在试题库中。`}
          confirmText="删除"
          danger
          busy={busy}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => void doDelete()}
        />
      )}
    </div>
  );
}
