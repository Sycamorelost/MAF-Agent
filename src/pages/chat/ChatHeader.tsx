import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  IconCheck,
  IconChevronDown,
  IconDownload,
  IconEdit,
  IconLayers,
  IconList,
  IconPullRequest,
  IconTrash,
  IconX,
  IconBrain,
  IconPanel,
  IconChat,
} from '../../components/icons';
import { PresetChip } from '../../components/PresetChip';
import { useWorkbenchChrome } from '../../lib/workbenchChrome';
import { useMessagesStore } from '../../stores/messages';
import { useSessionsStore } from '../../stores/sessions';
import * as ipc from '../../lib/ipc';
import { safeExportName } from '../../lib/export/names';
import { buildSessionHtml, buildSessionMarkdown } from './chatExport';

/** 分支显示名:用户自定义 > 默认(主分支 / 分支 xxxx)。 */
function branchLabel(b: ipc.BranchInfo | undefined, fallbackId: string): string {
  if (b?.name) return b.name;
  const id = b?.branch_id ?? fallbackId;
  return id === 'main' ? '主分支' : `分支 ${id.slice(0, 4)}`;
}

interface ChatHeaderProps {
  title: string;
  sessionId: string;
  /** 当前会话的生效形态:work 时头部多出示模式徽标、预设 chip 与检查器开关(ui-design b3)。 */
  mode: 'chat' | 'work';
  activeBranch: string;
  /** 递增信号:同一分支内分叉后也强制重载分支列表 */
  branchRefresh: number;
  current: { providerId: string; model: string } | null;
  memCollapsed: boolean;
  outlineOpen: boolean;
  onToggleMemPanel: () => void;
  onToggleOutline: () => void;
  onOpenModelSwitch: () => void;
  onError: (msg: string) => void;
}

