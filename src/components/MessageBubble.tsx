import { useMemo, useState } from 'react';
import React from 'react';
// 代码块语法高亮:VS Code Dark+ 配色。必须用深色主题——代码面为深底(见 app.css),
// 若导入浅色主题(github.css),其 `.hljs{color:#24292e}` 会把灰字压到深底上,可读性崩塌。
import 'highlight.js/styles/vs2015.css';
import type { MessageRow, SourceItem } from '../lib/ipc';
import { MemoMarkdown, CodeBlock } from './MemoMarkdown';
import ReasoningBlock from './ReasoningBlock';
import { ToolTrace, ToolTraceGroup } from './ToolTrace';
import { AttachmentCards } from './bubble/AttachmentCards';
import { OutputCards } from './bubble/OutputCards';
import { SourceCards } from './bubble/SourceCards';
import { groupTraceSegments, splitTraceSegments } from '../lib/toolTrace';
import { IconCheck, IconCopy, IconEdit, IconMore, IconPin, IconQuote, IconRefresh, IconSwap, IconTrash } from './icons';

/** 记忆 6 个标准节(与后端 KNOWN_SECTIONS 严格一致)。 */
export const MEMORY_SECTIONS = [
  '会话摘要',
  '关键事实与结论',
  '用户偏好与约束',
  '待办 / 未决问题',
  '文档摘要',
  '资料',
] as const;

/** 气泡渲染所需的最小消息字段(流式气泡无 session_id/tokens)。 */
export type BubbleMessage = Pick<
  MessageRow,
  | 'id'
  | 'role'
  | 'content'
  | 'model_id'
  | 'created'
  | 'attachments'
  | 'sources'
  | 'reasoning'
  | 'tokens'
  | 'partial'
>;

interface Props {
  msg: BubbleMessage;
  /** 所属会话(产物卡按该会话工作区解析相对路径;流式气泡可为空) */
  sessionId?: string;
  /** 是否为最后一条 AI 回复(决定显示「重新生成」) */
  isLastAssistant?: boolean;
  /** 引用:message 级别摘要,完整原文由外层保存(选区优先在调用方处理) */
  onQuote?: (content: string, meta?: { role?: string; createdAt?: string }) => void;
  onEditResend?: (msg: BubbleMessage) => void;
  onDelete?: (id: number) => void;
  onRegenerate?: () => void;
  /** 「换模型重试」(F5):弹模型选择,用新模型重发且不动会话模型 */
  onRetryModel?: () => void;
  /** 「存入记忆」:把来源列表写入 memory.md 资料节 */
  onSaveSources?: (sources: SourceItem[]) => void;
  /** 「钉入记忆」:把消息正文写入 memory.md 指定节(F8) */
  onPin?: (content: string, section: string) => void;
}

/** 代码块容器已从 MemoMarkdown.tsx 导出,此处仅做兼容 re-export。 */
export { CodeBlock };

/** P3:单条超长消息默认折叠预览 —— 超过字符/行数阈值时只渲染前段,
 *  避免一次挂载巨量 Markdown DOM(200k 字符级别)把主线程卡住;仍可一键展开全文。 */
const LONG_PREVIEW_CHARS = 8000;
const LONG_PREVIEW_LINES = 300;

/** 计算预览文本:按行数上限或字符上限(优先在上限前最后一个换行处截断)。 */
function longPreviewText(text: string): { shown: string; long: boolean } {
  let cut = text.length;
  let lines = 1;
  for (let i = 0; i < text.length; i++) {
    if (text.charCodeAt(i) === 10 && ++lines > LONG_PREVIEW_LINES) {
      cut = i;
      break;
    }
  }
  if (cut > LONG_PREVIEW_CHARS) {
    const at = text.lastIndexOf('\n', LONG_PREVIEW_CHARS);
    cut = at > 0 ? at : LONG_PREVIEW_CHARS;
  }
  if (cut >= text.length) return { shown: text, long: false };
  return { shown: text.slice(0, cut), long: true };
}

