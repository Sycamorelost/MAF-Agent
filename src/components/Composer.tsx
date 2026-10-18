import { useRef, useEffect, useState, type ReactNode } from 'react';
import { searchAll, type AttachmentMeta, type ReasoningCapability } from '../lib/ipc';
import { registerAtRef } from '../lib/atRefs';
import { THINKING_LEVEL_LABEL, effortHint } from '../lib/thinkingHint';
import type { ThinkingLevel, ThinkingSelection } from '../stores/thinking';
import { IconAttach, IconBrain, IconNote, IconX, IconDoc, IconImage, IconGlobe, IconSend, IconChevronDown } from './icons';

/** F9:光标前活跃的 @ 触发词(@ 须在行首/空白后,其后 ≥1 个非空白字符)。 */
function findAtTrigger(text: string, cursor: number): { start: number; query: string } | null {
  let i = cursor - 1;
  while (i >= 0) {
    const ch = text[i];
    if (ch === '@') {
      if (i === 0 || /\s/.test(text[i - 1])) {
        const query = text.slice(i + 1, cursor);
        if (query.length >= 1) return { start: i, query };
      }
      return null;
    }
    if (/\s/.test(ch)) return null;
    i--;
  }
  return null;
}

/** F9 候选项:记忆(其他会话 memory.md 命中)或知识库笔记。 */
interface AtItem {
  kind: 'memory' | 'note';
  key: string;
  title: string;
  snippet: string;
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  disabled?: boolean;
  placeholder?: string;
  /** 对话页底部样式(带分隔线、不自动顶到底);缺省为模板 .composer(首页 hero) */
  variant?: 'docked';
  /** 发送中:发送按钮替换为「停止」 */
  sending?: boolean;
  onStop?: () => void;
  /** 待发送附件列表 */
  attachments?: AttachmentMeta[];
  /** 选择文件回调(input[type=file] 内部处理) */
  onPickFiles?: (files: FileList) => void;
  onRemoveAttachment?: (index: number) => void;
  /** 输入框为空时按 ↑ 召回上一条用户消息 */
  onRecall?: () => void;
  /** 联网搜索开关(会话级持久化在 sessions.web_search_enabled) */
  webSearch?: boolean;
  onToggleWebSearch?: () => void;
  /** 思考控制(v1.9.0 工作块 I):null / enabled=false 时不渲染。
   *  levels=false 仅开关(四档不可用);can_disable=false 锁定为开。 */
  thinking?: {
    capability: ReasoningCapability;
    providerId: string;
    model: string;
    selection: ThinkingSelection;
    onSelect: (sel: ThinkingSelection) => void;
  } | null;
  /** 追加到工具条的自定义 chip(智能体 / 会话设置等) */
  children?: ReactNode;
}

