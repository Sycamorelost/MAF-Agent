import { useEffect, useMemo, useRef, useState } from 'react';
import { useMemoryStore } from '../stores/memory';
import { onMemoryChanged } from '../lib/ipc';
import * as ipc from '../lib/ipc';
import MemoryDiffDialog, { type DiffSide } from './MemoryDiffDialog';
import { MemoMarkdown } from './MemoMarkdown';
import { VersionHistory } from './mem/VersionHistory';

interface Props {
  sessionId: string;
  /** 覆盖面板宽度(px);不传 = 使用全局 token 默认值 */
  width?: number;
  /** 窄档(降字号/换行更紧凑) */
  narrow?: boolean;
  /** 收起本面板(折叠后由会话头部的按钮重新展开) */
  onCollapse?: () => void;
}

/** 从 frontmatter+body 拼出 memory.md 完整文件内容 */
function renderFull(fm: ipc.Frontmatter, body: string): string {
  const lines = ['---'];
  lines.push(`conversation: "${fm.conversation}"`);
  lines.push(`id: ${fm.id}`);
  if (fm.created) lines.push(`created: ${fm.created}`);
  if (fm.updated) lines.push(`updated: ${fm.updated}`);
  if (fm.memory_version) lines.push(`memory_version: ${fm.memory_version}`);
  if (fm.models_used?.length) lines.push(`models_used: [${fm.models_used.map((m) => `"${m}"`).join(', ')}]`);
  for (const [k, v] of Object.entries(fm)) {
    if (!['conversation', 'id', 'created', 'updated', 'memory_version', 'models_used'].includes(k)) {
      lines.push(`${k}: ${JSON.stringify(v)}`);
    }
  }
  lines.push('---');
  return `${lines.join('\n')}\n\n${body}`;
}

/** 从完整内容解析出正文(去掉 frontmatter) */
function bodyOf(full: string): string {
  if (!full.startsWith('---')) return full;
  const end = full.indexOf('\n---', 3);
  if (end < 0) return full;
  return full.slice(end + 4).replace(/^\s*\n/, '');
}

