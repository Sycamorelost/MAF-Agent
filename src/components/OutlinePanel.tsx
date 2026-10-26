import { useMemo } from 'react';
import { jumpToMessage } from '../lib/jumpToMessage';
import { useActiveMessageId } from '../lib/useActiveMessageId';
import type { MessageRow } from '../lib/ipc';

interface Props {
  messages: MessageRow[];
  onClose: () => void;
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

/** 会话纲要面板:按消息展示标题,点击平滑跳转并 flash,滚动时高亮活跃项。 */
export default function OutlinePanel({ messages, onClose }: Props) {
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

  return (
    <aside className="outline-panel">
      <div className="outline-head">
        <span>纲要</span>
        <span className="outline-index" style={{ marginLeft: 'auto' }}>{messages.length} 条</span>
        <button
          type="button"
          className="panel-close-btn"
          onClick={onClose}
          title="关闭纲要面板"
        >
          ✕
        </button>
      </div>
      <div className="outline-list">
        {items.length === 0 && (
          <div style={{ padding: 16, color: 'var(--text-3)', fontSize: 'var(--font-caption)' }}>
            暂无消息
          </div>
        )}
        {items.map((it) => (
          <button
            key={it.id}
            type="button"
            className={`outline-item ${activeId === it.id ? 'active' : ''}`}
            onClick={() => {
              jumpToMessage(it.id);
              onClose();
            }}
            title={it.title}
          >
            <span className={`outline-role ${it.role}`}>{it.role === 'user' ? 'Q' : 'A'}</span>
            <span className="outline-title">{it.title}</span>
          </button>
        ))}
      </div>
    </aside>
  );
}
