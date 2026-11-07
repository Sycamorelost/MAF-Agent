import { memo } from 'react';
import type { RefObject } from 'react';
import { Virtualizer, type VirtualizerHandle } from 'virtua';
import MessageBubble, { type BubbleMessage } from '../../components/MessageBubble';
import * as ipc from '../../lib/ipc';
import type { MessageRow } from '../../lib/ipc';

interface MessageListProps {
  messages: MessageRow[];
  /** 消息滚动容器(.chat-scroll):虚拟列表复用现有滚动条,不额外嵌套一层 */
  scrollRef: RefObject<HTMLDivElement | null>;
  /** 虚拟列表句柄:供搜索跳转按 index 定位(离屏消息不挂载,无法用 id 查找) */
  listRef: RefObject<VirtualizerHandle | null>;
  /** P3:虚拟列表在滚动容器内的起始偏移(上方横幅/任务清单等兄弟节点的高度) */
  startMargin: number;
  lastAssistantId: number | undefined;
  onQuote: (content: string, meta?: { role?: string; createdAt?: string }) => void;
  onEditResend: (msg: BubbleMessage) => void;
  onDelete: (id: number) => void;
  onRegenerate: (msg: BubbleMessage) => void;
  onRetryModel: (msg: BubbleMessage) => void;
  onSaveSources: (sources: ipc.SourceItem[]) => void;
  onPin: (content: string, section: string) => void;
}

/** 历史消息列表(memo + 虚拟滚动,P3):
 *  - 只挂载视口附近的消息,长历史不再随条数线性增加 DOM;
 *  - 流式刷新时整棵历史子树跳过渲染(流式气泡由 StreamingBubble 独立订阅)。 */
export const MessageList = memo(function MessageList({
  messages,
  scrollRef,
  listRef,
  startMargin,
  lastAssistantId,
  onQuote,
  onEditResend,
  onDelete,
  onRegenerate,
  onRetryModel,
  onSaveSources,
  onPin,
}: MessageListProps) {
  return (
    <Virtualizer
      ref={listRef}
      data={messages}
      scrollRef={scrollRef}
      startMargin={startMargin}
      // 分页向前插入历史时以列表尾部为锚点,保持当前阅读位置不跳
      shift
    >
      {(m: MessageRow) => (
        // 虚拟列表内不再有 .chat-inner 的 gap,用内边距保持消息间距
        <div key={m.id} id={`msg-${m.id}`} style={{ paddingBottom: 'var(--space-4)' }}>
          <MessageBubble
            msg={m}
            sessionId={m.session_id}
            isLastAssistant={m.role === 'assistant' && m.id === lastAssistantId}
            onQuote={onQuote}
            onEditResend={m.role === 'user' ? onEditResend : undefined}
            onDelete={onDelete}
            onRegenerate={
              m.role === 'assistant' && m.id === lastAssistantId ? () => onRegenerate(m) : undefined
            }
            onRetryModel={
              m.role === 'assistant' && m.id === lastAssistantId ? () => onRetryModel(m) : undefined
            }
            onSaveSources={m.role === 'assistant' ? onSaveSources : undefined}
            onPin={onPin}
          />
        </div>
      )}
    </Virtualizer>
  );
});
