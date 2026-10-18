import { useEffect, useState } from 'react';
import React from 'react';
import 'highlight.js/styles/github.css';
import * as ipc from '../lib/ipc';
import type { AttachmentMeta, MessageRow, SourceItem } from '../lib/ipc';
import { sanitizeHttpUrl } from '../lib/url';
import { MemoMarkdown, CodeBlock } from './MemoMarkdown';
import ReasoningBlock, { estimateReasoningTokens } from './ReasoningBlock';
import { IconBrain, IconCheck, IconCopy, IconDoc, IconEdit, IconPin, IconQuote, IconRefresh, IconSwap, IconTranslate, IconTrash } from './icons';

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

/** 单张图片附件:按需读 data URL 渲染缩略图,点击放大。 */
function ImageAttachment({ meta }: { meta: AttachmentMeta }) {
  const [url, setUrl] = useState<string | null>(null);
  const [zoom, setZoom] = useState(false);
  useEffect(() => {
    let alive = true;
    ipc
      .readAttachmentData(meta.path)
      .then((u) => alive && setUrl(u))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [meta.path]);
  if (!url) return <div className="attach-card">加载图片…</div>;
  return (
    <>
      <img
        className="attach-thumb"
        src={url}
        alt={meta.name}
        title={meta.name}
        onClick={() => setZoom(true)}
      />
      {zoom && (
        <div className="draw-lightbox" onClick={() => setZoom(false)}>
          <img src={url} alt={meta.name} />
        </div>
      )}
    </>
  );
}

/** 附件卡列表(渲染在气泡上方)。 */
function AttachmentCards({ attachments }: { attachments: string | null }) {
  if (!attachments) return null;
  let metas: AttachmentMeta[] = [];
  try {
    metas = JSON.parse(attachments);
  } catch {
    return null;
  }
  if (metas.length === 0) return null;
  return (
    <div className="attach-cards">
      {metas.map((m, i) =>
        m.kind === 'image' ? (
          <ImageAttachment key={`${m.path}-${i}`} meta={m} />
        ) : (
          <div key={`${m.path}-${i}`} className="attach-card" title={m.path}>
            <IconDoc size={16} />
            <div className="attach-card-info">
              <div className="attach-card-name">{m.name}</div>
              <div className="attach-card-kind">
                {m.kind.toUpperCase()}
                {m.extracted_text ? ' · 已解析文本' : ''}
              </div>
            </div>
          </div>
        ),
      )}
    </div>
  );
}

/** 联网来源卡片列表(渲染在气泡下方)。 */
function SourceCards({
  raw,
  onSaveSources,
}: {
  raw: string | null;
  onSaveSources?: (sources: SourceItem[]) => void;
}) {
  const [saved, setSaved] = useState(false);
  if (!raw) return null;
  let sources: SourceItem[] = [];
  try {
    sources = JSON.parse(raw);
  } catch {
    return null;
  }
  if (sources.length === 0) return null;
  const domain = (url: string) => {
    try {
      return new URL(url).hostname;
    } catch {
      return url;
    }
  };
  return (
    <div className="src-cards">
      <div className="src-cards-head">
        <span>来源 ({sources.length})</span>
        {onSaveSources && (
          <button
            type="button"
            className="src-save"
            disabled={saved}
            onClick={() => {
              onSaveSources(sources);
              setSaved(true);
            }}
            title="将来源摘要写入 memory.md 资料节"
          >
            <IconBrain size={12} />
            <span>{saved ? '已存入' : '存入记忆'}</span>
          </button>
        )}
      </div>
      {sources.map((s) => {
        const safeUrl = sanitizeHttpUrl(s.url);
        if (!safeUrl) {
          return (
            <div key={s.id} className="src-card" title="链接不安全或格式非法,已屏蔽">
              <span className="src-id">[{s.id}]</span>
              <span className="src-title">{s.title || '无效链接'}</span>
              <span className="src-domain">已屏蔽</span>
            </div>
          );
        }
        return (
          <a
            key={s.id}
            className="src-card"
            href={safeUrl}
            target="_blank"
            rel="noreferrer"
            title={s.snippet}
          >
            <span className="src-id">[{s.id}]</span>
            <span className="src-title">{s.title || s.url}</span>
            <span className="src-domain">{domain(safeUrl)}</span>
          </a>
        );
      })}
    </div>
  );
}

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
  const label = isUser ? '我' : 'AI';
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
      <div className="avatar">{label}</div>
      <div className="content">
        <AttachmentCards attachments={msg.attachments} />
        {msg.content && (
          <div className="bubble">
            {reasoning && <ReasoningBlock text={reasoning} />}
            <div className="md">
              <MemoMarkdown content={msg.content} withCodeBlock />
            </div>
          </div>
        )}
        <SourceCards raw={msg.sources ?? null} onSaveSources={onSaveSources} />
        <div className="meta">
          {msg.model_id && <span>{msg.model_id}</span>}
          {msg.model_id && msg.created && ' · '}
          {msg.created && <span>{new Date(msg.created).toLocaleTimeString('zh-CN')}</span>}
          {!isUser && (msg.tokens ?? 0) > 0 && (
            <span
              className="tok-split"
              title={
                reasoning
                  ? `思考 ${estimateReasoningTokens(reasoning)} tok(估算) / 回答 ${msg.tokens} tok`
                  : `回答 ${msg.tokens} tok`
              }
            >
              {reasoning ? `思考 ${estimateReasoningTokens(reasoning)} / 回答 ${msg.tokens} tok` : `${msg.tokens} tok`}
            </span>
          )}
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
