import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import * as ipc from '../lib/ipc';
import AsideDrawer from '../components/AsideDrawer';
import NotePresetBadge, { displayTags } from '../components/NotePresetBadge';
import { useUiStore } from '../stores/ui';
import { uiConfirm } from '../components/confirmService';
import {
  IconNote,
  IconPlus,
  IconTrash,
  IconEdit,
  IconDoc,
  IconSearch,
  IconRefresh,
  IconX,
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
  /** 预置导入报告(非空 = 弹窗展示) */
  const [report, setReport] = useState<ipc.ReimportReport | null>(null);
  const [importing, setImporting] = useState(false);

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
      if (
        dirty &&
        !(await uiConfirm({
          title: '切换笔记',
          message: '当前笔记有未保存的修改,切换后将丢失。确定切换?',
        }))
      )
        return;
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

  // 首次进入:扫盘;仅当 URL 带有效 ?id 时直接定位,
  // 否则停在「落地页」(最新 5 条小卡片 + 快速新建),不自动钻进第一条笔记。
  const booted = useRef(false);
  useEffect(() => {
    if (booted.current) return;
    booted.current = true;
    (async () => {
      const list = await reload();
      const want = params.get('id');
      if (want && list.some((n) => n.id === want)) await selectNote(want);
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
    if (
      dirty &&
      !(await uiConfirm({
        title: '新建笔记',
        message: '当前笔记有未保存的修改,新建后将丢失。确定新建?',
      }))
    )
      return;
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
    if (
      !(await uiConfirm({
        title: '删除笔记',
        message: '删除该笔记?此操作不可撤销(md 文件与索引一并清除)。',
        danger: true,
        confirmText: '删除',
      }))
    )
      return;
    setBusy(true);
    try {
      await ipc.notesDelete(selId);
      await reload();
      // 删除后回到落地页(最新 5 条 + 快速新建),不再跳到一条无关笔记
      setSelId(null);
      setMeta(null);
      setTitle('');
      setTags('');
      setBody('');
      setDirty(false);
      setParams({}, { replace: true });
      flash('已删除');
    } catch (e) {
      setStatus('删除失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [selId, reload, setParams]);

  /** 一键重新导入预置内容:恢复缺失 + 合并重复(不覆盖用户数据),弹窗展示明细。 */
  const importPresets = useCallback(async () => {
    if (
      dirty &&
      !(await uiConfirm({
        title: '导入预置',
        message: '当前笔记有未保存的修改,导入后将从磁盘重新加载。先保存或放弃修改,继续导入?',
      }))
    ) {
      return;
    }
    setImporting(true);
    try {
      const rep = await ipc.notesReimportPresets();
      setReport(rep);
      const list = await reload();
      // 当前打开的笔记可能被合并过,从磁盘重新加载避免陈旧缓冲
      if (selId && list.some((n) => n.id === selId)) await selectNote(selId);
    } catch (e) {
      setStatus('导入失败: ' + String(e));
    } finally {
      setImporting(false);
    }
  }, [dirty, reload, selId, selectNote]);

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

  /** 落地页「查看全部」:释放左侧玻璃抽屉(全部笔记 + 筛选) */
  const setAsideOpen = useUiStore((s) => s.setAsideOpen);

  /** 落地页展示的最新笔记(后端已按更新时间倒序)。 */
  const latest = useMemo(() => metas.slice(0, 5), [metas]);

  return (
    <div className="notes-page">
      <AsideDrawer>
      <div className="notes-list drawer">
        <div className="notes-list-head">
          <span className="t">知识库</span>
          <button
            className="new"
            onClick={importPresets}
            disabled={busy || importing}
            title="重新导入预置内容:恢复缺失、合并重复,不覆盖现有数据"
          >
            <IconRefresh size={14} />
            {importing ? '导入中…' : '导入预置'}
          </button>
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
                <NotePresetBadge id={n.id} />
                {displayTags(n.id, n.tags).map((t) => (
                  <span className="tag" key={t}>
                    {t}
                  </span>
                ))}
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
          <div className="notes-landing">
            <div className="notes-landing-inner">
              <div className="notes-landing-hero">
                <div className="notes-landing-icon">
                  <IconNote size={26} strokeWidth={1.2} />
                </div>
                <h2>知识库</h2>
                <p>
                  {metas.length > 0
                    ? `共 ${metas.length} 条笔记,最近更新如下`
                    : '还没有笔记,现在开始记录第一条'}
                </p>
                <div className="notes-landing-actions">
                  <button className="notes-cta primary" onClick={createNote} disabled={busy}>
                    <IconPlus size={15} />
                    新建笔记
                  </button>
                  {metas.length > 0 && (
                    <button className="notes-cta" onClick={() => setAsideOpen(true)}>
                      查看全部
                    </button>
                  )}
                </div>
              </div>

              {latest.length > 0 && (
                <>
                  <div className="notes-landing-sub">
                    <span>最近更新</span>
                    <span className="sp" />
                    <span className="hint">
                      最新 {latest.length} 条{metas.length > latest.length ? ` · 共 ${metas.length} 条` : ''}
                    </span>
                  </div>
                  <div className="notes-cards">
                    {latest.map((n) => (
                      <button
                        key={n.id}
                        className="notes-card"
                        onClick={() => void requestSelect(n.id)}
                      >
                        <span className="notes-card-title">{n.title || '(未命名笔记)'}</span>
                        <span className={`notes-card-excerpt${n.excerpt ? '' : ' empty'}`}>
                          {n.excerpt || '暂无摘要'}
                        </span>
                        <span className="notes-card-meta">
                          {n.updated ? <span>{fmtDate(n.updated)}</span> : null}
                          <NotePresetBadge id={n.id} />
                          {displayTags(n.id, n.tags).map((t) => (
                            <span className="tag" key={t}>
                              {t}
                            </span>
                          ))}
                        </span>
                      </button>
                    ))}
                  </div>
                </>
              )}

              {metas.length === 0 && (
                <div className="notes-landing-tip">
                  支持 Markdown;笔记以文件形式保存在本机 data/notes/ 目录,可随时用其他编辑器打开。
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* 预置导入报告弹窗:恢复 / 合并 / 完全重复 三组明细,可滚动翻阅 */}
      {report && (
        <div className="att-modal-mask" onClick={() => setReport(null)}>
          <div className="att-modal" onClick={(e) => e.stopPropagation()}>
            <div className="att-modal-head">
              <span className="t">预置内容导入结果</span>
              <span className="att-modal-sub">
                恢复 {report.added.length} · 合并 {report.merged.length} · 完全重复{' '}
                {report.skipped.length}
              </span>
              <button onClick={() => setReport(null)} title="关闭">
                <IconX size={14} />
              </button>
            </div>
            <div className="att-modal-body notes-import-report">
              {report.merged.length > 0 && (
                <section>
                  <h4>已合并({report.merged.length})——预置缺漏内容已并入,你的内容零改动</h4>
                  <ul>
                    {report.merged.map((it) => (
                      <li key={it.id}>
                        <span className="r-title">{it.title}</span>
                        <span className="r-detail">{it.detail}</span>
                      </li>
                    ))}
                  </ul>
                </section>
              )}
              {report.added.length > 0 && (
                <section>
                  <h4>新恢复({report.added.length})</h4>
                  <ul>
                    {report.added.map((it) => (
                      <li key={it.id}>
                        <span className="r-title">{it.title}</span>
                        <span className="r-detail">{it.detail}</span>
                      </li>
                    ))}
                  </ul>
                </section>
              )}
              {report.skipped.length > 0 && (
                <section>
                  <h4>完全重复({report.skipped.length})——未做任何改动</h4>
                  <ul>
                    {report.skipped.map((it) => (
                      <li key={it.id}>
                        <span className="r-title">{it.title}</span>
                        <span className="r-detail">{it.detail}</span>
                      </li>
                    ))}
                  </ul>
                </section>
              )}
              {report.added.length === 0 &&
                report.merged.length === 0 &&
                report.skipped.length === 0 && <p>没有可导入的预置内容。</p>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
