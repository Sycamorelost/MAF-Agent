/** 去 AI 味 · 智能体流式调用(chat_stream 复用)。 */

import { useRef } from 'react';
import * as ipc from '../../lib/ipc';
import { chatStream } from '../../lib/ipc/provider';
import { useProvidersStore } from '../../stores/providers';
import { withBase } from '../../lib/systemPrompt';

/** 统一智能体流式入口:取人格 → withBase 组装 → chat_stream;返回取消句柄供「停止」。 */
export function useAiFlavorStream(bareMode: boolean, setMsg: (m: string) => void) {
  const runCancel = useRef<(() => void) | null>(null);

  /**
   * 所有模型调用都走这里:取智能体人格 → withBase 组装(与聊天页同构)→ chat_stream。
   * 页面不再自行拼系统提示,避免「同一智能体在聊天页与工作台行为不一致」。
   */
  const startAgentStream = async (
    agentId: string,
    userMsg: string,
    emit: (acc: string) => void,
    attachments?: string[],
  ): Promise<string | null> => {
    const providers = useProvidersStore.getState();
    if (!providers.current || !providers.hasKey[providers.current.providerId]) {
      setMsg('请先在设置中配置可用的对话模型');
      return null;
    }
    const agents = await ipc.listAgentTemplates().catch(() => []);
    const core = agents.find((a) => a.agent_id === agentId)?.system_prompt ?? '';
    const sys = withBase(core, { identity: !bareMode });
    setMsg('');
    let acc = '';
    const { done, cancel } = chatStream(
      {
        provider_id: providers.current.providerId,
        model: providers.current.model,
        messages: [
          { role: 'system', content: sys },
          { role: 'user', content: userMsg, ...(attachments?.length ? { images: attachments } : {}) },
        ],
        session_id: null,
      },
      (chunk) => {
        if (chunk.type === 'delta') {
          acc += chunk.content;
          emit(acc);
        }
      },
    );
    runCancel.current = cancel;
    try {
      await done;
      return acc;
    } catch (e) {
      setMsg(`调用失败: ${e}`);
      return null;
    } finally {
      runCancel.current = null;
    }
  };

  return { runCancel, startAgentStream };
}
