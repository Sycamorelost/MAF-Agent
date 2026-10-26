import { useState } from 'react';
import React from 'react';
// 代码块语法高亮:VS Code Dark+ 配色。必须用深色主题——代码面为深底(见 app.css),
// 若导入浅色主题(github.css),其 `.hljs{color:#24292e}` 会把灰字压到深底上,可读性崩塌。
import 'highlight.js/styles/vs2015.css';
import type { MessageRow, SourceItem } from '../lib/ipc';
import { MemoMarkdown, CodeBlock } from './MemoMarkdown';
import ReasoningBlock from './ReasoningBlock';
import { AttachmentCards } from './bubble/AttachmentCards';
import { SourceCards } from './bubble/SourceCards';
import { IconCheck, IconCopy, IconEdit, IconPin, IconQuote, IconRefresh, IconSwap, IconTranslate, IconTrash } from './icons';

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
>;

interface Props {
  msg: BubbleMessage;
  /** 是否为最后一条 AI 回复(决定显示「重新生成」) */
  isLastAssistant?: boolean;
  onQuote?: (content: string) => void;
  onEditResend?: (msg: BubbleMessage) => void;
  onDelete?: (id: number) => void;
  onRegenerate?: () => void;
  /** 「换模型重试」(F5):弹模型选择,用新模型重发且不动会话模型 */
  onRetryModel?: () => void;
  /** 「存入记忆」:把来源列表写入 memory.md 资料节 */
  onSaveSources?: (sources: SourceItem[]) => void;
  /** 「翻译」:把消息内容带到翻译页(迭代 D6.5) */
  onTranslate?: (content: string) => void;
  /** 「钉入记忆」:把消息正文写入 memory.md 指定节(F8) */
  onPin?: (content: string, section: string) => void;
}

/** 代码块容器已从 MemoMarkdown.tsx 导出,此处仅做兼容 re-export。 */
export { CodeBlock };

function MessageBubbleInner({
  msg,
  isLastAssistant,
  onQuote,
  onEditResend,
  onDelete,
  onRegenerate,
  onRetryModel,
  onSaveSources,
  onTranslate,
  onPin,
}: Props) {
  const isUser = msg.role === 'user';
  const [copied, setCopied] = useState(false);
  const [pinOpen, setPinOpen] = useState(false);
  const reasoning = !isUser ? (msg.reasoning ?? '') : '';

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
    onQuote || onEditResend || onDelete || onRegenerate || onRetryModel || onTranslate || onPin;

  return (
    <div className={`msg ${isUser ? 'user' : 'assistant'}`}>
      <div className="content">
        <AttachmentCards attachments={msg.attachments} />
        {msg.content && (
          <div className="bubble">
            {reasoning && <ReasoningBlock text={reasoning} />}
            <div className="md">
              <MemoMarkdown content={msg.content} withCodeBlock={!isUser} />
            </div>
          </div>
        )}
        <SourceCards raw={msg.sources ?? null} onSaveSources={onSaveSources} />
        <div className="meta">
          {msg.model_id && <span>{msg.model_id}</span>}
          {msg.model_id && msg.created && ' · '}
          {msg.created && <span>{new Date(msg.created).toLocaleTimeString('zh-CN')}</span>}
          {hasActions && (
            <span className="msg-actions">
              <button type="button" title={copied ? '已复制' : '复制'} onClick={onCopy}>
                {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
              </button>
              {onQuote && (
                <button type="button" title="引用" onClick={() => onQuote(msg.content)}>
                  <IconQuote size={13} />
                </button>
              )}
              {onTranslate && msg.content && (
                <button type="button" title="翻译" onClick={() => onTranslate(msg.content)}>
                  <IconTranslate size={13} />
                </button>
              )}
              {onPin && msg.content && (
                <span className="pin-wrap">
                  <button
                    type="button"
                    title="钉入记忆"
                    onClick={() => setPinOpen((v) => !v)}
                  >
                    <IconPin size={13} />
                  </button>
                  {pinOpen && (
                    <>
                      <span className="pin-backdrop" onClick={() => setPinOpen(false)} />
                      <span className="pin-menu">
                        <span className="pin-menu-title">钉入到记忆节</span>
                        {MEMORY_SECTIONS.map((sec) => (
                          <button
                            key={sec}
                            type="button"
                            onClick={() => {
                              setPinOpen(false);
                              onPin(msg.content, sec);
                            }}
                          >
                            {sec}
                          </button>
                        ))}
                      </span>
                    </>
                  )}
                </span>
              )}
              {isUser && onEditResend && (
                <button type="button" title="编辑并重发(删除此条及其后消息)" onClick={() => onEditResend(msg)}>
                  <IconEdit size={13} />
                </button>
              )}
              {!isUser && isLastAssistant && onRegenerate && (
                <button type="button" title="重新生成" onClick={onRegenerate}>
                  <IconRefresh size={13} />
                </button>
              )}
              {!isUser && isLastAssistant && onRetryModel && (
                <button type="button" title="换模型重试(不改变会话模型)" onClick={onRetryModel}>
                  <IconSwap size={13} />
                </button>
              )}
              {onDelete && msg.id !== undefined && (
                <button type="button" title="删除" onClick={() => onDelete(msg.id!)}>
                  <IconTrash size={13} />
                </button>
              )}
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
    prev.isLastAssistant === next.isLastAssistant
  );
});

export default MessageBubble;
