import { useEffect, useRef } from 'react';
import { useMessagesStore } from '../stores/messages';
import { MemoMarkdown } from './MemoMarkdown';
import ReasoningBlock from './ReasoningBlock';

/** 流式气泡:独立订阅 streaming[sessionId],ChatPage 主树不随 50ms 去抖重渲染。
 *  流式期间降级渲染(无语法高亮 / 无 mermaid),结束后由历史消息的完整渲染接管。
 *  滚动跟随也在组件内部完成,避免整页 effect 依赖流式文本。
 *  v1.9.0:思考流独立累积,折叠块默认收起(展开/收起纯手动)。 */
export default function StreamingBubble({
  sessionId,
  modelId,
}: {
  sessionId: string;
  modelId: string | null;
}) {
  const text = useMessagesStore((s) => s.streaming[sessionId]);
  const reasoning = useMessagesStore((s) => s.streamingReasoning[sessionId]);
  const rInfo = useMessagesStore((s) => s.streamingReasoningInfo[sessionId]);
  const rootRef = useRef<HTMLDivElement>(null);

  const streaming = text !== undefined;
  const reasoningText = reasoning ?? '';

  // 流式增量到达:直接滚动到底部(无动画,避免去抖刷新时 smooth 动画堆积卡顿)
  useEffect(() => {
    if (!streaming) return;
    const scroller = rootRef.current?.closest('.chat-scroll');
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: 'auto' });
  }, [streaming, text, reasoningText]);

  if (!streaming) return null;

  return (
    <div className="msg assistant" ref={rootRef}>
      <div className="content">
        <div className="bubble">
          {(reasoningText || rInfo?.tokens) && (
            <ReasoningBlock
              text={reasoningText}
              tokens={rInfo?.tokens ?? null}
              elapsedMs={rInfo?.elapsedMs ?? null}
              streaming
            />
          )}
          <div className="md">
            <MemoMarkdown content={text} withCodeBlock streaming />
            <span className="stream-cursor" aria-hidden="true" />
          </div>
        </div>
        <div className="meta">{modelId && <span>{modelId}</span>}</div>
      </div>
    </div>
  );
}
