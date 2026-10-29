import { invoke } from '@tauri-apps/api/core';

// ============ 文档驻留层(R4)============

/** 本地预检索:返回可直接拼进用户消息的命中片段块;无命中返回空串。
 *  用于长文档分析,避免依赖模型是否愿意调用 doc_search。 */
export const docPrefetch = (sessionId: string, query: string, doc?: string, limit?: number) =>
  invoke<string>('doc_prefetch', { sessionId, query, doc, limit });
