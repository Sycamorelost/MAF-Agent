import { useCallback } from 'react';
import type { AgentTemplate, ReasoningCapability, SourceItem } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import type { ThinkingSelection } from '../../stores/thinking';
import { useMessagesStore } from '../../stores/messages';
import { maybeAutoSummarize } from './memoryContext';
import { collectSessionTools, resolveReasoningEffort } from './chatTools';
import { createStreamChunks } from './chatStream';

/** 一次流式发送的入参:历史 + 可选来源 + 可选仅本次生效的模型覆盖。 */
export type RunChatStream = (
  history: { role: string; content: string; images?: string[] }[],
  sources?: SourceItem[] | null,
  modelOverride?: { providerId: string; model: string },
) => Promise<void>;

interface ChatStreamParams {
  sessionId: string | undefined;
  current: { providerId: string; model: string } | null;
  agent: AgentTemplate | null;
  thinkCap: ReasoningCapability | null;
  thinkingSel: ThinkingSelection | undefined;
  setSending: (v: boolean) => void;
  setError: (msg: string | null) => void;
  cancelRef: { current: (() => void) | null };
}

/** 流式发送子流程:组装请求、写流式缓冲、落库助手消息、触发记忆自动提炼。 */
export function useChatStream({
  sessionId, current, agent, thinkCap, thinkingSel, setSending, setError, cancelRef,
}: ChatStreamParams): RunChatStream {
  const {
    appendAssistant, startStreaming, pushStreamDelta,
    pushStreamReasoning, setStreamReasoningTokens, endStreaming,
  } = useMessagesStore();
  /** F5:用指定模型(缺省当前模型)跑一次流式;modelOverride 只影响本次,不改全局/会话模型,
   *  新消息 model_id 记录为 override 模型。 */
  const runStream = useCallback(
    async (
      history: { role: string; content: string; images?: string[] }[],
      sources?: ipc.SourceItem[] | null,
      modelOverride?: { providerId: string; model: string },
    ) => {
      if (!sessionId) return;
      const eff = modelOverride ?? current;
      if (!eff) {
        // 调用方(onSend)可能已提前点亮占位:这里收掉,避免永久光标
        endStreaming(sessionId);
        setSending(false);
        setError('请先在右上角配置 API Key 并选择模型');
        return;
      }
      setSending(true);
      setError(null);
      try {
        startStreaming(sessionId);
        // 思考参数:模型不支持 → null(后端能力门控二次兜底);用户关闭 → 'off';否则档位
        const reasoningEffort = resolveReasoningEffort(thinkCap, thinkingSel);
        // Phase 5e:收集已启用 MCP server 的工具定义;空则不启用工具调用(行为不变)
        const tools = await collectSessionTools(agent);
        const chunks = createStreamChunks(sessionId, {
          pushStreamDelta, pushStreamReasoning, setStreamReasoningTokens,
        });
        const { done, cancel } = ipc.chatStream(
          {
            provider_id: eff.providerId, model: eff.model, messages: history,
            // 智能体采样温度偏好(v2):null = 跟随模型默认
            temperature: agent?.temperature ?? null, max_tokens: null, top_p: null,
            custom_base_url: null, custom_display_name: null,
            session_id: sessionId, reasoning_effort: reasoningEffort,
            tools,
          },
          chunks.onChunk,
        );
        cancelRef.current = cancel;
        await done;
        const { content: fullText, reasoning } = endStreaming(sessionId);
        if (fullText) {
          await appendAssistant(
            sessionId, fullText, eff.model, chunks.getTokens(),
            sources && sources.length > 0 ? JSON.stringify(sources) : null, reasoning,
          );
        }
        maybeAutoSummarize(sessionId, eff.providerId, eff.model, agent?.memory_write ?? true);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        endStreaming(sessionId);
      } finally {
        setSending(false);
        cancelRef.current = null;
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      sessionId, current, agent, thinkCap, thinkingSel, startStreaming, pushStreamDelta,
      pushStreamReasoning, setStreamReasoningTokens, endStreaming, appendAssistant,
    ],
  );
  return runStream;
}
