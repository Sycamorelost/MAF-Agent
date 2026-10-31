/**
 * 小测作答题卡:按题型渲染 选择/判断 chips、精确题输入框、主观题文本域 + 批改按钮。
 */
import type { GradeOutcome } from '../../lib/ipc/study';
import { isExact, isPick, isSelf, TYPE_LABEL, type QuizQ } from './qtype';
import { instantCorrect } from './quizGrade';

/** 提交前即时反馈提示(本地判分,正式判分以后端为准) */
function InstantHint({
  ok,
  answer,
  explanation,
}: {
  ok: boolean | null;
  answer: string;
  explanation: string;
}) {
  if (ok === null) return null;
  return (
    <div className={`pg-hint st-mt-1${ok ? '' : ' bad'}`}>
      {ok ? '回答正确' : `回答错误 · 正确答案:${answer}`}
      {explanation ? ` · ${explanation}` : ''}
    </div>
  );
}

export function QuizQuestion({
  q,
  picked,
  text,
  grade,
  gradeBusy,
  onPick,
  onText,
  onGrade,
}: {
  q: QuizQ;
  /** 点选题当前选项字母/对错 */
  picked: string | undefined;
  /** 文本题当前作答 */
  text: string;
  /** 主观题批改结果(用户点「批改此题」后才存在) */
  grade: GradeOutcome | undefined;
  gradeBusy: boolean;
  onPick: (v: string) => void;
  /** 文本变化;主观题文本变化时父组件同时作废旧批改结果 */
  onText: (v: string) => void;
  onGrade: () => void;
}) {
  return (
    <div className="pg-sec">
      <div>
        <b>
          {q.idx + 1}. {q.stem}
        </b>{' '}
        <span className="pg-hint">{TYPE_LABEL[q.type] ?? '简答'}</span>
      </div>

      {q.type === 'choice' && (
        <div className="pg-chips st-mt-2">
          {q.options.map((op) => {
            const letter = op.trim().charAt(0).toUpperCase();
            return (
              <button
                key={op}
                className={`pg-chip${picked === letter ? ' on' : ''}`}
                onClick={() => onPick(letter)}
              >
                {op}
              </button>
            );
          })}
        </div>
      )}
      {q.type === 'judge' && (
        <div className="pg-chips st-mt-2">
          <button className={`pg-chip${picked === '对' ? ' on' : ''}`} onClick={() => onPick('对')}>
            对
          </button>
          <button className={`pg-chip${picked === '错' ? ' on' : ''}`} onClick={() => onPick('错')}>
            错
          </button>
        </div>
      )}
      {isPick(q.type) && picked && (
        <InstantHint ok={instantCorrect(q, picked, undefined)} answer={q.answer} explanation={q.explanation} />
      )}

      {isExact(q.type) && (
        <div className="st-mt-2">
          <input
            className="pg-field"
            placeholder={q.type === 'calc' ? '写下计算结果(自动判分)…' : '写下所填内容(自动判分)…'}
            value={text}
            onChange={(e) => onText(e.target.value)}
          />
          {text.trim() !== '' && (
            <InstantHint ok={instantCorrect(q, undefined, text)} answer={q.answer} explanation={q.explanation} />
          )}
        </div>
      )}

      {isSelf(q.type) && (
        <div className="st-mt-2">
          <textarea
            className="pg-field st-area"
            placeholder="写下你的答案,写完再点下方「批改此题」…"
            value={text}
            onChange={(e) => onText(e.target.value)}
          />
          <div className="st-row-wrap st-mt-2">
            <button className="pg-chip" disabled={gradeBusy || !text.trim()} onClick={onGrade}>
              {gradeBusy ? '批改中…' : grade ? '重新批改' : '批改此题'}
            </button>
            <span className="pg-hint">只在你点按钮时批改,不会边输入边判分</span>
          </div>
          {grade && (
            <div className="pg-sec st-mt-2">
              <div className="pg-chips">
                <span className={`pg-chip${grade.correct ? ' on' : ''}`}>
                  得分 {Math.round(grade.score * 100)}%
                </span>
                <span className="pg-chip">{grade.correct ? '判定:答对' : '判定:未答对'}</span>
              </div>
              <div className="pg-hint st-mt-1">{grade.feedback}</div>
              <div className="pg-hint st-mt-1">
                参考答案:{q.answer}
                {q.explanation ? ` · ${q.explanation}` : ''}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
