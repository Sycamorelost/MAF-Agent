//! 运行时预设前端工具(P5):标签映射与预设差异计算(PresetDiffDialog 用)。
//! 只做纯函数;生效解析在 backend resolve_session_preset(单源事实)。

import type { Preset } from '../ipc/presets';

export const APPROVAL_LABEL: Record<string, string> = {
  ask: '逐次确认',
  workspace: '工作区写入',
  full: '完全信任',
};

export const MEMORY_STRATEGY_LABEL: Record<string, string> = {
  full: '全量注入',
  summary: '摘要注入',
  hybrid: '混合注入',
  none: '不注入',
};

export const PRESET_SOURCE_LABEL: Record<string, string> = {
  session: '本会话钉住',
  agent: 'Agent 模板默认',
  global: '全局默认',
};

/** 工具集摘要:白名单 N 个 / 全部工具 / 全部(排除 M 个)。 */
export function toolsSummary(p: Preset): string {
  const star = p.tools.include.includes('*');
  if (!star) return `${p.tools.include.length} 个白名单工具`;
  const namedExclude = p.tools.exclude.filter((x) => x !== '*');
  return namedExclude.length > 0 ? `全部工具(排除 ${namedExclude.length} 个)` : '全部工具';
}

/** 上下文策略摘要:条数 + 字符预算 + 工具结果裁剪阈值。 */
export function contextSummary(p: Preset): string {
  const { max_messages, max_chars, prune } = p.context;
  return `${max_messages} 条 / ${Math.round(max_chars / 1000)}k 字符,裁剪 ${Math.round(prune.threshold_chars / 1024)}k`;
}

/** 记忆注入摘要。 */
export function memorySummary(p: Preset): string {
  const s = MEMORY_STRATEGY_LABEL[p.memory.strategy] ?? p.memory.strategy;
  return `${s}(${p.memory.max_tokens} token)`;
}

/** 审批建议摘要(仅默认值;实际以会话权限档为准)。 */
export function approvalSummary(p: Preset): string {
  return APPROVAL_LABEL[p.approval] ?? p.approval;
}

/** 检索策略摘要:3-gram + 向量/重排开关 + top_k。 */
export function retrievalSummary(p: Preset): string {
  const parts = ['3-gram OR'];
  if (p.retrieval.vector) parts.push('向量');
  if (p.retrieval.rerank) parts.push('重排');
  return `${parts.join(' + ')},top${p.retrieval.top_k}`;
}

export interface PresetDiffRow {
  key: string;
  label: string;
  from: string;
  to: string;
  changed: boolean;
}

/** 两预设的五维差异(工具/上下文/记忆/审批/检索),供切换前确认。 */
export function diffPresets(from: Preset, to: Preset): PresetDiffRow[] {
  const rows: [string, string, string, string][] = [
    ['tools', '工具集', toolsSummary(from), toolsSummary(to)],
    ['context', '上下文', contextSummary(from), contextSummary(to)],
    ['memory', '记忆注入', memorySummary(from), memorySummary(to)],
    ['approval', '审批默认', approvalSummary(from), approvalSummary(to)],
    ['retrieval', '检索策略', retrievalSummary(from), retrievalSummary(to)],
  ];
  return rows.map(([key, label, f, t]) => ({ key, label, from: f, to: t, changed: f !== t }));
}
