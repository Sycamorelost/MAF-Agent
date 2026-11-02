import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import MemPanel from '../components/MemPanel';
import { LongTermMemoryPanel } from '../components/LongTermMemoryPanel';
import { IconBrain } from '../components/icons';
import { useSessionsStore } from '../stores/sessions';
import { sessionPath } from '../lib/workbench';

type Tab = 'native' | 'longterm';

/**
 * 记忆管理页(/memory):原生记忆(左会话列表 + 右 memory.md)与长期记忆(知识图谱)双视图。
 */
export default function MemoryPage() {
  const nav = useNavigate();
  const { sessions, refresh } = useSessionsStore();
  const [selected, setSelected] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('native');

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!selected && sessions.length > 0) {
      setSelected(sessions[0].id);
    }
  }, [sessions, selected]);

  return (
    <div className="memory-page-wrap">
      <div className="memory-tabs">
        <button
          type="button"
          className={`memory-tab${tab === 'native' ? ' on' : ''}`}
          onClick={() => setTab('native')}
        >
          原生记忆
        </button>
        <button
          type="button"
          className={`memory-tab${tab === 'longterm' ? ' on' : ''}`}
          onClick={() => setTab('longterm')}
        >
          长期记忆
        </button>
      </div>

      {tab === 'native' ? (
        <div className="memory-page">
          <div className="memory-list">
            <div className="memory-list-head">
              <span className="t">全部会话记忆</span>
            </div>
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
                <button className="btn ghost" onClick={() => nav(sessionPath(selected))}>
                  打开该会话
                </button>
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="memory-longterm">
          <h2>长期记忆(知识图谱)</h2>
          <p className="set-desc">
            由「长期记忆」MCP 工具跨会话共享的知识图谱;可按实体类型查看分布、关系网络与明细。
            数据保存在数据目录的 mcp-memory.jsonl。
          </p>
          <LongTermMemoryPanel />
        </div>
      )}
    </div>
  );
}
