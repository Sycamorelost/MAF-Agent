/**
 * 题库题目草稿(手动加题 / 编辑共用)与选项解析工具。
 */
import type { BankQuestion } from '../../lib/ipc/study';

export interface Draft {
  qtype: string;
  stem: string;
  options: string;
  answer: string;
  explanation: string;
  node_name: string;
  difficulty: number;
}

export const EMPTY_DRAFT: Draft = {
  qtype: 'short',
  stem: '',
  options: '',
  answer: '',
  explanation: '',
  node_name: '',
  difficulty: 2,
};

/** 编辑时由题目初始化草稿(options 数组转回多行文本) */
export const toDraft = (q: BankQuestion): Draft => ({
  qtype: q.qtype,
  stem: q.stem,
  options: q.options.join('\n'),
  answer: q.answer,
  explanation: q.explanation,
  node_name: q.node_name,
  difficulty: q.difficulty,
});

/** 选项文本(每行一个或逗号分隔)→ 数组 */
export function parseOptions(text: string): string[] {
  return text
    .split(/\n|,/)
    .map((s) => s.trim())
    .filter(Boolean);
}

/** 选择题选项补足字母前缀,避免用户输入「甲/乙」时正文缺 A/B。 */
export function normalizeOptions(list: string[]): string[] {
  return list.map((op, i) => {
    const letter = String.fromCharCode(65 + i);
    return /^[A-Za-z][.、]/.test(op) ? op : `${letter}. ${op}`;
  });
}
