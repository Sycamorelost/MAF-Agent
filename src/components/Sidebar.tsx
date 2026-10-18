import { useNavigate, useParams } from 'react-router-dom';
import { useSessionsStore } from '../stores/sessions';
import { useProvidersStore } from '../stores/providers';
import { IconPlus, IconChevronDown, IconBrain } from './icons';
import { useState } from 'react';
import AgentPickerDialog from './AgentPickerDialog';
import { ResizeHandle } from './ResizeHandle';
import { usePanelWidth } from '../lib/usePanelWidth';
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

export default function Sidebar() {
  const { sessions, create, remove, rename, pruneEmpty } = useSessionsStore();
  const current = useProvidersStore((s) => s.current);
  const nav = useNavigate();
  const { id: activeId } = useParams();
  const [pickerOpen, setPickerOpen] = useState(false);
  const panel = usePanelWidth('sidebar', 248, 200, 420);
  /** 正在内联重命名的会话 id(null = 无) */
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState('');

  const onNew = async (agentId: string | null) => {
    setPickerOpen(false);
    // 空对话默认不保留:新建前先清掉遗留的空会话
    void pruneEmpty(60);
    const title = `新会话 ${new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    // 统一为 provider/model 格式,便于按会话回填模型(P2)
    const modelId = current ? `${current.providerId}/${current.model}` : '';
    const id = await create(title, modelId, agentId);
    nav(`/chat/${id}`);
  };

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
      <div className="sidebar" style={{ width: panel.width }} data-narrow={panel.narrow}>
        <button className="new-chat" onClick={() => setPickerOpen(true)}>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
          <IconPlus size={13} />
          新建对话
        </span>
        <IconChevronDown size={13} />
      </button>

      <div className="side-list">
        {sessions.length === 0 && (
          <div className="side-empty">还没有会话，点上方「新建对话」开始。</div>
        )}
        {groups.map((g) => (
          <div className="side-section" key={g.label}>
            <div className="side-label">{g.label}</div>
            {g.items.map((s) => (
              <div
                key={s.id}
                className={`side-item ${s.id === activeId ? 'on' : ''}`}
                onClick={() => {
                  if (editingId !== s.id) nav(`/chat/${s.id}`);
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

      <div className="side-foot">
        <div className="side-foot-card" onClick={() => nav('/memory')} title="查看本地记忆文件">
          <span className="row1">
            <IconBrain size={14} />
            记忆文件
          </span>
          <span className="spacer" />
          <span className="row2">
            <span className="stat-dot ok" />
            本地 data/
          </span>
        </div>
      </div>

      {pickerOpen && (
        <AgentPickerDialog
          currentAgentId={null}
          onSelect={onNew}
          onClose={() => setPickerOpen(false)}
        />
      )}
      </div>
      <ResizeHandle panel={panel} />
    </>
  );
}