/** 会话头部:标题 / 分支指示条 / 本会话模型 / 记忆面板开关。 */
export function ChatHeader({
  title,
  sessionId,
  mode,
  activeBranch,
  branchRefresh,
  current,
  memCollapsed,
  outlineOpen,
  onToggleMemPanel,
  onToggleOutline,
  onOpenModelSwitch,
  onError,
}: ChatHeaderProps) {
  // ---------- F4:会话分支树 ----------
  const setActiveBranch = useSessionsStore((s) => s.setActiveBranch);
  const renameSession = useSessionsStore((s) => s.rename);
  const setWorkbenchMode = useSessionsStore((s) => s.setWorkbenchMode);
  const chrome = useWorkbenchChrome();
  const nav = useNavigate();
  const [branches, setBranches] = useState<ipc.BranchInfo[]>([]);
  const [branchOpen, setBranchOpen] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  // 会话标题内联改名(后端 rename_session 会同步 memory.md 标题)
  const [titleEdit, setTitleEdit] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exported, setExported] = useState(false);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);

  const onRenameTitle = async () => {
    if (titleEdit === null) return;
    const t = titleEdit.trim();
    setTitleEdit(null);
    if (!t || t === title) return;
    try {
      await renameSession(sessionId, t);
    } catch (e) {
      onError(`重命名会话失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  /** 导出当前分支消息:Markdown / HTML 两种纯文本单文件格式。 */
  const runExport = async (fmt: 'md' | 'html') => {
    if (exporting) return;
    const msgs = useMessagesStore.getState().bySession[sessionId] ?? [];
    if (msgs.length === 0) {
      onError('当前会话没有可导出的消息');
      return;
    }
    setExporting(true);
    setExportMenuOpen(false);
    try {
      const content =
        fmt === 'html' ? buildSessionHtml(title, msgs) : buildSessionMarkdown(title, msgs);
      const pick = await ipc.pickSavePath(
        fmt === 'html' ? '导出会话(HTML)' : '导出会话(Markdown)',
        `${safeExportName(title || '会话')}.${fmt}`,
        fmt === 'html' ? 'HTML 文档' : 'Markdown 文档',
        fmt,
      );
      if (!pick) return;
      const doExport = (overwrite: boolean) =>
        ipc.sessionExportFile(sessionId, content, pick.token, overwrite);
      let saved: string | null = null;
      try {
        saved = await doExport(pick.existed);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (msg === ipc.EXPORT_TARGET_EXISTS) {
          if (!window.confirm(`目标文件已存在:\n${pick.path}\n\n确认覆盖?`)) return;
          saved = await doExport(true);
        } else {
          throw e;
        }
      }
      if (saved) {
        setExported(true);
        setTimeout(() => setExported(false), 1500);
      }
    } catch (e) {
      onError(`导出失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setExporting(false);
    }
  };
  const activeBranchInfo = useMemo(
    () => branches.find((b) => b.branch_id === activeBranch),
    [branches, activeBranch],
  );
  const loadBranches = useCallback(async () => {
    if (!sessionId) return;
    try {
      setBranches(await ipc.listBranches(sessionId));
    } catch {
      // 分支列表加载失败不阻塞对话
    }
  }, [sessionId]);

  // F4:激活分支变化(切换/分叉/搜索跳转)时重载分支列表
  useEffect(() => {
    void loadBranches();
  }, [loadBranches, activeBranch, branchRefresh]);

  const onRenameBranch = async () => {
    if (!renaming) return;
    try {
      await ipc.renameBranch(sessionId, renaming.id, renaming.name);
    } catch (e) {
      onError(`重命名分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
    setRenaming(null);
    await loadBranches();
  };

  /** 删除分支(「确认放弃整支」:UI 两态确认,后端级联清理自有消息与 FTS)。 */
  const onDeleteBranch = async (bid: string) => {
    try {
      await ipc.deleteBranch(sessionId, bid);
    } catch (e) {
      onError(`删除分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
    setDeleting(null);
    // 若删除的是激活分支,后端已回退 main;刷新会话状态触发消息重载
    await useSessionsStore.getState().refresh();
    await useMessagesStore.getState().load(sessionId, true);
    await loadBranches();
  };

  /** 切换分支(store 更新 active_branch 后由页面 effect 统一重载消息)。 */
  const pickBranch = async (bid: string) => {
    setBranchOpen(false);
    if (bid === activeBranch) return;
    try {
      await setActiveBranch(sessionId, bid);
    } catch (e) {
      onError(`切换分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return (
    <div className="chat-head">
      {titleEdit !== null ? (
        <div className="chat-title-edit">
          <input
            autoFocus
            value={titleEdit}
            onChange={(e) => setTitleEdit(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void onRenameTitle();
              if (e.key === 'Escape') setTitleEdit(null);
            }}
            onBlur={() => void onRenameTitle()}
          />
          <button
            type="button"
            title="确认"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => void onRenameTitle()}
          >
            <IconCheck size={14} />
          </button>
          <button
            type="button"
            title="取消"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setTitleEdit(null)}
          >
            <IconX size={14} />
          </button>
        </div>
      ) : (
        <>
          <span className="chat-title ellipsis" title={title}>
            {title}
          </span>
          <button
            type="button"
            className="chat-title-btn"
            title="重命名会话(同步更新记忆标题)"
            onClick={() => setTitleEdit(title)}
          >
            <IconEdit size={13} />
          </button>
        </>
      )}
      {/* b3:工作台模式下头部出示预设 chip(预设唯一入口,composer 只保留权限 chip) */}
      {mode === 'work' && <PresetChip sessionId={sessionId} onError={onError} />}
      {/* F4:分支指示条(仅多分支或激活分支非 main 时显示) */}
      {(branches.length > 1 || activeBranch !== 'main') && (
        <div className="branch-chip-wrap">
          <button
            type="button"
            className="branch-chip"
            title="会话分支:编辑重发 / 重新生成会产生新分支"
            onClick={() => {
              setRenaming(null);
              setDeleting(null);
              setBranchOpen((v) => !v);
            }}
          >
            <IconPullRequest size={13} />
            <span>
              {branchLabel(activeBranchInfo, activeBranch)} · 共 {branches.length} 分支
            </span>
            <IconChevronDown size={12} />
          </button>
          {branchOpen && (
            <>
              <div className="cmd-backdrop" onClick={() => setBranchOpen(false)} />
              <div className="branch-pop">
                {branches.map((b) => (
                  <div
                    key={b.branch_id}
                    className={`branch-item${b.branch_id === activeBranch ? ' active' : ''}`}
                  >
                    {renaming?.id === b.branch_id ? (
                      <div className="branch-rename-row">
                        <input
                          autoFocus
                          value={renaming.name}
                          placeholder={b.branch_id === 'main' ? '主分支' : `分支 ${b.branch_id.slice(0, 4)}`}
                          onChange={(e) => setRenaming({ id: b.branch_id, name: e.target.value })}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') void onRenameBranch();
                            if (e.key === 'Escape') setRenaming(null);
                          }}
                        />
                        <button type="button" title="确认" onClick={() => void onRenameBranch()}>
                          <IconCheck size={14} />
                        </button>
                        <button type="button" title="取消" onClick={() => setRenaming(null)}>
                          <IconX size={14} />
                        </button>
                      </div>
                    ) : (
                      <>
                        <div className="branch-item-main" onClick={() => void pickBranch(b.branch_id)}>
                          <div className="branch-item-name">
                            {branchLabel(b, b.branch_id)}
                            {b.branch_id === activeBranch && <span className="tag">当前</span>}
                          </div>
                          <div className="branch-item-sub">
                            {b.first_summary || '(空分支)'} · {b.message_count} 条
                          </div>
                        </div>
                        <button
                          type="button"
                          title="重命名"
                          onClick={() => setRenaming({ id: b.branch_id, name: b.name })}
                        >
                          <IconEdit size={13} />
                        </button>
                        {b.branch_id !== 'main' &&
                          (deleting === b.branch_id ? (
                            <button
                              type="button"
                              className="branch-del-confirm"
                              title="确认删除该分支及其全部消息"
                              onClick={() => void onDeleteBranch(b.branch_id)}
                            >
                              确认
                            </button>
                          ) : (
                            <button
                              type="button"
                              title="删除分支"
                              onClick={() => setDeleting(b.branch_id)}
                            >
                              <IconTrash size={13} />
                            </button>
                          ))}
                      </>
                    )}
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}
      <span className="sp" />
      {mode === 'work' ? (
        <>
          <button
            type="button"
            className="btn-ghost btn-xs"
            title="切到普通对话(仅此会话;不改变全局默认)"
            onClick={() => {
              void (async () => {
                try {
                  await setWorkbenchMode(sessionId, 'chat');
                  nav(`/chat/${sessionId}`);
                } catch (e) {
                  onError(`切换普通对话失败:${e instanceof Error ? e.message : String(e)}`);
                }
              })();
            }}
          >
            <IconChat size={12} />
            切到普通对话
          </button>
          {chrome && (
            <button
              type="button"
              className={`chat-panel-toggle${chrome.sidebarOpen ? ' on' : ''}`}
              title={chrome.sidebarOpen ? '收起会话栏' : '展开会话栏'}
              onClick={chrome.onToggleSidebar}
            >
              <IconPanel size={15} className="flip" />
            </button>
          )}
          {chrome && (
            <button
              type="button"
              className={`chat-panel-toggle${chrome.inspectorOpen ? ' on' : ''}`}
              title={chrome.inspectorOpen ? '收起检查器' : '展开检查器'}
              onClick={chrome.onToggleInspector}
            >
              <IconPanel size={15} />
            </button>
          )}
        </>
      ) : (
        <button
          type="button"
          className="chat-panel-toggle"
          title="切到工作台(仅此会话;不改变全局默认)"
          onClick={() => {
            void (async () => {
              try {
                await setWorkbenchMode(sessionId, 'work');
                nav(`/work/${sessionId}`);
              } catch (e) {
                onError(`切换工作台失败:${e instanceof Error ? e.message : String(e)}`);
              }
            })();
          }}
        >
          <IconLayers size={14} />
        </button>
      )}
      <button
        type="button"
        className="model-switch"
        onClick={onOpenModelSwitch}
        title="切换本会话使用的模型"
      >
        <span className={`stat-dot ${current ? 'ok' : 'warn'}`} />
        <span className="model-name">{current ? current.model : '未配置模型'}</span>
        {current && <span className="tag">{current.providerId}</span>}
        <IconChevronDown size={13} />
      </button>
      <div style={{ position: 'relative' }}>
        <button
          type="button"
          className="chat-panel-toggle"
          onClick={() => setExportMenuOpen((v) => !v)}
          disabled={exporting}
          title={exported ? '已导出' : '导出会话'}
        >
          {exported ? <IconCheck size={14} /> : <IconDownload size={14} />}
        </button>
        {exportMenuOpen && (
          <>
            <div style={{ position: 'fixed', inset: 0, zIndex: 'var(--z-menu)' }} onClick={() => setExportMenuOpen(false)} />
            <div className="set-pop" style={{ right: 0, top: 'calc(100% + 4px)', minWidth: 150 }}>
              <button type="button" onClick={() => void runExport('md')}>Markdown</button>
              <button type="button" onClick={() => void runExport('html')}>HTML(可直接打开)</button>
            </div>
          </>
        )}
      </div>
      {/* 工作台模式下纲要/记忆由检查器承载,头部不再重复入口 */}
      {mode !== 'work' && (
        <>
          <button
            type="button"
            className={`chat-panel-toggle${outlineOpen ? ' on' : ''}`}
            onClick={onToggleOutline}
            title={outlineOpen ? '收起纲要面板' : '展开纲要面板'}
          >
            <IconList size={15} />
          </button>
          <button
            type="button"
            className={`chat-panel-toggle${memCollapsed ? '' : ' on'}`}
            onClick={onToggleMemPanel}
            title={memCollapsed ? '展开记忆面板' : '收起记忆面板'}
          >
            <IconBrain size={15} />
          </button>
        </>
      )}
    </div>
  );
}

