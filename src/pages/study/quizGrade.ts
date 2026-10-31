/**
 * 小测本地判分与错题归因(纯函数,无 React 依赖)。
 *
 * 本地判分仅作提交前的即时提示,正式判分以后端 grade() 为准;
 * 归一规则与后端对齐。
 */
import type { QuizResult } from '../../lib/ipc/study';
import { isExact, isPick, TYPE_LABEL, type QuizQ } from './qtype';

/** choice:取首个 ASCII 字母 */
const normLetter = (s: string): string => {
  const m = s.trim().match(/[A-Za-z]/);
  return m ? m[0].toUpperCase() : '';
};

/** judge:归一到 对/错 */
const normJudge = (s: string): string => {
  const t = s.trim().toLowerCase();
  if (['对', '正确', 'true', '√', '是', 't', 'yes'].includes(t) || t.startsWith('对') || t.startsWith('正确'))
    return '对';
  if (['错', '错误', 'false', '×', 'x', '否', 'f', 'no'].includes(t) || t.startsWith('错')) return '错';
  return '';
};

/** calc/cloze:去全部空白并转小写 */
const normExact = (s: string) => s.replace(/\s+/g, '').toLowerCase();

/** 本地即时判分(null = 未作答);仅作提交前提示,正式判分以后端为准 */
export function instantCorrect(
  q: { type: string; answer: string },
  picked: string | undefined,
  text: string | undefined,
): boolean | null {
  if (isPick(q.type)) {
    if (!picked) return null;
    if (q.type === 'choice') {
      const a = normLetter(q.answer);
      return a ? normLetter(picked) === a : picked.trim() === q.answer.trim();
    }
    const a = normJudge(q.answer);
    const u = normJudge(picked);
    return a && u ? a === u : picked.trim() === q.answer.trim();
  }
  if (isExact(q.type)) {
    const u = (text ?? '').trim();
    if (!u) return null;
    const a = normExact(q.answer);
    return a !== '' && a === normExact(u);
  }
  return null;
}

/** 答错题列表(结果页归因与变式再练共用) */
export const wrongOf = (r: QuizResult, all: QuizQ[]) =>
  all.filter((q) => r.results.find((x) => x.question_idx === q.idx)?.correct === false);

/** 答错题按题型分组(保持题型出现顺序) */
export const wrongGroups = (r: QuizResult, all: QuizQ[]) => {
  const m = new Map<string, QuizQ[]>();
  for (const q of wrongOf(r, all)) {
    const k = TYPE_LABEL[q.type] ?? '简答';
    m.set(k, [...(m.get(k) ?? []), q]);
  }
  return m;
};
