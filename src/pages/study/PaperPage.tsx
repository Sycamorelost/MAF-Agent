/**
 * 整卷测试独立页面(/plan/paper/:id)。
 *
 * - 作答阶段不显示对错,写完点「交卷并批改」后统一批改;
 * - 客观题本地确定性判分,主观题由模型按参考答案逐条判定并给出依据;
 * - 已经批改过的试卷显示逐题结果与总分。
 */
import { useCallback, useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { MemoMarkdown } from '../../components/MemoMarkdown';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { IconArrowLeft } from '../../components/icons';
import { resolveProvider } from '../../stores/study';
import type { PaperDetail, PaperItem } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';
import { isExact, isPick, TYPE_LABEL } from './qtype';

/** 题干渲染(公式/表格走 MemoMarkdown)。 */
function Stem({ text }: { text: string }) {
  return (
    <div className="md report-md">
      <MemoMarkdown content={text} />
    </div>
  );
}

function ItemResult({ item }: { item: PaperItem }) {
  return (
    <div className="pg-hint st-mt-2">
      <span className="pg-chip st-mr-2">
        得分 {Math.round((item.score ?? 0) * 100)}%
      </span>
      <span className="pg-chip">{item.correct ? '答对' : '未答对'}</span>
      <div className="st-mt-2">你的作答:{item.user_answer?.trim() || '(未作答)'}</div>
      <div>参考答案:{item.answer}{item.explanation ? ` · ${item.explanation}` : ''}</div>
      {item.feedback && <div>批改反馈:{item.feedback}</div>}
    </div>
  );
}

export function PaperPage() {
  const { id = '' } = useParams<{ id: string }>();
  const nav = useNavigate();
  const [detail, setDetail] = useState<PaperDetail | null>(null);
  const [ans, setAns] = useState<Record<number, string>>({});
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [confirmSubmit, setConfirmSubmit] = useState(false);

  const load = useCallback(async () => {
    if (!id) return;
    setLoading(true);
    setErr(null);
    try {
      const d = await studyIpc.studyPaperGet(id);
      setDetail(d);
      setAns(
        Object.fromEntries(d.items.map((it) => [it.idx, it.user_answer ?? ''])) as Record<
          number,
          string
        >,
      );
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  const items = detail?.items ?? [];
  const graded = detail?.paper.status === 'graded';
  const unanswered = items.filter((it) => !(ans[it.idx] ?? '').trim()).length;

  const submit = async () => {
    if (!detail || submitting) return;
    setConfirmSubmit(false);
    setSubmitting(true);
    setErr(null);
    try {
      const prov = resolveProvider();
      await studyIpc.studyPaperSubmit({
        paper_id: detail.paper.id,
        answers: items.map((it) => ({ idx: it.idx, user_answer: ans[it.idx] ?? '' })),
        ...prov,
      });
      // 重新拉取包含逐题得分与反馈的终态
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="pg">
      <main className="pg-main">
        <div className="rp-run">
          <div className="rp-head">
            <button className="btn-ghost" onClick={() => nav('/plan?tab=papers')}>
              <IconArrowLeft size={14} />
              返回学习中心
            </button>
            <span className="topic">{detail?.paper.title ?? '整卷测试'}</span>
            <span className={`mode-tag ${graded ? 'staged' : 'legacy'}`}>
              {graded ? '已批改' : '作答中'}
            </span>
            {graded && detail?.paper.score != null && (
              <span className="mode-tag staged">得分 {Math.round(detail.paper.score * 100)}%</span>
            )}
          </div>

          <div className="rp-body">
            <div className="rp-main">
              {loading && <div className="pg-hint">加载试卷…</div>}
              {err && <div className="pg-warn">{err}</div>}
              {!loading && !detail && !err && <div className="rp-empty">试卷不存在</div>}

              {detail && (
                <>
                  <div className="pg-hint st-mb-3">
                    {graded
                      ? `共 ${detail.paper.total} 道,答对 ${items.filter((i) => i.correct).length} 道。`
                      : `共 ${detail.paper.total} 道。作答阶段不显示对错;写完后点底部「交卷并批改」统一批改。`}
                  </div>

                  {items.map((it) => (
                    <div key={it.idx} className="rp-tl st-mb-3">
                      <div className="tt">
                        <span>
                          {it.idx + 1}. {TYPE_LABEL[it.qtype] ?? '简答'}
                        </span>
                        {graded && it.correct != null && (
                          <span className={`pill${it.correct ? '' : ' bad'}`}>
                            {it.correct ? '对' : '错'}
                          </span>
                        )}
                        <span className="sp" />
                        {it.node_name && <span className="pg-hint">{it.node_name}</span>}
                      </div>
                      <Stem text={it.stem} />

                      {!graded && it.qtype === 'choice' && (
                        <div className="pg-chips st-mt-2">
                          {it.options.map((op) => {
                            const letter = op.trim().charAt(0).toUpperCase();
                            return (
                              <button
                                key={op}
                                className={`pg-chip${ans[it.idx] === letter ? ' on' : ''}`}
                                onClick={() => setAns((m) => ({ ...m, [it.idx]: letter }))}
                              >
                                {op}
                              </button>
                            );
                          })}
                        </div>
                      )}
                      {!graded && it.qtype === 'judge' && (
                        <div className="pg-chips st-mt-2">
                          {['对', '错'].map((v) => (
                            <button
                              key={v}
                              className={`pg-chip${ans[it.idx] === v ? ' on' : ''}`}
                              onClick={() => setAns((m) => ({ ...m, [it.idx]: v }))}
                            >
                              {v}
                            </button>
                          ))}
                        </div>
                      )}
                      {!graded && isExact(it.qtype) && (
                        <input
                          className="pg-field st-mt-2"
                          placeholder={it.qtype === 'calc' ? '写下计算结果…' : '写下所填内容…'}
                          value={ans[it.idx] ?? ''}
                          onChange={(e) => setAns((m) => ({ ...m, [it.idx]: e.target.value }))}
                        />
                      )}
                      {!graded && !isPick(it.qtype) && !isExact(it.qtype) && (
                        <textarea
                          className="pg-field st-area st-area-xl st-mt-2"
                          placeholder="写下你的答案,交卷后统一批改…"
                          value={ans[it.idx] ?? ''}
                          onChange={(e) => setAns((m) => ({ ...m, [it.idx]: e.target.value }))}
                        />
                      )}
                      {graded && <ItemResult item={it} />}
                    </div>
                  ))}

                  {!graded && (
                    <div className="pg-foot st-mt-2">
                      <span className="pg-hint">
                        未作答 {unanswered} 道
                        {unanswered > 0 ? ';交卷后可补作再重新批改' : ''}
                      </span>
                      <span className="sp" />
                      <button
                        className="btn-primary"
                        disabled={submitting}
                        onClick={() => {
                          if (unanswered > 0) setConfirmSubmit(true);
                          else void submit();
                        }}
                      >
                        {submitting ? '批改中…(主观题逐题判定)' : '交卷并批改'}
                      </button>
                    </div>
                  )}
                  {graded && (
                    <div className="pg-foot">
                      <span className="pg-hint">试卷已批改;题目仍保留在试题库中。</span>
                      <span className="sp" />
                      <button className="btn-ghost" onClick={() => nav('/plan?tab=papers')}>
                        返回学习中心
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </div>
      </main>

      {confirmSubmit && (
        <ConfirmDialog
          title="交卷确认"
          message={`还有 ${unanswered} 道题未作答,未作答题目按 0 分计算。确定现在交卷并批改?`}
          confirmText="交卷批改"
          busy={submitting}
          onCancel={() => setConfirmSubmit(false)}
          onConfirm={() => void submit()}
        />
      )}
    </div>
  );
}
