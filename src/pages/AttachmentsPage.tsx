/**
 * 附件管理中心(/attachments,迭代 F7 / F7.1)。
 * 数据源:list_all_attachments 聚合全部历史消息附件(联表会话标题),按会话分组展示。
 * 操作:预览、插入当前会话、定位来源消息、删除、失败重试、批量清理;
 *       按会话展示配额用量;拒绝原因落盘后可在此查看与清空。
 */

import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  deleteAttachment,
  insertAttachmentToSession,
  readAttachmentData,
  retryAttachmentExtract,
  type AttachmentEntry,
} from '../lib/ipc';
import { useSessionsStore } from '../stores/sessions';
import { useAttachmentInsertStore } from '../stores/attachmentInsert';
import { sessionPath } from '../lib/workbench';
import { quotaLabel, quotaTone } from '../lib/attachmentQuota';
import { IconAttach, IconChat, IconCheck, IconSearch } from '../components/icons';
import { AttachmentsHeader } from './attachments/AttachmentsHeader';
import { AttachmentCard } from './attachments/AttachmentCard';
import { RejectLogModal } from './attachments/RejectLogModal';
import { ConfirmBatchModal, ImageLightbox, TextPreviewModal } from './attachments/AttachmentModals';
import { useAttachmentsData } from './attachments/useAttachmentsData';
import { useAttachmentBatch } from './attachments/useAttachmentBatch';
import { delKey, type Kind } from './attachments/shared';

