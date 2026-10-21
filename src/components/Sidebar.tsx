import { useEffect } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useSessionsStore } from '../stores/sessions';
import { useUiStore } from '../stores/ui';
import { useState } from 'react';
import { IconX } from './icons';
import type { SessionRow } from '../lib/ipc';

/** 会话按最近更新时间分组(今天 / 昨天 / 更早),对齐模板 01 的分组侧栏。 */
function groupLabel(iso: string): string {
  const t = new Date(iso.replace(' ', 'T')).getTime();
  if (Number.isNaN(t)) return '更早';
  const now = new Date();
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (t >= todayStart) return '今天';
  if (t >= todayStart - 86400000) return '昨天';
  return '更早';
}

const ORDER = ['今天', '昨天', '更早'];

function groupSessions(sessions: SessionRow[]) {
  const map = new Map<string, SessionRow[]>();
  for (const s of sessions) {
    const label = groupLabel(s.updated);
    const arr = map.get(label) ?? [];
    arr.push(s);
    map.set(label, arr);
  }
  return ORDER.filter((l) => map.has(l)).map((label) => ({ label, items: map.get(label)! }));
}

/**
 * 会话历史抽屉(简洁模式):
 * 不占布局列,由标题栏玻璃 pill 开关,固定在左上角下方;
 * 打开后持久保持(挤出式:主区自动让位),点 pill / ✕ / Esc 关闭。
 */
export default function Sidebar() {
  const { sessions, remove, rename } = useSessionsStore();
  const nav = useNavigate();
  const { id: activeId } = useParams();
  const open = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);
  /** 正在内联重命名的会话 id(null = 无) */
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState('');

  // 挤出式:挂 body class 让主区让位;卸载/关闭时移除
  useEffect(() => {
    document.body.classList.toggle('sidebar-open', open);
    return () => document.body.classList.remove('sidebar-open');
  }, [open]);

  // Esc 关闭抽屉
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSidebarOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, setSidebarOpen]);

  const startRename = (s: SessionRow) => {
    setEditingId(s.id);
    setEditingText(s.title || '');
  };

  const commitRename = async () => {
    if (!editingId) return;
    const t = editingText.trim();
    const id = editingId;
    setEditingId(null);
    if (t) await rename(id, t);
  };

  const groups = groupSessions(sessions);

  return (
    <>
      <div className={open ? 'sidebar open' : 'sidebar'} aria-hidden={!open}>
        <div className="side-head">
          <h1>会话</h1>
          <button className="panel-toggle" title="关闭" onClick={() => setSidebarOpen(false)}>
            <IconX size={14} />
          </button>
        </div>
        <div className="side-list">
          {sessions.length === 0 && (
            <div className="side-empty">还没有会话，点顶栏「新建对话」或直接在主页输入开始。</div>
          )}
          {groups.map((g) => (
            <div className="side-section" key={g.label}>
              <div className="side-label">{g.label}</div>
              {g.items.map((s) => (
                <div
                  key={s.id}
                  className={`side-item ${s.id === activeId ? 'on' : ''}`}
                  onClick={() => {
                    if (editingId !== s.id) {
                      nav(`/chat/${s.id}`);
                      setSidebarOpen(false);
                    }
                  }}
                  onDoubleClick={(e) => {
                    e.stopPropagation();
                    startRename(s);
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    if (confirm(`删除会话「${s.title}」?`)) {
                      remove(s.id);
                      if (s.id === activeId) nav('/');
                    }
                  }}
                  title={`${s.title}\n双击重命名 · 右键删除`}
                >
                  <span className="md-dot">M</span>
                  {editingId === s.id ? (
                    <input
                      className="side-rename"
                      value={editingText}
                      autoFocus
                      onFocus={(e) => e.target.select()}
                      onChange={(e) => setEditingText(e.target.value)}
                      onBlur={() => void commitRename()}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void commitRename();
                        if (e.key === 'Escape') setEditingId(null);
                      }}
                      onClick={(e) => e.stopPropagation()}
                    />
                  ) : (
                    <span className="txt">{s.title || s.id}</span>
                  )}
                </div>
              ))}
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
