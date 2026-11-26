import { useCallback, useMemo, useState } from 'react';
import { uiConfirm } from '../confirmService';
import { useSessionsStore } from '../../stores/sessions';
import type { SelectApi } from './types';

/**
 * 会话多选 + 批量操作状态。
 * 与 Sidebar 分离,避免主组件因批量逻辑继续膨胀(体积守卫)。
 */
export function useSessionBatch(showToast: (text: string) => void) {
  const [mode, setMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const store = useSessionsStore();

  const begin = useCallback((id?: string) => {
    setMode(true);
    setSelected(id ? new Set([id]) : new Set());
  }, []);

  const end = useCallback(() => {
    setMode(false);
    setSelected(new Set());
  }, []);

  const toggle = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const toggleAll = useCallback((ids: string[]) => {
    setSelected((prev) => (prev.size === ids.length ? new Set() : new Set(ids)));
  }, []);

  const ids = [...selected];

  /** 归档选中的未归档会话(归档仅隐藏,数据保留)。 */
  const archive = async () => {
    if (ids.length === 0) return;
    try {
      await store.archive(ids);
      showToast(`已归档 ${ids.length} 个会话`);
      end();
    } catch (e) {
      showToast(`归档失败:${String(e)}`);
    }
  };

  /** 恢复选中的已归档会话。 */
  const restore = async () => {
    if (ids.length === 0) return;
    try {
      await store.unarchive(ids);
      showToast(`已恢复 ${ids.length} 个会话`);
      end();
    } catch (e) {
      showToast(`恢复失败:${String(e)}`);
    }
  };

  /** 批量移入项目;null = 移出项目。 */
  const move = async (projectId: string | null) => {
    if (ids.length === 0) return;
    try {
      await store.setProjectMany(ids, projectId);
      showToast(projectId ? `已移动 ${ids.length} 个会话` : `已移出项目(${ids.length} 个会话)`);
      end();
    } catch (e) {
      showToast(`移动失败:${String(e)}`);
    }
  };

  /** 批量删除:先确认(不可撤销),再走与单条一致的清理链路。 */
  const remove = async () => {
    if (ids.length === 0) return;
    const ok = await uiConfirm({
      title: '删除会话',
      message: `将删除选中的 ${ids.length} 个会话及其消息、记忆与附件。\n此操作不可撤销。`,
      confirmText: '删除',
    });
    if (!ok) return;
    try {
      await store.removeMany(ids);
      showToast(`已删除 ${ids.length} 个会话`);
      end();
    } catch (e) {
      showToast(`删除失败:${String(e)}`);
    }
  };

  const sel: SelectApi = { mode, selected, begin, end, toggle, toggleAll };

  /** 每项目会话数(含已归档):删除项目会解绑全部会话,确认框数量须一致 */
  const projectCounts = useMemo(() => {
    const m: Record<string, number> = {};
    for (const s of [...store.sessions, ...store.archived]) {
      if (s.project_id) m[s.project_id] = (m[s.project_id] ?? 0) + 1;
    }
    return m;
  }, [store.sessions, store.archived]);

  return {
    sel,
    projectCounts,
    count: selected.size,
    archivedCount: ids.filter((id) => store.archived.some((s) => s.id === id)).length,
    activeCount: ids.filter((id) => !store.archived.some((s) => s.id === id)).length,
    archive,
    restore,
    move,
    remove,
  };
}
