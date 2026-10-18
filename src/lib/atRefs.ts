/**
 * F9 @memory 召回:输入框中 [@note:标题] / [@memory:会话标题] 占位标记的注册与解析。
 * 选中候选时注册「占位标记 → 检索片段 + 出处」;发送前统一替换为带出处行的片段正文,
 * 注入 LLM 上下文;前端展示仍保留用户输入的原始占位标记。
 * 命中片段纳入既有滑窗/预算策略,不额外突破 max_context_messages。
 */

export interface AtRefEntry {
  /** 出处描述,如「知识库 / 笔记标题」「记忆 / 会话标题」 */
  source: string;
  /** 检索命中的片段正文 */
  snippet: string;
}

/** 占位标记注册表(模块级;键为完整占位字符串精确匹配,不受正则特殊字符影响)。 */
const registry = new Map<string, AtRefEntry>();

/** 注册一条占位标记 → 片段映射。防御性上限 200 条,先进先出。 */
export function registerAtRef(placeholder: string, source: string, snippet: string): void {
  if (registry.has(placeholder)) registry.delete(placeholder);
  registry.set(placeholder, { source, snippet });
  if (registry.size > 200) {
    const first = registry.keys().next().value;
    if (first !== undefined) registry.delete(first);
  }
}

/** 把文本中已注册的占位标记替换为「出处行 + 片段正文」;未注册的(如手输)保留原文。 */
export function resolveAtRefs(text: string): string {
  if (!text.includes('[@')) return text;
  let out = text;
  for (const [placeholder, entry] of registry) {
    if (!out.includes(placeholder)) continue;
    const block = `【引用片段 · 出处:${entry.source}】\n${entry.snippet}`;
    out = out.split(placeholder).join(block);
  }
  return out;
}
