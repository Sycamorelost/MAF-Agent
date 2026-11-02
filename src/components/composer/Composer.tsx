import { useRef, useEffect, useState, type ReactNode } from 'react';
import { searchAll, MAX_ATTACHMENTS, type AttachmentMeta } from '../../lib/ipc';
import {
  collectDroppedFiles,
  entriesOf,
  screenFiles,
  skipMessage,
  type SkippedFile,
} from '../../lib/attachmentDrop';
import { loadAttachmentPolicy } from '../../lib/attachmentPolicy';
import { registerAtRef } from '../../lib/atRefs';
import AtMenu, { findAtTrigger, type AtItem } from './AtMenu';
import AttachTray from './AttachTray';
import QuoteTray from './QuoteTray';
import ToolBar, { type ThinkingControl } from './ToolBar';
import { useComposerHeight } from './useComposerHeight';
import type { QuoteRef } from '../../lib/quote';

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
  /** 选择/拖入/粘贴的文件回调(input[type=file] 内部处理)。
   *  notice 为跳过项提示(黑名单、阈值、空文件),无跳过时为 undefined;
   *  rejected 为逐文件拒绝明细,供调用方留存拒绝原因 */
  onPickFiles?: (files: File[], notice?: string, rejected?: SkippedFile[]) => void;
  onRemoveAttachment?: (index: number) => void;
  /** 待发送引用(完整原文存 state,输入框只显示摘要) */
  quotes?: QuoteRef[];
  onRemoveQuote?: (id: string) => void;
  /** 输入框为空时按 ↑ 召回上一条用户消息 */
  onRecall?: () => void;
  /** 联网搜索开关(会话级持久化在 sessions.web_search_enabled) */
  webSearch?: boolean;
  onToggleWebSearch?: () => void;
  /** 思考控制(v1.9.0 工作块 I):null / enabled=false 时不渲染。
   *  levels=false 仅开关(四档不可用);can_disable=false 锁定为开。 */
  thinking?: ThinkingControl | null;
  /** 本会话模型选择器(工具条右端,发送按钮前) */
  model?: { name: string; ready: boolean; onOpen: () => void } | null;
  /** 追加到工具条的自定义 chip(智能体 / 会话设置等) */
  children?: ReactNode;
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
  quotes,
  onRemoveQuote,
  onRecall,
  webSearch,
  onToggleWebSearch,
  thinking,
  model,
  children,
}: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const { boxH, resizing, onResizeDown, onResizeMove, onResizeUp } = useComposerHeight(ref);

  /** 从 clipboard event 中抽取 file items,交给统一的提交流程。 */
  const extractPasteFiles = (e: React.ClipboardEvent): File[] => {
    if (!onPickFiles || !e.clipboardData) return [];
    const items = e.clipboardData.items;
    const files: File[] = [];
    for (const item of Array.from(items)) {
      if (item.kind === 'file') {
        const f = item.getAsFile();
        if (f) files.push(f);
      }
    }
    return files;
  };

  /** 统一提交(选择/粘贴):逐个筛掉不允许的文件,其余照常加入,拒绝项逐个提示。 */
  const submitPicked = async (files: File[]) => {
    if (!onPickFiles || files.length === 0) return;
    const policy = await loadAttachmentPolicy();
    const { ok, skipped } = screenFiles(files, policy.blockedExt);
    const remaining = Math.max(0, MAX_ATTACHMENTS - (attachments?.length ?? 0));
    const kept = ok.slice(0, remaining);
    const notes =
      ok.length > kept.length
        ? [`受单条消息 ${MAX_ATTACHMENTS} 个附件上限约束,${ok.length - kept.length} 个文件未加入`]
        : [];
    onPickFiles(kept, skipMessage(skipped, notes), skipped);
  };

  /** 拖入:文件夹递归展开(受单文件夹阈值与消息附件总数约束),再走统一提交。 */
  const handleDrop = async (e: React.DragEvent) => {
    if (!onPickFiles) return;
    // DataTransfer 在 await 之后可能被清空,先同步取出条目与文件
    const entries = entriesOf(e.dataTransfer);
    const plain = Array.from(e.dataTransfer.files ?? []);
    if (entries.length === 0 && plain.length === 0) return;
    e.preventDefault();
    try {
      const policy = await loadAttachmentPolicy();
      const remaining = Math.max(0, MAX_ATTACHMENTS - (attachments?.length ?? 0));
      const { files, skipped, notices } = await collectDroppedFiles(entries, plain, {
        folderLimit: policy.folderLimit,
        totalLimit: remaining,
        blockedExt: policy.blockedExt,
      });
      onPickFiles(files, skipMessage(skipped, notices), skipped);
    } catch (err) {
      onPickFiles([], `读取拖入的文件失败:${err instanceof Error ? err.message : String(err)}`);
    }
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
        void handleDrop(e);
      }}
    >
      <div className="composer-box">
        {/* 可拖拽的顶部把手:上下拖动调整输入框高度 */}
        <div
          className={`composer-resize${resizing ? ' dragging' : ''}`}
          title="拖动调整输入框高度"
          onPointerDown={onResizeDown}
          onPointerMove={onResizeMove}
          onPointerUp={onResizeUp}
        />
        {quotes && quotes.length > 0 && (
          <QuoteTray quotes={quotes} onRemoveQuote={onRemoveQuote} />
        )}
        {hasAttachments && (
          <AttachTray attachments={attachments!} onRemoveAttachment={onRemoveAttachment} />
        )}
        <textarea
          ref={ref}
          rows={2}
          style={boxH ? { height: boxH } : undefined}
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            syncAtTrigger(e.target.value, e.target.selectionStart ?? e.target.value.length);
          }}
          onPaste={(e) => {
            const files = extractPasteFiles(e);
            if (files.length === 0) return;
            e.preventDefault();
            void submitPicked(files);
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
          placeholder={placeholder ?? ''}
          disabled={disabled}
        />
        {atState && (
          <AtMenu
            query={atState.query}
            items={atItems}
            active={atActive}
            onActive={setAtActive}
            onPick={pickAt}
          />
        )}
        <ToolBar
          disabled={disabled}
          onPickFiles={(files) => void submitPicked(files)}
          webSearch={webSearch}
          onToggleWebSearch={onToggleWebSearch}
          thinking={thinking}
          sending={sending}
          onStop={onStop}
          canSend={!disabled && (!!value.trim() || hasAttachments)}
          onSend={doSend}
          model={model}
        >
          {children}
        </ToolBar>
      </div>
    </div>
  );
}