export default function AttachmentsPage() {
  const nav = useNavigate();
  const sessions = useSessionsStore((s) => s.sessions);
  const setActiveBranch = useSessionsStore((s) => s.setActiveBranch);
  const enqueue = useAttachmentInsertStore((s) => s.enqueue);

  const [q, setQ] = useState('');
  const [kind, setKind] = useState<'all' | Kind>('all');
  const [lightbox, setLightbox] = useState<string | null>(null);
  const [textPreview, setTextPreview] = useState<{ name: string; text: string } | null>(null);
  const [confirmDel, setConfirmDel] = useState<string | null>(null);
  const [status, setStatus] = useState('');
  const [retrying, setRetrying] = useState<Set<string>>(new Set());
  const [rejectsOpen, setRejectsOpen] = useState(false);

  const flash = (text: string) => {
    setStatus(text);
    window.setTimeout(() => setStatus((s) => (s === text ? '' : s)), text.length > 40 ? 5000 : 2000);
  };

  const {
    entries,
    setEntries,
    quota,
    imgUrls,
    usageMap,
    rejects,
    rejectsLoading,
    clearingRejects,
    refreshRejects,
    clearRejects,
  } = useAttachmentsData(flash);
  const batch = useAttachmentBatch(entries, setEntries, flash);
  const { setConfirmBatch } = batch;

  /** Esc 关闭所有浮层(预览 / 批量确认 / 拒绝记录) */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      setLightbox(null);
      setTextPreview(null);
      setRejectsOpen(false);
      setConfirmBatch(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [setConfirmBatch]);

  // 删除两态确认 3 秒自动复位
  useEffect(() => {
    if (!confirmDel) return;
    const t = window.setTimeout(() => setConfirmDel(null), 3000);
    return () => window.clearTimeout(t);
  }, [confirmDel]);

  const shown = useMemo(() => {
    if (!entries) return [];
    const kw = q.trim().toLowerCase();
    return entries.filter(
      (e) =>
        (kind === 'all' || e.meta.kind === kind) && (!kw || e.meta.name.toLowerCase().includes(kw)),
    );
  }, [entries, q, kind]);

  /** 按会话分组(保持条目倒序,组序跟随首条出现顺序 = 最新组在前)。 */
  const groups = useMemo(() => {
    const map = new Map<string, { sessionId: string; title: string; items: AttachmentEntry[] }>();
    for (const e of shown) {
      let g = map.get(e.session_id);
      if (!g) {
        g = { sessionId: e.session_id, title: e.session_title || e.session_id, items: [] };
        map.set(e.session_id, g);
      }
      g.items.push(e);
    }
    return [...map.values()];
  }, [shown]);

  const onPreview = (e: AttachmentEntry) => {
    if (e.meta.kind === 'image') {
      const cached = imgUrls[e.meta.path];
      if (cached) {
        setLightbox(cached);
      } else {
        // 缩略尚未就绪时兜底读取大图
        readAttachmentData(e.meta.path)
          .then(setLightbox)
          .catch(() => flash('图片读取失败'));
      }
    } else if (e.meta.extracted_text) {
      setTextPreview({ name: e.meta.name, text: e.meta.extracted_text });
    }
  };

  /** 插入「当前会话」= 最近更新的会话;经队列交给 ChatPage 输入框。 */
  const onInsert = async (e: AttachmentEntry) => {
    const target = [...sessions].sort((a, b) => b.updated.localeCompare(a.updated))[0];
    if (!target) {
      flash('没有可用会话,请先新建会话');
      return;
    }
    try {
      const meta = await insertAttachmentToSession(target.id, e.meta.path);
      enqueue(target.id, [meta]);
      nav(sessionPath(target.id));
    } catch (err) {
      flash('插入失败: ' + String(err));
    }
  };

  const onLocate = (e: AttachmentEntry) => {
    const sess = sessions.find((s) => s.id === e.session_id);
    if (sess && sess.active_branch !== e.branch_id) {
      void setActiveBranch(e.session_id, e.branch_id).catch(() => {});
    }
    nav(`${sessionPath(e.session_id)}?msg=${e.message_id}`);
  };

  const onDelete = async (e: AttachmentEntry) => {
    const key = delKey(e);
    if (confirmDel !== key) {
      setConfirmDel(key);
      return;
    }
    setConfirmDel(null);
    try {
      await deleteAttachment(e.message_id, e.meta.path);
      setEntries(
        (prev) =>
          prev?.filter((x) => !(x.message_id === e.message_id && x.meta.path === e.meta.path)) ??
          null,
      );
      batch.dropKey(key);
      flash('已删除');
    } catch (err) {
      flash('删除失败: ' + String(err));
    }
  };

  /** 重试文本提取:成功后原地刷新卡片(失败时保留旧元数据并提示原因)。 */
  const onRetry = async (e: AttachmentEntry) => {
    const key = delKey(e);
    if (retrying.has(key)) return;
    setRetrying((prev) => new Set(prev).add(key));
    try {
      const meta = await retryAttachmentExtract(e.message_id, e.meta.path);
      setEntries(
        (prev) =>
          prev?.map((x) =>
            x.message_id === e.message_id && x.meta.path === e.meta.path ? { ...x, meta } : x,
          ) ?? null,
      );
      flash(meta.extract_note ? `重试后仍失败: ${meta.extract_note}` : '文本提取已重试成功');
    } catch (err) {
      flash('重试失败: ' + String(err));
    } finally {
      setRetrying((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  return (
    <div className="att-page">
      <AttachmentsHeader
        q={q}
        onQuery={setQ}
        kind={kind}
        onKind={setKind}
        batchMode={batch.batchMode}
        selectedCount={batch.selected.size}
        shownCount={shown.length}
        rejectTotal={rejects?.total ?? 0}
        deleting={batch.deleting}
        onToggleBatch={batch.toggleBatch}
        onSelectAll={() => batch.selectAllShown(shown)}
        onClearSelect={() => batch.setSelected(new Set())}
        onDeleteSelected={() => batch.setConfirmBatch(true)}
        onOpenRejects={() => {
          setRejectsOpen(true);
          void refreshRejects();
        }}
      />

      <div className="att-body">
        {entries === null && <div className="att-empty">加载中…</div>}
        {entries !== null && entries.length === 0 && (
          <div className="att-empty">
            <IconAttach size={36} strokeWidth={1.1} />
            <span>还没有附件,在会话中发送文件后会出现在这里</span>
          </div>
        )}
        {entries !== null && entries.length > 0 && shown.length === 0 && (
          <div className="att-empty">
            <IconSearch size={30} strokeWidth={1.1} />
            <span>没有匹配的附件</span>
          </div>
        )}
        {groups.map((g) => {
          const usage = usageMap.get(g.sessionId) ?? { files: 0, bytes: 0 };
          const tone = quotaTone(usage, quota.maxFiles, quota.maxMb);
          const allSelected = batch.groupAllSelected(g.items);
          return (
            <div key={g.sessionId} className="att-group">
              <div className="att-group-head">
                {batch.batchMode && (
                  <button
                    className={`att-select small${allSelected ? ' on' : ''}`}
                    title={allSelected ? '取消本组选择' : '选择本组全部'}
                    onClick={() => batch.toggleGroup(g.items)}
                  >
                    {allSelected && <IconCheck size={11} strokeWidth={2} />}
                  </button>
                )}
                <button
                  className="att-group-title"
                  onClick={() => nav(sessionPath(g.sessionId))}
                  title="打开会话"
                >
                  <IconChat size={14} />
                  <span className="t">{g.title}</span>
                  <span className="n">{g.items.length}</span>
                </button>
                <span
                  className={`att-quota-chip ${tone}`}
                  title="本会话附件配额(可在设置 · 存储位置 · 附件中调整)"
                >
                  {quotaLabel(usage, quota.maxFiles, quota.maxMb)}
                </span>
              </div>
              <div className="att-grid">
                {g.items.map((e) => (
                  <AttachmentCard
                    key={delKey(e)}
                    entry={e}
                    thumb={e.meta.kind === 'image' ? imgUrls[e.meta.path] : undefined}
                    selectMode={batch.batchMode}
                    selected={batch.selected.has(delKey(e))}
                    confirming={confirmDel === delKey(e)}
                    retrying={retrying.has(delKey(e))}
                    onPreview={onPreview}
                    onInsert={onInsert}
                    onLocate={onLocate}
                    onDelete={onDelete}
                    onToggleSelect={batch.toggleSelect}
                    onRetry={(x) => void onRetry(x)}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>

      {lightbox && <ImageLightbox src={lightbox} onClose={() => setLightbox(null)} />}

      {textPreview && (
        <TextPreviewModal
          name={textPreview.name}
          text={textPreview.text}
          onClose={() => setTextPreview(null)}
        />
      )}

      {batch.confirmBatch && (
        <ConfirmBatchModal
          count={batch.selected.size}
          deleting={batch.deleting}
          onCancel={() => batch.setConfirmBatch(false)}
          onConfirm={() => void batch.runBatchDelete()}
        />
      )}

      {rejectsOpen && (
        <RejectLogModal
          data={rejects}
          loading={rejectsLoading}
          clearing={clearingRejects}
          onClose={() => setRejectsOpen(false)}
          onRefresh={() => void refreshRejects()}
          onClear={() => void clearRejects()}
        />
      )}

      {status && <div className="att-toast">{status}</div>}
    </div>
  );
}
