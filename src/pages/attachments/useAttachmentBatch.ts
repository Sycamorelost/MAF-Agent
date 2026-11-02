//! 附件批量清理状态机:勾选、分组全选、全选当前筛选、批量删除。

import { useState, type Dispatch, type SetStateAction } from 'react';
import { deleteAttachmentsBatch, type AttachmentEntry } from '../../lib/ipc';
import { delKey } from './shared';

export function useAttachmentBatch(
  entries: AttachmentEntry[] | null,
  setEntries: Dispatch<SetStateAction<AttachmentEntry[] | null>>,
  flash: (text: string) => void,
) {
  const [batchMode, setBatchMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirmBatch, setConfirmBatch] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const toggleBatch = () => {
    if (batchMode) {
      setSelected(new Set());
      setConfirmBatch(false);
    }
    setBatchMode((v) => !v);
  };

  const toggleSelect = (e: AttachmentEntry) => {
    const key = delKey(e);
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  /** 删除单条后同步移除其选中态,避免残留 key 被批量删除重复提交。 */
  const dropKey = (key: string) =>
    setSelected((prev) => {
      if (!prev.has(key)) return prev;
      const next = new Set(prev);
      next.delete(key);
      return next;
    });

  const groupAllSelected = (items: AttachmentEntry[]) => items.every((e) => selected.has(delKey(e)));

  /** 分组全选 / 取消本组(只影响本组,不动其他组的选择)。 */
  const toggleGroup = (items: AttachmentEntry[]) => {
    const keys = items.map(delKey);
    const all = groupAllSelected(items);
    setSelected((prev) => {
      const next = new Set(prev);
      for (const k of keys) {
        if (all) next.delete(k);
        else next.add(k);
      }
      return next;
    });
  };

  /** 全选当前筛选结果 */
  const selectAllShown = (items: AttachmentEntry[]) => setSelected(new Set(items.map(delKey)));

  /** 执行批量清理:失败项保留在列表中并给出原因。 */
  const runBatchDelete = async () => {
    if (!entries || selected.size === 0) {
      setConfirmBatch(false);
      return;
    }
    const items = entries.filter((e) => selected.has(delKey(e)));
    if (items.length === 0) {
      setSelected(new Set());
      setConfirmBatch(false);
      return;
    }
    setDeleting(true);
    try {
      const outcomes = await deleteAttachmentsBatch(
        items.map((e) => ({ message_id: e.message_id, path: e.meta.path })),
      );
      const okKeys = new Set(outcomes.filter((o) => o.ok).map((o) => `${o.message_id}:${o.path}`));
      setEntries((prev) => (prev ? prev.filter((e) => !okKeys.has(delKey(e))) : prev));
      const okCount = outcomes.filter((o) => o.ok).length;
      const failed = outcomes.filter((o) => !o.ok);
      setSelected((prev) => {
        const next = new Set<string>();
        for (const k of prev) if (!okKeys.has(k)) next.add(k);
        return next;
      });
      setConfirmBatch(false);
      if (failed.length > 0) {
        flash(`已删除 ${okCount} 个,${failed.length} 个失败: ${failed[0]?.error ?? '未知原因'}`);
      } else {
        flash(`已删除 ${okCount} 个附件`);
      }
    } catch (err) {
      flash('批量删除失败: ' + String(err));
    } finally {
      setDeleting(false);
    }
  };

  return {
    batchMode,
    selected,
    confirmBatch,
    deleting,
    setSelected,
    setConfirmBatch,
    toggleBatch,
    toggleSelect,
    dropKey,
    groupAllSelected,
    toggleGroup,
    selectAllShown,
    runBatchDelete,
  };
}
