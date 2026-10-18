/**
 * 全局搜索框:位于 Topbar 左侧。
 * 后端 search_all:会话标题(LIKE)+ 消息全文(FTS5)+ memory.md 内容,分组展示并跳转。
 * 消息结果跳转 /chat/:id?msg=<messageId> 并高亮定位;记忆结果打开对应会话。
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { searchAll, type SearchResults } from '../lib/ipc';
import { useSessionsStore } from '../stores/sessions';
import { IconBrain, IconChat, IconNote, IconSearch, IconX } from './icons';

type Item =
  | { kind: 'session'; key: string; sessionId: string; title: string; meta: string }
  | { kind: 'message'; key: string; sessionId: string; title: string; snippet: string; messageId: number; branchId: string; meta: string }
  | { kind: 'memory'; key: string; sessionId: string; title: string; snippet: string }
  | { kind: 'note'; key: string; noteId: string; title: string; snippet: string; meta: string };

const EMPTY: SearchResults = { sessions: [], messages: [], memory: [], notes: [] };

export default function GlobalSearch() {
  const [q, setQ] = useState('');
  const [open, setOpen] = useState(false);
  const [activeIdx, setActiveIdx] = useState(0);
  const [res, setRes] = useState<SearchResults>(EMPTY);
  const nav = useNavigate();
  const boxRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<number>(0);

  // 去抖 200ms 调后端
  useEffect(() => {
    const kw = q.trim();
    window.clearTimeout(timerRef.current);
    if (!kw) {
      setRes(EMPTY);
      return;
    }
    timerRef.current = window.setTimeout(() => {
      searchAll(kw)
        .then((r) => {
          setRes(r);
          setActiveIdx(0);
        })
        .catch(() => setRes(EMPTY));
    }, 200);
    return () => window.clearTimeout(timerRef.current);
  }, [q]);

  // 拍平结果用于键盘导航(带分组标题渲染)
  const items = useMemo<Item[]>(() => {
    const out: Item[] = [];
    for (const s of res.sessions) {
      out.push({
        kind: 'session',
        key: `s-${s.id}`,
        sessionId: s.id,
        title: s.title || s.id,
        meta: new Date(s.updated).toLocaleString('zh-CN'),
      });
    }
    for (const m of res.messages) {
      out.push({
        kind: 'message',
        key: `m-${m.message_id}`,
        sessionId: m.session_id,
        title: m.session_title || m.session_id,
        snippet: m.snippet,
        messageId: m.message_id,
        branchId: m.branch_id,
        meta: `${m.role === 'user' ? '我' : 'AI'} · ${new Date(m.created).toLocaleString('zh-CN')}`,
      });
    }
    for (const h of res.memory) {
      out.push({
        kind: 'memory',
        key: `mem-${h.session_id}`,
        sessionId: h.session_id,
        title: h.session_title || h.session_id,
        snippet: h.snippet,
      });
    }
    for (const n of res.notes) {
      out.push({
        kind: 'note',
        key: `note-${n.id}`,
        noteId: n.id,
        title: n.title || `笔记 ${n.id}`,
        snippet: n.snippet,
        meta: new Date(n.updated).toLocaleString('zh-CN'),
      });
    }
    return out;
  }, [res]);

  // 外部点击关闭
  useEffect(() => {
    const onClickOutside = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onClickOutside);
    return () => document.removeEventListener('mousedown', onClickOutside);
  }, []);

  // 快捷键:Ctrl/Cmd + K 聚焦
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        inputRef.current?.focus();
        setOpen(true);
      }
      if (e.key === 'Escape') {
        setOpen(false);
        inputRef.current?.blur();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const jump = (item: Item) => {
    setOpen(false);
    setQ('');
    if (item.kind === 'note') {
      nav(`/notes?id=${item.noteId}`);
    } else if (item.kind === 'message') {
      // F4:命中消息可能属于非激活分支,先切换分支再跳转定位,否则目标不可见
      const sess = useSessionsStore.getState().sessions.find((s) => s.id === item.sessionId);
      if (item.branchId && sess && sess.active_branch !== item.branchId) {
        void useSessionsStore
          .getState()
          .setActiveBranch(item.sessionId, item.branchId)
          .catch(() => {});
      }
      nav(`/chat/${item.sessionId}?msg=${item.messageId}`);
    } else {
      nav(`/chat/${item.sessionId}`);
    }
  };

  const groupLabel = (kind: Item['kind']) =>
    kind === 'session'
      ? '会话'
      : kind === 'message'
        ? '消息'
        : kind === 'note'
          ? '知识库'
          : '记忆';

  // 是否需要在当前项前渲染分组标题
  const needHeader = (i: number) => i === 0 || items[i].kind !== items[i - 1].kind;

  return (
    <div className="global-search" ref={boxRef}>
      <IconSearch size={16} className="gs-icon" />
      <input
        ref={inputRef}
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (!open) return;
          if (e.key === 'ArrowDown') {
            e.preventDefault();
            setActiveIdx((i) => Math.min(i + 1, items.length - 1));
          } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            setActiveIdx((i) => Math.max(i - 1, 0));
          } else if (e.key === 'Enter' && items[activeIdx]) {
            e.preventDefault();
            jump(items[activeIdx]);
          }
        }}
        placeholder="搜索会话、消息、记忆…(Ctrl+K)"
        className="gs-input"
      />
      {q && (
        <button
          className="gs-clear"
          onClick={() => {
            setQ('');
            inputRef.current?.focus();
          }}
          title="清空"
        >
          <IconX size={14} />
        </button>
      )}

      {open && q.trim() && (
        <div className="gs-dropdown">
          {items.length === 0 && (
            <div className="gs-item empty">没有匹配「{q}」的内容</div>
          )}
          {items.map((it, i) => (
            <div key={it.key}>
              {needHeader(i) && <div className="gs-group-label">{groupLabel(it.kind)}</div>}
              <div
                className={`gs-item ${i === activeIdx ? 'active' : ''}`}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => jump(it)}
              >
                <div className="gs-item-row">
                  {it.kind === 'memory' ? (
                    <IconBrain size={14} className="gs-item-icon" />
                  ) : it.kind === 'note' ? (
                    <IconNote size={14} className="gs-item-icon" />
                  ) : (
                    <IconChat size={14} className="gs-item-icon" />
                  )}
                  <div className="gs-item-title">{it.title}</div>
                </div>
                {'snippet' in it && it.snippet && (
                  <div className="gs-item-snippet">{it.snippet}</div>
                )}
                {'meta' in it && it.meta && <div className="gs-item-meta">{it.meta}</div>}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
