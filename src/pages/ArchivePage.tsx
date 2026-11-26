/**
 * 归档视图(/archive,V41/V42)。
 * 会话与项目都可归档:归档只隐藏,数据与属性完全保留,可随时恢复。
 * 页面骨架复用附件中心的 .att-* 样式,保持与主应用一致。
 */

import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { uiConfirm } from '../components/confirmService';
import { listArchivedProjects, setProjectArchived, type ProjectRow } from '../lib/ipc';
import { sessionPath } from '../lib/workbench';
import { whenLabel } from '../components/sidebar/utils';
import { useSessionsStore } from '../stores/sessions';
import '../styles/archive.css';

export default function ArchivePage() {
  const nav = useNavigate();
  const archived = useSessionsStore((s) => s.archived);
  const loaded = useSessionsStore((s) => s.loaded);
  const refresh = useSessionsStore((s) => s.refresh);
  const unarchive = useSessionsStore((s) => s.unarchive);
  const removeMany = useSessionsStore((s) => s.removeMany);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [status, setStatus] = useState('');

  const flash = (text: string) => {
    setStatus(text);
    window.setTimeout(() => setStatus(''), 2200);
  };

  const loadProjects = useCallback(() => {
    void listArchivedProjects()
      .then(setProjects)
      .catch(() => setProjects([]));
  }, []);
  useEffect(loadProjects, [loadProjects]);

  const restoreSessions = async (ids: string[]) => {
    try {
      await unarchive(ids);
      flash(`已恢复 ${ids.length} 个会话`);
    } catch (e) {
      flash(`恢复失败:${String(e)}`);
    }
  };

  const restoreProject = async (p: ProjectRow) => {
    try {
      await setProjectArchived(p.id, false);
      await refresh();
      loadProjects();
      flash(`已恢复项目「${p.name}」及其会话`);
    } catch (e) {
      flash(`恢复失败:${String(e)}`);
    }
  };

  const remove = async (ids: string[], title?: string) => {
    const ok = await uiConfirm({
      title: '删除会话',
      message: title
        ? `将永久删除「${title}」及其消息、记忆与附件。\n此操作不可撤销。`
        : `将永久删除选中的 ${ids.length} 个会话及其消息、记忆与附件。\n此操作不可撤销。`,
      confirmText: '删除',
    });
    if (!ok) return;
    try {
      await removeMany(ids);
      flash(`已删除 ${ids.length} 个会话`);
    } catch (e) {
      flash(`删除失败:${String(e)}`);
    }
  };

  return (
    <div className="att-page">
      <div className="att-head">
        <span className="att-title">归档</span>
        <div className="att-actions">
          <button
            className="att-btn"
            disabled={archived.length === 0}
            onClick={() => void restoreSessions(archived.map((s) => s.id))}
          >
            全部恢复会话
          </button>
        </div>
      </div>

      <div className="att-body">
        <p className="arc-hint">
          归档不会删除任何内容,会话与项目的属性保持原样;可随时恢复或打开继续对话。
        </p>

        <div className="att-group">
          <div className="att-group-head">
            <span className="att-group-title">
              已归档会话 <span className="n">{archived.length}</span>
            </span>
          </div>
          {!loaded && <div className="att-empty">加载中…</div>}
          {loaded && archived.length === 0 && (
            <div className="att-empty">
              还没有归档的会话。在侧栏右键任意会话,选择「归档会话」即可。
            </div>
          )}
          <div className="arc-list">
            {archived.map((s) => (
              <div key={s.id} className="att-card arc-row">
                <button
                  className="arc-title"
                  title="打开会话"
                  onClick={() => nav(sessionPath(s.id))}
                >
                  {s.title || '未命名会话'}
                </button>
                <span className="arc-when">{whenLabel(s.updated)}</span>
                <div className="arc-actions">
                  <button className="att-btn" onClick={() => void restoreSessions([s.id])}>
                    恢复
                  </button>
                  <button
                    className="att-btn danger"
                    onClick={() => void remove([s.id], s.title)}
                  >
                    删除
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="att-group">
          <div className="att-group-head">
            <span className="att-group-title">
              已归档项目 <span className="n">{projects.length}</span>
            </span>
          </div>
          {projects.length === 0 && (
            <div className="att-empty">
              还没有归档的项目。在侧栏右键项目名称,选择「归档项目」即可。
            </div>
          )}
          <div className="arc-list">
            {projects.map((p) => (
              <div key={p.id} className="att-card arc-row">
                <span className="arc-title" title={p.path}>
                  {p.name || '未命名项目'}
                </span>
                <span className="arc-when" title={p.path}>
                  {p.path}
                </span>
                <div className="arc-actions">
                  <button className="att-btn" onClick={() => void restoreProject(p)}>
                    恢复
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {status && <div className="att-toast">{status}</div>}
    </div>
  );
}
