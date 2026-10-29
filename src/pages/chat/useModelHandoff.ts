import { useEffect, useRef, useState } from 'react';
import { useMemoryStore } from '../../stores/memory';
import { useProvidersStore } from '../../stores/providers';
import { useSessionsStore } from '../../stores/sessions';
import type { MemoryStrategy, SessionRow } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { estimateTokens } from '../../lib/memoryInject';

export interface ModelHandoffParams {
  sessionId: string | undefined;
  session: SessionRow | undefined;
  current: { providerId: string; model: string } | null;
  buildSystemWith: (memBody: string, strategy: MemoryStrategy) => string;
}

/** D3.1/D3.2:本会话模型切换与记忆交接(弹窗 / 横幅由页面渲染,状态与流转集中在此)。 */
export function useModelHandoff({
  sessionId,
  session,
  current,
  buildSystemWith,
}: ModelHandoffParams) {
  const selectModel = useProvidersStore((s) => s.select);
  const setSessionModel = useSessionsStore((s) => s.setModel);
  const setMemoryStrategy = useSessionsStore((s) => s.setMemoryStrategy);
  const loadMemory = useMemoryStore((s) => s.load);
  /** 检测到模型切换时弹出的交接信息 */
  const [handoff, setHandoff] = useState<{ from: string; to: string } | null>(null);
  /** 切换完成后的横幅(D3.2) */
  const [switchBanner, setSwitchBanner] = useState<{
    from: string;
    to: string;
    tokens: number;
    injected: string;
  } | null>(null);
  const [showInjected, setShowInjected] = useState(false);
  const prevModelRef = useRef<string | null>(null);

  /** 解析会话记录的模型(provider/model;兼容旧的裸模型名)→ 首个可用目标(P1)。 */
  const resolveSessionModel = (modelId: string | null) => {
    if (!modelId) return null;
    const { templates, hasKey } = useProvidersStore.getState();
    const ready = templates.filter((t) => hasKey[t.id] && t.models.length > 0);
    const i = modelId.indexOf('/');
    if (i > 0) {
      const pid = modelId.slice(0, i);
      const m = modelId.slice(i + 1);
      const t = ready.find((x) => x.id === pid);
      return t && t.models.includes(m) ? { providerId: pid, model: m } : null;
    }
    const t = ready.find((x) => x.models.includes(modelId));
    return t ? { providerId: t.id, model: modelId } : null;
  };

  // 会话切换:重置交接弹窗与横幅
  useEffect(() => {
    setSwitchBanner(null);
    setHandoff(null);
  }, [sessionId]);

  // P1:进入会话时用会话记录的模型覆盖全局选择(每个对话可自定义模型),并同步交接基线
  useEffect(() => {
    const target = resolveSessionModel(session?.model_id ?? null);
    if (target) {
      prevModelRef.current = `${target.providerId}/${target.model}`;
      if (current?.providerId !== target.providerId || current?.model !== target.model) {
        selectModel(target.providerId, target.model);
      }
    } else {
      prevModelRef.current = current ? `${current.providerId}/${current.model}` : null;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, session?.model_id]);

  /** 用户在弹窗中选定模型:显式触发切换与记忆交接(P1 显式化)。 */
  const onPickModel = (providerId: string, model: string) => {
    const from =
      prevModelRef.current ?? (current ? `${current.providerId}/${current.model}` : null);
    const to = `${providerId}/${model}`;
    selectModel(providerId, model);
    prevModelRef.current = to;
    if (from && from !== to && sessionId) setHandoff({ from, to });
  };

  /** 取消交接:把模型回滚到切换前,避免静默切换(P4)。 */
  const onCancelHandoff = () => {
    if (handoff) {
      const i = handoff.from.indexOf('/');
      if (i > 0) {
        prevModelRef.current = handoff.from;
        selectModel(handoff.from.slice(0, i), handoff.from.slice(i + 1));
      }
    }
    setHandoff(null);
  };

  /** 交接确认:快照 → 落盘策略 → 重读记忆 → 横幅展示注入统计(D3.1+D3.2)。 */
  const onHandoffConfirm = async (strategy: MemoryStrategy) => {
    if (!handoff || !sessionId || !current) return;
    const h = handoff;
    setHandoff(null);
    try {
      await ipc.snapshotMemory(sessionId, 'model_switch');
    } catch {
      // 快照失败不阻塞切换
    }
    await setMemoryStrategy(sessionId, strategy);
    await setSessionModel(sessionId, h.to);
    await loadMemory(sessionId);
    // 用新策略计算实际注入内容与 token 统计
    const mem = useMemoryStore.getState().bySession[sessionId];
    const injected = buildSystemWith(mem?.body ?? '', strategy);
    setSwitchBanner({ from: h.from, to: h.to, tokens: estimateTokens(injected), injected });
  };

  // D8:交接弹窗预选策略(目标模型的单模型配置 > 会话当前策略)
  const modelConfig = useProvidersStore((s) => s.modelConfig);
  const handoffDefaultStrategy: MemoryStrategy = (() => {
    const fallback = session?.memory_injection_strategy ?? 'hybrid';
    if (!handoff) return fallback;
    const i = handoff.to.indexOf('/');
    if (i < 0) return fallback;
    const pid = handoff.to.slice(0, i);
    const m = handoff.to.slice(i + 1);
    return modelConfig[pid]?.[m]?.memory_injection ?? fallback;
  })();

  return {
    handoff,
    switchBanner,
    setSwitchBanner,
    showInjected,
    setShowInjected,
    handoffDefaultStrategy,
    onPickModel,
    onCancelHandoff,
    onHandoffConfirm,
  };
}
