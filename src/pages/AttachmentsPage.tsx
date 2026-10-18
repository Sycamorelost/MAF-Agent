/**
 * 附件管理中心(/attachments,迭代 F7)。
 * 数据源:list_all_attachments 聚合全部历史消息附件(联表会话标题),按会话分组展示。
 * 操作:预览(图片 lightbox / 文本弹窗)、插入当前会话(复用磁盘文件,经 insertQueue 交 ChatPage)、
 *       定位来源消息(自动切分支)、删除(联动消息 JSON 与磁盘文件,两态确认)。
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  deleteAttachment,
  insertAttachmentToSession,
  listAllAttachments,
  readAttachmentData,
  type AttachmentEntry,
  type AttachmentMeta,
} from '../lib/ipc';
import { useSessionsStore } from '../stores/sessions';
import { useAttachmentInsertStore } from '../stores/attachmentInsert';
import {
  IconAttach,
  IconChat,
  IconCheck,
  IconClipboardList,
  IconDoc,
  IconExternalLink,
  IconEye,
  IconFileText,
  IconImage,
  IconPlus,
  IconSearch,
  IconTrash,
  IconX,
} from '../components/icons';

type Kind = AttachmentMeta['kind'];

const KIND_LABEL: Record<Kind, string> = {
  image: '图片',
  docx: '文档',
  xlsx: '表格',
  pdf: 'PDF',
  text: '文本',
  other: '其他',
};

const KIND_FILTERS: Array<{ key: 'all' | Kind; label: string }> = [
  { key: 'all', label: '全部' },
  { key: 'image', label: '图片' },
  { key: 'docx', label: '文档' },
  { key: 'xlsx', label: '表格' },
  { key: 'pdf', label: 'PDF' },
  { key: 'text', label: '文本' },
  { key: 'other', label: '其他' },
];

function KindIcon({ kind }: { kind: Kind }) {
  switch (kind) {
    case 'image':
      return <IconImage size={26} strokeWidth={1.2} />;
    case 'xlsx':
      return <IconClipboardList size={26} strokeWidth={1.2} />;
    case 'text':
      return <IconFileText size={26} strokeWidth={1.2} />;
    case 'docx':
    case 'pdf':
      return <IconDoc size={26} strokeWidth={1.2} />;
    default:
      return <IconAttach size={26} strokeWidth={1.2} />;
  }
}

const fmtSize = (n: number) =>
  n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`;

const fmtDate = (s: string) => {
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
};

/** 删除确认 key:同一消息可能引用同路径两次,需组合区分。 */
const delKey = (e: AttachmentEntry) => `${e.message_id}:${e.meta.path}`;

