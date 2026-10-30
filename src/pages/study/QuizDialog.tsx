import { useEffect, useState } from 'react';
import { resolveProvider } from '../../stores/study';
import type { QuizResult, StudyQuiz } from '../../lib/ipc/study';
import { IconX } from '../../components/icons';
import * as studyIpc from '../../lib/ipc/study';

/* ---------------- 测验弹窗(P1) ---------------- */

export function QuizDialog({
  planId,
  nodeId,
  nodeName,
  onClose,
  onSubmitted,
}: {
  planId: string;
  nodeId: number;
  nodeName: string;
  onClose: () => void;
  onSubmitted: () => void;
}) {
  const [quiz, setQuiz] = useState<StudyQuiz | null>(null);
  const [busy, setBusy] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [picked, setPicked] = useState<Record<number, string>>({});
  const [shortAns, setShortAns] = useState<Record<number, string>>({});
  const [selfGrade, setSelfGrade] = useState<Record<number, boolean>>({});
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const [result, setResult] = useState<QuizResult | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    let alive = true;
    void (async () => {
      setBusy(true);
      setErr(null);
      try {
        const prov = resolveProvider();
        const q = await studyIpc.studyQuizNew({ id: planId, node_id: nodeId, ...prov });
        if (alive) {
          setQuiz(q);
          setBusy(false);
        }
      } catch (e) {
        if (alive) {
          setErr(e instanceof Error ? e.message : String(e));
          setBusy(false);
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, [planId, nodeId]);

  const all = quiz?.questions ?? [];
  const answered = (q: (typeof all)[number]) =>
    q.type === 'short' ? selfGrade[q.idx] !== undefined : !!picked[q.idx];

  const submit = async () => {
    if (!quiz || submitting) return;
    setSubmitting(true);
    setErr(null);
    try {
      const answers = all.map((q) => ({
        question_idx: q.idx,
        user_answer: q.type === 'short' ? (shortAns[q.idx] ?? '') : (picked[q.idx] ?? ''),
        self_correct: q.type === 'short' ? (selfGrade[q.idx] ?? null) : null,
      }));
      const r = await studyIpc.studyQuizSubmit(planId, quiz.id, answers);
      setResult(r);
      onSubmitted();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const answerText = (idx: number) => {
    const q = all.find((x) => x.idx === idx);
    if (!q) return '';
    if (q.type === 'short') {
      const g = selfGrade[idx];
      const a = (shortAns[idx] ?? '').trim() || '(未作答)';
      return `${a} · 自评:${g === true ? '记得' : g === false ? '没记住' : '未评'}`;
    }
    return picked[idx] ?? '(未作答)';
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(720px, 92vw)', maxHeight: '84vh', display: 'flex', flexDirection: 'column' }}
      >
        <div className="dialog-head">
          <h2>
            知识测验 · {nodeName}
          </h2>
          <span className="sp" />
          <button className="btn-ghost" onClick={onClose}>
            <IconX size={14} />
          </button>
        </div>
        <div className="dialog-body" style={{ overflowY: 'auto', padding: '12px 16px' }}>
          {busy && <div className="pg-hint">正在出题…</div>}
          {err && <div className="pg-warn">{err}</div>}
          {result ? (
            <>
              <div className="pg-chips" style={{ marginBottom: 8 }}>
                <span className="pg-chip on">得分 {Math.round(result.score * 100)}%</span>
                <span className="pg-chip">掌握度 {Math.round(result.mastery * 100)}%</span>
                {result.score >= 0.8 && <span className="pg-chip">复习已排到下一间隔</span>}
                {result.score < 0.8 && <span className="pg-chip">明天安排复习</span>}
              </div>
              {all.map((q) => {
                const g = result.results.find((x) => x.question_idx === q.idx);
                return (
                  <div key={q.idx} className="pg-sec">
                    <div style={{ display: 'flex', gap: 6, alignItems: 'baseline' }}>
                      <span
                        className="pill"
                        style={g?.correct ? undefined : { color: 'var(--warn)', borderColor: '#EBD9B8', background: '#FCF6EA' }}
                      >
                        {g?.correct ? '对' : '错'}
                      </span>
                      <b>
                        {q.idx + 1}. {q.stem}
                      </b>
                    </div>
                    <div className="pg-hint">
                      参考答案:{g?.expected}
                      {g?.explanation ? ` · ${g.explanation}` : ''}
                    </div>
                    <div className="pg-hint">你的作答:{answerText(q.idx)}</div>
                  </div>
                );
              })}
            </>
          ) : (
            quiz && (
              <>
                {all.map((q) => (
                  <div key={q.idx} className="pg-sec">
                    <div>
                      <b>
                        {q.idx + 1}. {q.stem}
                      </b>{' '}
                      <span className="pg-hint">
                        {q.type === 'choice' ? '单选' : q.type === 'judge' ? '判断' : '简答'}
                      </span>
                    </div>
                    {q.type === 'choice' && (
                      <div className="pg-chips" style={{ marginTop: 6 }}>
                        {q.options.map((op) => {
                          const letter = op.trim().charAt(0).toUpperCase();
                          return (
                            <button
                              key={op}
                              className={`pg-chip${picked[q.idx] === letter ? ' on' : ''}`}
                              onClick={() => setPicked((p) => ({ ...p, [q.idx]: letter }))}
                            >
                              {op}
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {q.type === 'judge' && (
                      <div className="pg-chips" style={{ marginTop: 6 }}>
                        <button
                          className={`pg-chip${picked[q.idx] === '对' ? ' on' : ''}`}
                          onClick={() => setPicked((p) => ({ ...p, [q.idx]: '对' }))}
                        >
                          对
                        </button>
                        <button
                          className={`pg-chip${picked[q.idx] === '错' ? ' on' : ''}`}
                          onClick={() => setPicked((p) => ({ ...p, [q.idx]: '错' }))}
                        >
                          错
                        </button>
                      </div>
                    )}
                    {q.type === 'short' && (
                      <div style={{ marginTop: 6 }}>
                        <textarea
                          className="pg-field"
                          style={{ width: '100%', minHeight: 56, resize: 'vertical' }}
                          placeholder="写下你的答案…"
                          value={shortAns[q.idx] ?? ''}
                          onChange={(e) => setShortAns((m) => ({ ...m, [q.idx]: e.target.value }))}
                        />
                        {!revealed[q.idx] ? (
                          <button
                            className="pg-chip"
                            style={{ marginTop: 6 }}
                            onClick={() => setRevealed((m) => ({ ...m, [q.idx]: true }))}
                          >
                            显示参考答案并自评
                          </button>
                        ) : (
                          <div style={{ marginTop: 6 }}>
                            <div className="pg-hint">参考:{q.answer}</div>
                            <div className="pg-chips">
                              <button
                                className={`pg-chip${selfGrade[q.idx] === true ? ' on' : ''}`}
                                onClick={() => setSelfGrade((m) => ({ ...m, [q.idx]: true }))}
                              >
                                记得
                              </button>
                              <button
                                className={`pg-chip${selfGrade[q.idx] === false ? ' on' : ''}`}
                                onClick={() => setSelfGrade((m) => ({ ...m, [q.idx]: false }))}
                              >
                                没记住
                              </button>
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                ))}
                <div className="pg-hint">
                  选择/判断自动判分;简答题对照参考答案自评。交卷后按「自评 ×0.4 + 测验 ×0.6」更新掌握度,并自动安排复习。
                </div>
              </>
            )
          )}
        </div>
        <div className="pg-foot" style={{ padding: '10px 16px' }}>
          <span className="pg-hint">{quiz && !result ? `已答 ${all.filter(answered).length}/${all.length}` : ''}</span>
          <span className="sp" style={{ flex: 1 }} />
          <button className="btn-ghost" onClick={onClose}>
            {result ? '关闭' : '取消'}
          </button>
          {!result && (
            <button className="btn-primary" disabled={busy || submitting || !quiz} onClick={() => void submit()}>
              {submitting ? '判分中…' : '交卷'}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