/** 按字符预算截取渲染段:工具块本身已截断,按固定权重计;剩余正文段截到上限。 */
function cutSegments(
  segs: ReturnType<typeof groupTraceSegments>,
  maxChars: number,
): ReturnType<typeof groupTraceSegments> {
  const out: ReturnType<typeof groupTraceSegments> = [];
  let used = 0;
  for (const s of segs) {
    if (s.kind === 'traces') {
      out.push(s);
      used += 100;
      if (used >= maxChars) break;
      continue;
    }
    const remaining = maxChars - used;
    if (remaining <= 0) break;
    if (s.text.length <= remaining) {
      out.push(s);
      used += s.text.length;
      continue;
    }
    const cut = s.text.slice(0, remaining);
    const at = cut.lastIndexOf('\n');
    out.push({ ...s, text: at > 0 ? cut.slice(0, at) : cut });
    break;
  }
  return out;
}

function MessageBubbleInner({
  msg,
  sessionId,
  isLastAssistant,
  onQuote,
  onEditResend,
  onDelete,
  onRegenerate,
  onRetryModel,
  onSaveSources,
  onPin,
}: Props) {
  const isUser = msg.role === 'user';
  const [copied, setCopied] = useState(false);
  const [pinOpen, setPinOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  /** P3:超长消息默认折叠,点击展开/收起 */
  const [expanded, setExpanded] = useState(false);
  const reasoning = !isUser ? (msg.reasoning ?? '') : '';

  const closeMenus = () => { setPinOpen(false); setMoreOpen(false); };

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(msg.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // 静默
    }
  };

  const hasActions =
    onQuote || onEditResend || onDelete || onRegenerate || onRetryModel || onPin;

  // 工具块提到气泡外(对齐设计稿:思考块 / 工具行 / 子任务卡片与正文平级);相邻工具块成组折叠
  const allSegments = useMemo(
    () => (isUser ? [] : groupTraceSegments(splitTraceSegments(msg.content))),
    [isUser, msg.content],
  );
  const preview = useMemo(() => longPreviewText(msg.content), [msg.content]);
  const showFull = expanded || !preview.long;
  const shownContent = showFull ? msg.content : preview.shown;
  const segments = useMemo(
    () => (showFull ? allSegments : cutSegments(allSegments, shownContent.length)),
    [showFull, allSegments, shownContent],
  );

  return (
    <div className={`msg ${isUser ? 'user' : 'assistant'}`}>
      <div className="msg-col">
        <AttachmentCards attachments={msg.attachments} />
        {reasoning && <ReasoningBlock text={reasoning} />}
        {isUser
          ? shownContent && (
              <div className="bubble">
                <div className="md">
                  <MemoMarkdown content={shownContent} withCodeBlock={false} />
                </div>
              </div>
            )
          : segments.map((seg, i) =>
              seg.kind === 'traces' ? (
                seg.raws.length === 1 ? (
                  <ToolTrace key={`t${i}`} raw={seg.raws[0]} />
                ) : (
                  <ToolTraceGroup key={`t${i}`} raws={seg.raws} />
                )
              ) : (
                <div className="bubble" key={`m${i}`}>
                  <div className="md">
                    <MemoMarkdown content={seg.text} withCodeBlock />
                  </div>
                </div>
              ),
            )}
        {preview.long && (
          <button
            type="button"
            className="msg-expand"
            onClick={() => setExpanded((v) => !v)}
          >
            {expanded
              ? '收起'
              : `展开全文(共 ${msg.content.length.toLocaleString('zh-CN')} 字符)`}
          </button>
        )}
        {!isUser && msg.partial && (
          <div className="msg-partial" title="生成被中断，已保存中断前的内容">
            输出中断 · 已保存部分内容
          </div>
        )}
        <SourceCards raw={msg.sources ?? null} onSaveSources={onSaveSources} />
        {!isUser && (
          <OutputCards content={msg.content} sessionId={sessionId} messageId={msg.id} />
        )}
        <div className="msg-meta">
          {msg.model_id && <span>{msg.model_id}</span>}
          {msg.model_id && msg.created && ' · '}
          {msg.created && <span>{new Date(msg.created).toLocaleTimeString('zh-CN')}</span>}
          {msg.tokens != null && msg.tokens > 0 && (
            <>
              {(msg.model_id || msg.created) && ' · '}
              <span>本轮 {msg.tokens.toLocaleString('zh-CN')} tokens</span>
            </>
          )}
          {hasActions && (
            <span className="msg-actions">
              <button type="button" title={copied ? '已复制' : '复制'} onClick={onCopy}>
                {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
              </button>
              <span className="more-wrap">
                <button
                  type="button"
                  title="更多操作"
                  className={moreOpen ? 'on' : ''}
                  onClick={() => { setMoreOpen((v) => !v); setPinOpen(false); }}
                >
                  <IconMore size={13} />
                </button>
                {moreOpen && (
                  <>
                    <span className="more-backdrop" onClick={closeMenus} />
                    <span className="more-menu">
                      {onQuote && (
                        <button type="button" onClick={() => { closeMenus(); onQuote(msg.content, { role: msg.role, createdAt: msg.created }); }}>
                          <IconQuote size={13} /><span>引用</span>
                        </button>
                      )}
                      {onPin && msg.content && (
                        <span className="more-sub">
                          <button
                            type="button"
                            className={pinOpen ? 'on' : ''}
                            onClick={() => setPinOpen((v) => !v)}
                          >
                            <IconPin size={13} /><span>钉入记忆</span>
                          </button>
                          {pinOpen && (
                            <span className="more-sub-menu">
                              {MEMORY_SECTIONS.map((sec) => (
                                <button
                                  key={sec}
                                  type="button"
                                  onClick={() => { closeMenus(); onPin(msg.content, sec); }}
                                >
                                  {sec}
                                </button>
                              ))}
                            </span>
                          )}
                        </span>
                      )}
                      {isUser && onEditResend && (
                        <button type="button" onClick={() => { closeMenus(); onEditResend(msg); }}>
                          <IconEdit size={13} /><span>编辑并重发</span>
                        </button>
                      )}
                      {!isUser && isLastAssistant && onRegenerate && (
                        <button type="button" onClick={() => { closeMenus(); onRegenerate(); }}>
                          <IconRefresh size={13} /><span>重新生成</span>
                        </button>
                      )}
                      {!isUser && isLastAssistant && onRetryModel && (
                        <button type="button" onClick={() => { closeMenus(); onRetryModel(); }}>
                          <IconSwap size={13} /><span>换模型重试</span>
                        </button>
                      )}
                      {onDelete && msg.id !== undefined && (
                        <button type="button" className="danger" onClick={() => { closeMenus(); onDelete(msg.id!); }}>
                          <IconTrash size={13} /><span>删除</span>
                        </button>
                      )}
                    </span>
                  </>
                )}
              </span>
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * 消息气泡(带 memo):消息字段不变时不重渲染,
 * 避免流式输出 / 记忆面板刷新导致历史消息重复解析 Markdown。
 */
const MessageBubble = React.memo(MessageBubbleInner, (prev, next) => {
  const a = prev.msg;
  const b = next.msg;
  return (
    a.id === b.id &&
    a.content === b.content &&
    a.role === b.role &&
    a.model_id === b.model_id &&
    a.created === b.created &&
    a.attachments === b.attachments &&
    a.sources === b.sources &&
    a.reasoning === b.reasoning &&
    a.tokens === b.tokens &&
    a.partial === b.partial &&
    prev.isLastAssistant === next.isLastAssistant
  );
});

export default MessageBubble;
