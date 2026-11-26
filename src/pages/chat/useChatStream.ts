import { useCallback } from 'react';
import type { AgentTemplate, ReasoningCapability, SourceItem } from '../../lib/ipc';
import { useMessagesStore } from '../../stores/messages';
import { useChatRunsStore } from '../../stores/chatRuns';
import type { ThinkingSelection } from '../../stores/thinking';
import { runChatStream } from './chatRuntime';

/** 一次流式发送的入参:历史 + 可选来源 + 可选仅本次生效的模型覆盖。 */
export type RunChatStream = (
  history: { id?: number; role: string; content: string; images?: string[] }[],
  sources?: SourceItem[] | null,
  modelOverride?: { providerId: string; model: string },
) => Promise<void>;

interface ChatStreamParams {
  sessionId: string | undefined;
  current: { providerId: string; model: string } | null;
  agent: AgentTemplate | null;
  thinkCap: ReasoningCapability | null;
  thinkingSel: ThinkingSelection | undefined;
}

/** 薄适配层:模型兜底后交给模块级 runtime,生成不再绑定组件生命周期。 */
export function useChatStream({
  sessionId, current, agent, thinkCap, thinkingSel,
}: ChatStreamParams): RunChatStream {
  return useCallback(
    async (history, sources, modelOverride) => {
      if (!sessionId) return;
      const eff = modelOverride ?? current;
      if (!eff) {
        useMessagesStore.getState().endStreaming(sessionId);
        useChatRunsStore.getState().finish(sessionId);
        useChatRunsStore.getState().setError(sessionId, '请先在右上角配置 API Key 并选择模型');
        return;
      }
      await runChatStream({
        sessionId, providerId: eff.providerId, model: eff.model,
        history, sources, agent, thinkCap, thinkingSel,
      });
    },
    [sessionId, current, agent, thinkCap, thinkingSel],
  );
}
