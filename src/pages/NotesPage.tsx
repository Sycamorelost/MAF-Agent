import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import * as ipc from '../lib/ipc';
import AsideDrawer from '../components/AsideDrawer';
import {
  IconNote,
  IconPlus,
  IconTrash,
  IconEdit,
  IconDoc,
  IconSearch,
} from '../components/icons';

/**
 * 知识库 / 笔记库页(/notes,迭代 D8)。
 * 左侧笔记列表(可拖拽调宽),右侧 Markdown 编辑器 / 预览。
 * 文件即事实源:<repo>/data/notes/{id}.md,external 编辑后重新读盘即生效。
 * 支持 URL ?id=xx 直接定位(全局搜索「知识库」分组跳转用)。
 */
export default function NotesPage() {
  const [params, setParams] = useSearchParams();

  const [metas, setMetas] = useState<ipc.NoteMeta[]>([]);
  const [selId, setSelId] = useState<string | null>(null);
  const [q, setQ] = useState('');
  const [title, setTitle] = useState('');
  const [tags, setTags] = useState('');
  const [body, setBody] = useState('');
  const [mode, setMode] = useState<'edit' | 'preview'>('edit');
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState('');
  const [meta, setMeta] = useState<{ created: string; updated: string } | null>(null);

  const flash = (text: string) => {
    setStatus(text);
    window.setTimeout(() => setStatus((s) => (s === text ? '' : s)), 1800);
  };

  /** 选中某条笔记并载入(以磁盘文件为准)。 */
  const selectNote = useCallback(async (id: string) => {
    setSelId(id);
    setMode('edit');
    setDirty(false);
    setStatus('');
    try {
      const d = await ipc.notesGet(id);
      setTitle(d.title);
      setTags(d.tags.join(', '));
      setBody(d.body);
      setMeta({ created: d.created, updated: d.updated });
    } catch (e) {
      setTitle('');
      setTags('');
      setBody('');
      setMeta(null);
      setStatus('读取失败: ' + String(e));
    }
  }, []);

  /** 切换前若有未保存改动先确认。 */
  const requestSelect = useCallback(
    async (id: string) => {
      if (id === selId) return;
      if (dirty && !window.confirm('当前笔记有未保存的修改,切换后将丢失。确定切换?')) return;
      await selectNote(id);
      setParams({ id }, { replace: true });
    },
    [dirty, selId, selectNote, setParams],
  );

  /** 重新扫盘刷新列表。 */
  const reload = useCallback(async (): Promise<ipc.NoteMeta[]> => {
    const list = await ipc.notesList().catch(() => [] as ipc.NoteMeta[]);
    setMetas(list);
    return list;
  }, []);

  // 首次进入:扫盘 + 按 ?id 或首项定位
  const booted = useRef(false);
  useEffect(() => {
    if (booted.current) return;
    booted.current = true;
    (async () => {
      const list = await reload();
      const want = params.get('id');
      const pick = want && list.some((n) => n.id === want) ? want : (list[0]?.id ?? null);
      if (pick) await selectNote(pick);
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 保存(新建时 id 为 null)。 */
  const save = useCallback(async () => {
    setBusy(true);
    try {
      const tagList = tags
        .split(/[,，]/)
        .map((s) => s.trim())
        .filter(Boolean);
      const doc = await ipc.notesSave(selId, title, tagList, body);
      setSelId(doc.id);
      setTitle(doc.title);
      setTags(doc.tags.join(', '));
      setBody(doc.body);
      setMeta({ created: doc.created, updated: doc.updated });
      setDirty(false);
      setParams({ id: doc.id }, { replace: true });
      await reload();
      flash('已保存');
    } catch (e) {
      setStatus('保存失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [selId, title, tags, body, reload, setParams]);

  /** 新建空笔记并立即落盘。 */
  const createNote = useCallback(async () => {
    if (dirty && !window.confirm('当前笔记有未保存的修改,新建后将丢失。确定新建?')) return;
    setBusy(true);
    try {
      const doc = await ipc.notesSave(null, '', [], '');
      await reload();
      setSelId(doc.id);
      setTitle('');
      setTags('');
      setBody('');
      setMeta({ created: doc.created, updated: doc.updated });
      setMode('edit');
      setDirty(false);
      setParams({ id: doc.id }, { replace: true });
      flash('已新建');
    } catch (e) {
      setStatus('新建失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [dirty, reload, setParams]);

  /** 删除当前笔记(md + 索引)。 */
  const del = useCallback(async () => {
    if (!selId) return;
    if (!window.confirm('删除该笔记?此操作不可撤销(md 文件与索引一并清除)。')) return;
    setBusy(true);
    try {
      await ipc.notesDelete(selId);
      const list = await reload();
      const next = list[0]?.id ?? null;
      setSelId(null);
      setMeta(null);
      if (next) {
        await selectNote(next);
        setParams({ id: next }, { replace: true });
      } else {
        setTitle('');
        setTags('');
        setBody('');
        setParams({}, { replace: true });
      }
      flash('已删除');
    } catch (e) {
      setStatus('删除失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [selId, reload, selectNote, setParams]);

  // Ctrl/Cmd + S 保存
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
        e.preventDefault();
        if (selId && dirty && !busy) void save();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [selId, dirty, busy, save]);

  // 本地过滤(标题 / 标签 / 摘要)
  const shown = useMemo(() => {
    const kw = q.trim().toLowerCase();
    if (!kw) return metas;
    return metas.filter(
      (n) =>
        n.title.toLowerCase().includes(kw) ||
        n.tags.some((t) => t.toLowerCase().includes(kw)) ||
        n.excerpt.toLowerCase().includes(kw),
    );
  }, [metas, q]);

  const fmtDate = (s: string) => {
    if (!s) return '';
    const d = new Date(s);
    return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
  };

  return (
    <div className="notes-page">
      <AsideDrawer>
      <div className="notes-list drawer">
        <div className="notes-list-head">
          <span className="t">知识库</span>
          <button className="new" onClick={createNote} disabled={busy} title="新建笔记">
            <IconPlus size={14} />
            新建
          </button>
        </div>
        <div className="drawer-hint">点标题栏「面板」可开合本栏</div>
        <div className="notes-search">
          <IconSearch size={14} />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="筛选标题 / 标签 / 摘要…"
            spellCheck={false}
          />
        </div>
        <div className="notes-list-body">
          {metas.length === 0 && (
            <div className="notes-empty">
              <IconNote size={30} strokeWidth={1.2} />
              <span>还没有笔记,点「新建」开始记录</span>
            </div>
          )}
          {metas.length > 0 && shown.length === 0 && (
            <div className="notes-empty">
              <IconNote size={26} strokeWidth={1.2} />
              <span>没有匹配「{q}」的笔记</span>
            </div>
          )}
          {shown.map((n) => (
            <div
              key={n.id}
              className={`notes-item ${selId === n.id ? 'active' : ''}`}
              onClick={() => requestSelect(n.id)}
            >
              <div className="notes-item-title">{n.title || '(未命名笔记)'}</div>
              <div className="notes-item-meta">
                {n.updated ? <span>{fmtDate(n.updated)}</span> : null}
                {n.tags.length > 0 && <span className="tag">{n.tags.join(' · ')}</span>}
              </div>
              {n.excerpt && <div className="notes-item-excerpt">{n.excerpt}</div>}
            </div>
          ))}
        </div>
      </div>
      </AsideDrawer>

      <div className="notes-detail">
        {selId ? (
          <>
            <div className="notes-head">
              <input
                className="title"
                value={title}
                placeholder="笔记标题"
                spellCheck={false}
                onChange={(e) => {
                  setTitle(e.target.value);
                  setDirty(true);
                }}
              />
              <div className="mode-group">
                <button
                  className={mode === 'edit' ? 'active' : ''}
                  onClick={() => setMode('edit')}
                  title="编辑"
                >
                  <IconEdit size={13} />
                  编辑
                </button>
                <button
                  className={mode === 'preview' ? 'active' : ''}
                  onClick={() => setMode('preview')}
                  title="预览"
                >
                  <IconDoc size={13} />
                  预览
                </button>
              </div>
              <button className="act primary" onClick={save} disabled={busy || !dirty}>
                保存
              </button>
              <button className="act danger" onClick={del} disabled={busy} title="删除笔记">
                <IconTrash size={13} />
              </button>
            </div>
            <div className="notes-tags">
              <span className="lbl">标签</span>
              <input
                value={tags}
                placeholder="逗号分隔,如:学习, 方法"
                spellCheck={false}
                onChange={(e) => {
                  setTags(e.target.value);
                  setDirty(true);
                }}
              />
            </div>
            <div className="notes-body">
              {mode === 'edit' ? (
                <textarea
                  className="notes-editor"
                  value={body}
                  placeholder="开始写 Markdown…"
                  spellCheck={false}
                  onChange={(e) => {
                    setBody(e.target.value);
                    setDirty(true);
                  }}
                />
              ) : (
                <div className="notes-preview">
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm]}
                    rehypePlugins={[rehypeHighlight]}
                    components={{
                      table: ({ children }) => (
                        <div style={{ overflowX: 'auto', maxWidth: '100%' }}>
                          <table>{children}</table>
                        </div>
                      ),
                    }}
                  >
                    {body || '*（正文为空）*'}
                  </ReactMarkdown>
                </div>
              )}
            </div>
            <div className="notes-foot">
              <span>{meta?.updated ? `更新于 ${fmtDate(meta.updated)}` : '尚未保存'}</span>
              <span className="sp" />
              {status ? <span className="status">{status}</span> : dirty ? <span>未保存</span> : null}
            </div>
          </>
        ) : (
          <div className="notes-empty" style={{ flex: 1 }}>
            <IconNote size={40} strokeWidth={1.1} />
            <span>{metas.length === 0 ? '还没有笔记,点左侧「新建」开始记录' : '从左侧选择一条笔记'}</span>
          </div>
        )}
      </div>
    </div>
  );
}
