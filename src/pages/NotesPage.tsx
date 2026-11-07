import { useCallback, useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import * as ipc from '../lib/ipc';
import { uiConfirm } from '../components/confirmService';
import {
  IconNote,
  IconPlus,
  IconTrash,
  IconEdit,
  IconDoc,
  IconRefresh,
  IconX,
} from '../components/icons';

/** 广播知识库变更:侧栏笔记列表监听 notes_changed 后自行重载。 */
const broadcastNotesChanged = () => window.dispatchEvent(new CustomEvent('notes_changed'));

/**
 * 知识库 / 笔记编辑器(/knowledge;旧 /notes 深链兼容):
 * 列表由主侧栏承担,本页只做 Markdown 编辑 / 预览,避免嵌套侧栏。
 * 当前笔记完全由 URL ?id= 驱动;文件即事实源:<repo>/data/notes/{id}.md。
 */
export default function NotesPage() {
  const [params, setParams] = useSearchParams();
  const selId = params.get('id');

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

  /** 载入某条笔记(以磁盘文件为准)。 */
  const loadNote = useCallback(async (id: string) => {
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

  /** 当前已载入的笔记 id;保存/新建/删除流程内部同步,避免地址回写触发重复读盘。 */
  const loadedRef = useRef<string | null>(null);

  // URL 驱动:地址 ?id 变化时载入对应笔记;有未保存改动时不打断当前编辑
  useEffect(() => {
    if (!selId) {
      loadedRef.current = null;
      return;
    }
    if (selId === loadedRef.current || dirty) return;
    loadedRef.current = selId;
    void loadNote(selId);
  }, [selId, dirty, loadNote]);

  /** 保存(新建时 id 为 null)。 */
  const save = useCallback(async () => {
    setBusy(true);
    try {
      const tagList = tags
        .split(/[,，]/)
        .map((s) => s.trim())
        .filter(Boolean);
      const doc = await ipc.notesSave(selId, title, tagList, body);
      loadedRef.current = doc.id;
      setTitle(doc.title);
      setTags(doc.tags.join(', '));
      setBody(doc.body);
      setMeta({ created: doc.created, updated: doc.updated });
      setDirty(false);
      if (selId !== doc.id) setParams({ id: doc.id }, { replace: true });
      broadcastNotesChanged();
      flash('已保存');
    } catch (e) {
      setStatus('保存失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [selId, title, tags, body, setParams]);

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
      loadedRef.current = doc.id;
      setTitle('');
      setTags('');
      setBody('');
      setMeta({ created: doc.created, updated: doc.updated });
      setMode('edit');
      setDirty(false);
      setParams({ id: doc.id }, { replace: true });
      broadcastNotesChanged();
      flash('已新建');
    } catch (e) {
      setStatus('新建失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [dirty, setParams]);

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
      loadedRef.current = null;
      setMeta(null);
      setTitle('');
      setTags('');
      setBody('');
      setDirty(false);
      setParams({}, { replace: true });
      broadcastNotesChanged();
      flash('已删除');
    } catch (e) {
      setStatus('删除失败: ' + String(e));
    } finally {
      setBusy(false);
    }
  }, [selId, setParams]);

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
      broadcastNotesChanged();
      // 当前打开的笔记可能被合并过,从磁盘重新加载避免陈旧缓冲
      if (selId) await loadNote(selId);
    } catch (e) {
      setStatus('导入失败: ' + String(e));
    } finally {
      setImporting(false);
    }
  }, [dirty, selId, loadNote]);

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

  const fmtDate = (s: string) => {
    if (!s) return '';
    const d = new Date(s);
    return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
  };

  return (
    <div className="notes-page">
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
              <button
                className="act"
                onClick={importPresets}
                disabled={busy || importing}
                title="重新导入预置内容:恢复缺失、合并重复,不覆盖现有数据"
              >
                <IconRefresh size={13} />
                {importing ? '导入中…' : '导入预置'}
              </button>
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
              <button className="icon-btn danger" onClick={del} disabled={busy} title="删除笔记">
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
                <p>从左侧列表选择一篇笔记,或新建一条开始记录</p>
                <div className="notes-landing-actions">
                  <button className="notes-cta primary" onClick={createNote} disabled={busy}>
                    <IconPlus size={15} />
                    新建笔记
                  </button>
                  <button
                    className="notes-cta"
                    onClick={importPresets}
                    disabled={busy || importing}
                    title="重新导入预置内容:恢复缺失、合并重复,不覆盖现有数据"
                  >
                    <IconRefresh size={14} />
                    {importing ? '导入中…' : '导入预置'}
                  </button>
                </div>
              </div>
              <div className="notes-landing-tip">
                支持 Markdown;笔记以文件形式保存在本机 data/notes/ 目录,可随时用其他编辑器打开。
              </div>
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
