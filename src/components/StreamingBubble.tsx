import { useEffect, useRef } from 'react';
import { useMessagesStore } from '../stores/messages';
import { MemoMarkdown } from './MemoMarkdown';
import ReasoningBlock from './ReasoningBlock';
import { ToolTrace, ToolTraceGroup } from './ToolTrace';
import { groupTraceSegments, splitTraceSegments } from '../lib/toolTrace';

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

  // 流式增量到达:仅在用户停在底部时跟随滚动(上滚查看时不再被强行拽回底部)
  useEffect(() => {
    if (!streaming) return;
    const scroller = rootRef.current?.closest('.chat-scroll') as HTMLElement | null;
    if (!scroller) return;
    // 阈值与 ChatPage 的「跟随」判定保持一致(距底 <80px 才算停在底部)
    const dist = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
    if (dist >= 80) return;
    scroller.scrollTo({ top: scroller.scrollHeight, behavior: 'auto' });
  }, [streaming, text, reasoningText]);

  if (!streaming) return null;

  const segments = text ? groupTraceSegments(splitTraceSegments(text)) : [];
  const lastTextIdx = segments.reduce((acc, s, i) => (s.kind === 'text' ? i : acc), -1);

  return (
    <div className="msg assistant" ref={rootRef}>
      <div className="msg-col">
        {(reasoningText || rInfo?.tokens) && (
          <ReasoningBlock
            text={reasoningText}
            tokens={rInfo?.tokens ?? null}
            elapsedMs={rInfo?.elapsedMs ?? null}
            streaming
          />
        )}
        {segments.length === 0 && (
          <div className="bubble">
            <div className="md">
              {/* 即时响应:正文与思考都还没到时,先给「正在思考」动态提示(准备阶段/模型首字延迟) */}
              <div className="thinking-hint" aria-live="polite">
                <span className="th-dot" />
                <span className="th-dot" />
                <span className="th-dot" />
                <span className="th-label">正在思考…</span>
              </div>
            </div>
          </div>
        )}
        {segments.map((seg, i) =>
          seg.kind === 'traces' ? (
            seg.raws.length === 1 ? (
              <ToolTrace key={`t${i}`} raw={seg.raws[0]} />
            ) : (
              <ToolTraceGroup key={`t${i}`} raws={seg.raws} />
            )
          ) : (
            <div className="bubble" key={`m${i}`}>
              <div className="md">
                <MemoMarkdown content={seg.text} withCodeBlock streaming />
                {i === lastTextIdx && <span className="stream-cursor" aria-hidden="true" />}
              </div>
            </div>
          ),
        )}
        <div className="msg-meta">{modelId && <span>{modelId}</span>}</div>
      </div>
    </div>
  );
}
