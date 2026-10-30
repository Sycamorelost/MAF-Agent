//! 底部状态栏(ui-design b2-b11):模型 / 本会话 Token 用量 / 上下文占比 / 工具调用数 / 停止。
//! 仅做只读聚合展示;停止经 chat-cancel-request 窗口事件送达 useChatSend。

import { useEffect, useState } from 'react';
import { IconStop } from './icons';
import { requestChatCancel } from '../lib/chatCancel';
import { memoryBudgetStatus } from '../lib/ipc/memory';
import { collectSessionTraces } from '../lib/toolTrace';
import { useMessagesStore } from '../stores/messages';
import { useProvidersStore } from '../stores/providers';

interface Props {
  sessionId?: string | null;
  /** 非会话页(如工作台首页)可展示默认模式名 */
  modeLabel?: string;
  /** 非会话页可展示全局预设名 */
  presetName?: string;
}

export function StatusBar({ sessionId, modeLabel, presetName }: Props) {
  const current = useProvidersStore((s) => s.current);
  const messages = useMessagesStore((s) => (sessionId ? s.bySession[sessionId] : undefined));
  const streaming = useMessagesStore((s) =>
    sessionId ? s.streaming[sessionId] !== undefined : false,
  );
  const [budget, setBudget] = useState<{ tokens: number; limit: number } | null>(null);

  useEffect(() => {
    if (!sessionId) {
      setBudget(null);
      return;
    }
    memoryBudgetStatus(sessionId)
      .then((b) => setBudget({ tokens: b.tokens, limit: b.limit }))
      .catch(() => {});
  }, [sessionId, streaming, messages?.length]);

  const list = messages ?? [];
  const tokens = list.reduce((n, m) => n + (m.tokens || 0), 0);
  const toolCount = collectSessionTraces(list).length;
  const pct =
    budget && budget.limit > 0
      ? Math.min(100, Math.round((budget.tokens / budget.limit) * 100))
      : null;

  return (
    <footer className="wb-statusbar">
      {modeLabel && <span className="wb-status-item">默认模式 {modeLabel}</span>}
      {presetName && <span className="wb-status-item">预设 {presetName}</span>}
      <span className="wb-status-item">模型 {current?.model ?? '未配置'}</span>
      {sessionId && <span className="wb-status-item">Token {tokens.toLocaleString()}</span>}
      {pct !== null && (
        <span
          className="wb-status-item"
          title={`上下文 ${budget!.tokens}/${budget!.limit} tokens`}
        >
          上下文 {pct}%
        </span>
      )}
      {sessionId && <span className="wb-status-item">工具 {toolCount} 次</span>}
      <span className="sp" />
      {streaming && sessionId && (
        <button
          type="button"
          className="btn-ghost btn-xs"
          title="停止本轮生成"
          onClick={() => requestChatCancel(sessionId)}
        >
          <IconStop size={11} />
          停止
        </button>
      )}
    </footer>
  );
}
