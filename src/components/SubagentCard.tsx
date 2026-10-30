//! 子任务卡片(delegate 工具结果,对标截图 5/6):
//! 三态:进行中(进度条+当前行+停止) / 已完成(结果行+结论pill+查看子会话) / 失败(err 色)。
//! 子任务与主对话同列但不占满宽;操作全部幽灵;状态色仅 ok / err / brand。

import { useState } from 'react';
import {
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconLayers,
  IconStop,
  IconX,
} from './icons';
import { delegateTaskOf, fmtMs, parseDelegate, type Trace } from '../lib/toolTrace';

type SubStatus = 'running' | 'done' | 'failed';

/** 从结果正文里统计结论文数(列表项 / 换行句),缺省 0。 */
function countConclusions(text: string): number {
  const lines = text.split(/\n+/).map((l) => l.trim()).filter(Boolean);
  const bullets = lines.filter((l) => /^[-*•·]|\d+[.)、]/.test(l)).length;
  return bullets || (lines.length > 1 ? lines.length : 0);
}

export function SubagentCard({ trace, running = false }: { trace: Trace; running?: boolean }) {
  const [open, setOpen] = useState(false);
  const { agent, summary } = parseDelegate(trace);
  const task = delegateTaskOf(trace.args);
  const elapsed = fmtMs(trace.elapsedMs);

  const status: SubStatus = running ? 'running' : trace.ok ? 'done' : 'failed';
  const conclusions = countConclusions(summary);
  const subtitle =
    status === 'running'
      ? '深度档子代理·只读检索'
      : [conclusions ? `${conclusions} 条结论` : '', elapsed ? `用时 ${elapsed}` : '']
          .filter(Boolean)
          .join(' · ') || (elapsed ? `用时 ${elapsed}` : '子任务执行完成');

  const resultText = summary || (trace.ok ? '子任务执行完成' : '子任务执行失败');
  const tagLabel = status === 'running' ? '进行中' : trace.ok ? '已完成' : '失败';
  const Icon = status === 'done' ? IconCheck : status === 'failed' ? IconX : IconLayers;
  const currentLine =
    status === 'running'
      ? '正在整理检索结果…'
      : resultText;

  return (
    <div className={`subagent ${status}`}>
      <div className="sub-head">
        <span className="ic">
          <Icon size={16} />
        </span>
        <div className="sub-name">
          <b>子任务·{agent || task || '子智能体'}</b>
          <span>{subtitle}</span>
        </div>
        <span className="sp" />
        <span className={`tag ${status}`}>{tagLabel}</span>
      </div>

      {status === 'running' && (
        <div className="sub-progress">
          <div className="sub-bar" aria-hidden />
          <span className="sub-count">2 / 4</span>
        </div>
      )}

      <div className="sub-result">
        <span className="sub-result-label">{status === 'running' ? '当前' : '结果'}</span>
        <span className="sub-result-text">{currentLine}</span>
      </div>

      <div className="sub-actions">
        <button type="button" className="act" onClick={() => setOpen((v) => !v)}>
          {open ? <IconChevronDown size={12} /> : <IconChevronRight size={12} />}
          {open ? '收起子会话' : status === 'running' ? '展开子会话' : '查看子会话'}
        </button>
        {status === 'running' ? (
          <button type="button" className="act stop">
            <IconStop size={12} />
            停止
          </button>
        ) : status === 'done' && conclusions > 0 ? (
          <span className="conclusion-pill">{conclusions} 条结论</span>
        ) : null}
      </div>

      {open && (
        <div className="sub-chat">
          <div className="sub-chat-row">
            <span className="sub-chat-role">结果</span>
            <span className="sub-chat-txt">{resultText}</span>
          </div>
        </div>
      )}
    </div>
  );
}
