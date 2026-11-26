import { useEffect, useRef } from 'react';
import { mcpClearSessionAllowed } from '../../lib/ipc/mcp';
import { useSessionsStore } from '../../stores/sessions';
import { isChatRunBusy } from '../../stores/chatRuns';
import type { AgentTemplate, ProviderModelConfig, SessionRow } from '../../lib/ipc';
import { agentThinking, type ThinkingLevel } from '../../stores/thinking';

interface ChatSessionEffectsParams {
  sessionId: string | undefined;
  session: SessionRow | undefined;
  current: { providerId: string; model: string } | null;
  agent: AgentTemplate | null;
  agentTemplates: AgentTemplate[];
  modelConfig: ProviderModelConfig;
  activeBranch: string;
  load: (sessionId: string, force?: boolean) => Promise<void>;
  loadMemory: (sessionId: string) => Promise<void>;
  loadDeliverables: (sessionId: string, force?: boolean) => Promise<void>;
  loadAgents: () => Promise<void>;
  loadModelConfig: () => Promise<void>;
  initThinking: (sessionId: string, defaultLevel?: ThinkingLevel, defaultOn?: boolean) => void;
}

/** 会话页数据加载:进入会话拉消息/记忆/交付物、分支切换强制重载、
 *  智能体模板与单模型配置加载、思考档初始化。 */
export function useChatSessionEffects({
  sessionId,
  session,
  current,
  agent,
  agentTemplates,
  modelConfig,
  activeBranch,
  load,
  loadMemory,
  loadDeliverables,
  loadAgents,
  loadModelConfig,
  initThinking,
}: ChatSessionEffectsParams) {
  // 首次进入某会话的标记:用于让分支 effect 跳过首帧,避免同一次进入重复拉取
  const enteredSession = useRef<string | undefined>(undefined);

  // 进入会话时加载消息和 memory
  useEffect(() => {
    if (!sessionId) return;
    load(sessionId);
    loadMemory(sessionId);
    // 产物只在进入会话时强制拉取一次;分支切换的刷新由下一个 effect 负责
    void loadDeliverables(sessionId, true);
    // 离开会话时:空对话默认不保留(后端按真实消息数判断,流式中/已有内容的会话不受影响)
    // 同时清空 MCP「始终允许」会话缓存,避免跨会话静默放行(Phase 5e 安全要求)
    return () => {
      // 生成仍在进行(含准备阶段)时不能清理:后端还在跑,会话删了会丢消息/幽灵行
      if (!isChatRunBusy(sessionId)) {
        void useSessionsStore.getState().removeIfEmpty(sessionId);
      }
      void mcpClearSessionAllowed().catch(() => {});
    };
  }, [sessionId, load, loadMemory, loadDeliverables]);

  // F4:激活分支变化(切换/分叉/搜索跳转)时强制重载消息与分支列表。
  // 首帧(刚进入会话)由上一个 effect 负责,这里跳过,避免同一次进入发两遍请求。
  useEffect(() => {
    if (!sessionId) return;
    if (enteredSession.current !== sessionId) {
      enteredSession.current = sessionId;
      return;
    }
    void load(sessionId, true);
    void loadDeliverables(sessionId, true);
  }, [sessionId, activeBranch, load, loadDeliverables]);

  // 进入会话页加载 Agent 模板(B2)
  useEffect(() => {
    loadAgents();
  }, [loadAgents]);

  // 加载单模型配置(D8):交接弹窗按其默认记忆策略预选
  useEffect(() => {
    loadModelConfig().catch(() => {});
  }, [loadModelConfig]);

  // 会话思考选择初始化:智能体偏好 > 单模型默认档(default_reasoning)> 全局默认 medium;
  // 同会话切模型不覆盖已选(store 内 init 幂等)
  useEffect(() => {
    if (!sessionId || !current) return;
    // 等智能体模板加载完再初始化,避免模型默认档抢先占位
    if (session?.agent_id && agentTemplates.length === 0) return;
    const def = modelConfig[current.providerId]?.[current.model]?.default_reasoning;
    const lv = (['light', 'medium', 'high', 'ultra'] as const).find((x) => x === def);
    const pref = agentThinking(agent?.thinking_level);
    initThinking(sessionId, pref?.level ?? lv, pref?.on);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, current?.providerId, current?.model, modelConfig, agent?.thinking_level, agentTemplates.length, session?.agent_id]);
}
