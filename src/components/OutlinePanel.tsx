import { useMemo, useState } from 'react';
import { useActiveMessageId } from '../lib/useActiveMessageId';
import { estimateReasoningTokens } from './ReasoningBlock';
import type { MessageRow } from '../lib/ipc';

interface Props {
  messages: MessageRow[];
  /** 跳转定位(由 ChatPage 处理虚拟列表离屏渲染) */
  onJump: (msgId: number) => void;
}

/** 从消息提取纲要标题(用户:前 30 字;助手:优先首个标题,否则前 30 字)。 */
function extractTitle(msg: MessageRow, index: number): string {
  if (msg.role === 'user') {
    const plain = stripMd(msg.content);
    return plain.slice(0, 30) || `第 ${index} 轮 · 提问`;
  }
  // 助手:优先 markdown 首个标题
  const hMatch = msg.content.match(/^\s{0,3}(#{1,4})\s+(.+)$/m);
  if (hMatch) return hMatch[2].trim();
  const plain = stripMd(msg.content);
  return plain.slice(0, 40) || `第 ${index} 轮 · AI 回复`;
}

/** 剥离 Markdown 标记,保留纯文本。 */
function stripMd(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, '')
    .replace(/`[^`]*`/g, '')
    .replace(/[#>*_~`!\[\]()|]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** 固定 6 条灰色线,不再跟随消息数动态生成 */
const FIXED_RAILS = 6;

/**
 * 会话纲要:右缘贴边短横线(每条 = 一次回答),hover 展开完整纲要浮层。
 * 浮层无边框、可滚动;点击条目平滑跳转并 flash。
 */
export default function OutlinePanel({ messages, onJump }: Props) {
  const [open, setOpen] = useState(false);
  const ids = useMemo(() => messages.map((m) => m.id), [messages]);
  const activeId = useActiveMessageId(ids);

  const items = useMemo(
    () =>
      messages.map((m, i) => ({
        id: m.id,
        role: m.role,
        index: i + 1,
        title: extractTitle(m, i + 1),
      })),
    [messages],
  );

  // 会话统计(原消息 meta 行的 token 信息聚合展示,精简气泡下方排版)
  const stats = useMemo(() => {
    const assistants = messages.filter((m) => m.role === 'assistant');
    const outTokens = assistants.reduce((s, m) => s + (m.tokens ?? 0), 0);
    const thinkTokens = assistants.reduce(
      (s, m) => s + (m.reasoning ? estimateReasoningTokens(m.reasoning) : 0),
      0,
    );
    const models = [...new Set(assistants.map((m) => m.model_id).filter(Boolean))] as string[];
    const times = messages.map((m) => m.created).filter(Boolean).sort();
    const range =
      times.length > 0
        ? `${new Date(times[0]).toLocaleTimeString('zh-CN')} – ${new Date(times[times.length - 1]).toLocaleTimeString('zh-CN')}`
        : '';
    return {
      q: messages.length - assistants.length,
      a: assistants.length,
      outTokens,
      thinkTokens,
      models,
      range,
    };
  }, [messages]);

  /** 固定 6 条灰色线 */

  if (messages.length === 0) return null;

  return (
    <div
      className="outline-rail-wrap"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <div className="outline-rail" aria-label="会话纲要">
        {Array.from({ length: FIXED_RAILS }).map((_, i) => (
          <div key={i} className="outline-dash" />
        ))}
      </div>
      {open && (
        <div className="outline-float">
          <div className="outline-head">
            <span>纲要</span>
            <span className="outline-index" style={{ marginLeft: 'auto' }}>
              {messages.length} 条
            </span>
          </div>
          <div className="outline-stats">
            <div className="outline-stats-row">
              <span>
                提问 {stats.q} / 回答 {stats.a}
              </span>
              {stats.range && <span>{stats.range}</span>}
            </div>
            {stats.outTokens > 0 && (
              <div className="outline-stats-row">
                <span>
                  输出 {stats.outTokens} tok
                  {stats.thinkTokens > 0 && `(含思考约 ${stats.thinkTokens})`}
                </span>
              </div>
            )}
            {stats.models.length > 0 && (
              <div
                className="outline-stats-row outline-stats-models"
                title={stats.models.join('\n')}
              >
                <span>{stats.models.join(' · ')}</span>
              </div>
            )}
          </div>
          <div className="outline-list">
            {items.map((it) => (
              <button
                key={it.id}
                type="button"
                className={`outline-item ${activeId === it.id ? 'active' : ''}`}
                onClick={() => {
                  onJump(it.id);
                  setOpen(false);
                }}
                title={it.title}
              >
                <span className={`outline-role ${it.role}`}>{it.role === 'user' ? 'Q' : 'A'}</span>
                <span className="outline-title">{it.title}</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
