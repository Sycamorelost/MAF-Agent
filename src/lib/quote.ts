import { stripMarkdown } from './stripMarkdown';

/** 会话内「引用」条目:完整原文留在 state,输入框只显示摘要。 */
export interface QuoteRef {
  id: string;
  role: string;
  createdAt?: string;
  /** 完整引用原文(发送时作为独立上下文注入) */
  text: string;
  /** 单行摘要(剥离 Markdown 后 ≤120 字) */
  excerpt: string;
}

const EXCERPT_MAX = 120;

/** 构造引用条目;选区优先由调用方决定传入的 text。 */
export function buildQuote(text: string, role: string, createdAt?: string): QuoteRef {
  const clean = stripMarkdown(text).replace(/\s+/g, ' ').trim();
  return {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    role,
    createdAt,
    text: text.trim(),
    excerpt: clean.length > EXCERPT_MAX ? `${clean.slice(0, EXCERPT_MAX)}…` : clean,
  };
}

/** 引用来源的人类可读标签(引用条与发送上下文共用)。 */
export function quoteSourceLabel(q: QuoteRef): string {
  const role = q.role === 'user' ? '用户' : '助手';
  const time = q.createdAt
    ? new Date(q.createdAt).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
    : '';
  return `${role}${time ? ` · ${time}` : ''} · ${q.text.length} 字`;
}
