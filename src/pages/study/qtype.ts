/**
 * 题型元数据与作答类别判定(学习中心各视图共用,单一事实源)。
 */
import type { StudyQuiz } from '../../lib/ipc/study';

/** 题型展示名 */
export const TYPE_LABEL: Record<string, string> = {
  choice: '单选',
  judge: '判断',
  short: '简答',
  calc: '计算',
  proof: '证明',
  cloze: '填空',
  translate: '翻译',
  reading: '阅读',
  recite: '默写',
};

/** 手动加题可选题型(与 TYPE_LABEL 键一致,保持固定顺序) */
export const QTYPE_OPTIONS = [
  'choice',
  'judge',
  'short',
  'calc',
  'proof',
  'cloze',
  'translate',
  'reading',
  'recite',
];

/** 点选作答(自动判分) */
export const isPick = (t: string) => t === 'choice' || t === 'judge';
/** 文本作答且确定性判分 */
export const isExact = (t: string) => t === 'calc' || t === 'cloze';
/** 文本作答且对照参考自评/模型批改(short 与学科主观题) */
export const isSelf = (t: string) => !isPick(t) && !isExact(t);

/** 测验题(由后端出题接口返回) */
export type QuizQ = StudyQuiz['questions'][number];
