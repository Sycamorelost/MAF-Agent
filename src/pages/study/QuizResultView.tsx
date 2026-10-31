/**
 * 小测结果视图:得分总览、错题归因(按题型分组)、逐题判定明细。
 */
import type { QuizResult } from '../../lib/ipc/study';
import { wrongGroups, wrongOf } from './quizGrade';
import type { QuizQ } from './qtype';

export function QuizResultView({
  result,
  all,
  answerText,
}: {
  result: QuizResult;
  all: QuizQ[];
  /** 取某题的用户作答展示文本 */
  answerText: (idx: number) => string;
}) {
  return (
    <>
      <div className="pg-chips st-mb-2">
        <span className="pg-chip on">得分 {Math.round(result.score * 100)}%</span>
        <span className="pg-chip">掌握度 {Math.round(result.mastery * 100)}%</span>
        {result.score >= 0.8 && <span className="pg-chip">复习已排到下一间隔</span>}
        {result.score < 0.8 && <span className="pg-chip">明天安排复习</span>}
        {result.mastery < 0.8 && <span className="pg-chip bad">掌握度未达标,建议复习后再进阶</span>}
      </div>
      {wrongOf(result, all).length > 0 && (
        <div className="pg-sec">
          <div className="pg-lab">错题归因({wrongOf(result, all).length} 道,按题型分类)</div>
          {[...wrongGroups(result, all).entries()].map(([label, qs]) => (
            <div key={label} className="pg-hint st-mt-1">
              <b>{label}</b>:
              {qs
                .map((q) => `第 ${q.idx + 1} 题「${q.stem.length > 40 ? `${q.stem.slice(0, 40)}…` : q.stem}」`)
                .join(';')}
            </div>
          ))}
          <div className="pg-hint st-mt-1">
            逐题解析见下方列表;可点底部「生成变式再练」围绕同一要点换题面重练。
          </div>
        </div>
      )}
      {all.map((q) => {
        const g = result.results.find((x) => x.question_idx === q.idx);
        return (
          <div key={q.idx} className="pg-sec">
            <div className="st-row-base">
              <span className={`pill${g?.correct ? '' : ' bad'}`}>{g?.correct ? '对' : '错'}</span>
              <b>
                {q.idx + 1}. {q.stem}
              </b>
            </div>
            <div className="pg-hint">
              参考答案:{g?.expected}
              {g?.explanation ? ` · ${g.explanation}` : ''}
            </div>
            <div className="pg-hint">你的作答:{answerText(q.idx)}</div>
            {g && (
              <div className="pg-hint">
                本题得分:{Math.round(g.score * 100)}%
                {g.feedback ? ` · ${g.feedback}` : ''}
              </div>
            )}
          </div>
        );
      })}
    </>
  );
}
