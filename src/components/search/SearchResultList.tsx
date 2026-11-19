import type { ReactNode } from 'react';
import type { SearchResults } from '../../lib/ipc';
import { KIND_LABEL } from '../../pages/attachments/shared';
import { IconAttach, IconBrain, IconChat, IconNote } from '../icons';

/** 搜索结果统一条目(分组渲染 + 键盘导航共用) */
export type SearchItem =
  | { kind: 'session'; key: string; sessionId: string; title: string; meta: string }
  | {
      kind: 'message';
      key: string;
      sessionId: string;
      title: string;
      snippet: string;
      messageId: number;
      branchId: string;
      meta: string;
    }
  | { kind: 'memory'; key: string; sessionId: string; title: string; snippet: string }
  | { kind: 'note'; key: string; noteId: string; title: string; snippet: string; meta: string }
  | {
      kind: 'attachment';
      key: string;
      sessionId: string;
      title: string;
      snippet: string;
      messageId: number;
      branchId: string;
      meta: string;
    };

const GROUP_LABEL: Record<SearchItem['kind'], string> = {
  session: '会话',
  message: '消息',
  memory: '记忆',
  note: '知识库',
  attachment: '附件',
};

const fmtTime = (s: string) => {
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
};

/** 后端结果拍平为可导航列表,顺序即分组展示顺序 */
export function buildSearchItems(res: SearchResults): SearchItem[] {
  const out: SearchItem[] = [];
  for (const s of res.sessions) {
    out.push({
      kind: 'session',
      key: `s-${s.id}`,
      sessionId: s.id,
      title: s.title || s.id,
      meta: fmtTime(s.updated),
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
      meta: `${m.role === 'user' ? '我' : 'AI'} · ${fmtTime(m.created)}`,
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
      meta: fmtTime(n.updated),
    });
  }
  for (const a of res.attachments) {
    out.push({
      kind: 'attachment',
      key: `att-${a.path}`,
      sessionId: a.session_id,
      title: a.name,
      snippet: a.snippet,
      messageId: a.message_id,
      branchId: a.branch_id,
      meta: `${KIND_LABEL[a.kind as keyof typeof KIND_LABEL] ?? a.kind} · 来源:${a.session_title || '未知会话'}`,
    });
  }
  return out;
}

const iconOf = (kind: SearchItem['kind']): ReactNode => {
  switch (kind) {
    case 'memory':
      return <IconBrain size={14} className="gs-item-icon" />;
    case 'note':
      return <IconNote size={14} className="gs-item-icon" />;
    case 'attachment':
      return <IconAttach size={14} className="gs-item-icon" />;
    default:
      return <IconChat size={14} className="gs-item-icon" />;
  }
};

interface Props {
  items: SearchItem[];
  activeIdx: number;
  onActive: (idx: number) => void;
  onPick: (item: SearchItem) => void;
  loading?: boolean;
  error?: string | null;
  query: string;
}

/** 搜索面板结果区:按来源分组展示,方向键/Enter 由父组件处理 */
export default function SearchResultList({
  items,
  activeIdx,
  onActive,
  onPick,
  loading,
  error,
  query,
}: Props) {
  if (loading && items.length === 0) {
    return <div className="palette-empty">搜索中…</div>;
  }
  if (error) {
    return <div className="palette-empty">搜索失败:{error}</div>;
  }
  if (items.length === 0) {
    return <div className="palette-empty">没有匹配「{query}」的内容</div>;
  }
  const counts = new Map<SearchItem['kind'], number>();
  for (const it of items) counts.set(it.kind, (counts.get(it.kind) ?? 0) + 1);
  return (
    <>
      {items.map((it, i) => (
        <div key={it.key}>
          {(i === 0 || items[i - 1].kind !== it.kind) && (
            <div className="palette-group">
              {GROUP_LABEL[it.kind]}
              <span className="palette-group-count">{counts.get(it.kind)}</span>
            </div>
          )}
          <button
            type="button"
            className={`palette-item palette-result ${i === activeIdx ? 'active' : ''}`}
            onMouseEnter={() => onActive(i)}
            onClick={() => onPick(it)}
          >
            <span className="palette-result-body">
              <span className="palette-result-title">
                {iconOf(it.kind)}
                <span className="palette-item-title">{it.title}</span>
              </span>
              {'snippet' in it && it.snippet && (
                <span className="palette-result-snippet">{it.snippet}</span>
              )}
              {'meta' in it && it.meta && <span className="palette-result-meta">{it.meta}</span>}
            </span>
          </button>
        </div>
      ))}
    </>
  );
}
