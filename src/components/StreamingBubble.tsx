import { useEffect, useRef, useState } from 'react';
import { useMessagesStore } from '../stores/messages';
import { MemoMarkdown } from './MemoMarkdown';
import ReasoningBlock from './ReasoningBlock';
import { ToolTrace, ToolTraceGroup } from './ToolTrace';
import { groupTraceSegments, splitTraceSegments } from '../lib/toolTrace';

/** 占位阶段 → 展示文案(P0-2/P1-4:发送后各耗时阶段都有明确指示,不再「空窗」) */
const PHASE_LABELS: Record<string, string> = {
  preparing: '正在准备…',
  search: '正在联网搜索…',
  connecting: '连接模型中…',
};

/** 「正在思考」占位:三点动画 + 阶段文案 + 已等待秒数。
 *  P2-7:超过 3s 仍无首 chunk 追加「响应较慢」提示;超 30s 建议检查网络/服务商。 */
function ThinkingHint({ sessionId, hasReasoning }: { sessionId: string; hasReasoning: boolean }) {
  const phase = useMessagesStore((s) => s.streamingPhase[sessionId]);
  const startedAt = useMessagesStore((s) => s.streamStartedAt[sessionId]);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  const elapsed = startedAt ? Math.max(0, Math.floor((now - startedAt) / 1000)) : 0;
  const label = (phase && PHASE_LABELS[phase]) || '正在思考…';
  let suffix = '';
  if (elapsed >= 30) {
    suffix = ` · 已等待 ${elapsed}s,可检查网络或服务商状态`;
  } else if (elapsed >= 3 && phase !== 'search' && phase !== 'preparing' && !hasReasoning) {
    // 等待的是模型(而非搜索/准备/已在输出的思考流)才提示「响应较慢」
    suffix = ` · 模型响应较慢,已等待 ${elapsed}s`;
  } else if (elapsed >= 1) {
    suffix = ` · 已等待 ${elapsed}s`;
  }
  return (
    <div className="thinking-hint" aria-live="polite">
      <span className="th-dot" />
      <span className="th-dot" />
      <span className="th-dot" />
      <span className="th-label">
        {label}
        {suffix}
      </span>
    </div>
  );
}

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
              <ThinkingHint sessionId={sessionId} hasReasoning={reasoningText.length > 0} />
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