export default function MemPanel({ sessionId, width, narrow, onCollapse }: Props) {
  const { bySession, versions, load, write, loadVersions, rollback, startWatch, stopWatch } =
    useMemoryStore();
  const mem = bySession[sessionId];
  const verList = versions[sessionId] ?? [];

  const [draft, setDraft] = useState('');
  const [showVersions, setShowVersions] = useState(false);
  const [diffSel, setDiffSel] = useState<number[]>([]);
  const [diffOpen, setDiffOpen] = useState<{ left: DiffSide; right: DiffSide } | null>(null);
  const [conflict, setConflict] = useState(false);
  const [conflictOpen, setConflictOpen] = useState(false);
  /** 冲突弹窗中展示的磁盘最新内容 */
  const [incoming, setIncoming] = useState<string | null>(null);
  const [externalBlink, setExternalBlink] = useState(false);
  /** 用户是否动过 draft(区分「初始同步」与「未保存编辑」) */
  const [userDirty, setUserDirty] = useState(false);
  const lastLocalWriteRef = useRef(0);

  const savedFull = useMemo(
    () => (mem ? renderFull(mem.frontmatter, mem.body) : ''),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [mem?.body, mem?.frontmatter.memory_version],
  );

  const dirty = userDirty && draft !== savedFull;
  /** watcher 回调闭包内读取最新的 dirty(订阅只建一次,state 会过期) */
  const dirtyRef = useRef(false);
  dirtyRef.current = dirty;

  // 加载 + watcher + 事件订阅
  useEffect(() => {
    load(sessionId);
    loadVersions(sessionId);
    startWatch(sessionId);

    let unlisten: (() => void) | null = null;
    onMemoryChanged((p) => {
      if (p.session_id !== sessionId) return;
      // 忽略本地写操作后极短时间内触发的事件(避免自己保存后误报外部变更)
      if (Date.now() - lastLocalWriteRef.current < 800) return;
      if (dirtyRef.current) {
        // 编辑中不自动覆盖,标记冲突
        setConflict(true);
        return;
      }
      load(sessionId);
      loadVersions(sessionId);
      setExternalBlink(true);
      window.setTimeout(() => setExternalBlink(false), 3000);
    }).then((f) => (unlisten = f));

    return () => {
      stopWatch(sessionId);
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, load, loadVersions, startWatch, stopWatch]);

  // 非编辑态时 draft 随文件同步
  useEffect(() => {
    if (!userDirty && savedFull) {
      setDraft(savedFull);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [savedFull]);

  const onSave = async () => {
    lastLocalWriteRef.current = Date.now();
    await write(sessionId, draft, 'manual');
    setUserDirty(false);
  };

  const onCancelEdit = () => {
    setDraft(savedFull);
    setUserDirty(false);
  };

  const statusInfo = useMemo(() => {
    if (conflict || externalBlink) return { color: 'var(--brand)', text: '外部变更' };
    if (dirty) return { color: '#C9942A', text: '未保存编辑' };
    return { color: 'var(--ok)', text: '已同步' };
  }, [conflict, externalBlink, dirty]);

  // 查看冲突:拉取磁盘最新内容展示
  const onViewConflict = async () => {
    try {
      const dto = await ipc.readMemory(sessionId);
      setIncoming(renderFull(dto.frontmatter, dto.body));
    } catch {
      setIncoming(null);
    }
    setConflictOpen(true);
  };

  // 采用新版本:重载磁盘内容,放弃未保存编辑
  const onAdoptIncoming = async () => {
    await load(sessionId);
    await loadVersions(sessionId);
    setUserDirty(false);
    setConflict(false);
    setConflictOpen(false);
    setIncoming(null);
  };

  const onToggleDiff = (vid: number) => {
    setDiffSel((prev) => {
      if (prev.includes(vid)) return prev.filter((v) => v !== vid);
      if (prev.length >= 2) return [...prev.slice(1), vid];
      return [...prev, vid];
    });
  };

  const openDiff = (left: DiffSide, right: DiffSide) => {
    setDiffOpen({ left, right });
  };

  if (!mem) {
    return (
      <div className="mempanel" style={width ? { width } : undefined} data-narrow={!!narrow}>
        <div className="mem-head">
          <span className="t">memory.md</span>
        </div>
        <div className="mem-view" style={{ color: 'var(--text-4)' }}>
          加载中…
        </div>
      </div>
    );
  }

  return (
    <div className="mempanel" style={width ? { width } : undefined} data-narrow={!!narrow}>
      <div className="mem-head">
        <div className="mem-title-row">
          <span className="t">memory.md</span>
          <span
            className="mem-dot"
            style={{ background: statusInfo.color }}
            title={statusInfo.text}
          />
          {onCollapse && (
            <button
              type="button"
              className="panel-toggle"
              title="收起记忆面板"
              onClick={onCollapse}
              style={{ marginLeft: 'auto' }}
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.4">
                <path d="M5 3l4 4-4 4" />
              </svg>
            </button>
          )}
        </div>
        <div className="mem-actions">
          <button type="button" onClick={onSave} style={{ color: 'var(--brand)' }}>
            保存
          </button>
          <button type="button" onClick={onCancelEdit}>
            取消
          </button>
          <button type="button" onClick={() => setShowVersions((v) => !v)}>
            {showVersions ? '收起' : '历史'}
          </button>
        </div>
      </div>

      <div className="mem-frontmatter">
        <div>
          <span className="k">会话</span>
          {mem.frontmatter.conversation || '(未命名)'}
        </div>
        <div>
          <span className="k">版本</span>v{mem.frontmatter.memory_version}
          <span className="k" style={{ marginLeft: 12 }}>
            模型
          </span>
          {mem.frontmatter.models_used.join(', ') || '-'}
        </div>
      </div>

      {/* 冲突提示(D3.6) */}
      {conflict && !conflictOpen && (
        <div className="mem-conflict-bar">
          <span>检测到外部变更/新提炼结果</span>
          <div className="mem-conflict-actions">
            <button type="button" className="btn primary" onClick={onViewConflict}>
              查看
            </button>
            <button type="button" className="btn ghost" onClick={() => setConflict(false)}>
              丢弃
            </button>
          </div>
        </div>
      )}

      {showVersions && (
        <VersionHistory
          versions={verList}
          selected={diffSel}
          onToggle={onToggleDiff}
          onOpenDiff={openDiff}
          onRollback={(vid) => rollback(sessionId, vid)}
        />
      )}

      {/* 内容区:直接编辑 memory.md(移除预览/源码/分屏三态切换) */}
      <textarea
        className="mem-view edit"
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          setUserDirty(true);
        }}
      />

      {diffOpen && (
        <MemoryDiffDialog
          sessionId={sessionId}
          left={diffOpen.left}
          right={diffOpen.right}
          currentText={savedFull}
          onClose={() => setDiffOpen(null)}
        />
      )}

      {conflictOpen && (
        <div className="dialog-mask" onClick={() => setConflictOpen(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ width: 520 }}>
            <div className="dialog-head">新版本内容</div>
            <div className="dialog-body">
              <p style={{ fontSize: 'var(--font-caption)', color: 'var(--text-3)', marginBottom: 'var(--space-3)' }}>
                文件已被外部程序或自动提炼更新,以下为新内容(采用后将放弃你未保存的编辑):
              </p>
              <div className="mem-view mem-preview" style={{ maxHeight: 320, overflowY: 'auto' }}>
                <MemoMarkdown content={incoming !== null ? bodyOf(incoming) : '加载中…'} />
              </div>
              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>
                <button className="btn ghost" onClick={() => setConflictOpen(false)}>
                  继续我的编辑
                </button>
                <button className="btn primary" onClick={onAdoptIncoming}>
                  采用新版本
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
