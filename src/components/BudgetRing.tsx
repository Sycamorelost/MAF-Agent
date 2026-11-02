//! 头部预算用量环(方案 4.4/5.2):两种模式下常显上下文预算占比,永不隐藏。
//! 数据来自 memoryBudgetStatus;点击打开检查器「上下文与记忆」页签。

import { useEffect, useState } from 'react';
import { memoryBudgetStatus } from '../lib/ipc/memory';
import { useMessagesStore } from '../stores/messages';

const R = 8;
const C = 2 * Math.PI * R;

export function BudgetRing({ sessionId, onOpen }: { sessionId: string; onOpen: () => void }) {
  const len = useMessagesStore((s) => (sessionId ? (s.bySession[sessionId]?.length ?? 0) : 0));
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
  }, [sessionId, streaming, len]);

  if (!budget || budget.limit <= 0) return null;
  const pct = Math.min(100, Math.round((budget.tokens / budget.limit) * 100));
  const warn = pct >= 80;
  return (
    <button
      type="button"
      className={`icon-btn budget-ring${warn ? ' warn' : ''}`}
      title={`上下文预算 ${budget.tokens.toLocaleString()}/${budget.limit.toLocaleString()} tokens(${pct}%)${
        warn ? '\n已超 80%,发送时将自动压缩' : ''
      }\n点击查看「上下文与记忆」`}
      onClick={onOpen}
    >
      <svg width={16} height={16} viewBox="0 0 20 20" aria-hidden="true">
        <circle cx={10} cy={10} r={R} fill="none" stroke="var(--border)" strokeWidth={2.5} />
        <circle
          cx={10}
          cy={10}
          r={R}
          fill="none"
          stroke="currentColor"
          strokeWidth={2.5}
          strokeDasharray={`${(pct / 100) * C} ${C}`}
          strokeLinecap="round"
          transform="rotate(-90 10 10)"
        />
      </svg>
    </button>
  );
}