const ACCEPT =
  '.docx,.xlsx,.xls,.pdf,.txt,.md,.markdown,.csv,.tsv,.log,.json,.yaml,.yml,.png,.jpg,.jpeg,.gif,.webp,.bmp';

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / 1024 / 1024).toFixed(1)}M`;
}

export default function Composer({
  value,
  onChange,
  onSend,
  disabled,
  placeholder,
  variant,
  sending,
  onStop,
  attachments,
  onPickFiles,
  onRemoveAttachment,
  onRecall,
  webSearch,
  onToggleWebSearch,
  thinking,
  children,
}: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const [thinkMenu, setThinkMenu] = useState(false);

  /** 从 clipboard event 中抽取 file items,转成 FileList 供 onPickFiles 统一处理。 */
  const extractPasteFiles = (e: React.ClipboardEvent): FileList | null => {
    if (!onPickFiles || !e.clipboardData) return null;
    const items = e.clipboardData.items;
    const files: File[] = [];
    for (const item of Array.from(items)) {
      if (item.kind === 'file') {
        const f = item.getAsFile();
        if (f && f.size > 0) files.push(f);
      }
    }
    if (files.length === 0) return null;
    const dt = new DataTransfer();
    files.forEach((f) => dt.items.add(f));
    return dt.files;
  };

  // ---------- F9:@ 召回(记忆 + 知识库) ----------
  const [atState, setAtState] = useState<{ start: number; query: string } | null>(null);
  const [atItems, setAtItems] = useState<AtItem[]>([]);
  const [atActive, setAtActive] = useState(0);
  const cursorRef = useRef(0);
  const atTimer = useRef<number>(0);

  /** 输入或光标移动后重新探测 @ 触发词。 */
  const syncAtTrigger = (text: string, cursor: number) => {
    cursorRef.current = cursor;
    setAtState(findAtTrigger(text, cursor));
  };

  // 去抖 200ms 调 search_all,仅取记忆 + 知识库两组
  useEffect(() => {
    window.clearTimeout(atTimer.current);
    if (!atState) {
      setAtItems([]);
      return;
    }
    const kw = atState.query;
    atTimer.current = window.setTimeout(() => {
      searchAll(kw)
        .then((r) => {
          setAtItems([
            ...r.memory.map((h) => ({
              kind: 'memory' as const,
              key: `mem-${h.session_id}`,
              title: h.session_title || h.session_id,
              snippet: h.snippet,
            })),
            ...r.notes.map((n) => ({
              kind: 'note' as const,
              key: `note-${n.id}`,
              title: n.title || `笔记 ${n.id}`,
              snippet: n.snippet,
            })),
          ]);
          setAtActive(0);
        })
        .catch(() => setAtItems([]));
    }, 200);
    return () => window.clearTimeout(atTimer.current);
  }, [atState]);

  /** 确认候选:占位标记替换 @query 段,注册片段供发送前解析注入。 */
  const pickAt = (item: AtItem) => {
    if (!atState) return;
    const placeholder = item.kind === 'note' ? `[@note:${item.title}]` : `[@memory:${item.title}]`;
    const cursor = cursorRef.current;
    const next = value.slice(0, atState.start) + placeholder + value.slice(cursor);
    registerAtRef(
      placeholder,
      item.kind === 'note' ? `知识库 / ${item.title}` : `记忆 / ${item.title}`,
      item.snippet || '(无片段内容)',
    );
    setAtState(null);
    setAtItems([]);
    onChange(next);
    const pos = atState.start + placeholder.length;
    requestAnimationFrame(() => {
      ref.current?.focus();
      ref.current?.setSelectionRange(pos, pos);
      cursorRef.current = pos;
    });
  };

  useEffect(() => {
    ref.current?.focus();
  }, []);

  const doSend = () => {
    const v = value.trim();
    if ((!v && (attachments?.length ?? 0) === 0) || disabled) return;
    onSend();
  };

  const hasAttachments = (attachments?.length ?? 0) > 0;

  const [dragOver, setDragOver] = useState(false);

  return (
    <div
      className={`composer${variant === 'docked' ? ' composer-docked' : ''}${dragOver ? ' composer-drag' : ''}`}
      onDragOver={(e) => {
        if (!onPickFiles) return;
        // 无条件 preventDefault:WebView2 在 dragover 阶段 dataTransfer.types
        // 经常为空,不能用它来判断是否调用 preventDefault,否则 drop 会被浏览器静默取消
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
        setDragOver(true);
      }}
      onDragLeave={(e) => {
        // 仅当离开整个 composer(非子元素间穿梭)才清状态
        if (e.currentTarget.contains(e.relatedTarget as Node)) return;
        setDragOver(false);
      }}
      onDrop={(e) => {
        setDragOver(false);
        if (!onPickFiles) return;
        if (e.dataTransfer.files.length > 0) {
          e.preventDefault();
          onPickFiles(e.dataTransfer.files);
        }
      }}
    >
      <div className="composer-box">
        {hasAttachments && (
          <div className="attach-list">
            {attachments!.map((a, i) => (
              <span key={`${a.path}-${i}`} className="attach-chip" title={a.name}>
                {a.kind === 'image' ? <IconImage size={13} /> : <IconDoc size={13} />}
                <span className="attach-name">{a.name}</span>
                <span className="attach-size">{fmtSize(a.size)}</span>
                {onRemoveAttachment && (
                  <button
                    type="button"
                    className="attach-x"
                    onClick={() => onRemoveAttachment(i)}
                    title="移除附件"
                  >
                    <IconX size={11} />
                  </button>
                )}
              </span>
            ))}
          </div>
        )}
        <textarea
          ref={ref}
          rows={2}
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            syncAtTrigger(e.target.value, e.target.selectionStart ?? e.target.value.length);
          }}
          onPaste={(e) => {
            const fl = extractPasteFiles(e);
            if (fl) {
              e.preventDefault();
              onPickFiles!(fl);
            }
          }}
          onSelect={(e) => {
            const t = e.currentTarget;
            syncAtTrigger(t.value, t.selectionStart ?? t.value.length);
          }}
          onKeyDown={(e) => {
            // F9:候选浮层打开时,↑↓/Enter/Tab/Esc 优先归属浮层交互
            if (atState) {
              if (e.key === 'ArrowDown' && atItems.length > 0) {
                e.preventDefault();
                setAtActive((i) => Math.min(i + 1, atItems.length - 1));
                return;
              }
              if (e.key === 'ArrowUp' && atItems.length > 0) {
                e.preventDefault();
                setAtActive((i) => Math.max(i - 1, 0));
                return;
              }
              if ((e.key === 'Enter' || e.key === 'Tab') && atItems[atActive]) {
                e.preventDefault();
                pickAt(atItems[atActive]);
                return;
              }
              if (e.key === 'Escape') {
                e.preventDefault();
                setAtState(null);
                setAtItems([]);
                return;
              }
            }
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              doSend();
            } else if (e.key === 'ArrowUp' && !value && onRecall) {
              // 仅当输入框为空时召回上一条用户消息
              e.preventDefault();
              onRecall();
            }
          }}
          placeholder={placeholder ?? '输入消息,Enter 发送,Shift+Enter 换行,@ 召回记忆/笔记'}
          disabled={disabled}
        />
        {atState && (
          <div className="at-dropdown">
            {atItems.length === 0 ? (
              <div className="gs-item empty">没有匹配「{atState.query}」的记忆/笔记</div>
            ) : (
              atItems.map((it, i) => (
                <div key={it.key}>
                  {(i === 0 || it.kind !== atItems[i - 1].kind) && (
                    <div className="gs-group-label">{it.kind === 'memory' ? '记忆' : '知识库'}</div>
                  )}
                  <div
                    className={`gs-item ${i === atActive ? 'active' : ''}`}
                    onMouseEnter={() => setAtActive(i)}
                    onClick={() => pickAt(it)}
                  >
                    <div className="gs-item-row">
                      {it.kind === 'memory' ? (
                        <IconBrain size={14} className="gs-item-icon" />
                      ) : (
                        <IconNote size={14} className="gs-item-icon" />
                      )}
                      <div className="gs-item-title">{it.title}</div>
                    </div>
                    {it.snippet && <div className="gs-item-snippet">{it.snippet}</div>}
                  </div>
                </div>
              ))
            )}
          </div>
        )}
        <div className="composer-bar">
          {onPickFiles && (
            <>
              <button
                type="button"
                className="tool-chip"
                onClick={() => fileRef.current?.click()}
                disabled={disabled}
                title="添加附件(docx / xlsx / pdf / 图片)"
              >
                <IconAttach size={13} />
                <span>附件</span>
              </button>
              <input
                ref={fileRef}
                type="file"
                multiple
                accept={ACCEPT}
                style={{ display: 'none' }}
                onChange={(e) => {
                  if (e.target.files?.length) onPickFiles(e.target.files);
                  e.target.value = '';
                }}
              />
            </>
          )}
          {onToggleWebSearch && (
            <button
              type="button"
              className={`tool-chip ${webSearch ? 'on' : ''}`}
              onClick={onToggleWebSearch}
              disabled={disabled}
              title={webSearch ? '联网搜索已开启(点击关闭)' : '开启联网搜索'}
            >
              <IconGlobe size={13} />
              <span>联网</span>
            </button>
          )}
          {thinking && thinking.capability.enabled && (() => {
            const cap = thinking.capability;
            const sel = thinking.selection;
            // 仅开关模型(如 Kimi K2.6 / GLM-4.5):无档位差异,四档控件不可用
            const noLevels = !cap.levels;
            // 不可关模型(如 Kimi K3 / GLM-5.3):锁定为开
            const locked = !cap.can_disable;
            const chipTitle = locked
              ? noLevels
                ? '该模型始终开启思考'
                : '该模型始终开启思考,可选择强度'
              : noLevels
                ? sel.on ? '思考已开启(点击关闭)' : '开启思考'
                : '选择思考强度(展示实际发送参数)';
            return (
              <span className="pin-wrap">
                <button
                  type="button"
                  className={`tool-chip ${sel.on ? 'on' : ''}`}
                  onClick={() => {
                    if (noLevels) {
                      if (cap.can_disable) thinking.onSelect({ ...sel, on: !sel.on });
                      return;
                    }
                    setThinkMenu((v) => !v);
                  }}
                  disabled={disabled || (locked && noLevels)}
                  title={chipTitle}
                >
                  <IconBrain size={13} />
                  <span>思考{!noLevels && sel.on ? ` · ${THINKING_LEVEL_LABEL[sel.level]}` : ''}</span>
                  {!noLevels && <IconChevronDown size={11} />}
                </button>
                {thinkMenu && !noLevels && (
                  <>
                    <div className="pin-backdrop" onClick={() => setThinkMenu(false)} />
                    <div className="pin-menu think-menu">
                      <div className="pin-menu-title">思考强度(实际发送参数)</div>
                      {(['light', 'medium', 'high', 'ultra'] as ThinkingLevel[]).map((lv) => (
                        <button
                          key={lv}
                          type="button"
                          className={`think-item ${sel.on && sel.level === lv ? 'active' : ''}`}
                          onClick={() => {
                            thinking.onSelect({ on: true, level: lv });
                            setThinkMenu(false);
                          }}
                        >
                          <span>{THINKING_LEVEL_LABEL[lv]}</span>
                          <span className="hint">{effortHint(thinking.providerId, thinking.model, lv)}</span>
                        </button>
                      ))}
                      {cap.can_disable && (
                        <>
                          <div className="think-divider" />
                          <button
                            type="button"
                            className={`think-item ${!sel.on ? 'active' : ''}`}
                            onClick={() => {
                              thinking.onSelect({ ...sel, on: false });
                              setThinkMenu(false);
                            }}
                          >
                            <span>关闭思考</span>
                          </button>
                        </>
                      )}
                    </div>
                  </>
                )}
              </span>
            );
          })()}
          {children}
          <span className="spacer" />
          {sending && onStop ? (
            <button type="button" className="stop-btn" onClick={onStop}>
              停止
            </button>
          ) : (
            <button
              className="send"
              onClick={doSend}
              disabled={disabled || (!value.trim() && !hasAttachments)}
              title="发送"
            >
              <IconSend size={14} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
