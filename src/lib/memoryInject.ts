//! 记忆注入策略工具(迭代 D3.1 / D3.5)。
//!
//! token 估算规则与后端 src-tauri/src/memory/budget.rs 保持一致:字符数 / 2 上取整。

import type { MemoryStrategy } from './ipc';

/** 注入预算上限(tokens),与后端 MEMORY_BUDGET_TOKENS 一致。 */
export const MEMORY_BUDGET_TOKENS = 2000;

/** 估算文本 token 数:字符数 / 2 上取整。 */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 2);
}

/**
 * 按一级~三级 markdown 标题切分正文,提取指定章节(含标题行)。
 * wanted 为前缀匹配:如「待办」可命中「待办 / 未决问题」。
 */
export function extractSections(body: string, wanted: string[]): string {
  const lines = body.split('\n');
  const out: string[] = [];
  let taking = false;
  for (const line of lines) {
    const m = /^#{1,3}\s+(.*)$/.exec(line.trim());
    if (m) {
      const title = m[1].trim();
      taking = wanted.some((w) => title === w || title.startsWith(w));
    }
    if (taking) out.push(line);
  }
  return out.join('\n').trim();
}

/**
 * 按策略计算实际注入的记忆正文:
 *   full    — 全文
 *   summary — 仅「会话摘要」节
 *   hybrid  — 「会话摘要」+「关键事实与结论」+「待办」+「文档摘要」
 *   none    — 不注入
 * 摘要类策略提取不到任何章节时回退全文,避免模型完全丢失上下文。
 */
export function injectByStrategy(body: string, strategy: MemoryStrategy): string {
  if (!body.trim() || strategy === 'none') return '';
  if (strategy === 'full') return body;
  const wanted =
    strategy === 'summary'
      ? ['会话摘要']
      : ['会话摘要', '关键事实与结论', '待办', '文档摘要'];
  const picked = extractSections(body, wanted);
  return picked || body;
}

/** 注入前预算护栏:超限则硬截断并追加标记(自动压缩失败时的兜底)。 */
export function enforceBudget(injected: string): string {
  if (estimateTokens(injected) <= MEMORY_BUDGET_TOKENS) return injected;
  return `${injected.slice(0, MEMORY_BUDGET_TOKENS * 2)}\n\n…(记忆过长,已截断)`;
}
