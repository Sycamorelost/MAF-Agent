/**
 * P16:弹出式搜索面板(从 Sidebar 拆出,控制单文件体积)。
 * 空查询时展示最近会话;输入后走后端多源检索(会话/消息/记忆/知识库/附件),
 * 方向键上下移动、Enter 打开,跳转规则与旧 GlobalSearch 保持一致。
 */

import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { sessionPath } from '../../lib/workbench';
import { useSessionsStore } from '../../stores/sessions';
import type { SessionRow } from '../../lib/ipc';
import { IconSearch, IconX } from '../icons';
import { useSearchAll } from './useSearchAll';
import SearchResultList, { buildSearchItems, type SearchItem } from './SearchResultList';

export interface SearchPaletteProps {
  query: string;
  onQuery: (v: string) => void;
  sessions: SessionRow[];
  onClose: () => void;
  inputRef: React.MutableRefObject<HTMLInputElement | null>;
}

export default function SearchPalette({
  query, onQuery, sessions, onClose, inputRef,
}: SearchPaletteProps) {
  const nav = useNavigate();
  const kw = query.trim();
  const { res, loading, error } = useSearchAll(query);
  const [activeIdx, setActiveIdx] = useState(0);

  const items = useMemo(() => (kw ? buildSearchItems(res) : []), [kw, res]);
  const recent = useMemo(() => sessions.slice(0, 20), [sessions]);

  // 关键词变化后回到第一项,避免沿用旧列表的下标
  useEffect(() => {
    setActiveIdx(0);
  }, [kw]);

  const handleBg = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  };

  const jump = (it: SearchItem) => {
    onClose();
    if (it.kind === 'note') {
      nav(`/notes?id=${it.noteId}`);
      return;
    }
    if (it.kind === 'message' || it.kind === 'attachment') {
      // 命中消息可能属于非激活分支,先切换再跳转定位,否则目标不可见
      const sess = useSessionsStore.getState().sessions.find((s) => s.id === it.sessionId);
      if (it.branchId && sess && sess.active_branch !== it.branchId) {
        void useSessionsStore
          .getState()
          .setActiveBranch(it.sessionId, it.branchId)
          .catch(() => {});
      }
      nav(`${sessionPath(it.sessionId)}?msg=${it.messageId}`);
      return;
    }
    nav(sessionPath(it.sessionId));
  };

  return (
    <div className="palette-mask" onMouseDown={handleBg}>
      <div className="palette-panel" role="dialog" aria-label="全局搜索">
        <div className="palette-head">
          <IconSearch size={15} />
          <input
            ref={inputRef}
            type="text"
            className="palette-input"
            placeholder="搜索会话、消息、记忆、知识库、附件…"
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            onKeyDown={(e) => {
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
          />
          <button className="palette-close" onClick={onClose} title="关闭 (Esc)">
            <IconX size={14} />
          </button>
        </div>
        <div className="palette-list">
          {kw ? (
            <SearchResultList
              items={items}
              activeIdx={activeIdx}
              onActive={setActiveIdx}
              onPick={jump}
              loading={loading}
              error={error}
              query={kw}
            />
          ) : recent.length === 0 ? (
            <div className="palette-empty">还没有会话</div>
          ) : (
            recent.map((s) => (
              <button
                key={s.id}
                type="button"
                className="palette-item"
                onClick={() => {
                  onClose();
                  nav(sessionPath(s.id));
                }}
              >
                <span className="palette-item-title">{s.title || '(无标题)'}</span>
                {s.agent_id && <span className="palette-item-tag">{s.agent_id}</span>}
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
