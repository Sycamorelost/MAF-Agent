import { memo } from 'react';
import MessageBubble, { type BubbleMessage } from '../../components/MessageBubble';
import * as ipc from '../../lib/ipc';
import type { MessageRow } from '../../lib/ipc';

interface MessageListProps {
  messages: MessageRow[];
  lastAssistantId: number | undefined;
  onQuote: (content: string) => void;
  onTranslate: (content: string) => void;
  onEditResend: (msg: BubbleMessage) => void;
  onDelete: (id: number) => void;
  onRegenerate: (msg: BubbleMessage) => void;
  onRetryModel: (msg: BubbleMessage) => void;
  onSaveSources: (sources: ipc.SourceItem[]) => void;
  onPin: (content: string, section: string) => void;
}

/** 历史消息列表(memo):流式刷新时整棵历史子树跳过渲染(流式气泡由 StreamingBubble 独立订阅)。 */
export const MessageList = memo(function MessageList({
  messages,
  lastAssistantId,
  onQuote,
  onTranslate,
  onEditResend,
  onDelete,
  onRegenerate,
  onRetryModel,
  onSaveSources,
  onPin,
}: MessageListProps) {
  return (
    <>
      {messages.map((m) => (
        <div key={m.id} id={`msg-${m.id}`}>
          <MessageBubble
            msg={m}
            isLastAssistant={m.role === 'assistant' && m.id === lastAssistantId}
            onQuote={onQuote}
            onTranslate={onTranslate}
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
      ))}
    </>
  );
});
