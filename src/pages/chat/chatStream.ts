import type { StreamChunk } from '../../lib/ipc';

/** 流式 chunk 写入所依赖的消息 store 动作(由调用方注入,保持引用稳定)。 */
export interface StreamHandlers {
  pushStreamDelta: (sessionId: string, delta: string) => void;
  pushStreamReasoning: (sessionId: string, delta: string) => void;
  setStreamReasoningTokens: (sessionId: string, tokens: number | null) => void;
}

/** 创建本次流式的 chunk 处理器与 token 计数(正文/思考分流、工具结果围栏块、done 计数)。 */
export function createStreamChunks(sessionId: string, handlers: StreamHandlers) {
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
      const payload = JSON.stringify({
        s: chunk.server_name, t: chunk.tool_name, ok: chunk.ok, c: body,
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
