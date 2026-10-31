/**
 * 测验弹窗(P1)容器:出题/作答/交卷/变式再练的状态编排。
 *
 * 渲染拆分:作答题卡 QuizQuestion、结果页 QuizResultView;
 * 判分与归因纯函数在 quizGrade.ts;题型元数据在 qtype.ts。
 */
import { useEffect, useState } from 'react';
import { resolveProvider } from '../../stores/study';
import type { GradeOutcome, QuizResult, StudyQuiz } from '../../lib/ipc/study';
import { IconX } from '../../components/icons';
import * as studyIpc from '../../lib/ipc/study';
import { isExact, isPick, isSelf, TYPE_LABEL } from './qtype';
import { wrongOf } from './quizGrade';
import { QuizQuestion } from './QuizQuestion';
import { QuizResultView } from './QuizResultView';

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
  /** 主观题批改结果:只能由「批改此题」按钮触发,避免输入中自动批改 */
  const [grades, setGrades] = useState<Record<number, GradeOutcome>>({});
  const [gradeBusy, setGradeBusy] = useState<Record<number, boolean>>({});
  const [result, setResult] = useState<QuizResult | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [variantBusy, setVariantBusy] = useState(false);

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
    isPick(q.type)
      ? !!picked[q.idx]
      : isExact(q.type)
        ? (shortAns[q.idx] ?? '').trim() !== ''
        : (shortAns[q.idx] ?? '').trim() !== '' && !!grades[q.idx];

  /** 主观题批改:用户点按钮才调用,输入框内容变化会作废旧结果 */
  const gradeOne = async (q: (typeof all)[number]) => {
    const text = (shortAns[q.idx] ?? '').trim();
    if (!text || gradeBusy[q.idx]) return;
    setGradeBusy((m) => ({ ...m, [q.idx]: true }));
    setErr(null);
    try {
      const prov = resolveProvider();
      const g = await studyIpc.studyQuestionGrade({
        qtype: q.type,
        stem: q.stem,
        answer: q.answer,
        explanation: q.explanation,
        user_answer: text,
        ...prov,
      });
      setGrades((m) => ({ ...m, [q.idx]: g }));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setGradeBusy((m) => ({ ...m, [q.idx]: false }));
    }
  };

  /** 主观题作答变化:写入的同时作废旧批改结果,避免旧分数对应新答案 */
  const changeText = (idx: number, v: string) => {
    setShortAns((m) => ({ ...m, [idx]: v }));
    setGrades((m) => {
      if (!m[idx]) return m;
      const next = { ...m };
      delete next[idx];
      return next;
    });
  };

  const readyToSubmit = all.length > 0 && all.every(answered);

  const submit = async () => {
    if (!quiz || submitting) return;
    setSubmitting(true);
    setErr(null);
    try {
      const answers = all.map((q) => {
        const text = shortAns[q.idx] ?? '';
        // 主观题只提交用户点按钮后得到的批改结果
        const g = grades[q.idx];
        return {
          question_idx: q.idx,
          user_answer: isPick(q.type) ? (picked[q.idx] ?? '') : text,
          self_correct: null,
          grade_correct: isSelf(q.type) ? (g?.correct ?? null) : null,
          grade_score: isSelf(q.type) ? (g?.score ?? null) : null,
          grade_feedback: isSelf(q.type) ? (g?.feedback ?? null) : null,
        };
      });
      const r = await studyIpc.studyQuizSubmit(planId, quiz.id, answers);
      setResult(r);
      onSubmitted();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  /** 生成变式再练(P3):带答错题上下文出新卷,替换当前卷当场重练 */
  const regenVariant = async () => {
    if (!result || variantBusy) return;
    setVariantBusy(true);
    setErr(null);
    try {
      const wc = wrongOf(result, all)
        .map(
          (q) =>
            `【${TYPE_LABEL[q.type] ?? '简答'}】${q.stem} → 正确答案:${q.answer}${q.explanation ? `(解析:${q.explanation})` : ''}`,
        )
        .join('\n');
      const prov = resolveProvider();
      const q2 = await studyIpc.studyQuizNew({ id: planId, node_id: nodeId, wrong_context: wc, ...prov });
      setQuiz(q2);
      setPicked({});
      setShortAns({});
      setGrades({});
      setGradeBusy({});
      setResult(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setVariantBusy(false);
    }
  };

  const answerText = (idx: number) => {
    const q = all.find((x) => x.idx === idx);
    if (!q) return '';
    if (isSelf(q.type)) return (shortAns[idx] ?? '').trim() || '(未作答)';
    if (isExact(q.type)) return (shortAns[idx] ?? '').trim() || '(未作答)';
    return picked[idx] ?? '(未作答)';
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog st-quiz-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head st-dlg-head">
          <h2>知识测验 · {nodeName}</h2>
          <span className="sp" />
          <button className="btn-ghost" onClick={onClose}>
            <IconX size={14} />
          </button>
        </div>
        <div className="dialog-body">
          {busy && <div className="pg-hint">正在出题…</div>}
          {err && <div className="pg-warn">{err}</div>}
          {result ? (
            <QuizResultView result={result} all={all} answerText={answerText} />
          ) : (
            quiz && (
              <>
                {all.map((q) => (
                  <QuizQuestion
                    key={q.idx}
                    q={q}
                    picked={picked[q.idx]}
                    text={shortAns[q.idx] ?? ''}
                    grade={grades[q.idx]}
                    gradeBusy={!!gradeBusy[q.idx]}
                    onPick={(v) => setPicked((p) => ({ ...p, [q.idx]: v }))}
                    onText={(v) => changeText(q.idx, v)}
                    onGrade={() => void gradeOne(q)}
                  />
                ))}
                <div className="pg-hint">
                  选择/判断/计算/填空即时判分;简答与学科主观题写完后点「批改此题」,由模型按参考答案逐条批改并给出依据。交卷后按「自评 ×0.4 + 测验 ×0.6」更新掌握度,并自动安排复习。
                </div>
              </>
            )
          )}
        </div>
        <div className="dialog-foot">
          <span className="pg-hint">
            {quiz && !result
              ? `已答 ${all.filter(answered).length}/${all.length}${readyToSubmit ? '' : ' · 主观题需先点「批改此题」'}`
              : ''}
          </span>
          <span className="sp" />
          {result && wrongOf(result, all).length > 0 && (
            <button className="btn-primary" disabled={variantBusy} onClick={() => void regenVariant()}>
              {variantBusy ? '出题中…' : '生成变式再练'}
            </button>
          )}
          <button className="btn-ghost" onClick={onClose}>
            {result ? '关闭' : '取消'}
          </button>
          {!result && (
            <button
              className="btn-primary"
              disabled={busy || submitting || !quiz || !readyToSubmit}
              onClick={() => void submit()}
            >
              {submitting ? '判分中…' : '交卷'}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
