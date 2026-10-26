//! 切换模型交接弹窗(迭代 D3.1):4 种记忆注入策略 + 各策略 token 预估。

import { useEffect, useMemo, useState } from 'react';
import type { MemoryStrategy } from '../lib/ipc';
import { estimateTokens, injectByStrategy } from '../lib/memoryInject';

interface Props {
  from: string;
  to: string;
  /** 当前 memory.md 正文(用于各策略 token 预估) */
  memoryBody: string;
  /** 会话当前策略(预选) */
  currentStrategy: MemoryStrategy;
  onConfirm: (strategy: MemoryStrategy) => void;
  onCancel: () => void;
}

const STRATEGY_META: { key: MemoryStrategy; name: string; desc: string }[] = [
  { key: 'full', name: '全文注入', desc: '完整 memory.md 注入上下文,信息最全' },
  { key: 'summary', name: '仅摘要', desc: '只注入「会话摘要」节,最省 token' },
  { key: 'hybrid', name: '摘要 + 事实 + 待办', desc: '摘要、关键事实与结论、待办,平衡方案' },
  { key: 'none', name: '不注入', desc: '全新开始,不带任何历史记忆' },
];

export default function MemoryHandoffDialog({
  from,
  to,
  memoryBody,
  currentStrategy,
  onConfirm,
  onCancel,
}: Props) {
  const [strategy, setStrategy] = useState<MemoryStrategy>(currentStrategy);

  /** Esc 取消(与全局搜索一致,避免只能点遮罩) */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onCancel]);

  // 各策略的 token 预估(与注入逻辑同一套函数,保证预估 = 实际)
  const estimates = useMemo(() => {
    const out = {} as Record<MemoryStrategy, number>;
    for (const s of STRATEGY_META) {
      out[s.key] = estimateTokens(injectByStrategy(memoryBody, s.key));
    }
    return out;
  }, [memoryBody]);

  return (
    <div className="dialog-mask" onClick={onCancel}>
      <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ width: 520 }}>
        <div className="dialog-head">模型切换 · 记忆交接</div>
        <div className="dialog-body">
          <div className="handoff-route">
            <span className="handoff-model">{from}</span>
            <span className="handoff-arrow">→</span>
            <span className="handoff-model to">{to}</span>
          </div>
          <div className="handoff-tip">选择切换后注入新模型的记忆范围(会话级,可随时再改):</div>
          <div className="handoff-options">
            {STRATEGY_META.map((s) => (
              <button
                type="button"
                key={s.key}
                className={`handoff-option ${strategy === s.key ? 'active' : ''}`}
                onClick={() => setStrategy(s.key)}
              >
                <div className="handoff-option-head">
                  <span className="handoff-option-name">{s.name}</span>
                  <span className="handoff-option-tokens">约 {estimates[s.key]} tokens</span>
                </div>
                <div className="handoff-option-desc">{s.desc}</div>
              </button>
            ))}
          </div>
          <div className="handoff-footer">
            <button type="button" className="btn ghost" onClick={onCancel}>
              保持原策略
            </button>
            <button type="button" className="btn primary" onClick={() => onConfirm(strategy)}>
              确认切换
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
