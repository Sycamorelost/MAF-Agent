import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import MemPanel from '../components/MemPanel';
import AsideDrawer from '../components/AsideDrawer';
import { IconBrain } from '../components/icons';
import { useSessionsStore } from '../stores/sessions';

/**
 * 记忆管理页(/memory):与对话页区分开的全局记忆入口。
 * 左侧列出全部会话,右侧查看/编辑选中会话的 memory.md(复用 MemPanel,含版本历史)。
 */
export default function MemoryPage() {
  const nav = useNavigate();
  const { sessions, refresh } = useSessionsStore();
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 默认选中最近更新的会话
  useEffect(() => {
    if (!selected && sessions.length > 0) {
      setSelected(sessions[0].id);
    }
  }, [sessions, selected]);

  return (
    <div className="memory-page">
      <AsideDrawer>
      <div className="memory-list drawer">
        <div className="memory-list-head">
          <span className="t">全部会话记忆</span>
        </div>
        <div className="drawer-hint">点标题栏「面板」可开合本栏</div>
        <div className="memory-list-body">
          {sessions.length === 0 && (
            <div className="memory-empty">
              <IconBrain size={32} strokeWidth={1} />
              <span>还没有会话,先去对话页聊一句吧</span>
            </div>
          )}
          {sessions.map((s) => (
            <div
              key={s.id}
              className={`memory-item ${selected === s.id ? 'active' : ''}`}
              onClick={() => setSelected(s.id)}
            >
              <div className="memory-item-title">{s.title || '(未命名会话)'}</div>
              <div className="memory-item-meta">
                v{s.memory_version} · {new Date(s.updated).toLocaleDateString('zh-CN')}
              </div>
            </div>
          ))}
        </div>
      </div>
      </AsideDrawer>
      <div className="memory-detail">
        {selected ? (
          <MemPanel sessionId={selected} />
        ) : (
          <div className="memory-empty" style={{ flex: 1 }}>
            <IconBrain size={40} strokeWidth={1} />
            <span>从左侧选择一个会话查看它的 memory.md</span>
          </div>
        )}
        {selected && (
          <div className="memory-goto">
            <button className="btn ghost" onClick={() => nav(`/chat/${selected}`)}>
              打开该会话
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