export default function AttachmentsPage() {
  const nav = useNavigate();
  const sessions = useSessionsStore((s) => s.sessions);
  const setActiveBranch = useSessionsStore((s) => s.setActiveBranch);
  const enqueue = useAttachmentInsertStore((s) => s.enqueue);

  const [entries, setEntries] = useState<AttachmentEntry[] | null>(null);
  const [q, setQ] = useState('');
  const [kind, setKind] = useState<'all' | Kind>('all');
  const [imgUrls, setImgUrls] = useState<Record<string, string>>({});
  const [lightbox, setLightbox] = useState<string | null>(null);
  const [textPreview, setTextPreview] = useState<{ name: string; text: string } | null>(null);
  const [confirmDel, setConfirmDel] = useState<string | null>(null);
  const [status, setStatus] = useState('');
  const requestedRef = useRef<Set<string>>(new Set());

  const flash = (text: string) => {
    setStatus(text);
    window.setTimeout(() => setStatus((s) => (s === text ? '' : s)), 2000);
  };

  useEffect(() => {
    listAllAttachments()
      .then(setEntries)
      .catch((e) => {
        setEntries([]);
        flash('加载失败: ' + String(e));
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 图片缩略:逐个读 dataURL(已请求过的跳过,失败保持图标)
  useEffect(() => {
    if (!entries) return;
    for (const e of entries) {
      if (e.meta.kind !== 'image' || requestedRef.current.has(e.meta.path)) continue;
      requestedRef.current.add(e.meta.path);
      readAttachmentData(e.meta.path)
        .then((url) => setImgUrls((prev) => ({ ...prev, [e.meta.path]: url })))
        .catch(() => {});
    }
  }, [entries]);

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
      (e) => (kind === 'all' || e.meta.kind === kind) && (!kw || e.meta.name.toLowerCase().includes(kw)),
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
      nav(`/chat/${target.id}`);
    } catch (err) {
      flash('插入失败: ' + String(err));
    }
  };

  const onLocate = (e: AttachmentEntry) => {
    const sess = sessions.find((s) => s.id === e.session_id);
    if (sess && sess.active_branch !== e.branch_id) {
      void setActiveBranch(e.session_id, e.branch_id).catch(() => {});
    }
    nav(`/chat/${e.session_id}?msg=${e.message_id}`);
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
          prev?.filter((x) => !(x.message_id === e.message_id && x.meta.path === e.meta.path)) ?? null,
      );
      flash('已删除');
    } catch (err) {
      flash('删除失败: ' + String(err));
    }
  };

  return (
    <div className="att-page">
      <div className="att-head">
        <span className="att-title">附件中心</span>
        <div className="att-search">
          <IconSearch size={14} />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="按文件名搜索…"
            spellCheck={false}
          />
        </div>
        <div className="att-kinds">
          {KIND_FILTERS.map((f) => (
            <button
              key={f.key}
              className={`att-kind ${kind === f.key ? 'active' : ''}`}
              onClick={() => setKind(f.key)}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

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
        {groups.map((g) => (
          <div key={g.sessionId} className="att-group">
            <button className="att-group-head" onClick={() => nav(`/chat/${g.sessionId}`)} title="打开会话">
              <IconChat size={14} />
              <span className="t">{g.title}</span>
              <span className="n">{g.items.length}</span>
            </button>
            <div className="att-grid">
              {g.items.map((e) => {
                const thumb = e.meta.kind === 'image' ? imgUrls[e.meta.path] : undefined;
                const canPreview = e.meta.kind === 'image' || !!e.meta.extracted_text;
                const confirming = confirmDel === delKey(e);
                return (
                  <div key={delKey(e)} className="att-card">
                    <div
                      className={`att-thumb ${canPreview ? 'clickable' : ''}`}
                      onClick={() => canPreview && onPreview(e)}
                      title={canPreview ? '预览' : e.meta.name}
                    >
                      {thumb ? <img src={thumb} alt={e.meta.name} /> : <KindIcon kind={e.meta.kind} />}
                    </div>
                    <div className="att-name" title={e.meta.name}>
                      {e.meta.name}
                    </div>
                    <div className="att-meta">
                      {KIND_LABEL[e.meta.kind]} · {fmtSize(e.meta.size)} · {fmtDate(e.created)}
                    </div>
                    <div className="att-ops">
                      {canPreview && (
                        <button title="预览" onClick={() => onPreview(e)}>
                          <IconEye size={14} />
                        </button>
                      )}
                      <button title="插入当前会话" onClick={() => void onInsert(e)}>
                        <IconPlus size={14} />
                      </button>
                      <button title="定位来源消息" onClick={() => onLocate(e)}>
                        <IconExternalLink size={14} />
                      </button>
                      <button
                        className={confirming ? 'danger' : ''}
                        title={confirming ? '再次点击确认删除' : '删除'}
                        onClick={() => void onDelete(e)}
                      >
                        {confirming ? <IconCheck size={14} /> : <IconTrash size={14} />}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>

      {lightbox && (
        <div className="att-lightbox" onClick={() => setLightbox(null)}>
          <img src={lightbox} alt="预览" />
        </div>
      )}

      {textPreview && (
        <div className="att-modal-mask" onClick={() => setTextPreview(null)}>
          <div className="att-modal" onClick={(ev) => ev.stopPropagation()}>
            <div className="att-modal-head">
              <span className="t" title={textPreview.name}>
                {textPreview.name}
              </span>
              <button onClick={() => setTextPreview(null)} title="关闭">
                <IconX size={14} />
              </button>
            </div>
            <pre className="att-modal-body">{textPreview.text}</pre>
          </div>
        </div>
      )}

      {status && <div className="att-toast">{status}</div>}
    </div>
  );
}
