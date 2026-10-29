/**
 * 去 AI 味 · AI 生成概率分析(R3 B2/B3)。
 * 与 src-tauri/src/aiflavor 引擎及 commands/aiflavor.rs 对齐;
 * 本地统计零模型调用,改写走现有 chat_stream。
 */

import { invoke } from '@tauri-apps/api/core';

// ============ 类型(与 Rust FlavorReport 逐字段对齐) ============

/** 六维单维评分。 */
export interface AiflavorDim {
  /** cliche / burstiness / lexical / connective / concrete / template */
  key: string;
  /** 中文名:套话密度 / 句长均匀度 / 词汇多样性 / 连接词密度 / 具体性 / 结构模板度 */
  name: string;
  /** 人类可读原始值,如 "4.2/千字" */
  raw: string;
  /** 0-100(0=人味,100=AI 味) */
  score: number;
  /** 0 正常 / 1 轻度 / 2 中度 / 3 高度 */
  level: number;
  /** 命中例句(截断) */
  example: string;
}

/** 句子级标注(热力图)。 */
export interface AiflavorSentenceMark {
  index: number;
  text: string;
  /** 0-100 句级 AI 味 */
  score: number;
  /** 0..3 四级热力 */
  heat: number;
  /** 最多 3 条,如「套话「综上所述」」 */
  reasons: string[];
}

/** 规则命中(R01-R24)。 */
export interface AiflavorRuleHit {
  rule_id: number;
  name: string;
  category: string;
  /** 1-3 */
  severity: number;
  count: number;
  example: string;
}

/** 引擎原始统计值。 */
export interface AiflavorStats {
  char_count: number;
  sentence_count: number;
  paragraph_count: number;
  cliche_per_kilo: number;
  sentence_len_mean: number;
  sentence_len_std: number;
  lexical_diversity: number;
  connective_per_kilo: number;
  concrete_per_kilo: number;
  para_len_cv: number;
  academic_style: boolean;
}

/** 完整检测报告。 */
export interface AiflavorReport {
  id: number | null;
  created: string | null;
  score: number;
  /** 低 / 中 / 偏高 / 高 / 极高 */
  level: string;
  /** 0-4 */
  level_code: number;
  char_count: number;
  /** ≤100 字 → 仅供参考,level 封顶「中」 */
  degraded: boolean;
  dims: AiflavorDim[];
  sentence_marks: AiflavorSentenceMark[];
  rule_hits: AiflavorRuleHit[];
  stats: AiflavorStats;
  disclaimer: string;
  version: number;
}

/** 历史报告摘要(列表用)。 */
export interface AiflavorReportSummary {
  id: number;
  created: string;
  source_kind: string;
  char_count: number;
  score: number;
  level: string;
}

// ============ IPC 封装 ============

/** 运行本地检测并落库。 */
export const aiflavorAnalyze = (text: string, sourceKind?: string) =>
  invoke<AiflavorReport>('aiflavor_analyze', { text, sourceKind: sourceKind ?? null });

/** 历史报告列表(新→旧)。 */
export const aiflavorReportsList = (limit?: number) =>
  invoke<AiflavorReportSummary[]>('aiflavor_reports_list', { limit: limit ?? null });

/** 回看完整报告。 */
export const aiflavorReportGet = (id: number) =>
  invoke<AiflavorReport>('aiflavor_report_get', { id });

/** 删除单条历史报告。 */
export const aiflavorReportDelete = (id: number) =>
  invoke<void>('aiflavor_report_delete', { id });

/** 记录一次改写结果(用于后续对比/回退)。 */
export const aiflavorRewriteSave = (
  reportId: number,
  level: string,
  diffJson: string,
  accepted: boolean,
) => invoke<number>('aiflavor_rewrite_save', { reportId, level, diffJson, accepted });

/** 导出报告到工作区 exports/(md = Markdown,html = 可打印为 PDF 的自包含页面),返回绝对路径。 */
export const aiflavorExportReport = (id: number, format: 'md' | 'html' = 'md') =>
  invoke<string>('aiflavor_export_report', { id, format });

/** 五级判定配色(徽标)。 */
export const LEVEL_COLORS: Record<string, string> = {
  低: 'var(--ok)',
  中: 'var(--warn)',
  偏高: '#e67e22',
  高: 'var(--err)',
  极高: '#c0392b',
};

/** 热力图四级底色(0 人味正常 → 3 高度像 AI)。 */
export const HEAT_COLORS = [
  'transparent',
  'rgba(230, 126, 34, 0.18)',
  'rgba(231, 76, 60, 0.30)',
  'rgba(192, 57, 43, 0.50)',
];
