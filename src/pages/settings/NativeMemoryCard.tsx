import { useEffect, useState } from 'react';
import MemPanel from '../../components/MemPanel';
import { IconChevronRight } from '../../components/icons';
import { useSessionsStore } from '../../stores/sessions';

/**
 * 记忆管理 · 原生记忆卡。
 * 默认折叠:展开才挂载 MemPanel(避免打开设置页就触发 memory.md 读取与 watch)。
 */
export function NativeMemoryCard() {
  const sessions = useSessionsStore((s) => s.sessions);
  const refresh = useSessionsStore((s) => s.refresh);
  const [selected, setSelected] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // 默认选中最近更新的会话(列表按 updated 排序,首个即最新)
  useEffect(() => {
    if (sessions.length === 0) {
      if (selected) setSelected(null);
      return;
    }
    if (!selected || !sessions.some((s) => s.id === selected)) {
      setSelected(sessions[0].id);
    }
  }, [sessions, selected]);

  return (
    <div className="set-card">
      <button
        type="button"
        className={`set-card-h set-card-toggle${open ? ' open' : ''}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <IconChevronRight
          size={14}
          className={`set-toggle-arrow${open ? ' open' : ''}`}
        />
        原生记忆(会话 memory.md)
        <span className="st">{sessions.length} 个会话</span>
      </button>

      {open && (
        <div className="set-card-b">
          <p className="set-desc" style={{ marginTop: 0 }}>
            每个会话独占一份 memory.md,对话后自动提炼;可在此直接编辑、查看版本或回滚。
          </p>
          {sessions.length === 0 ? (
            <div className="set-hint">还没有会话,先去对话页聊一句吧</div>
          ) : (
            <>
              <select
                className="pg-field"
                value={selected ?? ''}
                onChange={(e) => setSelected(e.target.value || null)}
              >
                {sessions.map((s) => (
                  <option key={s.id} value={s.id}>
                    {(s.title || '(未命名会话)') + ` · v${s.memory_version}`}
                  </option>
                ))}
              </select>
              {selected && (
                <div className="set-native-mem">
                  <MemPanel sessionId={selected} />
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
