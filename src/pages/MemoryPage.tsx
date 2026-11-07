import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import MemPanel from '../components/MemPanel';
import { LongTermMemoryPanel } from '../components/LongTermMemoryPanel';
import { IconBrain } from '../components/icons';
import { useSessionsStore } from '../stores/sessions';
import { sessionPath } from '../lib/workbench';

type Tab = 'native' | 'longterm';

/**
 * 记忆库统一入口(/memory):原生记忆(会话记忆)、长期记忆(知识图谱)两个分区。
 * 会话列表由主侧栏承担(点击写入 ?session=),本页不再内嵌列表,避免嵌套侧栏。
 * 旧深链兼容:/memory?tab=notes 与 ?id=xx 一律跳转知识库 /knowledge。
 */
export default function MemoryPage() {
  const nav = useNavigate();
  const [params] = useSearchParams();
  const { sessions, refresh } = useSessionsStore();
  const [selected, setSelected] = useState<string | null>(null);
  const tab: Tab = params.get('tab') === 'longterm' ? 'longterm' : 'native';

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 旧深链:?tab=notes 或 ?id=xx → 知识库(保留 id 定位)
  useEffect(() => {
    const t = params.get('tab');
    const id = params.get('id');
    if (t === 'notes' || id) {
      nav(id ? `/knowledge?id=${id}` : '/knowledge', { replace: true });
    }
  }, [params, nav]);

  /** 侧栏「记忆库」点选会话时,通过 ?session= 直接定位到对应记忆。 */
  useEffect(() => {
    const sid = params.get('session');
    if (sid) setSelected(sid);
  }, [params]);

  useEffect(() => {
    if (!selected && sessions.length > 0) {
      setSelected(sessions[0].id);
    }
  }, [sessions, selected]);

  /** 切换分区时同步地址,保证侧栏「记忆库」高亮与当前分区一致。 */
  const selectTab = (next: Tab) => {
    nav(`/memory?tab=${next}`, { replace: true });
  };

  return (
    <div className="memory-page-wrap">
      <div className="memory-tabs">
        <button
          type="button"
          className={`memory-tab${tab === 'native' ? ' on' : ''}`}
          onClick={() => selectTab('native')}
        >
          原生记忆
        </button>
        <button
          type="button"
          className={`memory-tab${tab === 'longterm' ? ' on' : ''}`}
          onClick={() => selectTab('longterm')}
        >
          长期记忆
        </button>
      </div>

      {tab === 'native' ? (
        <div className="memory-page">
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
