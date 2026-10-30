import type { StreamChunk } from '../../lib/ipc';

/** 流式 chunk 写入所依赖的消息 store 动作(由调用方注入,保持引用稳定)。 */
export interface StreamHandlers {
  pushStreamDelta: (sessionId: string, delta: string) => void;
  pushStreamReasoning: (sessionId: string, delta: string) => void;
  setStreamReasoningTokens: (sessionId: string, tokens: number | null) => void;
}

/** 创建本次流式的 chunk 处理器与 token 计数(正文/思考分流、工具结果围栏块、done 计数)。
 *  getTaskId:可选,返回当前进行中任务 id,打进 maf-tool 块的 tid 字段(val.md §5.4)。 */
export function createStreamChunks(
  sessionId: string,
  handlers: StreamHandlers,
  getTaskId?: () => string | null,
) {
  const { pushStreamDelta, pushStreamReasoning, setStreamReasoningTokens } = handlers;
  let tokens = 0;
  const onChunk = (chunk: StreamChunk) => {
    if (chunk.type === 'delta') pushStreamDelta(sessionId, chunk.content);
    // 思考流分流:独立缓冲去抖,折叠块展示,不进正文
    else if (chunk.type === 'reasoning') pushStreamReasoning(sessionId, chunk.content);
    // Phase 5e:工具执行结果 —— 以 maf-tool 围栏块并入正文(小字一行 + 折叠详情)
    // 性能:该正文会落库并随历史每轮重发,故单条结果截断到 1000 字符(原 4000)
    else if (chunk.type === 'tool_call_result') {
      const body = chunk.content.length > 1000
        ? `${chunk.content.slice(0, 1000)}\n…(已截断,完整结果见工具详情)`
        : chunk.content;
      // P6:a=实际执行参数摘要(截断 300),ms=耗时;tid=关联任务(供检查器工具流水/产物/子任务卡片归集)
      // 写文件类工具 content 巨大且 path 在 content 之后会被截断:只保留 path/rel,保证检查器能提取产物路径
      let args: string | null = chunk.arguments ?? null;
      if (args && /^fs_write|write_file|fs_export/.test(chunk.tool_name)) {
        try {
          const v = JSON.parse(chunk.arguments!) as Record<string, unknown>;
          const path = typeof v.path === 'string' ? v.path : typeof v.rel === 'string' ? v.rel : null;
          if (path) args = JSON.stringify({ path });
        } catch {
          /* 解析失败则走原截断 */
        }
      }
      if (args && args.length > 300) args = `${args.slice(0, 300)}…`;
      const payload = JSON.stringify({
        s: chunk.server_name, t: chunk.tool_name, ok: chunk.ok, c: body,
        a: args, ms: chunk.elapsed_ms ?? null, tid: getTaskId?.() ?? null,
      });
      pushStreamDelta(sessionId, `\n\n\`\`\`maf-tool\n${payload}\n\`\`\`\n\n`);
    }
    else if (chunk.type === 'done') {
      tokens = (chunk.prompt_tokens ?? 0) + (chunk.completion_tokens ?? 0);
      if (chunk.reasoning_tokens != null) {
        setStreamReasoningTokens(sessionId, chunk.reasoning_tokens);
      }
    }
  };
  return { onChunk, getTokens: () => tokens };
}
